//! Guided, background MCP installation use case.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use futures::future::{BoxFuture, FutureExt};

use crate::application::extension_directory::supports;
use crate::compat::ccswitch::extension::ExtensionStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::operation::phase;
use crate::domain::{
    AppError, DesktopAppId, ErrorCode, Extension, ExtensionKind, ExtensionScope, McpInstallDraft,
    OperationExtension, OperationId, OperationKind, OperationStatus,
};
use crate::infrastructure::OperationManager;
use crate::platform::redact::{redact_secrets, truncate_tail};

pub type McpInstaller = Arc<
    dyn Fn(
            ExtensionScope,
            String,
            McpInstallDraft,
        ) -> BoxFuture<'static, Result<Extension, AppError>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct McpInstallRequest {
    pub scope: ExtensionScope,
    pub id: String,
    pub draft: McpInstallDraft,
}

pub struct McpInstallationService {
    operations: Arc<OperationManager>,
    installer: McpInstaller,
}

impl McpInstallationService {
    pub fn new(operations: Arc<OperationManager>, installer: McpInstaller) -> Self {
        Self {
            operations,
            installer,
        }
    }

    pub fn system(app_handle: tauri::AppHandle, operations: Arc<OperationManager>) -> Self {
        let installer: McpInstaller = Arc::new(move |scope, id, draft| {
            let app_handle = app_handle.clone();
            Box::pin(async move {
                tauri::async_runtime::spawn_blocking(move || {
                    ExtensionStore::open(&app_handle)?.install_mcp_in_scope(scope, &id, &draft)
                })
                .await
                .map_err(|error| {
                    AppError::new(ErrorCode::Internal, "error.mcp.actionPanicked")
                        .with_technical(safe_detail(error))
                        .with_remediation("error.remediation.retryOrViewDetails")
                })?
            })
        });
        Self::new(operations, installer)
    }

    pub fn prepare(
        scope: ExtensionScope,
        draft: McpInstallDraft,
    ) -> Result<McpInstallRequest, AppError> {
        gate(scope)?;
        draft.validate()?;
        Ok(McpInstallRequest {
            scope,
            id: product_id(&draft.normalized_name()),
            draft,
        })
    }

    pub fn begin(&self, request: &McpInstallRequest) -> Result<OperationId, AppError> {
        gate(request.scope)?;
        request.draft.validate()?;
        let extension = OperationExtension {
            kind: ExtensionKind::Mcp,
            id: request.id.clone(),
            name: request.draft.normalized_name(),
        };
        let id = match request.scope {
            ExtensionScope::Tool { id } => {
                self.operations
                    .begin_extension(OperationKind::Install, id, extension)?
            }
            ExtensionScope::DesktopApp { id } => {
                self.operations
                    .begin_desktop_extension(OperationKind::Install, id, extension)?
            }
        };
        let _ = self
            .operations
            .update_progress(&id, 5, Some(phase::PREPARING.to_string()));
        Ok(id)
    }

