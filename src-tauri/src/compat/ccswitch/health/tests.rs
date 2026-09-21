use super::snapshot_with_reader;
use crate::app_config::{AppType, McpApps, McpServer};
use crate::database::Database;
use crate::domain::{ConfigReadStatus, ToolId};
use crate::provider::{Provider, ProviderMeta};
use crate::store::AppState;
use serde_json::json;
use std::sync::Arc;

fn state() -> AppState {
    AppState::new(Arc::new(
        Database::memory().expect("create health test database"),
    ))
}

fn provider(id: &str, name: &str, settings: serde_json::Value) -> Provider {
    Provider::with_id(id.to_string(), name.to_string(), settings, None)
}

fn mcp(id: &str, apps: McpApps) -> McpServer {
    McpServer {
        id: id.to_string(),
        name: id.to_string(),
        server: json!({ "command": "unused" }),
        apps,
        description: None,
        homepage: None,
        docs: None,
        tags: Vec::new(),
    }
}

#[test]
fn switch_mode_counts_only_the_valid_selected_provider() {
    let state = state();
    let selected = provider(
        "selected",
        "Selected relay",
        json!({ "env": { "ANTHROPIC_BASE_URL": "https://selected.example" } }),
    );
    let ignored = provider(
        "ignored",
        "Ignored relay",
        json!({ "env": { "ANTHROPIC_BASE_URL": "https://ignored.example" } }),
    );
    state
        .db
        .save_provider("claude", &selected)
        .expect("save selected provider");
    state
        .db
        .save_provider("claude", &ignored)
        .expect("save ignored provider");
    state
        .db
        .set_current_provider("claude", "selected")
        .expect("select provider");

    let snapshot = snapshot_with_reader(
        &state,
        &[ToolId::ClaudeCode],
        |_| Some(true),
        |_| Ok(json!({})),
    )
    .expect("health snapshot");
    let health = &snapshot.providers[0];
    assert!(health.configured);
    assert_eq!(health.configured_count, 1);
    assert_eq!(health.check_targets.len(), 1);
    assert_eq!(health.check_targets[0].provider_id, "selected");
}

#[test]
fn additive_mode_counts_live_managed_and_legacy_rows_but_not_db_only_rows() {
    let state = state();
    for (id, marker) in [
        ("managed", Some(true)),
        ("legacy", None),
        ("db-only", Some(false)),
    ] {
        let mut entry = provider(
            id,
            id,
            json!({ "options": { "baseURL": format!("https://{id}.example") } }),
        );
        entry.meta = Some(ProviderMeta {
            live_config_managed: marker,
            ..ProviderMeta::default()
        });
        state
            .db
            .save_provider("opencode", &entry)
            .expect("save OpenCode provider");
    }

    let snapshot = snapshot_with_reader(
        &state,
        &[ToolId::OpenCode],
        |_| Some(true),
        |_| Ok(json!({})),
    )
    .expect("health snapshot");
    let health = &snapshot.providers[0];
    assert_eq!(health.configured_count, 2);
    assert_eq!(health.check_targets.len(), 2);
    assert!(health
        .check_targets
        .iter()
        .all(|target| target.provider_id != "db-only"));
}

#[test]
fn config_failures_are_status_only_and_mcp_enabled_is_a_distinct_global_count() {
    let state = state();
    state
        .db
        .save_mcp_server(&mcp(
            "product",
            McpApps {
                claude: true,
                codex: true,
                ..McpApps::default()
            },
        ))
        .expect("save product MCP");
    state
        .db
        .save_mcp_server(&mcp(
            "outside-product",
            McpApps {
                hermes: true,
                ..McpApps::default()
            },
        ))
        .expect("save non-product MCP");
    state
        .db
        .save_mcp_server(&mcp("disabled", McpApps::default()))
        .expect("save disabled MCP");

    let snapshot = snapshot_with_reader(
        &state,
        &[ToolId::ClaudeCode, ToolId::Codex],
        |_| Some(true),
        |app_type| {
            if app_type == AppType::Codex {
                Err(crate::error::AppError::Message(
                    "bad config with secret sk-example".to_string(),
                ))
            } else {
                Ok(json!({}))
            }
        },
    )
    .expect("health snapshot");

    assert_eq!(snapshot.configs[0].status, ConfigReadStatus::Readable);
    assert_eq!(snapshot.configs[1].status, ConfigReadStatus::Unreadable);
    assert_eq!(snapshot.mcp.total, 3);
    assert_eq!(snapshot.mcp.enabled, 1);
    let wire = serde_json::to_string(&snapshot).expect("serialize health snapshot");
    assert!(!wire.contains("bad config"));
    assert!(!wire.contains("sk-example"));
}

#[test]
fn a_missing_live_config_is_not_configured_rather_than_unreadable() {
    let state = state();
    let mut reads = Vec::new();
    let snapshot = snapshot_with_reader(
        &state,
        &[ToolId::ClaudeCode, ToolId::GeminiCli],
        |app_type| Some(app_type != AppType::Gemini),
        |app_type| {
            reads.push(app_type.clone());
            Ok(json!({}))
        },
    )
    .expect("health snapshot");

    assert_eq!(snapshot.configs[0].status, ConfigReadStatus::Readable);
    assert_eq!(snapshot.configs[1].status, ConfigReadStatus::Missing);
    // A file that does not exist is never opened, so no "missing file" error
    // can be mistaken for a parse failure.
    assert_eq!(reads, vec![AppType::Claude]);
    let wire = serde_json::to_string(&snapshot).expect("serialize health snapshot");
    assert!(wire.contains(r#""status":"missing""#));
}

#[test]
fn an_unknown_live_layout_still_defers_to_the_reader() {
    let state = state();
    let snapshot = snapshot_with_reader(
        &state,
        &[ToolId::ClaudeCode],
        |_| None,
        |_| Err(crate::error::AppError::Message("bad".to_string())),
    )
    .expect("health snapshot");
    assert_eq!(snapshot.configs[0].status, ConfigReadStatus::Unreadable);
}
