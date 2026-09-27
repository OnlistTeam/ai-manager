use serde_json::json;

use super::*;

#[test]
fn the_variable_outranks_the_key() {
    let settings = json!({"model": "opus[1m]", "env": {"ANTHROPIC_MODEL": "glm-5"}});
    assert_eq!(named(&settings), Some("glm-5"));

    let settings = json!({"model": "opus[1m]", "env": {"ANTHROPIC_MODEL": " "}});
    assert_eq!(named(&settings), Some("opus[1m]"));

    assert_eq!(named(&json!({"env": {}})), None);
}

#[test]
fn setting_writes_the_key_and_drops_the_variable() {
    let mut settings = json!({
        "env": {"ANTHROPIC_MODEL": "glm-5", "ANTHROPIC_BASE_URL": "https://relay.example.test"},
        "hooks": {}
    });
    set(&mut settings, Some("glm-5.1"));
    assert_eq!(
        settings,
        json!({
            "env": {"ANTHROPIC_BASE_URL": "https://relay.example.test"},
            "hooks": {},
            "model": "glm-5.1"
        })
    );

    set(&mut settings, None);
    assert!(settings.get("model").is_none());
    assert_eq!(named(&settings), None);
}

#[test]
fn an_empty_variable_stays_because_it_cancels_the_terminal() {
    let mut settings = json!({"env": {"ANTHROPIC_MODEL": ""}});
    set(&mut settings, Some("sonnet"));
    assert_eq!(settings["env"]["ANTHROPIC_MODEL"], "");
    assert_eq!(settings["model"], "sonnet");
}

#[test]
fn moving_keeps_the_model_that_was_in_force() {
    let mut settings = json!({"model": "claude-fable-5", "env": {"ANTHROPIC_MODEL": "opus[1m]"}});
    move_variable_to_key(&mut settings);
    assert_eq!(settings, json!({"model": "opus[1m]", "env": {}}));

    let mut only_key = json!({"model": "sonnet", "env": {}});
    move_variable_to_key(&mut only_key);
    assert_eq!(only_key, json!({"model": "sonnet", "env": {}}));
}
