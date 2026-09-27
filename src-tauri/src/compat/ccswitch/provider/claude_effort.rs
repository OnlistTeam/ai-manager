//! Claude Code's thinking effort, read the way Claude Code resolves it for a
//! new session and written the way its own `/effort` saves it (ADR-0055).
//!
//! Claude Code takes the first of: `CLAUDE_CODE_EFFORT_LEVEL` (the settings
//! file's `env` block, else the terminal), the level saved for the model in use
//! under `modelSettings`, the top-level `effortLevel` (which the user settings
//! file no longer applies to Opus 5.5 and later), and the model's own default.
//! The settings keys hold `low` to `xhigh`; `max` persists only through the
//! variable, which this product never writes. A running session takes up a
//! new value of that variable at once but never its removal, so one found in
//! the file is cleared to an empty value, which counts as unset.

use serde_json::{Map, Value};

use super::live_key;
use crate::compat::ccswitch::provider_runtime::ToolTerminal;
use crate::domain::{
    canonical_model_name, AppError, EffortInForce, ErrorCode, ModelChoiceSpec, ModelEffort,
};

/// The tool-wide level, the older form of `/effort`.
pub(super) const EFFORT_KEY: &str = "effortLevel";
/// Levels saved per model, which `/effort` writes today.
pub(super) const MODEL_SETTINGS_KEY: &str = "modelSettings";
/// Outranks both keys and `/effort`, and is the only place `max` persists.
pub(super) const EFFORT_VARIABLE: &str = "CLAUDE_CODE_EFFORT_LEVEL";
const MODEL_VARIABLE: &str = "ANTHROPIC_MODEL";
const MODEL_KEY: &str = "model";

/// Names that leave the model to the account or the mode, so the model a new
/// session runs cannot be read from the file.
const UNNAMED_MODELS: [&str; 3] = ["default", "best", "opusplan"];

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// A variable in the settings file's `env` block: `Some(None)` when it is set
/// to an empty string, which cancels the terminal's value of the same name.
fn env_value<'a>(settings: &'a Value, name: &str) -> Option<Option<&'a str>> {
    settings
        .get("env")
        .and_then(|env| env.get(name))
        .map(|value| text(Some(value)))
}

/// Levels saved per model, in file order.
fn saved_levels(settings: &Value) -> Vec<(&str, &str)> {
    settings
        .get(MODEL_SETTINGS_KEY)
        .and_then(Value::as_object)
        .map(|models| {
            models
                .iter()
                .filter_map(|(model, entry)| Some((model.as_str(), text(entry.get(EFFORT_KEY))?)))
                .collect()
        })
        .unwrap_or_default()
}

/// The effort a new session will run at.
pub(super) fn resolve(
    settings: &Value,
    terminal: &ToolTerminal,
    spec: &ModelChoiceSpec,
) -> EffortInForce {
    match env_value(settings, EFFORT_VARIABLE) {
        Some(Some(level)) => {
            return EffortInForce::Fixed {
                level: level.to_string(),
            }
        }
        Some(None) => {}
        None => {
            if let Some((level, source)) = terminal.get(EFFORT_VARIABLE) {
                return EffortInForce::Terminal {
                    level: level.to_string(),
                    source: source.clone(),
                };
            }
        }
    }

    let saved = saved_levels(settings);
    let tool_wide = text(settings.get(EFFORT_KEY));
    if saved.is_empty() && tool_wide.is_none() {
        return EffortInForce::ToolDefault;
    }
    let from_file = || text(settings.get(MODEL_KEY));
    let model = match env_value(settings, MODEL_VARIABLE) {
        Some(Some(model)) => Some(model),
        Some(None) => from_file(),
        None => terminal
            .get(MODEL_VARIABLE)
            .map(|(model, _)| model)
            .or_else(from_file),
    };
    match model.filter(|model| !UNNAMED_MODELS.contains(&canonical_model_name(model).as_str())) {
        Some(model) => for_model(spec, &saved, tool_wide, model),
        None => across_models(spec, &saved, tool_wide),
    }
}

fn saved_for<'a>(saved: &[(&str, &'a str)], id: &str) -> Option<&'a str> {
    saved
        .iter()
        .find(|(model, _)| canonical_model_name(model) == id)
        .map(|(_, level)| *level)
}

