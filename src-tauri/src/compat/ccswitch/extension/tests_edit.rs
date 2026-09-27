//! Editing an MCP connection in place (ADR-0062), against the real upstream
//! database and live files in a temporary home.

use std::sync::Arc;

use crate::app_config::AppType;
use crate::domain::{
    ErrorCode, ExtensionScope, McpConnectionDraft, McpInstallDraft, McpVariableDraft, ToolId,
};

struct TestHome(Option<std::ffi::OsString>);

impl TestHome {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("AI_MANAGER_TEST_HOME");
        std::env::set_var("AI_MANAGER_TEST_HOME", path);
        Self(previous)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
            None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
        }
    }
}

fn claude() -> ExtensionScope {
    ExtensionScope::tool(ToolId::ClaudeCode)
}

fn state() -> crate::store::AppState {
    crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ))
}

fn seed_tools(home: &std::path::Path) {
    std::fs::create_dir_all(home.join(".claude")).expect("initialized Claude dir");
    std::fs::create_dir_all(home.join(".codex")).expect("initialized Codex dir");
    std::fs::write(home.join(".claude.json"), br#"{"theme":"dark"}"#).expect("seed Claude");
    std::fs::write(home.join(".codex/config.toml"), "model = \"gpt-5\"\n").expect("seed Codex");
}

fn draft(name: &str, arguments: &[&str], env: &[(&str, &str)]) -> McpInstallDraft {
    McpInstallDraft {
        name: name.to_string(),
        description: None,
        connection: McpConnectionDraft::Stdio {
            command: "npx".to_string(),
            arguments: arguments.iter().map(|value| value.to_string()).collect(),
            env: env
                .iter()
                .map(|(name, value)| McpVariableDraft {
                    name: name.to_string(),
                    value: value.to_string(),
                })
                .collect(),
        },
    }
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).expect("read JSON")).expect("valid JSON")
}

#[test]
#[serial_test::serial]
fn an_update_keeps_the_id_every_app_flag_and_unshown_fields_and_rewrites_each_enabled_app() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    seed_tools(temp.path());
    let state = state();

    super::mcp::install(
        &state,
        claude(),
        &AppType::Claude,
        "files-a1b2c3d4",
        &draft("Files", &["-y", "server-files"], &[]),
    )
    .expect("install");
    super::mcp::set_enabled(&state, &AppType::Codex, "files-a1b2c3d4", true)
        .expect("switch on for Codex");
    // A field the form does not show, e.g. from a hand-written config.
    let mut row = state
        .db
        .get_all_mcp_servers()
        .expect("rows")
        .shift_remove("files-a1b2c3d4")
        .expect("row");
    row.server["timeout"] = serde_json::json!(30);
    state.db.save_mcp_server(&row).expect("save extra field");

    super::mcp::update(
        &state,
        claude(),
        "files-a1b2c3d4",
        &draft(
            "Files v2",
            &["-y", "server-files@2"],
            &[("FILES_TOKEN", "t-1")],
        ),
    )
    .expect("update");

    let stored = state
        .db
        .get_all_mcp_servers()
        .expect("rows")
        .shift_remove("files-a1b2c3d4")
        .expect("same id");
    assert_eq!(stored.name, "Files v2");
    assert!(stored.apps.claude && stored.apps.codex, "flags are kept");
    assert!(!stored.apps.gemini, "no app is switched on by an edit");
    assert_eq!(stored.server["timeout"], 30);
    assert_eq!(
        stored.server["args"],
        serde_json::json!(["-y", "server-files@2"])
    );
    assert_eq!(
        stored.server["env"],
        serde_json::json!({"FILES_TOKEN": "t-1"})
    );

    let live = read_json(&temp.path().join(".claude.json"));
    assert_eq!(live["theme"], "dark");
    let written = &live["mcpServers"]["files-a1b2c3d4"];
    assert_eq!(written["env"], serde_json::json!({"FILES_TOKEN": "t-1"}));
    if !cfg!(target_os = "windows") {
        assert_eq!(written["args"], serde_json::json!(["-y", "server-files@2"]));
    }
    let codex =
        std::fs::read_to_string(temp.path().join(".codex/config.toml")).expect("read Codex config");
    assert!(codex.contains("server-files@2"), "{codex}");
    assert!(codex.contains("FILES_TOKEN"), "{codex}");
}

#[test]
#[serial_test::serial]
fn the_edit_form_reads_a_found_connection_with_its_values_and_saving_needs_a_takeover() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    std::fs::create_dir_all(temp.path().join(".claude")).expect("create Claude directory");
    std::fs::write(
        temp.path().join(".claude.json"),
        r#"{"mcpServers":{"browser":{"command":"npx","args":["browser-package"],"env":{"API_TOKEN":"tok-1"}}}}"#,
    )
    .expect("write Claude MCP config");
    let state = state();

    let form = super::mcp::form(&state, claude(), &AppType::Claude, "browser").expect("form");
    assert_eq!(
        serde_json::to_value(&form).expect("serialize form"),
        serde_json::json!({
            "id": "browser",
            "name": "browser",
            "description": null,
            "connection": {
                "transport": "stdio",
                "command": "npx",
                "arguments": ["browser-package"],
                "env": [{"name": "API_TOKEN", "value": "tok-1"}]
            }
        })
    );

    let before = std::fs::read(temp.path().join(".claude.json")).expect("bytes before");
    let error = super::mcp::update(
        &state,
        claude(),
        "browser",
        &draft("Browser", &["browser-package"], &[]),
    )
    .expect_err("a found connection is taken over before it is saved");
    assert_eq!(error.code, ErrorCode::ExtensionNotFound);
    assert_eq!(
        std::fs::read(temp.path().join(".claude.json")).expect("bytes after"),
        before,
        "a refused update writes nothing"
    );

    let missing = super::mcp::form(&state, claude(), &AppType::Claude, "absent")
        .err()
        .expect("unknown id");
    assert_eq!(missing.code, ErrorCode::ExtensionNotFound);
}

#[test]
#[serial_test::serial]
fn an_invalid_draft_is_refused_before_anything_is_written() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    seed_tools(temp.path());
    let state = state();
    super::mcp::install(
        &state,
        claude(),
        &AppType::Claude,
        "files-a1b2c3d4",
        &draft("Files", &["server-files"], &[]),
    )
    .expect("install");

    let error = super::mcp::update(
        &state,
        claude(),
        "files-a1b2c3d4",
        &draft("Files", &["server-files"], &[("BAD-NAME", "x")]),
    )
    .expect_err("invalid env name");
    assert_eq!(error.message_key, "error.mcp.envInvalid");
    let stored = state.db.get_all_mcp_servers().expect("rows");
    assert_eq!(stored["files-a1b2c3d4"].name, "Files");
}
