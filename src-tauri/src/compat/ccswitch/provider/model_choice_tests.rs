use std::sync::Arc;

use serde_json::{json, Value};

use super::super::tests_create::TestHome;
use super::super::ProviderStore;
use super::*;
use crate::app_config::AppType;
use crate::database::Database;
use crate::store::AppState;

fn none() -> ToolTerminal {
    ToolTerminal::default()
}

fn level(level: &str) -> EffortInForce {
    EffortInForce::Level {
        level: level.to_string(),
    }
}

fn store() -> ProviderStore {
    ProviderStore {
        state: AppState::new(Arc::new(Database::memory().expect("db"))),
    }
}

fn record(id: &str, category: &str, settings: Value) -> UpstreamProvider {
    let mut provider = UpstreamProvider::with_id(id.to_string(), id.to_string(), settings, None);
    provider.category = Some(category.to_string());
    provider
}

fn seed(store: &ProviderStore, app: AppType, providers: &[&UpstreamProvider], current: &str) {
    for provider in providers {
        store
            .state
            .db
            .save_provider(app.as_str(), provider)
            .expect("save");
    }
    store
        .state
        .db
        .set_current_provider(app.as_str(), current)
        .expect("current");
}

fn claude_live() -> Value {
    crate::config::read_json_file(&crate::config::get_claude_settings_path()).expect("read live")
}

fn write_claude_live(value: &Value) {
    let path = crate::config::get_claude_settings_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

#[test]
#[serial_test::serial]
fn reads_the_effort_a_new_session_of_the_model_in_use_runs_at() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    write_claude_live(&json!({
        "env": {"CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1", "ANTHROPIC_MODEL": "opus"},
        "effortLevel": "xhigh",
        "modelSettings": {
            "claude-opus-5-5": {"effortLevel": "medium"},
            "claude-sonnet-4-6": {"maxEffortLevel": "high"}
        }
    }));

    let choice = read(ToolId::ClaudeCode, &none()).expect("read");
    assert_eq!(choice.model.as_deref(), Some("opus"));
    assert_eq!(choice.effort, level("medium"));
    assert_eq!(choice.effort_levels, ["low", "medium", "high", "xhigh"]);
}

#[test]
#[serial_test::serial]
fn a_model_follows_its_endpoint_across_switches() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    write_claude_live(&json!({"env": {}, "hooks": {"Stop": []}}));
    let store = store();
    let official = record("claude-official", "official", json!({"env": {}}));
    let relay = record(
        "relay",
        "custom",
        json!({"env": {"ANTHROPIC_BASE_URL": "https://relay.example.test", "ANTHROPIC_MODEL": "glm-5"}}),
    );
    seed(&store, AppType::Claude, &[&official, &relay], &official.id);

    store
        .set_model(
            ToolId::ClaudeCode,
            Some(&official.id),
            Some("opus"),
            &none(),
        )
        .expect("set model");
    let live = claude_live();
    assert_eq!(live["model"], "opus");
    assert!(live["env"].get("ANTHROPIC_MODEL").is_none());
    assert!(live.get("hooks").is_some(), "other keys stay");

    // The relay's copy still names its model in the variable, as presets do; the
    // file gets it in the key `/model` writes.
    store
        .switch(ToolId::ClaudeCode, &relay.id)
        .expect("to relay");
    let live = claude_live();
    assert_eq!(live["model"], "glm-5");
    assert!(live["env"].get("ANTHROPIC_MODEL").is_none());

    store
        .switch(ToolId::ClaudeCode, &official.id)
        .expect("back to official");
    assert_eq!(claude_live()["model"], "opus");

    store
        .set_model(ToolId::ClaudeCode, Some(&official.id), None, &none())
        .expect("tool default");
    assert!(claude_live().get("model").is_none());
    assert_eq!(read(ToolId::ClaudeCode, &none()).unwrap().model, None);
}

#[test]
#[serial_test::serial]
fn a_model_for_an_endpoint_not_in_use_is_saved_without_touching_the_file() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    write_claude_live(&json!({"env": {}}));
    let store = store();
    let official = record("claude-official", "official", json!({"env": {}}));
    let relay = record("relay", "custom", json!({"env": {}}));
    seed(&store, AppType::Claude, &[&official, &relay], &official.id);

    store
        .set_model(ToolId::ClaudeCode, Some(&relay.id), Some("glm-5"), &none())
        .expect("set model");

    assert!(claude_live().get("model").is_none());
    let saved = store.find_raw(ToolId::ClaudeCode, &relay.id).unwrap();
    assert_eq!(saved.settings_config["model"], "glm-5");
}

