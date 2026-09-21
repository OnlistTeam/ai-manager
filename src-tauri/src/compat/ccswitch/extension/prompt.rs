//! Product compatibility layer for prompts (the instruction file a tool reads on every launch).
//!
//! The list only ever projects `Extension`; the body is returned through `PromptDetail` only when
//! the user explicitly opens the editor. Standard tools use the product transaction rather than
//! the inherited `upsert_prompt` / `enable_prompt` paths: those paths can clear an unmanaged live
//! file or back-fill a hand edit into the selected database row. Pi is the exception because its
//! active selection is derived from native AGENTS.md instead of a persisted flag; it delegates to
//! the inherited, revision-checked Pi service and adds product-level recovery verification.

use std::sync::{OnceLock, RwLock, RwLockWriteGuard};

use chrono::Utc;

use crate::app_config::AppType;
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, ExtensionManagement, ExtensionScope,
    PromptDetail, PromptDraft, ToolId,
};
use crate::prompt::Prompt;
use crate::prompt_files::prompt_file_path;
use crate::services::PromptService;
use crate::store::AppState;

use super::{detail as safe_detail, list_failed, non_empty, reject_disable};

mod live;
mod rollback;
use live::{backup_live, read_live, replace_live, verify_live, LiveSnapshot};
use rollback::{restore_removed_or_escalate, restore_row_or_escalate, restore_rows_or_escalate};

fn prompt_write_guard() -> RwLockWriteGuard<'static, ()> {
    static LOCK: OnceLock<RwLock<()>> = OnceLock::new();
    LOCK.get_or_init(|| RwLock::new(()))
        .write()
        .unwrap_or_else(|poisoned| {
            log::warn!("Prompt product write lock was poisoned; recovering protected state");
            poisoned.into_inner()
        })
}

pub(super) fn extension_from_prompt(tool: ToolId, id: &str, raw: &Prompt) -> Extension {
    Extension {
        kind: ExtensionKind::Prompt,
        id: id.to_string(),
        scope: ExtensionScope::tool(tool),
        name: raw.name.clone(),
        description: non_empty(raw.description.clone()),
        management: ExtensionManagement::Managed,
        enabled: raw.enabled,
        // Upstream is single-select; turning off the last one would clear the instruction file, so only "switch to this one" is offered.
        can_disable: false,
    }
}

pub(super) fn list(
    state: &AppState,
    tool: ToolId,
    app_type: &AppType,
) -> Result<Vec<Extension>, AppError> {
    let raw = PromptService::get_prompts(state, app_type.clone()).map_err(list_failed)?;
    Ok(raw
        .iter()
        .map(|(id, entry)| extension_from_prompt(tool, id, entry))
        .collect())
}

pub(super) fn guard_enabled(enabled: bool) -> Result<(), AppError> {
    if enabled {
        Ok(())
    } else {
        Err(reject_disable())
    }
}

/// "Switch to this one": the same eight-step protocol as `save_active` (read live -> back up ->
/// atomic replace -> database -> verify, rolling back on failure), except the previously enabled
/// row must also be turned off. The body in the database is the single source of truth; the
/// current state of the live file only goes into the backup and is never written back into any row.
pub(super) fn set_enabled(
    state: &AppState,
    tool: ToolId,
    app_type: &AppType,
    id: &str,
    enabled: bool,
) -> Result<(), AppError> {
    guard_enabled(enabled)?;
    let _guard = prompt_write_guard();
    let prompts = PromptService::get_prompts(state, app_type.clone()).map_err(read_failed)?;
    let target = prompts
        .get(id)
        .cloned()
        .ok_or_else(|| prompt_not_found(tool, id))?;
    if target.enabled {
        // Already this one. Do not rewrite live: instructions the user edited by hand are only
        // replaced (with a backup) when they explicitly switch to another entry.
        return Ok(());
    }
    if matches!(app_type, AppType::Pi) {
        // Pi does not persist an `enabled` bit. Its native AGENTS.md is the
        // source of truth, and PromptService performs the revision-checked
        // replacement while preserving an unmatched live file as a managed
        // backup row. Running the generic switch transaction here would write
        // a false persisted state and bypass that conflict check.
        PromptService::enable_prompt(state, app_type.clone(), id).map_err(save_failed)?;
        let refreshed = PromptService::get_prompts(state, app_type.clone()).map_err(read_failed)?;
        if refreshed.get(id).is_some_and(|prompt| prompt.enabled) {
            return Ok(());
        }
        return Err(verify_failed(
            "Pi AGENTS.md does not select the requested Prompt after switching",
        ));
    }
    let previously_active = prompts
        .values()
        .filter(|prompt| prompt.enabled)
        .cloned()
        .collect::<Vec<_>>();
    switch_active(state, app_type, &target, &previously_active)
}

