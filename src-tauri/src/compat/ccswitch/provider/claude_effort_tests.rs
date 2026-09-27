use serde_json::{json, Value};

use super::*;
use crate::domain::{EffectiveConnectionSource, ToolId};

fn spec() -> ModelChoiceSpec {
    ModelChoiceSpec::for_tool(ToolId::ClaudeCode).expect("claude code")
}

fn resolved(settings: &Value) -> EffortInForce {
    resolve(settings, &ToolTerminal::default(), &spec())
}

fn level(level: &str) -> EffortInForce {
    EffortInForce::Level {
        level: level.to_string(),
    }
}

fn applied(mut settings: Value, effort: Option<&str>) -> Value {
    apply(&mut settings, &spec(), effort).expect("apply");
    settings
}

fn model_effort(model: &str, effort: &str, model_default: bool) -> ModelEffort {
    ModelEffort {
        model: model.to_string(),
        effort: effort.to_string(),
        model_default,
    }
}

/// A settings file as `/effort` leaves it after a few models were used, with
/// the older top-level key still there and no model chosen.
fn real_example() -> Value {
    json!({
        "effortLevel": "xhigh",
        "modelSettings": {
            "claude-fable-5-1": {"effortLevel": "xhigh"},
            "claude-opus-5": {"effortLevel": "high"},
            "claude-opus-5-5": {"effortLevel": "xhigh"}
        }
    })
}

#[test]
fn the_real_example_shows_that_the_models_differ() {
    assert_eq!(
        resolved(&real_example()),
        EffortInForce::Mixed {
            per_model: vec![
                model_effort("claude-fable-5-1", "xhigh", false),
                model_effort("claude-opus-5-5", "xhigh", false),
                model_effort("claude-opus-5", "high", false),
                model_effort("claude-sonnet-5", "xhigh", false),
            ]
        }
    );
}

#[test]
fn the_real_example_reads_the_level_of_the_model_in_use() {
    let mut settings = real_example();
    settings["model"] = json!("opus[1m]");
    assert_eq!(resolved(&settings), level("xhigh"));
    settings["env"] = json!({"ANTHROPIC_MODEL": "claude-opus-5"});
    assert_eq!(resolved(&settings), level("high"));
    settings["env"] = json!({"ANTHROPIC_MODEL": "sonnet"});
    assert_eq!(resolved(&settings), level("xhigh"), "tool-wide level");
}

#[test]
fn choosing_a_level_sets_it_for_every_model_as_effort_does() {
    let settings = applied(real_example(), Some("medium"));
    assert_eq!(
        settings,
        json!({
            "effortLevel": "medium",
            "modelSettings": {
                "claude-fable-5-1": {"effortLevel": "medium"},
                "claude-opus-5": {"effortLevel": "medium"},
                "claude-opus-5-5": {"effortLevel": "medium"},
                "claude-sonnet-5": {"effortLevel": "medium"}
            }
        })
    );
    assert_eq!(resolved(&settings), level("medium"));
}

#[test]
fn choosing_a_level_keeps_other_fields_and_models_outside_the_table() {
    let settings = applied(
        json!({
            "env": {"CLAUDE_CODE_EFFORT_LEVEL": "max", "OTHER": "1"},
            "modelSettings": {
                "claude-opus-4-6": {"maxEffortLevel": "high"},
                "claude-opus-5-5": {"effortLevel": "low", "maxEffortLevel": "xhigh"}
            }
        }),
        Some("high"),
    );
    assert_eq!(
        settings["env"],
        json!({"CLAUDE_CODE_EFFORT_LEVEL": "", "OTHER": "1"})
    );
    assert_eq!(
        settings["modelSettings"]["claude-opus-4-6"],
        json!({"maxEffortLevel": "high", "effortLevel": "high"})
    );
    assert_eq!(
        settings["modelSettings"]["claude-opus-5-5"],
        json!({"effortLevel": "high", "maxEffortLevel": "xhigh"})
    );
    assert_eq!(resolved(&settings), level("high"));
}

#[test]
fn a_later_effort_command_changes_only_its_own_model() {
    let mut settings = applied(real_example(), Some("high"));
    settings["modelSettings"]["claude-opus-5-5"]["effortLevel"] = json!("low");
    settings["model"] = json!("opus");
    assert_eq!(resolved(&settings), level("low"));
    settings["model"] = json!("fable");
    assert_eq!(resolved(&settings), level("high"));
}

#[test]
fn a_level_the_variable_holds_is_cleared_rather_than_removed() {
    let mut settings = real_example();
    settings["env"] = json!({"CLAUDE_CODE_EFFORT_LEVEL": "max"});
    assert_eq!(
        resolved(&settings),
        EffortInForce::Fixed {
            level: "max".to_string()
        }
    );

    // A running session keeps a removed variable, but takes up an empty one.
    let settings = applied(settings, Some("xhigh"));
    assert_eq!(settings["env"]["CLAUDE_CODE_EFFORT_LEVEL"], "");
    assert_eq!(resolved(&settings), level("xhigh"));

    // Nothing is added to a file that never set the variable.
    let settings = applied(real_example(), Some("xhigh"));
    assert!(settings.get("env").is_none());
}

