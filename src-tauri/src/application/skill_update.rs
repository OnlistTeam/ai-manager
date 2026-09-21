//! Safe background updates for one managed Skill.

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
    OperationId, OperationKind, OperationStatus, SkillUpdate, ToolId,
};
use crate::infrastructure::OperationManager;
use crate::platform::redact::{redact_secrets, truncate_tail};

pub type SkillUpdater =
    Arc<dyn Fn(ToolId, String) -> BoxFuture<'static, Result<Extension, AppError>> + Send + Sync>;

#[derive(Clone)]
pub struct SkillUpdateRequest {
    pub tool: ToolId,
    pub id: String,
    pub name: String,
}

pub struct SkillUpdateService {
    operations: Arc<OperationManager>,
    updater: SkillUpdater,
}

impl SkillUpdateService {
    pub fn new(operations: Arc<OperationManager>, updater: SkillUpdater) -> Self {
        Self {
            operations,
            updater,
        }
    }

    pub fn system(app_handle: tauri::AppHandle, operations: Arc<OperationManager>) -> Self {
        let updater: SkillUpdater = Arc::new(move |tool, id| {
            let app_handle = app_handle.clone();
            Box::pin(async move {
                SkillCatalogStore::open(&app_handle)?
                    .update(tool, &id)
                    .await
            })
        });
        Self::new(operations, updater)
    }

    pub async fn check(app_handle: &tauri::AppHandle) -> Result<Vec<SkillUpdate>, AppError> {
        SkillCatalogStore::open(app_handle)?.updates().await
    }

    /// The renderer supplies only the stable Skill id. Resolve the task label
    /// and repository-backed eligibility from the authoritative inventory.
    pub fn resolve(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<SkillUpdateRequest, AppError> {
        gate(tool)?;
        let target = SkillCatalogStore::open(app_handle)?.update_target(tool, id)?;
        Ok(SkillUpdateRequest {
            tool,
            id: target.id,
            name: target.name,
        })
    }

    pub fn begin(&self, request: &SkillUpdateRequest) -> Result<OperationId, AppError> {
        gate(request.tool)?;
        if request.id.trim().is_empty() || request.name.trim().is_empty() {
            return Err(
                AppError::new(ErrorCode::UpdateFailed, "error.skill.updateFailed")
                    .with_technical("resolved Skill update target is incomplete")
                    .with_remediation("error.remediation.retryOrViewDetails"),
            );
        }
        let id = self.operations.begin_extension(
            OperationKind::Update,
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

    pub async fn run(&self, id: OperationId, request: SkillUpdateRequest) {
        if let Err(reason) = self.verify_pairing(&id, &request) {
            let failure = AppError::new(ErrorCode::Internal, "error.skill.updateFailed")
                .with_technical(reason)
                .with_remediation("error.remediation.retryOrViewDetails")
                .with_context_id(id.as_str());
            if let Err(error) = self.operations.finish(&id, Err(failure)) {
                log::warn!("failed to finish mismatched Skill update: {error}");
            }
            return;
        }

        let _ = self
            .operations
            .update_progress(&id, 20, Some(phase::DOWNLOADING.to_string()));
        let outcome = AssertUnwindSafe((self.updater)(request.tool, request.id.clone()))
            .catch_unwind()
            .await
            .unwrap_or_else(|payload| {
                Err(
                    AppError::new(ErrorCode::Internal, "error.skill.updatePanicked")
                        .with_technical(panic_summary(payload))
                        .with_remediation("error.remediation.retryOrViewDetails"),
                )
            })
            .and_then(|updated| {
                let _ = self
                    .operations
                    .update_progress(&id, 90, Some(phase::CHECKING.to_string()));
                verify_update(&request, &updated)
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
            log::warn!("failed to finish Skill update operation: {error}");
        }
    }

    fn verify_pairing(&self, id: &OperationId, request: &SkillUpdateRequest) -> Result<(), String> {
        let operation = self
            .operations
            .get(id)
            .ok_or_else(|| "no such operation".to_string())?;
        let expected = operation.extension.as_ref();
        if operation.status != OperationStatus::Running
            || operation.kind != OperationKind::Update
            || operation.tool != Some(request.tool)
            || expected.map(|target| target.kind) != Some(ExtensionKind::Skill)
            || expected.map(|target| target.id.as_str()) != Some(request.id.as_str())
        {
            return Err("operation and Skill update request do not match".to_string());
        }
        Ok(())
    }
}

fn gate(tool: ToolId) -> Result<(), AppError> {
    if supports(ExtensionKind::Skill, &capabilities_for(tool)) {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::UpdateFailed, "error.skill.unsupportedTool")
            .with_technical(format!("{} cannot manage Skills", tool.as_str())),
    )
}

fn verify_update(request: &SkillUpdateRequest, updated: &Extension) -> Result<(), AppError> {
    if updated.kind == ExtensionKind::Skill
        && updated.management == ExtensionManagement::Managed
        && updated.scope == crate::domain::ExtensionScope::tool(request.tool)
        && updated.id == request.id
    {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::UpdateFailed, "error.skill.updateVerifyFailed")
            .with_technical("updated Skill was not present after refresh")
            .with_remediation("error.remediation.retryOrViewDetails"),
    )
}

fn panic_summary(payload: Box<dyn Any + Send>) -> String {
    let raw = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "Skill update panicked with a non-string payload".to_string()
    };
    truncate_tail(&redact_secrets(&raw), 8, 512)
}

#[cfg(test)]
#[path = "skill_update/tests.rs"]
mod tests;