pub(super) fn get(
    state: &AppState,
    tool: ToolId,
    app_type: &AppType,
    id: &str,
) -> Result<PromptDetail, AppError> {
    let prompts = PromptService::get_prompts(state, app_type.clone()).map_err(read_failed)?;
    let prompt = prompts.get(id).ok_or_else(|| prompt_not_found(tool, id))?;
    ensure_stored_content_is_editable(prompt)?;
    Ok(detail_from_prompt(tool, prompt))
}

pub(super) fn save(
    state: &AppState,
    tool: ToolId,
    app_type: &AppType,
    id: Option<&str>,
    draft: &PromptDraft,
) -> Result<Vec<Extension>, AppError> {
    draft.validate()?;
    let _guard = prompt_write_guard();
    let prompts = PromptService::get_prompts(state, app_type.clone()).map_err(read_failed)?;
    let now = unix_timestamp()?;

    let (id, previous) = match id {
        Some(id) => {
            let previous = prompts
                .get(id)
                .cloned()
                .ok_or_else(|| prompt_not_found(tool, id))?;
            (id.to_string(), Some(previous))
        }
        None => (unique_prompt_id(&prompts), None),
    };
    let stored = Prompt {
        id: id.clone(),
        name: draft.normalized_name(),
        content: draft.normalized_content(),
        description: draft.normalized_description(),
        enabled: previous.as_ref().is_some_and(|prompt| prompt.enabled),
        created_at: previous
            .as_ref()
            .and_then(|prompt| prompt.created_at)
            .or(Some(now)),
        updated_at: Some(now),
    };

    if matches!(app_type, AppType::Pi) {
        save_pi(state, app_type, previous.as_ref(), &stored)?;
    } else if stored.enabled {
        let previous = previous
            .as_ref()
            .ok_or_else(|| save_failed("active Prompt edit has no prior row"))?;
        save_active(state, app_type, previous, &stored)?;
    } else {
        save_inactive(state, app_type, previous.as_ref(), &stored)?;
    }

    list(state, tool, app_type)
}

pub(super) fn remove(
    state: &AppState,
    tool: ToolId,
    app_type: &AppType,
    id: &str,
) -> Result<Vec<Extension>, AppError> {
    let _guard = prompt_write_guard();
    let prompts = PromptService::get_prompts(state, app_type.clone()).map_err(remove_failed)?;
    let original = prompts
        .get(id)
        .cloned()
        .ok_or_else(|| prompt_not_found(tool, id))?;
    if original.enabled {
        return Err(
            AppError::new(ErrorCode::ConfigWriteFailed, "error.prompt.removeActive")
                .with_remediation("error.remediation.chooseAnotherPrompt"),
        );
    }

    if matches!(app_type, AppType::Pi) {
        PromptService::delete_prompt(state, app_type.clone(), id).map_err(remove_failed)?;
        let rows = PromptService::get_prompts(state, app_type.clone()).map_err(remove_failed)?;
        if rows.contains_key(id) {
            PromptService::upsert_prompt(state, app_type.clone(), id, original.clone())
                .map_err(remove_failed)?;
            return Err(verify_failed("Pi Prompt row remained after removal"));
        }
        return list(state, tool, app_type);
    }

    state
        .db
        .delete_prompt(app_type.as_str(), id)
        .map_err(remove_failed)?;
    match state.db.get_prompts(app_type.as_str()) {
        Ok(rows) if !rows.contains_key(id) => list(state, tool, app_type),
        Ok(_) => Err(restore_removed_or_escalate(
            state,
            app_type,
            &original,
            verify_failed("Prompt row remained after removal"),
        )),
        Err(error) => Err(restore_removed_or_escalate(
            state,
            app_type,
            &original,
            remove_failed(error),
        )),
    }
}