#[test]
#[serial_test::serial]
fn a_model_picked_with_slash_model_is_read_and_can_be_replaced() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let store = store();
    let relay = record(
        "relay",
        "custom",
        json!({"env": {"ANTHROPIC_BASE_URL": "https://relay.example.test"}, "model": "glm-5"}),
    );
    seed(&store, AppType::Claude, &[&relay], &relay.id);
    // What Claude Code's `/model` saves: the top-level key.
    write_claude_live(&json!({
        "env": {"ANTHROPIC_BASE_URL": "https://relay.example.test"},
        "model": "opus[1m]"
    }));
    assert_eq!(
        read(ToolId::ClaudeCode, &none()).unwrap().model.as_deref(),
        Some("opus[1m]")
    );

    store
        .set_model(
            ToolId::ClaudeCode,
            Some(&relay.id),
            Some("glm-5.1"),
            &none(),
        )
        .expect("set model");
    assert_eq!(claude_live()["model"], "glm-5.1");
}

#[test]
#[serial_test::serial]
fn a_pick_clears_a_variable_that_would_outrank_it() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let store = store();
    let relay = record(
        "relay",
        "custom",
        json!({"env": {"ANTHROPIC_BASE_URL": "https://relay.example.test", "ANTHROPIC_MODEL": "glm-5"}}),
    );
    seed(&store, AppType::Claude, &[&relay], &relay.id);
    // Written before this rule, or by another tool: the variable is in force.
    write_claude_live(&json!({
        "env": {"ANTHROPIC_BASE_URL": "https://relay.example.test", "ANTHROPIC_MODEL": "glm-5"},
        "model": "opus[1m]"
    }));
    assert_eq!(
        read(ToolId::ClaudeCode, &none()).unwrap().model.as_deref(),
        Some("glm-5")
    );

    store
        .set_model(
            ToolId::ClaudeCode,
            Some(&relay.id),
            Some("glm-5.1"),
            &none(),
        )
        .expect("set model");
    let live = claude_live();
    assert_eq!(live["model"], "glm-5.1");
    assert!(live["env"].get("ANTHROPIC_MODEL").is_none());
    let saved = store.find_raw(ToolId::ClaudeCode, &relay.id).unwrap();
    assert_eq!(saved.settings_config["model"], "glm-5.1");
    assert!(saved.settings_config["env"]
        .get("ANTHROPIC_MODEL")
        .is_none());
}

#[test]
#[serial_test::serial]
fn the_effort_survives_a_switch_to_an_endpoint_with_an_older_copy() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    write_claude_live(&json!({"env": {}, "effortLevel": "low"}));
    let store = store();
    let official = record(
        "claude-official",
        "official",
        json!({"env": {}, "effortLevel": "low"}),
    );
    let relay = record(
        "relay",
        "custom",
        json!({"env": {"ANTHROPIC_BASE_URL": "https://relay.example.test"}, "effortLevel": "medium"}),
    );
    seed(&store, AppType::Claude, &[&official, &relay], &official.id);

    let choice = store
        .set_effort(ToolId::ClaudeCode, Some("xhigh"), &none())
        .expect("set effort");
    assert_eq!(choice.effort, level("xhigh"));
    let saved = store.find_raw(ToolId::ClaudeCode, &official.id).unwrap();
    assert_eq!(saved.settings_config["effortLevel"], "xhigh");
    assert_eq!(
        saved.settings_config["modelSettings"]["claude-opus-5-5"]["effortLevel"],
        "xhigh"
    );

    store
        .switch(ToolId::ClaudeCode, &relay.id)
        .expect("to relay");
    let live = claude_live();
    assert_eq!(live["effortLevel"], "xhigh");
    assert_eq!(
        live["modelSettings"]["claude-opus-5-5"]["effortLevel"],
        "xhigh"
    );
    assert_eq!(
        live["env"]["ANTHROPIC_BASE_URL"],
        "https://relay.example.test"
    );

    // Another tool leaves `max` in the variable, which Home never writes.
    let mut live = claude_live();
    live["env"]["CLAUDE_CODE_EFFORT_LEVEL"] = json!("max");
    write_claude_live(&live);
    store
        .switch(ToolId::ClaudeCode, &official.id)
        .expect("back to official");
    assert_eq!(claude_live()["env"]["CLAUDE_CODE_EFFORT_LEVEL"], "max");
    assert_eq!(
        read(ToolId::ClaudeCode, &none()).unwrap().effort,
        EffortInForce::Fixed {
            level: "max".to_string()
        }
    );

    store
        .set_effort(ToolId::ClaudeCode, None, &none())
        .expect("tool default");
    store
        .switch(ToolId::ClaudeCode, &relay.id)
        .expect("to relay again");
    let live = claude_live();
    assert!(live.get("effortLevel").is_none());
    assert!(live.get("modelSettings").is_none());
    assert_eq!(live["env"]["CLAUDE_CODE_EFFORT_LEVEL"], "");
    // No model is named, so Opus 5.5 runs, at its own default.
    assert_eq!(
        read(ToolId::ClaudeCode, &none()).unwrap().effort,
        level("medium")
    );
}

