//! Transactional product wrapper around the upstream provider delete path.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::app_config::AppType;
use crate::domain::{AppError, ErrorCode, Provider, ToolId};
use crate::provider::Provider as UpstreamProvider;
use crate::services::provider::provider_exists_in_live_config;
use crate::services::ProviderService;
use crate::store::AppState;

use super::{app_type_for, provider_from_upstream, upstream_detail};

pub(super) fn can_remove(provider: &UpstreamProvider, active: bool) -> bool {
    !active && !tool_managed(provider)
}

pub(super) fn remove(state: &AppState, tool: ToolId, id: &str) -> Result<Vec<Provider>, AppError> {
    remove_with(state, tool, id, ProviderService::delete)
}

pub(super) fn remove_with<F>(
    state: &AppState,
    tool: ToolId,
    id: &str,
    deleter: F,
) -> Result<Vec<Provider>, AppError>
where
    F: FnOnce(&AppState, AppType, &str) -> Result<(), crate::error::AppError>,
{
    let app_type = app_type_for(tool);
    let rows = ProviderService::list(state, app_type.clone()).map_err(remove_failed)?;
    let original = rows.get(id).cloned().ok_or_else(|| not_found(tool, id))?;
    let current = ProviderService::current(state, app_type.clone()).map_err(remove_failed)?;

    if current == id {
        return Err(
            AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.removeActive")
                .with_remediation("error.remediation.chooseAnotherService"),
        );
    }
    if tool_managed(&original) {
        return Err(AppError::new(
            ErrorCode::ConfigWriteFailed,
            "error.provider.removeUnsupported",
        )
        .with_remediation("error.remediation.removeInTool"));
    }
    ensure_not_final_failover_member(state, &app_type, &rows, id)?;

    let was_live = live_presence(&app_type, &original).map_err(remove_failed)?;
    let deletion = catch_unwind(AssertUnwindSafe(|| deleter(state, app_type.clone(), id)));
    let outcome = match deletion {
        Ok(Ok(())) => verify_removed(state, &app_type, id, &original),
        Ok(Err(error)) => {
            let failure = remove_failed(error);
            // Upstream refused before touching anything. Re-adding a row that
            // is still there only fails (Pi reports "already exists") and would
            // turn a plain failure into `removeRestoreFailed`.
            if nothing_changed(state, &app_type, &original, was_live) {
                return Err(failure);
            }
            Err(failure)
        }
        Err(payload) => Err(remove_panicked(payload)),
    };

    match outcome {
        Ok(remaining) => Ok(remaining
            .values()
            .map(|provider| provider_from_upstream(tool, provider, &current))
            .collect()),
        Err(error) => Err(restore_or_escalate(
            state, &app_type, &original, was_live, error,
        )),
    }
}

fn tool_managed(provider: &UpstreamProvider) -> bool {
    matches!(provider.category.as_deref(), Some("omo") | Some("omo-slim"))
}

/// The routing page refuses to drop the last eligible queue member while
/// automatic failover is on (`routing.rs::remove_from_queue`, ADR-0007).
/// Deleting the service row would empty that queue through a side door, so
/// the same rule and the same message key apply here. The active service is
/// already refused above as `removeActive`.
fn ensure_not_final_failover_member(
    state: &AppState,
    app_type: &AppType,
    rows: &indexmap::IndexMap<String, UpstreamProvider>,
    id: &str,
) -> Result<(), AppError> {
    if !app_type.supports_local_proxy() {
        return Ok(());
    }
    let app = app_type.as_str();
    let config = futures::executor::block_on(state.db.get_proxy_config_for_app(app))
        .map_err(remove_failed)?;
    if !config.auto_failover_enabled {
        return Ok(());
    }
    let eligible = state
        .db
        .get_failover_queue(app)
        .map_err(remove_failed)?
        .into_iter()
        .filter(|item| {
            rows.get(&item.provider_id).is_some_and(|provider| {
                crate::proxy::provider_router::provider_supports_failover(app, provider)
            })
        })
        .collect::<Vec<_>>();
    if eligible.len() <= 1 && eligible.iter().any(|item| item.provider_id == id) {
        return Err(
            AppError::new(ErrorCode::OperationConflict, "error.routing.queueLocked").with_technical(
                safe_detail(format!(
                    "{app} cannot remove the final failover candidate while automatic failover is enabled"
                )),
            ),
        );
    }
    Ok(())
}

fn live_presence(
    app_type: &AppType,
    provider: &UpstreamProvider,
) -> Result<bool, crate::error::AppError> {
    if !app_type.is_additive_mode() {
        return Ok(false);
    }
    match provider_exists_in_live_config(app_type, &provider.id) {
        Ok(present) => Ok(present),
        Err(_) if provider_live_managed(provider) == Some(false) => Ok(false),
        Err(error) => Err(error),
    }
}