pub(super) fn import_current(
    state: &AppState,
    tool: ToolId,
    app_type: &AppType,
) -> Result<Vec<Extension>, AppError> {
    let _guard = prompt_write_guard();
    let content = PromptService::get_current_file_content(app_type.clone())
        .map_err(import_failed)?
        .ok_or_else(|| {
            AppError::new(ErrorCode::ConfigParseFailed, "error.prompt.importMissing")
                .with_remediation("error.remediation.openToolManually")
        })?;
    if content.trim().is_empty() {
        return Err(
            AppError::new(ErrorCode::ConfigParseFailed, "error.prompt.importEmpty")
                .with_remediation("error.remediation.openToolManually"),
        );
    }

    let probe = PromptDraft {
        name: "Imported".to_string(),
        description: None,
        content: content.clone(),
    };
    probe.validate()?;

    let prompts = PromptService::get_prompts(state, app_type.clone()).map_err(import_failed)?;
    if prompts.values().any(|prompt| prompt.content == content) {
        return list(state, tool, app_type);
    }

    let now = unix_timestamp()?;
    let prompt = Prompt {
        id: unique_prompt_id(&prompts),
        name: format!("Imported {}", Utc::now().format("%Y-%m-%d %H:%M")),
        content,
        description: None,
        enabled: false,
        created_at: Some(now),
        updated_at: Some(now),
    };
    save_inactive(state, app_type, None, &prompt)?;
    list(state, tool, app_type)
}

fn save_inactive(
    state: &AppState,
    app_type: &AppType,
    previous: Option<&Prompt>,
    stored: &Prompt,
) -> Result<(), AppError> {
    let save = if matches!(app_type, AppType::Pi) {
        PromptService::upsert_prompt(state, app_type.clone(), &stored.id, stored.clone())
    } else {
        state.db.save_prompt(app_type.as_str(), stored)
    };
    if let Err(error) = save {
        return Err(save_failed(error));
    }
    match verify_stored(state, app_type, stored) {
        Ok(()) => Ok(()),
        Err(error) => Err(restore_row_or_escalate(
            state, app_type, previous, stored, None, error,
        )),
    }
}

/// Pi's active selection is derived from the exact contents of AGENTS.md. The
/// inherited service owns its native-file lock and revision comparison; this
/// wrapper adds the product's timestamped recovery copy and verifies the
/// database/live projection without ever persisting `enabled = true`.
fn save_pi(
    state: &AppState,
    app_type: &AppType,
    previous: Option<&Prompt>,
    stored: &Prompt,
) -> Result<(), AppError> {
    if stored.enabled {
        let path = prompt_file_path(app_type).map_err(save_failed)?;
        let snapshot = read_live(&path)?;
        if let LiveSnapshot::Present(bytes) = &snapshot {
            backup_live(&path, bytes)?;
        }
    }

    if let Err(error) =
        PromptService::upsert_prompt(state, app_type.clone(), &stored.id, stored.clone())
    {
        return Err(save_failed(error));
    }

    let verified = PromptService::get_prompts(state, app_type.clone())
        .map_err(read_failed)
        .and_then(|rows| {
            let saved = rows
                .get(&stored.id)
                .ok_or_else(|| verify_failed("Pi Prompt row is absent after save"))?;
            if same_prompt(saved, stored) {
                Ok(())
            } else {
                Err(verify_failed(
                    "Pi Prompt row or AGENTS.md selection differs after save",
                ))
            }
        });
    if let Err(failure) = verified {
        return Err(restore_pi_or_escalate(
            state, app_type, previous, stored, failure,
        ));
    }
    Ok(())
}

fn restore_pi_or_escalate(
    state: &AppState,
    app_type: &AppType,
    previous: Option<&Prompt>,
    attempted: &Prompt,
    failure: AppError,
) -> AppError {
    let database_rollback = match previous {
        Some(previous) => {
            PromptService::upsert_prompt(state, app_type.clone(), &previous.id, previous.clone())
        }
        None => PromptService::delete_prompt(state, app_type.clone(), &attempted.id),
    };
    if let Err(rollback) = database_rollback {
        return rollback_escalation(&failure, rollback);
    }

    failure
}

fn rollback_escalation(failure: &AppError, rollback: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.prompt.saveRestoreFailed",
    )
    .with_technical(safe_detail(format!(
        "Prompt update failed ({}); rollback also failed: {rollback}",
        failure.message_key
    )))
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn save_active(
    state: &AppState,
    app_type: &AppType,
    previous: &Prompt,
    stored: &Prompt,
) -> Result<(), AppError> {
    let path = prompt_file_path(app_type).map_err(save_failed)?;
    let live = read_live(&path)?;
    if let LiveSnapshot::Present(bytes) = &live {
        backup_live(&path, bytes)?;
    }

    if let Err(error) = replace_live(&path, stored.content.as_bytes()) {
        return Err(restore_row_or_escalate(
            state,
            app_type,
            Some(previous),
            stored,
            Some((&path, &live)),
            error,
        ));
    }
    if let Err(error) = state.db.save_prompt(app_type.as_str(), stored) {
        return Err(restore_row_or_escalate(
            state,
            app_type,
            Some(previous),
            stored,
            Some((&path, &live)),
            save_failed(error),
        ));
    }
    if let Err(error) = verify_stored(state, app_type, stored)
        .and_then(|()| verify_live(&path, stored.content.as_bytes()))
    {
        return Err(restore_row_or_escalate(
            state,
            app_type,
            Some(previous),
            stored,
            Some((&path, &live)),
            error,
        ));
    }
    Ok(())
}

