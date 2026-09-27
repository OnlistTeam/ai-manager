//! Edit one MCP connection in place (ADR-0062).
//!
//! Once a connection is managed, the product database is the source of truth
//! and each tool's file is a written copy, so a hand edit of `~/.claude.json`
//! is overwritten by the next write. Editing therefore happens here: the form
//! is prefilled from the stored spec, and saving replaces only the fields the
//! form shows, keeps every app's on/off flag, and lets upstream's
//! `McpService::upsert_server` write the new spec to every app that has it on,
//! the same path an install takes.

use std::panic::{catch_unwind, AssertUnwindSafe};

use serde_json::Value;

use crate::app_config::{AppType, McpServer};
use crate::domain::{
    AppError, ErrorCode, ExtensionKind, ExtensionScope, McpConnectionDraft, McpConnectionForm,
    McpEditForm, McpInstallDraft, McpVariableForm,
};
use crate::services::McpService;
use crate::store::AppState;

use super::super::{detail, list_failed, non_empty, not_found};
use super::{connection_spec, mcp_write_guard, panic_detail, same_server, scan_live};

/// The keys the guided form owns. Everything else in a stored spec
/// (`timeout`, tool allow-lists, …) is kept as it is on save.
const FORM_KEYS: [&str; 7] = [
    "type",
    "command",
    "args",
    "env",
    "url",
    "headers",
    "http_headers",
];
/// Only meaningful for a local command, so dropped when the connection
/// becomes remote.
const LOCAL_ONLY_KEYS: [&str; 2] = ["cwd", "working_dir"];

fn edit_unavailable(technical: &'static str) -> AppError {
    AppError::new(ErrorCode::ConfigParseFailed, "error.mcp.editUnavailable")
        .with_technical(technical)
        .with_remediation("error.remediation.checkMcpSettings")
}

fn update_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.mcp.updateFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

/// The saved connection in the form's shape: the managed row when there is
/// one, else the entry found in this scope's own file.
pub(in crate::compat::ccswitch::extension) fn form(
    state: &AppState,
    scope: ExtensionScope,
    app_type: &AppType,
    id: &str,
) -> Result<McpEditForm, AppError> {
    let managed = McpService::get_all_servers(state)
        .map_err(list_failed)?
        .shift_remove(id);
    let server = match managed {
        Some(server) => server,
        None => scan_live(app_type)
            .map_err(list_failed)?
            .into_iter()
            .find(|server| server.id == id)
            .ok_or_else(|| not_found(scope, ExtensionKind::Mcp, id))?,
    };
    form_from_server(&server)
}

fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn variables(spec: &Value, keys: &[&str]) -> Vec<McpVariableForm> {
    keys.iter()
        .find_map(|key| spec.get(*key).and_then(Value::as_object))
        .map(|object| {
            object
                .iter()
                .map(|(name, value)| McpVariableForm {
                    name: name.clone(),
                    value: text(value),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn connection_form(spec: &Value) -> Option<McpConnectionForm> {
    let command = spec.get("command").and_then(Value::as_str);
    let url = spec.get("url").and_then(Value::as_str).map(str::to_string);
    let headers = || variables(spec, &["headers", "http_headers"]);
    match spec.get("type").and_then(Value::as_str) {
        Some("stdio") | None if command.is_some() => Some(McpConnectionForm::Stdio {
            command: command.unwrap_or_default().to_string(),
            arguments: spec
                .get("args")
                .and_then(Value::as_array)
                .map(|values| values.iter().map(text).collect())
                .unwrap_or_default(),
            env: variables(spec, &["env"]),
        }),
        Some("http") | None => Some(McpConnectionForm::Http {
            url: url?,
            headers: headers(),
        }),
        Some("sse") => Some(McpConnectionForm::Sse {
            url: url?,
            headers: headers(),
        }),
        Some(_) => None,
    }
}

fn form_from_server(server: &McpServer) -> Result<McpEditForm, AppError> {
    let connection = connection_form(&server.server).ok_or_else(|| {
        edit_unavailable("saved MCP connection is neither a local command nor an address")
    })?;
    Ok(McpEditForm {
        id: server.id.clone(),
        name: server.name.clone(),
        description: non_empty(server.description.clone()),
        connection,
    })
}

/// The stored spec with the form's fields replaced and everything else kept.
fn merged_spec(current: &Value, connection: &McpConnectionDraft) -> Value {
    let mut merged = current.as_object().cloned().unwrap_or_default();
    for key in FORM_KEYS {
        merged.remove(key);
    }
    if !matches!(connection, McpConnectionDraft::Stdio { .. }) {
        for key in LOCAL_ONLY_KEYS {
            merged.remove(key);
        }
    }
    if let Value::Object(fresh) = connection_spec(connection) {
        merged.extend(fresh);
    }
    Value::Object(merged)
}

/// Replace one managed connection's name, description and connection, keep
/// its id and every app flag, and write it to every app that has it on. A
/// failed write puts the previous row back and rewrites it the same way.
pub(in crate::compat::ccswitch::extension) fn update(
    state: &AppState,
    scope: ExtensionScope,
    id: &str,
    draft: &McpInstallDraft,
) -> Result<(), AppError> {
    draft.validate()?;
    let _guard = mcp_write_guard();
    let original = McpService::get_all_servers(state)
        .map_err(update_failed)?
        .shift_remove(id)
        .ok_or_else(|| not_found(scope, ExtensionKind::Mcp, id))?;

    let next = McpServer {
        name: draft.normalized_name(),
        description: draft.normalized_description(),
        server: merged_spec(&original.server, &draft.connection),
        ..original.clone()
    };

    let outcome = catch_unwind(AssertUnwindSafe(|| {
        McpService::upsert_server(state, next.clone())
    }));
    let failure = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(update_failed(error)),
        Err(payload) => Some(update_failed(format!(
            "MCP update panicked: {}",
            panic_detail(payload)
        ))),
    };
    if let Some(error) = failure {
        return Err(restore_or_escalate(state, &original, error));
    }

    let stored = McpService::get_all_servers(state)
        .map_err(update_failed)?
        .shift_remove(id);
    if !stored.is_some_and(|stored| same_server(&stored, &next)) {
        return Err(restore_or_escalate(
            state,
            &original,
            update_failed("MCP row did not match the requested connection after the update"),
        ));
    }
    Ok(())
}

fn restore_or_escalate(state: &AppState, original: &McpServer, action: AppError) -> AppError {
    let restore = catch_unwind(AssertUnwindSafe(|| {
        McpService::upsert_server(state, original.clone())
    }));
    let restore_detail = match restore {
        Ok(Ok(())) => return action,
        Ok(Err(error)) => detail(error),
        Err(payload) => panic_detail(payload),
    };
    let action_detail = action
        .technical_message
        .as_deref()
        .unwrap_or("MCP update failed");
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.mcp.updateRestoreFailed",
    )
    .with_technical(detail(format!(
        "update: {action_detail}; restore: {restore_detail}"
    )))
    .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
mod tests {
    use super::{connection_form, merged_spec};
    use crate::domain::{McpConnectionDraft, McpConnectionForm, McpVariableDraft};
    use serde_json::json;

    #[test]
    fn a_found_spec_without_a_type_reads_as_the_transport_it_describes() {
        let local = connection_form(&json!({"command": "npx", "args": ["-y", 3]}));
        assert!(matches!(
            local,
            Some(McpConnectionForm::Stdio { ref arguments, .. }) if arguments == &["-y", "3"]
        ));
        let remote = connection_form(&json!({"url": "https://x.test", "http_headers": {"A": "b"}}));
        assert!(matches!(
            remote,
            Some(McpConnectionForm::Http { ref headers, .. }) if headers.len() == 1
        ));
        assert!(connection_form(&json!({"type": "websocket", "url": "wss://x"})).is_none());
        assert!(connection_form(&json!({"type": "stdio"})).is_none());
    }

    #[test]
    fn saving_replaces_the_form_fields_and_keeps_the_rest() {
        let current = json!({
            "type": "stdio",
            "command": "node",
            "args": ["old.js"],
            "env": {"OLD": "1"},
            "cwd": "/srv/mcp",
            "timeout": 30
        });
        let local = merged_spec(
            &current,
            &McpConnectionDraft::Stdio {
                command: "npx".to_string(),
                arguments: vec!["new".to_string()],
                env: Vec::new(),
            },
        );
        assert_eq!(
            local,
            json!({"type": "stdio", "command": "npx", "args": ["new"], "cwd": "/srv/mcp", "timeout": 30})
        );

        let remote = merged_spec(
            &current,
            &McpConnectionDraft::Http {
                url: "https://x.test/mcp".to_string(),
                headers: vec![McpVariableDraft {
                    name: "Authorization".to_string(),
                    value: "Bearer t".to_string(),
                }],
            },
        );
        assert_eq!(
            remote,
            json!({
                "type": "http",
                "url": "https://x.test/mcp",
                "headers": {"Authorization": "Bearer t"},
                "timeout": 30
            })
        );
    }
}
