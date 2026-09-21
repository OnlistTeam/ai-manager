//! Safe, background removal of one global MCP connection.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use futures::future::{BoxFuture, FutureExt};

use crate::application::extension_directory::supports;
use crate::compat::ccswitch::extension::ExtensionStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::operation::phase;
use crate::domain::{
    AppError, DesktopAppId, ErrorCode, ExtensionKind, ExtensionScope, OperationExtension,
    OperationId, OperationKind, OperationStatus,
};
use crate::infrastructure::OperationManager;
use crate::platform::redact::{redact_secrets, truncate_tail};

pub type McpRemover =
    Arc<dyn Fn(ExtensionScope, String) -> BoxFuture<'static, Result<(), AppError>> + Send + Sync>;

#[derive(Clone)]
pub struct McpRemovalRequest {
    pub scope: ExtensionScope,
    pub id: String,
    pub name: String,
}

pub struct McpRemovalService {
    operations: Arc<OperationManager>,
    remover: McpRemover,
}

impl McpRemovalService {
    pub fn new(operations: Arc<OperationManager>, remover: McpRemover) -> Self {
        Self {
            operations,
            remover,
        }
    }

    pub fn system(app_handle: tauri::AppHandle, operations: Arc<OperationManager>) -> Self {
        let remover: McpRemover = Arc::new(move |scope, id| {
            let app_handle = app_handle.clone();
            Box::pin(async move {
                tauri::async_runtime::spawn_blocking(move || {
                    ExtensionStore::open(&app_handle)?.remove_mcp_in_scope(scope, &id)
                })
                .await
                .map_err(|error| {
                    AppError::new(ErrorCode::Internal, "error.mcp.removePanicked")
                        .with_technical(safe_detail(error))
                        .with_remediation("error.remediation.retryOrViewDetails")
                })?
            })
        });
        Self::new(operations, remover)
    }

    /// Resolve name and kind from the authoritative global MCP inventory. The
    /// renderer supplies only a stable id for this destructive operation.
    pub fn resolve(
        app_handle: &tauri::AppHandle,
        scope: ExtensionScope,
        id: &str,
    ) -> Result<McpRemovalRequest, AppError> {
        gate(scope)?;
        let connection = ExtensionStore::open(app_handle)?.installed_mcp_in_scope(scope, id)?;
        Ok(McpRemovalRequest {
            scope,
            id: connection.id,
            name: connection.name,
        })
    }

    pub fn begin(&self, request: &McpRemovalRequest) -> Result<OperationId, AppError> {
        gate(request.scope)?;
        if request.id.trim().is_empty() || request.name.trim().is_empty() {
            return Err(
                AppError::new(ErrorCode::UninstallFailed, "error.mcp.removeFailed")
                    .with_technical("resolved MCP target is incomplete")
                    .with_remediation("error.remediation.retryOrViewDetails"),
            );
        }

        let extension = OperationExtension {
            kind: ExtensionKind::Mcp,
            id: request.id.clone(),
            name: request.name.trim().to_string(),
        };
        let id = match request.scope {
            ExtensionScope::Tool { id } => {
                self.operations
                    .begin_extension(OperationKind::Uninstall, id, extension)?
            }
            ExtensionScope::DesktopApp { id } => {
                self.operations
                    .begin_desktop_extension(OperationKind::Uninstall, id, extension)?
            }
        };
        let _ = self
            .operations
            .update_progress(&id, 5, Some(phase::PREPARING.to_string()));
        Ok(id)
    }

    pub async fn run(&self, id: OperationId, request: McpRemovalRequest) {
        if let Err(reason) = self.verify_pairing(&id, &request) {
            self.finish_mismatch(&id, reason);
            return;
        }

        let _ = self
            .operations
            .update_progress(&id, 25, Some(phase::REMOVING.to_string()));
        let future = catch_unwind(AssertUnwindSafe(|| {
            (self.remover)(request.scope, request.id.clone())
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
                "failed to finish MCP removal operation {}: {error}",
                id.as_str()
            );
        }
    }

    fn verify_pairing(&self, id: &OperationId, request: &McpRemovalRequest) -> Result<(), String> {
        let operation = self
            .operations
            .get(id)
            .ok_or_else(|| "no such operation".to_string())?;
        let target = operation.extension.as_ref();
        if operation.status != OperationStatus::Running
            || operation.kind != OperationKind::Uninstall
            || !operation_matches_scope(&operation, request.scope)
            || target.map(|value| value.kind) != Some(ExtensionKind::Mcp)
            || target.map(|value| value.id.as_str()) != Some(request.id.as_str())
        {
            return Err("operation and MCP removal request do not match".to_string());
        }
        Ok(())
    }

    fn finish_mismatch(&self, id: &OperationId, reason: String) {
        log::error!(
            "refusing to run MCP removal operation {}: {reason}",
            id.as_str()
        );
        let failure = AppError::new(ErrorCode::Internal, "error.mcp.removeFailed")
            .with_technical(reason)
            .with_remediation("error.remediation.retryOrViewDetails")
            .with_context_id(id.as_str());
        if let Err(error) = self.operations.finish(id, Err(failure)) {
            log::warn!("failed to finish mismatched MCP removal: {error}");
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
                format!("{} cannot remove MCP connections", scope.stable_key()),
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

fn panic_error(payload: Box<dyn Any + Send>) -> AppError {
    AppError::new(ErrorCode::Internal, "error.mcp.removePanicked")
        .with_technical(panic_summary(payload))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn panic_summary(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        safe_detail(message)
    } else if let Some(message) = payload.downcast_ref::<String>() {
        safe_detail(message)
    } else {
        "MCP removal panicked with a non-string payload".to_string()
    }
}

fn safe_detail(value: impl std::fmt::Display) -> String {
    truncate_tail(&redact_secrets(&value.to_string()), 8, 512)
}

#[cfg(test)]
#[path = "mcp_removal/tests.rs"]
mod tests;
