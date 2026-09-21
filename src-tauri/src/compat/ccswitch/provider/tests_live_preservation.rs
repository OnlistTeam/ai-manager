use std::sync::Arc;

use serde_json::{json, Value};

use super::tests_create::TestHome;
use super::ProviderStore;
use crate::app_config::AppType;
use crate::database::Database;
use crate::domain::ToolId;
use crate::provider::Provider as UpstreamProvider;
use crate::store::AppState;

const SECRET: &str = "sk-preserve-secret-0123456789ABCD";

fn state() -> AppState {
    AppState::new(Arc::new(Database::memory().expect("db")))
}

fn claude(id: &str, category: &str, settings: Value) -> UpstreamProvider {
    let mut provider = UpstreamProvider::with_id(id.to_string(), id.to_string(), settings, None);
    provider.category = Some(category.to_string());
    provider
}

#[test]
#[serial_test::serial]
fn switching_claude_keeps_hooks_and_cancels_relay_variables() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let settings_path = crate::config::get_claude_settings_path();
    std::fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.test",
                "ANTHROPIC_AUTH_TOKEN": SECRET,
                "DISABLE_AUTO_COMPACT": "1"
            },
            "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "afplay x"}]}]},
            "statusLine": {"type": "command", "command": "npx ccstatusline"}
        }))
        .unwrap(),
    )
    .unwrap();

    let state = state();
    let relay = claude(
        "relay-1a2b",
        "custom",
        json!({"env": {"ANTHROPIC_BASE_URL": "https://relay.example.test", "ANTHROPIC_AUTH_TOKEN": SECRET}}),
    );
    let official = claude("claude-official", "official", json!({"env": {}}));
    for provider in [&relay, &official] {
        state
            .db
            .save_provider(AppType::Claude.as_str(), provider)
            .expect("save");
    }
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), &relay.id)
        .expect("current");
    let store = ProviderStore {
        state: state.clone(),
    };

    store
        .switch(ToolId::ClaudeCode, &official.id)
        .expect("switch to official");

    let live: Value = crate::config::read_json_file(&settings_path).expect("read live");
    let env = live.get("env").expect("env block");
    assert_eq!(env.get("ANTHROPIC_BASE_URL"), Some(&json!("")));
    assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN"), Some(&json!("")));
    assert_eq!(env.get("ANTHROPIC_API_KEY"), Some(&json!("")));
    assert_eq!(env.get("DISABLE_AUTO_COMPACT"), Some(&json!("1")));
    assert!(live.get("hooks").is_some(), "hooks survive the switch");
    assert!(
        live.get("statusLine").is_some(),
        "statusLine survives the switch"
    );
    assert!(!serde_json::to_string(&live).unwrap().contains(SECRET));
}

#[test]
#[serial_test::serial]
fn switching_gemini_keeps_project_variables() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let env_path = crate::gemini_config::get_gemini_env_path();
    std::fs::create_dir_all(env_path.parent().unwrap()).unwrap();
    std::fs::write(
        &env_path,
        "GEMINI_API_KEY=k-old\nGOOGLE_GEMINI_BASE_URL=https://old.example.test\nGOOGLE_CLOUD_PROJECT=my-project\n",
    )
    .unwrap();

    let state = state();
    let old = claude(
        "gemini-old",
        "custom",
        json!({"env": {"GEMINI_API_KEY": "k-old", "GOOGLE_GEMINI_BASE_URL": "https://old.example.test"}, "config": null}),
    );
    let new = claude(
        "gemini-new",
        "custom",
        json!({"env": {"GEMINI_API_KEY": "k-new", "GOOGLE_GEMINI_BASE_URL": "https://new.example.test"}, "config": null}),
    );
    for provider in [&old, &new] {
        state
            .db
            .save_provider(AppType::Gemini.as_str(), provider)
            .expect("save");
    }
    state
        .db
        .set_current_provider(AppType::Gemini.as_str(), &old.id)
        .expect("current");
    let store = ProviderStore {
        state: state.clone(),
    };

    store
        .switch(ToolId::GeminiCli, &new.id)
        .expect("switch gemini");

    let env = crate::gemini_config::read_gemini_env().expect("read .env");
    assert_eq!(env.get("GEMINI_API_KEY").map(String::as_str), Some("k-new"));
    assert_eq!(
        env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str),
        Some("https://new.example.test")
    );
    assert_eq!(
        env.get("GOOGLE_CLOUD_PROJECT").map(String::as_str),
        Some("my-project")
    );
}

/// Covers Important 1: the `config.toml` in previous holds a `[mcp_servers.stale]` table
/// (simulating a stale projection left over from before the switch — for instance that MCP server
/// was later deleted from the DB but nobody cleaned live up at deletion time). The DB has no Codex
/// MCP servers at all, so `McpService::sync_enabled_for_app` (which runs at the end of
/// `ProviderService::switch`) projects no mcp_servers table into written — the config.toml it
/// writes has no mcp_servers key whatsoever.
/// Before mcp_servers was counted among CODEX_MANAGED_KEYS, `restore()` would paste the `stale`
/// table from previous back as an "unrecognized user-defined key", effectively resurrecting an MCP
/// server that had already been deleted from the app.
#[test]
#[serial_test::serial]
fn switching_codex_does_not_resurrect_a_stale_mcp_servers_table() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());

    let previous_config = r#"model_reasoning_effort = "high"
sandbox_mode = "workspace-write"

[mcp_servers.stale]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem"]
"#;
    crate::codex_config::write_codex_live_atomic(
        &json!({"OPENAI_API_KEY": "sk-old-preserve"}),
        Some(previous_config),
    )
    .expect("seed existing codex live config");

    let state = state();
    let old = claude(
        "codex-old",
        "custom",
        json!({"auth": {"OPENAI_API_KEY": "sk-old-preserve"}, "config": "# old provider\n"}),
    );
    let new = claude(
        "codex-new",
        "custom",
        json!({"auth": {"OPENAI_API_KEY": "sk-new-preserve"}, "config": "# new provider\n"}),
    );
    for provider in [&old, &new] {
        state
            .db
            .save_provider(AppType::Codex.as_str(), provider)
            .expect("save");
    }
    state
        .db
        .set_current_provider(AppType::Codex.as_str(), &old.id)
        .expect("current");
    let store = ProviderStore {
        state: state.clone(),
    };

    store.switch(ToolId::Codex, &new.id).expect("switch codex");

    let config_text = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read config.toml");
    assert!(
        !config_text.contains("stale"),
        "a DB-untracked mcp_servers table must not survive a switch: {config_text}"
    );
    assert!(
        config_text.contains("sandbox_mode"),
        "the user's own settings still survive the switch: {config_text}"
    );
    assert!(!config_text.contains("sk-old-preserve"));
}