fn provider_live_managed(provider: &UpstreamProvider) -> Option<bool> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.live_config_managed)
}

fn verify_removed(
    state: &AppState,
    app_type: &AppType,
    id: &str,
    original: &UpstreamProvider,
) -> Result<indexmap::IndexMap<String, UpstreamProvider>, AppError> {
    let remaining = ProviderService::list(state, app_type.clone()).map_err(remove_failed)?;
    if remaining.contains_key(id) {
        return Err(verify_failed("provider row remained after removal"));
    }
    if app_type.is_additive_mode() && live_presence(app_type, original).map_err(remove_failed)? {
        return Err(verify_failed("provider remained in the tool configuration"));
    }
    Ok(remaining)
}

fn restore_or_escalate(
    state: &AppState,
    app_type: &AppType,
    original: &UpstreamProvider,
    was_live: bool,
    failure: AppError,
) -> AppError {
    match restore(state, app_type, original, was_live) {
        Ok(()) => failure,
        Err(restore) => {
            let removal = failure
                .technical_message
                .as_deref()
                .unwrap_or("service removal failed");
            AppError::new(
                ErrorCode::ConfigWriteFailed,
                "error.provider.removeRestoreFailed",
            )
            .with_technical(safe_detail(format!(
                "remove: {removal}; restore: {restore}"
            )))
            .with_remediation("error.remediation.retryOrViewDetails")
        }
    }
}

fn restore(
    state: &AppState,
    app_type: &AppType,
    original: &UpstreamProvider,
    was_live: bool,
) -> Result<(), String> {
    let restored = catch_unwind(AssertUnwindSafe(|| {
        ProviderService::add(state, app_type.clone(), original.clone(), was_live)
    }));
    match restored {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => return Err(upstream_detail(&error)),
        Err(payload) => return Err(panic_detail(payload)),
    }

    let rows =
        ProviderService::list(state, app_type.clone()).map_err(|error| upstream_detail(&error))?;
    let restored = rows
        .get(&original.id)
        .ok_or_else(|| "service row is absent after restore".to_string())?;
    if !same_after_restore(restored, original, app_type, was_live)? {
        return Err("service row does not match the pre-removal snapshot".to_string());
    }
    if app_type.is_additive_mode()
        && live_presence(app_type, restored).map_err(|error| upstream_detail(&error))? != was_live
    {
        return Err("tool configuration does not match the pre-removal state".to_string());
    }
    Ok(())
}

fn same_after_restore(
    restored: &UpstreamProvider,
    original: &UpstreamProvider,
    app_type: &AppType,
    was_live: bool,
) -> Result<bool, String> {
    let mut expected = original.clone();
    if app_type.is_additive_mode() {
        expected
            .meta
            .get_or_insert_with(Default::default)
            .live_config_managed = Some(was_live);
    }
    rows_match(restored, &expected)
}

fn rows_match(left: &UpstreamProvider, right: &UpstreamProvider) -> Result<bool, String> {
    let left = serde_json::to_value(left).map_err(safe_detail)?;
    let right = serde_json::to_value(right).map_err(safe_detail)?;
    Ok(left == right)
}

/// True only when the row still equals the pre-removal snapshot and the tool's
/// live file still agrees with `was_live`. Any read error counts as "changed",
/// so the restore path stays the fail-closed default.
fn nothing_changed(
    state: &AppState,
    app_type: &AppType,
    original: &UpstreamProvider,
    was_live: bool,
) -> bool {
    let row_intact = ProviderService::list(state, app_type.clone())
        .ok()
        .and_then(|rows| rows.get(&original.id).cloned())
        .and_then(|row| rows_match(&row, original).ok())
        .unwrap_or(false);
    row_intact && live_presence(app_type, original).is_ok_and(|present| present == was_live)
}

fn not_found(tool: ToolId, id: &str) -> AppError {
    AppError::new(ErrorCode::ProviderNotFound, "error.provider.notFound")
        .with_technical(safe_detail(format!("{}/{id}", tool.as_str())))
}

fn verify_failed(detail: &str) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.provider.removeVerifyFailed",
    )
    .with_technical(detail)
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn remove_failed(error: crate::error::AppError) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.removeFailed")
        .with_technical(upstream_detail(&error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn remove_panicked(payload: Box<dyn Any + Send>) -> AppError {
    AppError::new(ErrorCode::Internal, "error.provider.removePanicked")
        .with_technical(panic_detail(payload))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn panic_detail(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        safe_detail(message)
    } else if let Some(message) = payload.downcast_ref::<String>() {
        safe_detail(message)
    } else {
        "service removal panicked with a non-string payload".to_string()
    }
}

fn safe_detail(value: impl std::fmt::Display) -> String {
    crate::platform::redact::truncate_tail(
        &crate::platform::redact::redact_secrets(&value.to_string()),
        10,
        1000,
    )
}
