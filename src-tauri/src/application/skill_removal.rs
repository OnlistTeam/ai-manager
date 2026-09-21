//! Safe, background removal of one managed Skill.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use futures::future::{BoxFuture, FutureExt};

use crate::application::extension_directory::supports;
use crate::compat::ccswitch::skill_catalog::SkillCatalogStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::operation::phase;
use crate::domain::{
    AppError, ErrorCode, ExtensionKind, OperationExtension, OperationId, OperationKind,
    OperationStatus, ToolId,
};
use crate::infrastructure::OperationManager;
use crate::platform::redact::{redact_secrets, truncate_tail};

pub type SkillRemover =
    Arc<dyn Fn(ToolId, String) -> BoxFuture<'static, Result<(), AppError>> + Send + Sync>;

#[derive(Clone)]
pub struct SkillRemovalRequest {
    pub tool: ToolId,
    pub id: String,
    pub name: String,
}

pub struct SkillRemovalService {
    operations: Arc<OperationManager>,
    remover: SkillRemover,
}

impl SkillRemovalService {
    pub fn new(operations: Arc<OperationManager>, remover: SkillRemover) -> Self {
        Self {
            operations,
            remover,
        }
    }

    pub fn system(app_handle: tauri::AppHandle, operations: Arc<OperationManager>) -> Self {
        let remover: SkillRemover = Arc::new(move |tool, id| {
            let app_handle = app_handle.clone();
            Box::pin(async move {
                tauri::async_runtime::spawn_blocking(move || {
                    SkillCatalogStore::open(&app_handle)?.remove(tool, &id)
                })
                .await
                .map_err(|error| {
                    AppError::new(ErrorCode::Internal, "error.skill.removePanicked")
                        .with_technical(truncate_tail(&redact_secrets(&error.to_string()), 8, 512))
                        .with_remediation("error.remediation.retryOrViewDetails")
                })?
            })
        });
        Self::new(operations, remover)
    }

    /// Resolve name and kind from the authoritative installed inventory before
    /// taking the mutation lock. The renderer supplies only the stable id.
    pub fn resolve(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<SkillRemovalRequest, AppError> {
        gate(tool)?;
        let skill = SkillCatalogStore::open(app_handle)?.installed(tool, id)?;
        Ok(SkillRemovalRequest {
            tool,
            id: skill.id,
            name: skill.name,
        })
    }

    pub fn begin(&self, request: &SkillRemovalRequest) -> Result<OperationId, AppError> {
        gate(request.tool)?;
        if request.id.trim().is_empty() || request.name.trim().is_empty() {
            return Err(
                AppError::new(ErrorCode::UninstallFailed, "error.skill.removeFailed")
                    .with_technical("resolved Skill target is incomplete")
                    .with_remediation("error.remediation.retryOrViewDetails"),
            );
        }

        let id = self.operations.begin_extension(
            OperationKind::Uninstall,
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

    pub async fn run(&self, id: OperationId, request: SkillRemovalRequest) {
        if let Err(reason) = self.verify_pairing(&id, &request) {
            log::error!(
                "refusing to run Skill removal operation {}: {reason}",
                id.as_str()
            );
            let failure = AppError::new(ErrorCode::Internal, "error.skill.removeFailed")
                .with_technical(reason)
                .with_remediation("error.remediation.retryOrViewDetails")
                .with_context_id(id.as_str());
            if let Err(error) = self.operations.finish(&id, Err(failure)) {
                log::warn!(
                    "failed to finish mismatched Skill removal {}: {error}",
                    id.as_str()
                );
            }
            return;
        }

        let _ = self
            .operations
            .update_progress(&id, 25, Some(phase::REMOVING.to_string()));
        let future = catch_unwind(AssertUnwindSafe(|| {
            (self.remover)(request.tool, request.id.clone())
        }));
        let outcome = match future {
            Ok(future) => AssertUnwindSafe(future)
                .catch_unwind()
                .await
                .unwrap_or_else(|payload| Err(panic_error(payload))),
            Err(payload) => Err(panic_error(payload)),
        }
        .map(|()| {
            let _ = self
                .operations
                .update_progress(&id, 90, Some(phase::CHECKING.to_string()));
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
                .update_progress(&id, 100, Some(phase::REMOVED.to_string()));
        }
        if let Err(error) = self.operations.finish(&id, outcome) {
            log::warn!(
                "failed to finish Skill removal operation {}: {error}",
                id.as_str()
            );
        }
    }

    fn verify_pairing(
        &self,
        id: &OperationId,
        request: &SkillRemovalRequest,
    ) -> Result<(), String> {
        let operation = self
            .operations
            .get(id)
            .ok_or_else(|| "no such operation".to_string())?;
        let expected = operation.extension.as_ref();
        if operation.status != OperationStatus::Running
            || operation.kind != OperationKind::Uninstall
            || operation.tool != Some(request.tool)
            || expected.map(|target| target.kind) != Some(ExtensionKind::Skill)
            || expected.map(|target| target.id.as_str()) != Some(request.id.as_str())
        {
            return Err("operation and Skill removal request do not match".to_string());
        }
        Ok(())
    }
}

fn gate(tool: ToolId) -> Result<(), AppError> {
    if supports(ExtensionKind::Skill, &capabilities_for(tool)) {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::UninstallFailed, "error.skill.unsupportedTool")
            .with_technical(format!("{} cannot remove Skills", tool.as_str())),
    )
}

fn panic_summary(payload: Box<dyn Any + Send>) -> String {
    let raw = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "Skill removal panicked with a non-string payload".to_string()
    };
    truncate_tail(&redact_secrets(&raw), 8, 512)
}

fn panic_error(payload: Box<dyn Any + Send>) -> AppError {
    AppError::new(ErrorCode::Internal, "error.skill.removePanicked")
        .with_technical(panic_summary(payload))
        .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
#[path = "skill_removal/tests.rs"]
mod tests;
