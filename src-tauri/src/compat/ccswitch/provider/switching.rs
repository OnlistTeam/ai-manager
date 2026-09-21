//! Transactional product wrapper around the upstream provider switch path.

use crate::app_config::AppType;
use crate::domain::{AppError, ErrorCode, Provider, ToolId};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::services::ProviderService;
use crate::store::AppState;

use super::live_preservation::{self, LivePreservation};
use super::{app_type_for, log_switch_warnings, upstream_detail, ProviderStore};

pub(super) fn switch(
    store: &ProviderStore,
    tool: ToolId,
    id: &str,
) -> Result<Vec<Provider>, AppError> {
    // Confirm the record still exists first, so "it was already deleted" can be said in plain
    // words instead of falling into upstream's generic failure branch.
    store.find_raw(tool, id)?;
    let app_type = app_type_for(tool);
    let previous = ProviderService::current(&store.state, app_type.clone())
        .map_err(|error| switch_failed(&error))?;
    // During a proxy takeover, live holds only placeholders and upstream does not write live
    // either; the preservation logic stands aside as well. The check is identical to
    // `should_hot_switch` in the upstream `switch_normal` (both conditions are checked): the
    // proxy service may be temporarily stopped, but a live backup in the DB still means the
    // takeover has not ended.
    let takeover_active_before_switch = is_takeover_active(&store.state, &app_type);
    let preservation = if takeover_active_before_switch {
        LivePreservation::None
    } else {
        live_preservation::capture(tool)
    };

    match ProviderService::switch(&store.state, app_type.clone(), id) {
        Ok(outcome) => {
            // Advanced users can see the diagnostics in the log; the UI only shows the fact that the switch succeeded.
            log_switch_warnings(tool, id, &outcome.warnings);
            // The takeover switch uses a different lock (services/proxy.rs) and is not in the same
            // critical section as this switch(), so a takeover may go from "not taken over" to
            // "taken over" while switch() runs. Re-check before writing back and skip only when the
            // takeover was turned on during this switch: if it was already on before switch()
            // started, preservation is already None above and restore() is a no-op anyway, so no
            // special handling and no warning are needed.
            // The reverse (the takeover turning off mid-switch) needs no handling either: we never
            // wrote anything, upstream already handled the hot switch, and once the takeover ends
            // the live file naturally returns to what this switch wrote.
            if !takeover_active_before_switch && is_takeover_active(&store.state, &app_type) {
                log::warn!(
                    "provider switch: skipped live preservation because proxy takeover became active mid-switch"
                );
            } else {
                // The switch already succeeded; a preservation failure is reported as a preservation failure only and does not roll back current.
                preservation.restore()?;
            }
            store.list(tool)
        }
        Err(error) => Err(undo_half_switch(
            &store.state,
            &app_type,
            id,
            &previous,
            switch_failed(&error),
        )),
    }
}

/// During a proxy takeover the DB has a live backup, or the live file still holds placeholders —
/// either condition means live belongs to the proxy takeover, not to this switch. Matches the
/// upstream `should_hot_switch` check line by line (`services/provider/mod.rs`).
fn is_takeover_active(state: &AppState, app_type: &AppType) -> bool {
    let has_live_backup = futures::executor::block_on(state.db.get_live_backup(app_type.as_str()))
        .ok()
        .flatten()
        .is_some();
    has_live_backup
        || state
            .proxy_service
            .detect_takeover_in_live_config_for_app(app_type)
}

/// Upstream `switch_normal` commits `current` (local settings + DB) before it
/// writes the live file for Claude/Gemini/Grok. When that write fails, the
/// pointer already names a service whose config was never applied, and the
/// list and launch paths would both trust it. Move `current` back to where it
/// was; if even that fails, surface both errors in the technical detail.
fn undo_half_switch(
    state: &AppState,
    app_type: &AppType,
    id: &str,
    previous: &str,
    failure: AppError,
) -> AppError {
    let moved = match ProviderService::current(state, app_type.clone()) {
        Ok(current) => current,
        Err(error) => return with_rollback_detail(failure, &upstream_detail(&error)),
    };
    if moved != id || moved == previous {
        return failure;
    }
    match restore_current(state, app_type, previous) {
        Ok(()) => failure,
        Err(error) => with_rollback_detail(failure, &upstream_detail(&error)),
    }
}

fn restore_current(
    state: &AppState,
    app_type: &AppType,
    previous: &str,
) -> Result<(), crate::error::AppError> {
    crate::settings::set_current_provider(app_type, (!previous.is_empty()).then_some(previous))?;
    // The DAO has no "clear current" entry point. It first resets every
    // `is_current` flag for the app and then marks the given id; an empty id
    // matches no row, so this also restores the "no current service" state.
    state.db.set_current_provider(app_type.as_str(), previous)
}

fn with_rollback_detail(failure: AppError, rollback: &str) -> AppError {
    let switch = failure
        .technical_message
        .clone()
        .unwrap_or_else(|| "service switch failed".to_string());
    failure.with_technical(truncate_tail(
        &redact_secrets(&format!("switch: {switch}; rollback: {rollback}")),
        20,
        2000,
    ))
}

fn switch_failed(error: &crate::error::AppError) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.switchFailed")
        .with_technical(upstream_detail(error))
        .with_remediation("error.remediation.checkServiceSettings")
}
