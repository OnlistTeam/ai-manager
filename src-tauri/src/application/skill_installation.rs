//! Skill catalog and background installation use cases.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures::future::{BoxFuture, FutureExt};

use crate::application::extension_directory::supports;
use crate::compat::ccswitch::skill_catalog::SkillCatalogStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::operation::phase;
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, OperationExtension, OperationId, OperationKind,
    OperationStatus, SkillCatalogItem, ToolId,
};
use crate::infrastructure::OperationManager;
use crate::platform::redact::{redact_secrets, truncate_tail};

pub type SkillInstaller = Arc<
    dyn Fn(ToolId, SkillCatalogItem) -> BoxFuture<'static, Result<Extension, AppError>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct SkillInstallRequest {
    pub tool: ToolId,
    pub item: SkillCatalogItem,
}

pub struct SkillInstallationService {
    operations: Arc<OperationManager>,
    installer: SkillInstaller,
}

impl SkillInstallationService {
    pub fn new(operations: Arc<OperationManager>, installer: SkillInstaller) -> Self {
        Self {
            operations,
            installer,
        }
    }

    pub fn system(app_handle: tauri::AppHandle, operations: Arc<OperationManager>) -> Self {
        let installer: SkillInstaller = Arc::new(move |tool, item| {
            let app_handle = app_handle.clone();
            Box::pin(async move {
                SkillCatalogStore::open(&app_handle)?
                    .install(tool, &item)
                    .await
            })
        });
        Self::new(operations, installer)
    }

    pub async fn catalog(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
    ) -> Result<Vec<SkillCatalogItem>, AppError> {
        gate(tool)?;
        SkillCatalogStore::open(app_handle)?.list().await
    }

    pub fn begin(&self, request: &SkillInstallRequest) -> Result<OperationId, AppError> {
        gate(request.tool)?;
        request.item.validate_for_install()?;
        let id = self.operations.begin_extension(
            OperationKind::Install,
            request.tool,
            OperationExtension {
                kind: ExtensionKind::Skill,
                id: request.item.id.clone(),
                name: request.item.name.trim().to_string(),
            },
        )?;
        let _ = self
            .operations
            .update_progress(&id, 5, Some(phase::PREPARING.to_string()));
        Ok(id)
    }

    pub async fn run(&self, id: OperationId, request: SkillInstallRequest) {
        if let Err(reason) = self.verify_pairing(&id, &request) {
            log::error!("refusing to run Skill operation {}: {reason}", id.as_str());
            let failure = AppError::new(ErrorCode::Internal, "error.skill.installFailed")
                .with_technical(reason)
                .with_remediation("error.remediation.retryOrViewDetails")
                .with_context_id(id.as_str());
            if let Err(error) = self.operations.finish(&id, Err(failure)) {
                log::warn!(
                    "failed to finish mismatched Skill operation {}: {error}",
                    id.as_str()
                );
            }
            return;
        }

        let _ = self
            .operations
            .update_progress(&id, 20, Some(phase::DOWNLOADING.to_string()));
        let outcome = AssertUnwindSafe((self.installer)(request.tool, request.item.clone()))
            .catch_unwind()
            .await
            .unwrap_or_else(|payload| {
                Err(
                    AppError::new(ErrorCode::Internal, "error.skill.actionPanicked")
                        .with_technical(panic_summary(payload))
                        .with_remediation("error.remediation.retryOrViewDetails"),
                )
            })
            .and_then(|installed| {
                let _ = self
                    .operations
                    .update_progress(&id, 90, Some(phase::CHECKING.to_string()));
                verify_install(&request, &installed)
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
            log::warn!("failed to finish Skill operation {}: {error}", id.as_str());
        }
    }

    fn verify_pairing(
        &self,
        id: &OperationId,
        request: &SkillInstallRequest,
    ) -> Result<(), String> {
        let operation = self
            .operations
            .get(id)
            .ok_or_else(|| "no such operation".to_string())?;
        let expected = operation.extension.as_ref();
        if operation.status != OperationStatus::Running
            || operation.kind != OperationKind::Install
            || operation.tool != Some(request.tool)
            || expected.map(|target| target.kind) != Some(ExtensionKind::Skill)
            || expected.map(|target| target.id.as_str()) != Some(request.item.id.as_str())
        {
            return Err("operation and Skill request do not match".to_string());
        }
        Ok(())
    }
}

fn gate(tool: ToolId) -> Result<(), AppError> {
    if supports(ExtensionKind::Skill, &capabilities_for(tool)) {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::InstallFailed, "error.skill.unsupportedTool")
            .with_technical(format!("{} cannot install Skills", tool.as_str())),
    )
}

fn verify_install(request: &SkillInstallRequest, installed: &Extension) -> Result<(), AppError> {
    if installed.kind == ExtensionKind::Skill
        && installed.scope == crate::domain::ExtensionScope::tool(request.tool)
        && installed.id == request.item.id
        && installed.enabled
    {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::InstallFailed, "error.skill.verifyFailed")
            .with_technical("installed Skill was not present and enabled after refresh")
            .with_remediation("error.remediation.retryOrViewDetails"),
    )
}

fn panic_summary(payload: Box<dyn Any + Send>) -> String {
    let raw = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "Skill installation panicked with a non-string payload".to_string()
    };
    truncate_tail(&redact_secrets(&raw), 8, 512)
}

#[cfg(test)]
#[path = "skill_installation/tests.rs"]
mod tests;