/// The model in use is named: its saved level, else the tool-wide one where
/// it applies, else its own default. A model outside the table is assumed to
/// read the tool-wide level, as every model before Opus 5.5 does.
fn for_model(
    spec: &ModelChoiceSpec,
    saved: &[(&str, &str)],
    tool_wide: Option<&str>,
    model: &str,
) -> EffortInForce {
    let entry = spec.effort_model(model);
    let id = entry.map_or_else(|| canonical_model_name(model), |entry| entry.id.to_string());
    let reads_tool_wide = entry.is_none_or(|entry| entry.reads_tool_wide_level);
    match saved_for(saved, &id).or(tool_wide.filter(|_| reads_tool_wide)) {
        Some(level) => EffortInForce::Level {
            level: level.to_string(),
        },
        None => EffortInForce::ToolDefault,
    }
}

/// The model in use is not named: one level when every model runs at it,
/// else each model's level.
fn across_models(
    spec: &ModelChoiceSpec,
    saved: &[(&str, &str)],
    tool_wide: Option<&str>,
) -> EffortInForce {
    let mut per_model: Vec<ModelEffort> = spec
        .effort_models
        .iter()
        .map(|model| {
            let chosen =
                saved_for(saved, model.id).or(tool_wide.filter(|_| model.reads_tool_wide_level));
            ModelEffort {
                model: model.id.to_string(),
                effort: chosen.unwrap_or(model.default_level).to_string(),
                model_default: chosen.is_none(),
            }
        })
        .collect();
    per_model.extend(
        saved
            .iter()
            .filter(|(model, _)| spec.effort_model(model).is_none())
            .map(|(model, level)| ModelEffort {
                model: model.to_string(),
                effort: level.to_string(),
                model_default: false,
            }),
    );
    match per_model.first() {
        Some(first) if per_model.iter().all(|model| model.effort == first.effort) => {
            EffortInForce::Level {
                level: first.effort.clone(),
            }
        }
        _ => EffortInForce::Mixed { per_model },
    }
}

fn not_an_object(what: &'static str) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.modelChoice.writeFailed",
    )
    .with_technical(what)
    .with_remediation("error.remediation.retryOrViewDetails")
}

/// Sets the effort for every model, or hands it back to each model's default
/// when `effort` is `None`.
///
/// A level goes where `/effort` puts it, the entry of every model already in
/// `modelSettings` and of every model in the table, and into the top-level
/// key that older models read. A level the variable holds is cleared so the
/// choice, and a later `/effort`, take effect, in the sessions already running
/// too. The tool default removes both keys, keeping any other field of a
/// model's entry.
pub(super) fn apply(
    settings: &mut Value,
    spec: &ModelChoiceSpec,
    effort: Option<&str>,
) -> Result<(), AppError> {
    if !settings.is_object() {
        return Err(not_an_object("the settings root is not an object"));
    }
    if let Some(Some(_)) = env_value(settings, EFFORT_VARIABLE) {
        live_key::json_set(settings, &["env", EFFORT_VARIABLE], Some(""))?;
    }
    let root = settings
        .as_object_mut()
        .ok_or_else(|| not_an_object("the settings root is not an object"))?;
    match effort {
        Some(level) => {
            root.insert(EFFORT_KEY.to_string(), Value::String(level.to_string()));
            let models = root
                .entry(MODEL_SETTINGS_KEY.to_string())
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .ok_or_else(|| not_an_object("modelSettings is not an object"))?;
            for model in spec.effort_models {
                models
                    .entry(model.id.to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
            }
            for entry in models.values_mut() {
                if !entry.is_object() {
                    *entry = Value::Object(Map::new());
                }
                if let Some(entry) = entry.as_object_mut() {
                    entry.insert(EFFORT_KEY.to_string(), Value::String(level.to_string()));
                }
            }
        }
        None => {
            root.remove(EFFORT_KEY);
            if let Some(models) = root
                .get_mut(MODEL_SETTINGS_KEY)
                .and_then(Value::as_object_mut)
            {
                for entry in models.values_mut() {
                    if let Some(entry) = entry.as_object_mut() {
                        entry.remove(EFFORT_KEY);
                    }
                }
                models.retain(|_, entry| entry.as_object().is_none_or(|entry| !entry.is_empty()));
                if models.is_empty() {
                    root.remove(MODEL_SETTINGS_KEY);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "claude_effort_tests.rs"]
mod tests;
