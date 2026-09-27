//! Read projection for MCP connections.
//!
//! Upstream stores MCP as **one global record plus a per-tool switch** (`McpApps`,
//! `app_config.rs:8-22`), so "the MCP list of this tool" is every record projected one by one.

use std::any::Any;
use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{OnceLock, RwLock, RwLockWriteGuard};

use serde_json::{Map, Value};

use crate::app_config::{AppType, McpApps, McpServer, MultiAppConfig};
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, ExtensionManagement, ExtensionScope,
    McpConnectionDraft, McpInstallDraft, McpVariableDraft,
};
use crate::services::McpService;
use crate::store::AppState;

use super::{
    adopt_failed, detail, list_failed, non_empty, not_found, reject_adopt_unsupported,
    toggle_failed,
};

mod display;
mod editing;
mod removal;

pub(super) use editing::{form, update};

/// Product MCP writes share a global record set even though each operation is
/// scoped to one tool. This lock closes the read-create-write race between two
/// different tool operations and coordinates installs with product toggles.
fn mcp_write_guard() -> RwLockWriteGuard<'static, ()> {
    static LOCK: OnceLock<RwLock<()>> = OnceLock::new();
    LOCK.get_or_init(|| RwLock::new(()))
        .write()
        .unwrap_or_else(|poisoned| {
            log::warn!("MCP product write lock was poisoned; recovering protected state");
            poisoned.into_inner()
        })
}

pub(super) fn extension_from_server(
    scope: ExtensionScope,
    raw: &McpServer,
    app_type: &AppType,
) -> Extension {
    let (detail, portability) = display::describe(&raw.server);
    Extension {
        kind: ExtensionKind::Mcp,
        id: raw.id.clone(),
        scope,
        name: raw.name.clone(),
        description: non_empty(raw.description.clone()),
        detail,
        portability,
        management: ExtensionManagement::Managed,
        enabled: raw.apps.is_enabled_for(app_type),
        // Turning off an MCP entry only affects the current tool; the switches of the other tools are left untouched.
        can_disable: true,
    }
}

fn extension_from_detected_server(scope: ExtensionScope, raw: &McpServer) -> Extension {
    let (detail, portability) = display::describe(&raw.server);
    Extension {
        kind: ExtensionKind::Mcp,
        id: raw.id.clone(),
        scope,
        name: raw.name.clone(),
        description: non_empty(raw.description.clone()),
        detail,
        portability,
        management: ExtensionManagement::Detected,
        enabled: true,
        can_disable: false,
    }
}

/// Read one tool's live MCP file into an in-memory upstream config. The free-
/// form server spec never crosses the product boundary; only the small
/// `Extension` projection is returned by `list`, and the edit form reads
/// typed fields out of it (`editing::form`).
fn scan_live(app_type: &AppType) -> Result<Vec<McpServer>, crate::error::AppError> {
    let mut config = MultiAppConfig::default();
    match app_type {
        AppType::Claude => crate::mcp::import_from_claude(&mut config)?,
        AppType::Codex => crate::mcp::import_from_codex(&mut config)?,
        AppType::Gemini => crate::mcp::import_from_gemini(&mut config)?,
        AppType::GrokBuild => crate::mcp::import_from_grokbuild(&mut config)?,
        AppType::OpenCode => crate::mcp::import_from_opencode(&mut config)?,
        AppType::Hermes => crate::mcp::import_from_hermes(&mut config)?,
        AppType::ClaudeDesktop => crate::mcp::import_from_claude_desktop(&mut config)?,
        AppType::OpenClaw | AppType::Pi => 0,
    };
    Ok(config
        .mcp
        .servers
        .unwrap_or_default()
        .into_values()
        .collect())
}

