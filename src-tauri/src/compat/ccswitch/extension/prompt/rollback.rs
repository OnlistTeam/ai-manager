//! Rollback for the Prompt write paths: put every touched database row back
//! into its exact previous shape and the live file back to the snapshot taken
//! first. When even that fails, the caller gets one escalated error that
//! names both failures in its technical detail.

use std::path::Path;

use crate::app_config::AppType;
use crate::compat::ccswitch::extension::detail as safe_detail;
use crate::domain::{AppError, ErrorCode};
use crate::prompt::Prompt;
use crate::store::AppState;

use super::live::{restore_live, LiveSnapshot};

pub(super) fn restore_row_or_escalate(
    state: &AppState,
    app_type: &AppType,
    previous: Option<&Prompt>,
    attempted: &Prompt,
    live: Option<(&Path, &LiveSnapshot)>,
    failure: AppError,
) -> AppError {
    let row_restore = match previous {
        Some(previous) => state.db.save_prompt(app_type.as_str(), previous),
        None => state.db.delete_prompt(app_type.as_str(), &attempted.id),
    };
    let live_restore = live.map_or(Ok(()), |(path, snapshot)| restore_live(path, snapshot));
    escalate_unless_restored(failure, row_restore, live_restore)
}

/// Rollback for a switch: every row that was touched goes back to its exact
/// previous shape, and the live file goes back to the snapshot taken first.
pub(super) fn restore_rows_or_escalate(
    state: &AppState,
    app_type: &AppType,
    originals: &[Prompt],
    live: (&Path, &LiveSnapshot),
    failure: AppError,
) -> AppError {
    let row_restore = originals
        .iter()
        .try_for_each(|prompt| state.db.save_prompt(app_type.as_str(), prompt));
    let live_restore = restore_live(live.0, live.1);
    escalate_unless_restored(failure, row_restore, live_restore)
}

fn escalate_unless_restored(
    failure: AppError,
    row_restore: Result<(), crate::error::AppError>,
    live_restore: Result<(), crate::error::AppError>,
) -> AppError {
    if row_restore.is_ok() && live_restore.is_ok() {
        return failure;
    }

    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.prompt.saveRestoreFailed",
    )
    .with_technical(safe_detail(format!(
        "save: {}; row restore: {}; live restore: {}",
        failure.technical_message.as_deref().unwrap_or("failed"),
        row_restore
            .err()
            .map(safe_detail)
            .unwrap_or_else(|| "ok".to_string()),
        live_restore
            .err()
            .map(safe_detail)
            .unwrap_or_else(|| "ok".to_string())
    )))
    .with_remediation("error.remediation.retryOrViewDetails")
}

pub(super) fn restore_removed_or_escalate(
    state: &AppState,
    app_type: &AppType,
    original: &Prompt,
    failure: AppError,
) -> AppError {
    match state.db.save_prompt(app_type.as_str(), original) {
        Ok(()) => failure,
        Err(error) => AppError::new(
            ErrorCode::ConfigWriteFailed,
            "error.prompt.removeRestoreFailed",
        )
        .with_technical(safe_detail(format!(
            "remove: {}; restore: {error}",
            failure.technical_message.as_deref().unwrap_or("failed")
        )))
        .with_remediation("error.remediation.retryOrViewDetails"),
    }
}