#[test]
#[serial_test::serial]
fn the_real_example_file_reads_as_the_level_of_the_default_model() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    write_claude_live(&json!({
        "effortLevel": "xhigh",
        "modelSettings": {
            "claude-fable-5-1": {"effortLevel": "xhigh"},
            "claude-opus-5": {"effortLevel": "high"},
            "claude-opus-5-5": {"effortLevel": "xhigh"}
        }
    }));
    let choice = read(ToolId::ClaudeCode, &none()).expect("read");
    assert_eq!(choice.effort, level("xhigh"));

    let store = store();
    let choice = store
        .set_effort(ToolId::ClaudeCode, Some("xhigh"), &none())
        .expect("set effort");
    assert_eq!(choice.effort, level("xhigh"));
    assert_eq!(
        claude_live()["modelSettings"]["claude-opus-5"]["effortLevel"],
        "xhigh"
    );
}

#[test]
#[serial_test::serial]
fn codex_effort_keeps_the_rest_of_config_toml() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let path = crate::codex_config::get_codex_config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "# mine\nmodel = \"gpt-5.6-sol\" # daily\nmodel_reasoning_effort = \"low\"\n\n[projects.\"/x\"]\ntrust_level = \"trusted\"\n",
    )
    .unwrap();
    let store = store();

    let choice = store
        .set_effort(ToolId::Codex, Some("high"), &none())
        .expect("set effort");

    assert_eq!(choice.model.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(choice.effort, level("high"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# mine\nmodel = \"gpt-5.6-sol\" # daily\nmodel_reasoning_effort = \"high\"\n\n[projects.\"/x\"]\ntrust_level = \"trusted\"\n"
    );
}

#[test]
#[serial_test::serial]
fn an_unset_codex_effort_reads_as_the_models_own_default() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let path = crate::codex_config::get_codex_config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    std::fs::write(&path, "model = \"gpt-5.6-sol\"\n").unwrap();
    assert_eq!(read(ToolId::Codex, &none()).unwrap().effort, level("low"));

    std::fs::write(&path, "model = \"glm-5\"\n").unwrap();
    assert_eq!(
        read(ToolId::Codex, &none()).unwrap().effort,
        EffortInForce::ToolDefault
    );

    std::fs::write(&path, "").unwrap();
    assert_eq!(
        read(ToolId::Codex, &none()).unwrap().effort,
        EffortInForce::ToolDefault
    );
}

#[test]
#[serial_test::serial]
fn only_levels_the_tool_accepts_are_written() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let store = store();
    let error = store
        .set_effort(ToolId::ClaudeCode, Some("ultracode"), &none())
        .expect_err("not an effort level");
    assert_eq!(error.message_key, "error.modelChoice.invalid");
    let error = store
        .set_effort(ToolId::Codex, Some("max"), &none())
        .expect_err("codex keeps no variable-only level");
    assert_eq!(error.message_key, "error.modelChoice.invalid");
    let error = store
        .set_effort(ToolId::GeminiCli, Some("low"), &none())
        .expect_err("no effort setting");
    assert_eq!(error.message_key, "error.provider.unsupportedTool");
    assert!(!crate::config::get_claude_settings_path().exists());
}