pub(super) fn list(
    state: &AppState,
    scope: ExtensionScope,
    app_type: &AppType,
) -> Result<Vec<Extension>, AppError> {
    let raw = McpService::get_all_servers(state).map_err(list_failed)?;
    let managed_ids = raw.keys().cloned().collect::<HashSet<_>>();
    let mut projected = raw
        .values()
        .map(|server| extension_from_server(scope, server, app_type))
        .collect::<Vec<_>>();

    match scan_live(app_type) {
        Ok(live) => {
            let mut detected = live
                .iter()
                .filter(|server| !managed_ids.contains(&server.id))
                .map(|server| extension_from_detected_server(scope, server))
                .collect::<Vec<_>>();
            detected.sort_by_cached_key(|entry| (entry.name.to_lowercase(), entry.id.clone()));
            projected.extend(detected);
        }
        Err(error) => {
            // A malformed external config must not hide the last trustworthy
            // managed inventory. The detail is redacted before it reaches logs.
            log::warn!(
                "could not inspect live MCP inventory for {}: {}",
                app_type.as_str(),
                detail(error)
            );
        }
    }

    Ok(projected)
}

pub(super) fn set_enabled(
    state: &AppState,
    app_type: &AppType,
    id: &str,
    enabled: bool,
) -> Result<(), AppError> {
    let _guard = mcp_write_guard();
    toggle_locked(state, app_type, id, enabled)
}

/// The toggle body without the lock, for callers that already hold
/// `mcp_write_guard` (a std `RwLock` is not reentrant).
fn toggle_locked(
    state: &AppState,
    app_type: &AppType,
    id: &str,
    enabled: bool,
) -> Result<(), AppError> {
    // This upstream step does more than flip a DB flag: enabling writes the entry into that tool's
    // live config and disabling removes it again (services/mcp.rs:80-88). The eight-step write flow
    // is guaranteed by upstream itself.
    let original = McpService::get_all_servers(state)
        .map_err(toggle_failed)?
        .shift_remove(id)
        .ok_or_else(|| toggle_failed(format!("MCP {id} was absent before toggle")))?;

    let outcome = catch_unwind(AssertUnwindSafe(|| {
        McpService::toggle_app(state, id, app_type.clone(), enabled)
    }));
    let action_error = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(toggle_failed(error)),
        Err(payload) => Some(toggle_failed(format!(
            "MCP toggle panicked: {}",
            panic_detail(payload)
        ))),
    };
    if let Some(error) = action_error {
        return Err(restore_toggle_row(state, &original, error));
    }

    let verified = match McpService::get_all_servers(state) {
        Ok(mut rows) => rows.shift_remove(id),
        Err(error) => {
            return Err(restore_toggle_row(state, &original, toggle_failed(error)));
        }
    };
    if verified
        .as_ref()
        .is_none_or(|server| server.apps.is_enabled_for(app_type) != enabled)
    {
        return Err(restore_toggle_row(
            state,
            &original,
            toggle_failed(format!(
                "MCP {id} did not have the requested {} flag after toggle",
                app_type.as_str()
            )),
        ));
    }
    Ok(())
}

