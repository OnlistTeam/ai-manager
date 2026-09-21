//! Safe background installation of one or more Skills from a user-selected ZIP.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::future::{BoxFuture, FutureExt};

use crate::application::extension_directory::supports;
use crate::compat::ccswitch::skill_catalog::SkillCatalogStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::operation::phase;
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, ExtensionManagement, OperationExtension,
    OperationId, OperationKind, OperationStatus, ToolId,
};
use crate::infrastructure::OperationManager;
use crate::platform::redact::{redact_secrets, truncate_tail};

const LOCAL_ZIP_OPERATION_ID: &str = "local-zip-install";
const FALLBACK_ARCHIVE_NAME: &str = "Skill ZIP";
const MAX_ARCHIVE_NAME_CHARS: usize = 80;

pub type SkillZipInstaller = Arc<
    dyn Fn(ToolId, PathBuf) -> BoxFuture<'static, Result<Vec<Extension>, AppError>> + Send + Sync,
>;

#[derive(Debug, Clone)]
pub struct SkillZipInstallRequest {
    tool: ToolId,
    path: PathBuf,
    archive_name: String,
}

impl SkillZipInstallRequest {
    /// Construct only from a native picker result. Renderer input never owns
    /// or supplies this path.
    pub fn selected(tool: ToolId, path: PathBuf) -> Result<Self, AppError> {
        ensure_supported(tool)?;
        validate_archive_path(&path)?;
        Ok(Self {
            tool,
            archive_name: archive_name(&path),
            path,
        })
    }
}

pub struct SkillZipInstallationService {
    operations: Arc<OperationManager>,
    installer: SkillZipInstaller,
}

impl SkillZipInstallationService {
    pub fn new(operations: Arc<OperationManager>, installer: SkillZipInstaller) -> Self {
        Self {
            operations,
            installer,
        }
    }

    pub fn system(app_handle: tauri::AppHandle, operations: Arc<OperationManager>) -> Self {
        let installer: SkillZipInstaller = Arc::new(move |tool, path| {
            let app_handle = app_handle.clone();
            Box::pin(async move {
                tauri::async_runtime::spawn_blocking(move || {
                    SkillCatalogStore::open(&app_handle)?.install_zip(tool, &path)
                })
                .await
                .map_err(worker_failed)?
            })
        });
        Self::new(operations, installer)
    }

    pub fn ensure_supported(tool: ToolId) -> Result<(), AppError> {
        ensure_supported(tool)
    }

    pub fn begin(&self, request: &SkillZipInstallRequest) -> Result<OperationId, AppError> {
        ensure_supported(request.tool)?;
        validate_archive_path(&request.path)?;
        let id = self.operations.begin_extension(
            OperationKind::Install,
            request.tool,
            OperationExtension {
                kind: ExtensionKind::Skill,
                id: LOCAL_ZIP_OPERATION_ID.to_string(),
                name: request.archive_name.clone(),
            },
        )?;
        let _ = self
            .operations
            .update_progress(&id, 5, Some(phase::PREPARING.to_string()));
        Ok(id)
    }

    pub async fn run(&self, id: OperationId, request: SkillZipInstallRequest) {
        if let Err(reason) = self.verify_pairing(&id, &request) {
            let failure = AppError::new(ErrorCode::Internal, "error.skill.zipInstallFailed")
                .with_technical(reason)
                .with_remediation("error.remediation.retryOrViewDetails")
                .with_context_id(id.as_str());
            if let Err(error) = self.operations.finish(&id, Err(failure)) {
                log::warn!("failed to finish mismatched ZIP Skill operation: {error}");
            }
            return;
        }

        let _ = self
            .operations
            .update_progress(&id, 30, Some(phase::INSTALLING.to_string()));
        let outcome = AssertUnwindSafe((self.installer)(request.tool, request.path.clone()))
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
                verify_install(request.tool, &installed)
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
            log::warn!("failed to finish ZIP Skill operation: {error}");
        }
    }

    fn verify_pairing(
        &self,
        id: &OperationId,
        request: &SkillZipInstallRequest,
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
            || target.map(|value| value.id.as_str()) != Some(LOCAL_ZIP_OPERATION_ID)
            || target.map(|value| value.name.as_str()) != Some(request.archive_name.as_str())
        {
            return Err("operation and local ZIP request do not match".to_string());
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
            .with_technical(format!("{} cannot install Skills", tool.as_str())),
    )
}

fn validate_archive_path(path: &Path) -> Result<(), AppError> {
    let is_zip = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"));
    let is_file = std::fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false);
    if is_zip && is_file {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::InstallFailed, "error.skill.zipInvalid")
            .with_technical("selected Skill archive is not an existing regular ZIP file")
            .with_remediation("error.remediation.retryOrViewDetails"),
    )
}

fn archive_name(path: &Path) -> String {
    let bounded = path
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_ARCHIVE_NAME_CHARS)
        .collect::<String>();
    let trimmed = bounded.trim();
    if trimmed.is_empty() {
        FALLBACK_ARCHIVE_NAME.to_string()
    } else {
        trimmed.to_string()
    }
}

fn verify_install(tool: ToolId, installed: &[Extension]) -> Result<(), AppError> {
    if installed.is_empty() {
        return Err(
            AppError::new(ErrorCode::InstallFailed, "error.skill.zipNoNewSkills")
                .with_technical("the archive contained no new non-conflicting Skills")
                .with_remediation("error.remediation.retryOrViewDetails"),
        );
    }
    if installed.iter().all(|extension| {
        extension.kind == ExtensionKind::Skill
            && extension.management == ExtensionManagement::Managed
            && extension.scope == crate::domain::ExtensionScope::tool(tool)
            && extension.enabled
            && !extension.id.trim().is_empty()
            && !extension.name.trim().is_empty()
    }) {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::InstallFailed, "error.skill.zipVerifyFailed")
            .with_technical("installed ZIP Skills did not match the requested tool inventory")
            .with_remediation("error.remediation.retryOrViewDetails"),
    )
}

fn worker_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::Internal, "error.skill.zipInstallFailed")
        .with_technical(truncate_tail(&redact_secrets(&error.to_string()), 8, 512))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn panic_summary(payload: Box<dyn Any + Send>) -> String {
    let raw = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "Skill ZIP installation panicked with a non-string payload".to_string()
    };
    truncate_tail(&redact_secrets(&raw), 8, 512)
}

#[cfg(test)]
#[path = "skill_zip_installation/tests.rs"]
mod tests;
