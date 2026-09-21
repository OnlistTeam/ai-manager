//! Transactional product wrapper around the upstream global MCP delete path.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::app_config::{AppType, McpServer};
use crate::domain::{AppError, ErrorCode, Extension, ExtensionScope};
use crate::services::McpService;
use crate::store::AppState;

use super::{extension_from_server, mcp_write_guard};
use crate::compat::ccswitch::extension::detail;

pub(super) fn installed(
    state: &AppState,
    scope: ExtensionScope,
    app_type: &AppType,
    id: &str,
) -> Result<Extension, AppError> {
    let _guard = mcp_write_guard();
    let rows = McpService::get_all_servers(state).map_err(remove_failed)?;
    let server = rows.get(id).ok_or_else(|| not_installed(scope, id))?;
    Ok(extension_from_server(scope, server, app_type))
}

/// Remove the global row and its enabled live projections. Upstream deletes
/// the database row first, so every failure or panic after the snapshot is
/// followed by an exact-row restore attempt before the error leaves this layer.
pub(super) fn remove(state: &AppState, scope: ExtensionScope, id: &str) -> Result<(), AppError> {
    let _guard = mcp_write_guard();
    let rows = McpService::get_all_servers(state).map_err(remove_failed)?;
    let original = rows
        .get(id)
        .cloned()
        .ok_or_else(|| not_installed(scope, id))?;

    let deletion = catch_unwind(AssertUnwindSafe(|| McpService::delete_server(state, id)));
    let outcome = match deletion {
        Ok(Ok(true)) => verify_absent(state, id),
        Ok(Ok(false)) => Err(not_installed(scope, id)),
        Ok(Err(error)) => Err(remove_failed(error)),
        Err(payload) => Err(remove_panicked(payload)),
    };

    match outcome {
        Ok(()) => Ok(()),
        Err(error) => Err(restore_or_escalate(state, &original, error)),
    }
}

fn verify_absent(state: &AppState, id: &str) -> Result<(), AppError> {
    let remaining = McpService::get_all_servers(state).map_err(remove_failed)?;
    if remaining.contains_key(id) {
        return Err(
            AppError::new(ErrorCode::UninstallFailed, "error.mcp.removeVerifyFailed")
                .with_technical("MCP row remained after global removal")
                .with_remediation("error.remediation.retryOrViewDetails"),
        );
    }
    Ok(())
}

fn restore_or_escalate(state: &AppState, original: &McpServer, failure: AppError) -> AppError {
    match restore_exact(state, original) {
        Ok(()) => failure,
        Err(restore) => {
            let removal = failure
                .technical_message
                .as_deref()
                .unwrap_or("MCP removal failed");
            AppError::new(
                ErrorCode::ConfigWriteFailed,
                "error.mcp.removeRestoreFailed",
            )
            .with_technical(detail(format!("remove: {removal}; restore: {restore}")))
            .with_remediation("error.remediation.retryOrViewDetails")
        }
    }
}

fn restore_exact(state: &AppState, original: &McpServer) -> Result<(), String> {
    let restore = catch_unwind(AssertUnwindSafe(|| {
        McpService::upsert_server(state, original.clone())
    }));
    match restore {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(detail(error)),
        Err(payload) => return Err(panic_detail(payload)),
    }

    let rows = McpService::get_all_servers(state).map_err(detail)?;
    let restored = rows
        .get(&original.id)
        .ok_or_else(|| "MCP row is absent after restore".to_string())?;
    if same_server(restored, original) {
        Ok(())
    } else {
        Err("MCP row does not match the pre-removal snapshot".to_string())
    }
}

fn same_server(left: &McpServer, right: &McpServer) -> bool {
    left.id == right.id
        && left.name == right.name
        && left.server == right.server
        && left.apps == right.apps
        && left.description == right.description
        && left.homepage == right.homepage
        && left.docs == right.docs
        && left.tags == right.tags
}

fn not_installed(scope: ExtensionScope, id: &str) -> AppError {
    AppError::new(ErrorCode::ExtensionNotFound, "error.mcp.notInstalled")
        .with_technical(detail(format!("{}/{id}", scope.stable_key())))
}

fn remove_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UninstallFailed, "error.mcp.removeFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn remove_panicked(payload: Box<dyn Any + Send>) -> AppError {
    AppError::new(ErrorCode::Internal, "error.mcp.removePanicked")
        .with_technical(panic_detail(payload))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn panic_detail(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        detail(message)
    } else if let Some(message) = payload.downcast_ref::<String>() {
        detail(message)
    } else {
        "MCP removal panicked with a non-string payload".to_string()
    }
}