/// `McpService::toggle_app` updates the database before it touches a live
/// client config. Keep the product-facing operation atomic by restoring the
/// exact authoritative row whenever the live write or post-write check fails.
fn restore_toggle_row(state: &AppState, original: &McpServer, action: AppError) -> AppError {
    let restore = catch_unwind(AssertUnwindSafe(|| state.db.save_mcp_server(original)));
    let restore_detail = match restore {
        Ok(Ok(())) => match McpService::get_all_servers(state) {
            Ok(mut rows) => match rows.shift_remove(&original.id) {
                Some(restored) if same_server(&restored, original) => return action,
                Some(_) => {
                    "restored MCP row did not match the exact pre-toggle snapshot".to_string()
                }
                None => "restored MCP row was absent during verification".to_string(),
            },
            Err(error) => detail(error),
        },
        Ok(Err(error)) => detail(error),
        Err(payload) => format!("database restore panicked: {}", panic_detail(payload)),
    };
    let action_detail = action
        .technical_message
        .as_deref()
        .unwrap_or("MCP toggle failed");
    AppError::new(ErrorCode::ConfigWriteFailed, "error.extension.toggleFailed")
        .with_technical(detail(format!(
            "toggle: {action_detail}; database restore: {restore_detail}"
        )))
        .with_remediation("error.remediation.retryOrViewDetails")
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

/// Bring one MCP connection under product management and set this scope to
/// `enabled`. A connection found in this scope's own file is imported the way
/// upstream's `import_from_*` merge does, narrowed to a single id: a new row
/// keeps the live spec, an existing row keeps its fields and gains this
/// scope's flag, and the live file is read, never written. A connection that
/// is already managed but absent from this scope goes through the ordinary
/// toggle, which writes the live file, so adopting from every scope converges.
pub(super) fn adopt_detected(
    state: &AppState,
    scope: ExtensionScope,
    app_type: &AppType,
    id: &str,
    enabled: bool,
) -> Result<(), AppError> {
    if matches!(app_type, AppType::OpenClaw | AppType::Pi) {
        return Err(reject_adopt_unsupported(ExtensionKind::Mcp));
    }
    // MCP rows are global across tools, so discovery, import and verification
    // share the same product write lock as install/toggle/remove.
    let _guard = mcp_write_guard();
    let mut managed = McpService::get_all_servers(state).map_err(adopt_failed)?;
    let found = scan_live(app_type)
        .map_err(adopt_failed)?
        .into_iter()
        .find(|server| server.id == id);

    let row = match (managed.shift_remove(id), found) {
        (Some(existing), _) if existing.apps.is_enabled_for(app_type) == enabled => return Ok(()),
        (Some(mut existing), Some(_)) if enabled => {
            existing.apps.set_enabled_for(app_type, true);
            existing
        }
        (Some(_), _) => return toggle_locked(state, app_type, id, enabled),
        (None, Some(found)) => found,
        (None, None) => return Err(not_found(scope, ExtensionKind::Mcp, id)),
    };
    state.db.save_mcp_server(&row).map_err(adopt_failed)?;

    let imported = McpService::get_all_servers(state)
        .map_err(adopt_failed)?
        .shift_remove(id)
        .ok_or_else(|| adopt_failed(format!("MCP {id} was absent after import")))?;
    if !imported.apps.is_enabled_for(app_type) {
        return Err(adopt_failed(format!(
            "MCP {id} was not enabled for {} after import",
            app_type.as_str()
        )));
    }
    if enabled {
        Ok(())
    } else {
        // The import mirrors what is on disk, so "off" is applied afterwards
        // and removes the entry from this scope's live file.
        toggle_locked(state, app_type, id, false)
    }
}

pub(super) fn install(
    state: &AppState,
    scope: ExtensionScope,
    app_type: &AppType,
    id: &str,
    draft: &McpInstallDraft,
) -> Result<Extension, AppError> {
    draft.validate()?;
    if !valid_product_id(id) {
        return Err(
            AppError::new(ErrorCode::InstallFailed, "error.mcp.installFailed")
                .with_technical("generated MCP id is invalid")
                .with_remediation("error.remediation.retryOrViewDetails"),
        );
    }

    let _guard = mcp_write_guard();
    let current = McpService::get_all_servers(state).map_err(list_failed)?;
    if current.contains_key(id) {
        return Err(
            AppError::new(ErrorCode::InstallFailed, "error.mcp.installFailed")
                .with_technical("generated MCP id already exists")
                .with_remediation("error.remediation.retryOrViewDetails"),
        );
    }

    let server = server_from_draft(id, app_type, draft);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        McpService::upsert_server(state, server.clone())
    }));
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            return Err(rollback_or_escalate(state, id, install_failed(error)));
        }
        Err(payload) => {
            return Err(rollback_or_escalate(state, id, action_panicked(payload)));
        }
    }

    let stored = match McpService::get_all_servers(state) {
        Ok(mut rows) => rows.shift_remove(id),
        Err(error) => {
            return Err(rollback_or_escalate(state, id, verify_failed(error)));
        }
    };
    let Some(stored) = stored else {
        return Err(rollback_or_escalate(
            state,
            id,
            verify_failed("new MCP row is absent after write"),
        ));
    };
    if stored.name != server.name
        || stored.server != server.server
        || !stored.apps.is_enabled_for(app_type)
    {
        return Err(rollback_or_escalate(
            state,
            id,
            verify_failed("new MCP row does not match the requested connection"),
        ));
    }

    Ok(extension_from_server(scope, &stored, app_type))
}

pub(super) fn installed(
    state: &AppState,
    scope: ExtensionScope,
    app_type: &AppType,
    id: &str,
) -> Result<Extension, AppError> {
    removal::installed(state, scope, app_type, id)
}

