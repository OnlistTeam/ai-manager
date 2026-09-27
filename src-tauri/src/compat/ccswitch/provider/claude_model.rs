//! Where Claude Code's model is kept (ADR-0055 decision 5).
//!
//! Claude Code saves an in-tool `/model` choice as the top-level `model` of
//! its user settings file, and an `env.ANTHROPIC_MODEL` in that file outranks
//! the key in every new session. This product writes the same `model`, so a
//! choice made on either side replaces the other's. The variable is still
//! read: presets and copies saved before this rule carry it, and another tool
//! may write it. Whenever this product writes the file, a model the variable
//! names moves to the key.

use serde_json::Value;

/// The key `/model` writes.
pub(crate) const MODEL_KEY: &str = "model";
/// The variable that outranks the key.
pub(crate) const MODEL_VARIABLE: &str = "ANTHROPIC_MODEL";

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn variable(settings: &Value) -> Option<&str> {
    text(settings.get("env").and_then(|env| env.get(MODEL_VARIABLE)))
}

/// The model a new session runs, as the settings name it: the variable,
/// else the key.
pub(crate) fn named(settings: &Value) -> Option<&str> {
    variable(settings).or_else(|| text(settings.get(MODEL_KEY)))
}

/// Sets or clears the model and removes a variable that would outrank it.
/// An empty variable stays: it names no model, and it cancels one exported in
/// the terminal.
pub(crate) fn set(settings: &mut Value, model: Option<&str>) {
    let Some(root) = settings.as_object_mut() else {
        return;
    };
    if let Some(env) = root.get_mut("env").and_then(Value::as_object_mut) {
        if text(env.get(MODEL_VARIABLE)).is_some() {
            env.remove(MODEL_VARIABLE);
        }
    }
    match model {
        Some(model) => {
            root.insert(MODEL_KEY.to_string(), Value::String(model.to_string()));
        }
        None => {
            root.remove(MODEL_KEY);
        }
    }
}

/// Moves a model the variable names to the key, where it was in force over
/// any value the key held.
pub(crate) fn move_variable_to_key(settings: &mut Value) {
    if let Some(model) = variable(settings).map(str::to_string) {
        set(settings, Some(&model));
    }
}

#[cfg(test)]
#[path = "claude_model_tests.rs"]
mod tests;