#[test]
fn the_tool_default_removes_every_level_and_keeps_caps() {
    let settings = applied(
        json!({
            "env": {"CLAUDE_CODE_EFFORT_LEVEL": "max"},
            "effortLevel": "xhigh",
            "modelSettings": {
                "claude-fable-5-1": {"effortLevel": "xhigh"},
                "claude-opus-5-5": {"effortLevel": "xhigh", "maxEffortLevel": "high"}
            },
            "hooks": {}
        }),
        None,
    );
    assert_eq!(
        settings,
        json!({
            "env": {"CLAUDE_CODE_EFFORT_LEVEL": ""},
            "modelSettings": {"claude-opus-5-5": {"maxEffortLevel": "high"}},
            "hooks": {}
        })
    );
    // Each model is back at its own default, and they differ.
    assert!(matches!(resolved(&settings), EffortInForce::Mixed { .. }));

    let settings = applied(real_example(), None);
    assert_eq!(settings, json!({}));
}

#[test]
fn the_tool_wide_level_alone_does_not_reach_opus_5_5() {
    let settings = json!({"effortLevel": "xhigh"});
    assert!(matches!(resolved(&settings), EffortInForce::Mixed { .. }));
    let per_model = match resolved(&settings) {
        EffortInForce::Mixed { per_model } => per_model,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        per_model[1],
        model_effort("claude-opus-5-5", "medium", true)
    );

    let with_model = json!({"effortLevel": "xhigh", "model": "opus"});
    assert_eq!(resolved(&with_model), level("medium"));
    let with_model = json!({"effortLevel": "xhigh", "model": "claude-fable-5-1[1m]"});
    assert_eq!(resolved(&with_model), level("xhigh"));
    let other_model = json!({"effortLevel": "low", "env": {"ANTHROPIC_MODEL": "glm-5"}});
    assert_eq!(resolved(&other_model), level("low"));
}

#[test]
fn an_unnamed_model_reads_as_one_level_only_when_every_model_agrees() {
    let settings = applied(json!({"model": "default"}), Some("low"));
    assert_eq!(resolved(&settings), level("low"));
    assert!(matches!(resolved(&json!({})), EffortInForce::Mixed { .. }));
}

#[test]
fn an_unset_level_reads_as_the_models_own_default() {
    assert_eq!(resolved(&json!({"model": "opus[1m]"})), level("medium"));
    assert_eq!(
        resolved(&json!({"model": "anthropic/claude-sonnet-5"})),
        level("high")
    );
    // A model outside the table has no default this product knows.
    assert_eq!(
        resolved(&json!({"model": "glm-5"})),
        EffortInForce::ToolDefault
    );
}

#[test]
fn the_terminal_variable_outranks_the_file_unless_the_file_sets_or_cancels_it() {
    let source = EffectiveConnectionSource::ShellFile {
        variable: "CLAUDE_CODE_EFFORT_LEVEL".to_string(),
        path: "~/.zshrc:3".to_string(),
    };
    let terminal = ToolTerminal::default().push("CLAUDE_CODE_EFFORT_LEVEL", "max", source.clone());
    assert_eq!(
        resolve(&real_example(), &terminal, &spec()),
        EffortInForce::Terminal {
            level: "max".to_string(),
            source,
        }
    );

    let mut cancelled = real_example();
    cancelled["env"] = json!({"CLAUDE_CODE_EFFORT_LEVEL": ""});
    cancelled["model"] = json!("opus");
    assert_eq!(resolve(&cancelled, &terminal, &spec()), level("xhigh"));

    let mut fixed = real_example();
    fixed["env"] = json!({"CLAUDE_CODE_EFFORT_LEVEL": "low"});
    assert_eq!(
        resolve(&fixed, &terminal, &spec()),
        EffortInForce::Fixed {
            level: "low".to_string()
        }
    );
}

#[test]
fn the_terminal_model_names_the_model_in_use() {
    let terminal = ToolTerminal::default().push(
        "ANTHROPIC_MODEL",
        "claude-opus-5",
        EffectiveConnectionSource::Environment {
            variable: "ANTHROPIC_MODEL".to_string(),
        },
    );
    assert_eq!(resolve(&real_example(), &terminal, &spec()), level("high"));
    let mut own = real_example();
    own["env"] = json!({"ANTHROPIC_MODEL": "fable"});
    assert_eq!(resolve(&own, &terminal, &spec()), level("xhigh"));
}

#[test]
fn a_settings_file_of_another_shape_is_refused() {
    let mut settings = json!({"modelSettings": []});
    assert!(apply(&mut settings, &spec(), Some("low")).is_err());
    let mut settings = json!([]);
    assert!(apply(&mut settings, &spec(), None).is_err());
}