pub(super) fn remove(state: &AppState, scope: ExtensionScope, id: &str) -> Result<(), AppError> {
    removal::remove(state, scope, id)
}

fn server_from_draft(id: &str, app_type: &AppType, draft: &McpInstallDraft) -> McpServer {
    let mut apps = McpApps::default();
    apps.set_enabled_for(app_type, true);
    McpServer {
        id: id.to_string(),
        name: draft.normalized_name(),
        server: connection_spec(&draft.connection),
        apps,
        description: draft.normalized_description(),
        homepage: None,
        docs: None,
        tags: Vec::new(),
    }
}

pub(super) fn connection_spec(connection: &McpConnectionDraft) -> Value {
    // Upstream's unified spec (`mcp/validation.rs`) is Claude's shape: `env`
    // for a local command, `headers` for a remote one. Each tool adapter
    // renames them where its own format differs (Codex `http_headers`,
    // OpenCode `environment`).
    let mut spec = Map::new();
    match connection {
        McpConnectionDraft::Stdio {
            command,
            arguments,
            env,
        } => {
            spec.insert("type".to_string(), Value::String("stdio".to_string()));
            spec.insert(
                "command".to_string(),
                Value::String(command.trim().to_string()),
            );
            if !arguments.is_empty() {
                spec.insert(
                    "args".to_string(),
                    Value::Array(arguments.iter().cloned().map(Value::String).collect()),
                );
            }
            insert_variables(&mut spec, "env", env);
        }
        McpConnectionDraft::Http { url, headers } => {
            spec.insert("type".to_string(), Value::String("http".to_string()));
            spec.insert("url".to_string(), Value::String(url.trim().to_string()));
            insert_variables(&mut spec, "headers", headers);
        }
        McpConnectionDraft::Sse { url, headers } => {
            spec.insert("type".to_string(), Value::String("sse".to_string()));
            spec.insert("url".to_string(), Value::String(url.trim().to_string()));
            insert_variables(&mut spec, "headers", headers);
        }
    }
    Value::Object(spec)
}

fn insert_variables(spec: &mut Map<String, Value>, key: &str, variables: &[McpVariableDraft]) {
    if variables.is_empty() {
        return;
    }
    let object = variables
        .iter()
        .map(|variable| {
            (
                variable.normalized_name().to_string(),
                Value::String(variable.normalized_value().to_string()),
            )
        })
        .collect::<Map<_, _>>();
    spec.insert(key.to_string(), Value::Object(object));
}

fn rollback_or_escalate(state: &AppState, id: &str, original: AppError) -> AppError {
    match rollback_new_server(state, id) {
        Ok(()) => original,
        Err(rollback) => {
            let original_detail = original
                .technical_message
                .as_deref()
                .unwrap_or("MCP installation failed");
            AppError::new(
                ErrorCode::ConfigWriteFailed,
                "error.mcp.installCleanupFailed",
            )
            .with_technical(detail(format!(
                "install: {original_detail}; cleanup: {rollback}"
            )))
            .with_remediation("error.remediation.retryOrViewDetails")
        }
    }
}

fn rollback_new_server(state: &AppState, id: &str) -> Result<(), String> {
    let removal = catch_unwind(AssertUnwindSafe(|| McpService::delete_server(state, id)));
    match removal {
        Ok(Ok(_)) => {
            let remains = McpService::get_all_servers(state)
                .map_err(detail)?
                .contains_key(id);
            if remains {
                Err("new MCP row remained after cleanup".to_string())
            } else {
                Ok(())
            }
        }
        Ok(Err(error)) => Err(detail(error)),
        Err(payload) => Err(panic_detail(payload)),
    }
}

fn install_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.mcp.installFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn verify_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::InstallFailed, "error.mcp.verifyFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn action_panicked(payload: Box<dyn Any + Send>) -> AppError {
    AppError::new(ErrorCode::Internal, "error.mcp.actionPanicked")
        .with_technical(panic_detail(payload))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn panic_detail(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        detail(message)
    } else if let Some(message) = payload.downcast_ref::<String>() {
        detail(message)
    } else {
        "MCP installation panicked with a non-string payload".to_string()
    }
}

pub(super) fn valid_product_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}