    pub async fn run(&self, id: OperationId, request: McpInstallRequest) {
        if let Err(reason) = self.verify_pairing(&id, &request) {
            self.finish_mismatch(&id, reason);
            return;
        }

        let _ = self
            .operations
            .update_progress(&id, 35, Some(phase::CONFIGURING.to_string()));
        let future = catch_unwind(AssertUnwindSafe(|| {
            (self.installer)(request.scope, request.id.clone(), request.draft.clone())
        }));
        let outcome = match future {
            Ok(future) => AssertUnwindSafe(future)
                .catch_unwind()
                .await
                .unwrap_or_else(|payload| Err(panic_error(payload))),
            Err(payload) => Err(panic_error(payload)),
        }
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
            log::warn!("failed to finish MCP operation {}: {error}", id.as_str());
        }
    }

    fn verify_pairing(&self, id: &OperationId, request: &McpInstallRequest) -> Result<(), String> {
        let operation = self
            .operations
            .get(id)
            .ok_or_else(|| "no such operation".to_string())?;
        let target = operation.extension.as_ref();
        if operation.status != OperationStatus::Running
            || operation.kind != OperationKind::Install
            || !operation_matches_scope(&operation, request.scope)
            || target.map(|value| value.kind) != Some(ExtensionKind::Mcp)
            || target.map(|value| value.id.as_str()) != Some(request.id.as_str())
        {
            return Err("operation and MCP request do not match".to_string());
        }
        Ok(())
    }

    fn finish_mismatch(&self, id: &OperationId, reason: String) {
        log::error!("refusing to run MCP operation {}: {reason}", id.as_str());
        let failure = AppError::new(ErrorCode::Internal, "error.mcp.installFailed")
            .with_technical(reason)
            .with_remediation("error.remediation.retryOrViewDetails")
            .with_context_id(id.as_str());
        if let Err(error) = self.operations.finish(id, Err(failure)) {
            log::warn!("failed to finish mismatched MCP operation: {error}");
        }
    }
}

fn gate(scope: ExtensionScope) -> Result<(), AppError> {
    let supported = match scope {
        ExtensionScope::Tool { id } => supports(ExtensionKind::Mcp, &capabilities_for(id)),
        ExtensionScope::DesktopApp {
            id: DesktopAppId::ClaudeDesktop,
        } => matches!(
            crate::platform::Platform::current(),
            crate::platform::Platform::MacOs | crate::platform::Platform::Windows
        ),
        ExtensionScope::DesktopApp { .. } => false,
    };
    if supported {
        Ok(())
    } else {
        Err(
            AppError::new(ErrorCode::McpUnavailable, "error.mcp.unsupportedScope").with_technical(
                format!("{} cannot install MCP connections", scope.stable_key()),
            ),
        )
    }
}

fn operation_matches_scope(operation: &crate::domain::Operation, scope: ExtensionScope) -> bool {
    match scope {
        ExtensionScope::Tool { id } => {
            operation.tool == Some(id) && operation.desktop_app.is_none()
        }
        ExtensionScope::DesktopApp { id } => {
            operation.desktop_app == Some(id) && operation.tool.is_none()
        }
    }
}

fn product_id(name: &str) -> String {
    let mut base = String::new();
    let mut separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !base.is_empty() {
                base.push('-');
            }
            separator = false;
            base.push(character.to_ascii_lowercase());
        } else {
            separator = true;
        }
        if base.len() >= 48 {
            break;
        }
    }
    while base.ends_with('-') {
        base.pop();
    }
    if base.is_empty() {
        base.push_str("mcp");
    }
    let unique = uuid::Uuid::new_v4().simple().to_string();
    format!("{base}-{}", &unique[..8])
}

fn verify_install(request: &McpInstallRequest, installed: &Extension) -> Result<(), AppError> {
    if installed.kind == ExtensionKind::Mcp
        && installed.scope == request.scope
        && installed.id == request.id
        && installed.enabled
    {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::InstallFailed, "error.mcp.verifyFailed")
            .with_technical("installed MCP connection was not present and enabled after refresh")
            .with_remediation("error.remediation.retryOrViewDetails"),
    )
}

fn panic_error(payload: Box<dyn Any + Send>) -> AppError {
    AppError::new(ErrorCode::Internal, "error.mcp.actionPanicked")
        .with_technical(panic_summary(payload))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn panic_summary(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        safe_detail(message)
    } else if let Some(message) = payload.downcast_ref::<String>() {
        safe_detail(message)
    } else {
        "MCP installation panicked with a non-string payload".to_string()
    }
}

fn safe_detail(value: impl std::fmt::Display) -> String {
    truncate_tail(&redact_secrets(&value.to_string()), 8, 512)
}

#[cfg(test)]
#[path = "mcp_installation/tests.rs"]
mod tests;