fn switch_active(
    state: &AppState,
    app_type: &AppType,
    target: &Prompt,
    previously_active: &[Prompt],
) -> Result<(), AppError> {
    let path = prompt_file_path(app_type).map_err(save_failed)?;
    let live = read_live(&path)?;
    if let LiveSnapshot::Present(bytes) = &live {
        backup_live(&path, bytes)?;
    }

    let activated = Prompt {
        enabled: true,
        ..target.clone()
    };
    let deactivated = previously_active
        .iter()
        .map(|prompt| Prompt {
            enabled: false,
            ..prompt.clone()
        })
        .collect::<Vec<_>>();
    let mut originals = previously_active.to_vec();
    originals.push(target.clone());

    let attempt = || -> Result<(), AppError> {
        replace_live(&path, activated.content.as_bytes())?;
        for prompt in &deactivated {
            state
                .db
                .save_prompt(app_type.as_str(), prompt)
                .map_err(save_failed)?;
        }
        state
            .db
            .save_prompt(app_type.as_str(), &activated)
            .map_err(save_failed)?;
        verify_stored(state, app_type, &activated)?;
        for prompt in &deactivated {
            verify_stored(state, app_type, prompt)?;
        }
        verify_live(&path, activated.content.as_bytes())
    };
    attempt().map_err(|failure| {
        restore_rows_or_escalate(state, app_type, &originals, (&path, &live), failure)
    })
}

fn verify_stored(state: &AppState, app_type: &AppType, expected: &Prompt) -> Result<(), AppError> {
    let rows = state
        .db
        .get_prompts(app_type.as_str())
        .map_err(save_failed)?;
    let stored = rows
        .get(&expected.id)
        .ok_or_else(|| verify_failed("Prompt row is absent after save"))?;
    if same_prompt(stored, expected) {
        Ok(())
    } else {
        Err(verify_failed(
            "Prompt row does not match the requested draft",
        ))
    }
}

fn same_prompt(left: &Prompt, right: &Prompt) -> bool {
    left.id == right.id
        && left.name == right.name
        && left.content == right.content
        && left.description == right.description
        && left.enabled == right.enabled
        && left.created_at == right.created_at
        && left.updated_at == right.updated_at
}

fn detail_from_prompt(tool: ToolId, prompt: &Prompt) -> PromptDetail {
    PromptDetail {
        id: prompt.id.clone(),
        tool,
        name: prompt.name.clone(),
        description: non_empty(prompt.description.clone()),
        content: prompt.content.clone(),
        enabled: prompt.enabled,
    }
}

fn ensure_stored_content_is_editable(prompt: &Prompt) -> Result<(), AppError> {
    PromptDraft {
        name: prompt.name.clone(),
        description: prompt.description.clone(),
        content: prompt.content.clone(),
    }
    .validate()
}

fn unique_prompt_id(prompts: &indexmap::IndexMap<String, Prompt>) -> String {
    loop {
        let id = format!("prompt-{}", uuid::Uuid::new_v4().simple());
        if !prompts.contains_key(&id) {
            return id;
        }
    }
}

fn unix_timestamp() -> Result<i64, AppError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .map_err(save_failed)
}

fn prompt_not_found(tool: ToolId, id: &str) -> AppError {
    AppError::new(ErrorCode::ExtensionNotFound, "error.prompt.notFound")
        .with_technical(safe_detail(format!("{}/{id}", tool.as_str())))
}

fn read_failed(error: crate::error::AppError) -> AppError {
    AppError::new(ErrorCode::ConfigParseFailed, "error.prompt.readFailed")
        .with_technical(safe_detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn save_failed(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.prompt.saveFailed")
        .with_technical(safe_detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn verify_failed(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.prompt.saveVerifyFailed",
    )
    .with_technical(safe_detail(error))
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn remove_failed(error: crate::error::AppError) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.prompt.removeFailed")
        .with_technical(safe_detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn import_failed(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::ConfigParseFailed, "error.prompt.importFailed")
        .with_technical(safe_detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
#[path = "prompt/tests.rs"]
mod tests;
