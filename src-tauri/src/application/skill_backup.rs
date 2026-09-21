//! Product use cases for Skill uninstall recovery copies.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures::future::{BoxFuture, FutureExt};

use crate::application::extension_directory::supports;
use crate::compat::ccswitch::skill_catalog::SkillCatalogStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::operation::phase;
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, ExtensionManagement, OperationExtension,
    OperationId, OperationKind, OperationStatus, SkillBackup, ToolId,
};
use crate::infrastructure::OperationManager;
use crate::platform::redact::{redact_secrets, truncate_tail};

pub type SkillBackupRestorer =
    Arc<dyn Fn(ToolId, String) -> BoxFuture<'static, Result<Extension, AppError>> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct SkillBackupRestoreRequest {
    tool: ToolId,
    id: String,
    name: String,
}

pub struct SkillBackupDirectory;

impl SkillBackupDirectory {
    pub fn list(app_handle: &tauri::AppHandle) -> Result<Vec<SkillBackup>, AppError> {
        SkillCatalogStore::open(app_handle)?.backups()
    }

    pub fn delete(app_handle: &tauri::AppHandle, id: &str) -> Result<Vec<SkillBackup>, AppError> {
        SkillCatalogStore::open(app_handle)?.delete_backup(id)
    }

    pub fn resolve(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<SkillBackupRestoreRequest, AppError> {
        ensure_supported(tool)?;
        let backup = SkillCatalogStore::open(app_handle)?.backup_target(id)?;
        if backup.conflicts {
            return Err(
                AppError::new(ErrorCode::OperationConflict, "error.skill.backupConflict")
                    .with_technical("the recovery copy conflicts with a managed Skill")
                    .with_remediation("error.remediation.retryOrViewDetails"),
            );
        }
        Ok(SkillBackupRestoreRequest {
            tool,
            id: backup.id,
            name: backup.name,
        })
    }
}

pub struct SkillBackupRestoreService {
    operations: Arc<OperationManager>,
    restorer: SkillBackupRestorer,
}

impl SkillBackupRestoreService {
    pub fn new(operations: Arc<OperationManager>, restorer: SkillBackupRestorer) -> Self {
        Self {
            operations,
            restorer,
        }
    }

    pub fn system(app_handle: tauri::AppHandle, operations: Arc<OperationManager>) -> Self {
        let restorer: SkillBackupRestorer = Arc::new(move |tool, id| {
            let app_handle = app_handle.clone();
            Box::pin(async move {
                tauri::async_runtime::spawn_blocking(move || {
                    SkillCatalogStore::open(&app_handle)?.restore_backup(tool, &id)
                })
                .await
                .map_err(worker_failed)?
            })
        });
        Self::new(operations, restorer)
    }

    pub fn begin(&self, request: &SkillBackupRestoreRequest) -> Result<OperationId, AppError> {
        ensure_supported(request.tool)?;
        if request.id.trim().is_empty() || request.name.trim().is_empty() {
            return Err(AppError::new(
                ErrorCode::ConfigWriteFailed,
                "error.skill.backupRestoreFailed",
            )
            .with_technical("resolved Skill recovery copy is incomplete")
            .with_remediation("error.remediation.retryOrViewDetails"));
        }
        let id = self.operations.begin_extension(
            OperationKind::Install,
            request.tool,
            OperationExtension {
                kind: ExtensionKind::Skill,
                id: request.id.clone(),
                name: request.name.trim().to_string(),
            },
        )?;
        let _ = self
            .operations
            .update_progress(&id, 5, Some(phase::PREPARING.to_string()));
        Ok(id)
    }

    pub async fn run(&self, id: OperationId, request: SkillBackupRestoreRequest) {
        if let Err(reason) = self.verify_pairing(&id, &request) {
            let failure = AppError::new(ErrorCode::Internal, "error.skill.backupRestoreFailed")
                .with_technical(reason)
                .with_remediation("error.remediation.retryOrViewDetails")
                .with_context_id(id.as_str());
            if let Err(error) = self.operations.finish(&id, Err(failure)) {
                log::warn!("failed to finish mismatched Skill restore: {error}");
            }
            return;
        }

        let _ = self
            .operations
            .update_progress(&id, 25, Some(phase::INSTALLING.to_string()));
        let outcome = AssertUnwindSafe((self.restorer)(request.tool, request.id.clone()))
            .catch_unwind()
            .await
            .unwrap_or_else(|payload| {
                Err(
                    AppError::new(ErrorCode::Internal, "error.skill.actionPanicked")
                        .with_technical(panic_summary(payload))
                        .with_remediation("error.remediation.retryOrViewDetails"),
                )
            })
            .and_then(|restored| {
                let _ = self
                    .operations
                    .update_progress(&id, 90, Some(phase::CHECKING.to_string()));
                verify_restore(request.tool, &restored)
            })
            .map_err(|mut error| {
                if error.context_id.is_none() {
                    error.context_id = Some(id.as_str().to_string());
                }
                error
            });

        if outcome.is_ok() {
            let _ = self
                .operations
                .update_progress(&id, 100, Some(phase::READY.to_string()));
        }
        if let Err(error) = self.operations.finish(&id, outcome) {
            log::warn!("failed to finish Skill restore operation: {error}");
        }
    }

    fn verify_pairing(
        &self,
        id: &OperationId,
        request: &SkillBackupRestoreRequest,
    ) -> Result<(), String> {
        let operation = self
            .operations
            .get(id)
            .ok_or_else(|| "no such operation".to_string())?;
        let target = operation.extension.as_ref();
        if operation.status != OperationStatus::Running
            || operation.kind != OperationKind::Install
            || operation.tool != Some(request.tool)
            || target.map(|value| value.kind) != Some(ExtensionKind::Skill)
            || target.map(|value| value.id.as_str()) != Some(request.id.as_str())
            || target.map(|value| value.name.as_str()) != Some(request.name.trim())
        {
            return Err("operation and Skill recovery request do not match".to_string());
        }
        Ok(())
    }
}

fn ensure_supported(tool: ToolId) -> Result<(), AppError> {
    if supports(ExtensionKind::Skill, &capabilities_for(tool)) {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::InstallFailed, "error.skill.unsupportedTool")
            .with_technical(format!("{} cannot restore Skills", tool.as_str())),
    )
}

fn verify_restore(tool: ToolId, restored: &Extension) -> Result<(), AppError> {
    if restored.kind == ExtensionKind::Skill
        && restored.management == ExtensionManagement::Managed
        && restored.scope == crate::domain::ExtensionScope::tool(tool)
        && restored.enabled
        && !restored.id.trim().is_empty()
        && !restored.name.trim().is_empty()
    {
        return Ok(());
    }
    Err(AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.skill.backupVerifyFailed",
    )
    .with_technical("restored Skill was not present in the requested tool inventory")
    .with_remediation("error.remediation.retryOrViewDetails"))
}

fn worker_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::Internal, "error.skill.backupRestoreFailed")
        .with_technical(truncate_tail(&redact_secrets(&error.to_string()), 8, 512))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn panic_summary(payload: Box<dyn Any + Send>) -> String {
    let raw = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "Skill recovery panicked with a non-string payload".to_string()
    };
    truncate_tail(&redact_secrets(&raw), 8, 512)
}

#[cfg(test)]
#[path = "skill_backup/tests.rs"]
mod tests;
