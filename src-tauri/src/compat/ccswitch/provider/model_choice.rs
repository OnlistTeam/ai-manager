//! The model and effort a tool runs, read from and written to its own files
//! (ADR-0054).
//!
//! The model belongs to the endpoint it was picked under: it is the same slot
//! the endpoint edit form writes, saved with the endpoint and written to the
//! live file while that endpoint is in use. The effort belongs to the tool: it
//! is written to the live file and to the in-use endpoint's saved copy, and a
//! switch keeps it (`live_preservation`).

use std::path::PathBuf;

use serde_json::Value;

use crate::domain::{
    normalize_model_name, validate_effort, AppError, EffortOverride, ErrorCode, ModelChoiceSpec,
    ToolId, ToolModelChoice,
};
use crate::provider::Provider as UpstreamProvider;

use super::live_key::{self, ConfigKey};
use super::{advanced, app_type_for, upstream_detail, ProviderStore};

/// Where one tool keeps the two settings.
struct Slots {
    file: fn() -> PathBuf,
    model: ConfigKey,
    effort: Option<ConfigKey>,
}

/// Claude Code saves an effort per model under this key; it outranks the
/// tool-wide `effortLevel` written here.
const CLAUDE_MODEL_SETTINGS: &str = "modelSettings";

/// The tool-wide effort key, for the switch's settings preservation.
pub(super) const CLAUDE_EFFORT_KEY: &str = "effortLevel";
pub(super) const CODEX_EFFORT_KEY: &str = "model_reasoning_effort";

fn slots(tool: ToolId) -> Option<Slots> {
    match tool {
        ToolId::ClaudeCode => Some(Slots {
            file: crate::config::get_claude_settings_path,
            model: ConfigKey::Json(&["env", "ANTHROPIC_MODEL"]),
            effort: Some(ConfigKey::Json(&[CLAUDE_EFFORT_KEY])),
        }),
        ToolId::Codex => Some(Slots {
            file: crate::codex_config::get_codex_config_path,
            model: ConfigKey::Toml("model"),
            effort: Some(ConfigKey::Toml(CODEX_EFFORT_KEY)),
        }),
        ToolId::GeminiCli => Some(Slots {
            file: crate::gemini_config::get_gemini_env_path,
            model: ConfigKey::DotEnv("GEMINI_MODEL"),
            effort: None,
        }),
        ToolId::OpenCode
        | ToolId::GrokBuild
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => None,
    }
}

fn unsupported(tool: ToolId) -> AppError {
    AppError::new(
        ErrorCode::ProviderNotFound,
        "error.provider.unsupportedTool",
    )
    .with_technical(format!("{} has no model choice", tool.as_str()))
}

fn spec_and_slots(tool: ToolId) -> Result<(ModelChoiceSpec, Slots), AppError> {
    match (ModelChoiceSpec::for_tool(tool), slots(tool)) {
        (Some(spec), Some(slots)) => Ok((spec, slots)),
        _ => Err(unsupported(tool)),
    }
}

fn backups() -> PathBuf {
    crate::infrastructure::paths::product_data_dir()
        .join("backups")
        .join("tool-config")
}

fn save_failed(error: &crate::error::AppError) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.modelChoice.writeFailed",
    )
    .with_technical(upstream_detail(error))
    .with_remediation("error.remediation.retryOrViewDetails")
}

/// The model and effort in force, with what may be chosen.
pub(super) fn read(tool: ToolId) -> Result<ToolModelChoice, AppError> {
    let (spec, slots) = spec_and_slots(tool)?;
    let text = live_key::read_file(&(slots.file)())?.unwrap_or_default();
    let effort = match slots.effort {
        Some(key) => live_key::get(&text, key)?,
        None => None,
    };
    Ok(ToolModelChoice {
        tool,
        model: live_key::get(&text, slots.model)?,
        effort,
        effort_levels: spec
            .effort_levels
            .iter()
            .map(|level| level.to_string())
            .collect(),
        official_models: spec
            .official_models
            .iter()
            .map(|model| model.to_string())
            .collect(),
        effort_overrides: effort_overrides(tool, &text),
    })
}

/// The models a tool keeps its own effort for. Only Claude Code has them.
fn effort_overrides(tool: ToolId, text: &str) -> Vec<EffortOverride> {
    if tool != ToolId::ClaudeCode {
        return Vec::new();
    }
    let Ok(root) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    root.get(CLAUDE_MODEL_SETTINGS)
        .and_then(Value::as_object)
        .map(|models| {
            models
                .iter()
                .filter_map(|(model, settings)| {
                    let effort = settings.get(CLAUDE_EFFORT_KEY)?.as_str()?.trim();
                    (!effort.is_empty()).then(|| EffortOverride {
                        model: model.clone(),
                        effort: effort.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn effective_current(store: &ProviderStore, tool: ToolId) -> Result<Option<String>, AppError> {
    crate::settings::get_effective_current_provider(&store.state.db, &app_type_for(tool))
        .map_err(|error| save_failed(&error))
}

fn save_raw(store: &ProviderStore, tool: ToolId, raw: &UpstreamProvider) -> Result<(), AppError> {
    store
        .state
        .db
        .save_provider(app_type_for(tool).as_str(), raw)
        .map_err(|error| save_failed(&error))
}

/// Writes the live key; when that fails, the saved endpoint goes back to what
/// it was, so the two never disagree.
fn write_live_or_restore(
    store: &ProviderStore,
    tool: ToolId,
    slots: &Slots,
    key: ConfigKey,
    value: Option<&str>,
    original: Option<&UpstreamProvider>,
) -> Result<(), AppError> {
    let written = live_key::write(&(slots.file)(), &backups(), key, value);
    if let (Err(_), Some(original)) = (&written, original) {
        if let Err(error) = save_raw(store, tool, original) {
            log::warn!(
                "model choice: restoring the saved endpoint after a failed write also failed: {}",
                error.message_key
            );
        }
    }
    written
}

/// Sets the model of one saved endpoint, or of the connection in force when
/// no saved endpoint is named. The live file changes only when the endpoint
/// is the one in use.
pub(super) fn set_model(
    store: &ProviderStore,
    tool: ToolId,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<ToolModelChoice, AppError> {
    let (_, slots) = spec_and_slots(tool)?;
    let model = normalize_model_name(model)?;
    let mut original = None;
    if let Some(id) = provider_id {
        let raw = store.find_raw(tool, id)?;
        let mut updated = raw.clone();
        advanced::write_models(tool, &mut updated, model.as_slice())?;
        if updated.settings_config != raw.settings_config {
            save_raw(store, tool, &updated)?;
            original = Some(raw);
        }
        if effective_current(store, tool)?.as_deref() != Some(id) {
            return read(tool);
        }
    }
    write_live_or_restore(
        store,
        tool,
        &slots,
        slots.model,
        model.as_deref(),
        original.as_ref(),
    )?;
    log::info!(
        "model choice: {} model {}",
        tool.as_str(),
        if model.is_some() { "set" } else { "cleared" }
    );
    read(tool)
}

/// Sets the tool-wide effort in the live file and in the in-use endpoint's
/// saved copy, which an edit of that endpoint writes back to the file.
pub(super) fn set_effort(
    store: &ProviderStore,
    tool: ToolId,
    effort: Option<&str>,
) -> Result<ToolModelChoice, AppError> {
    let (spec, slots) = spec_and_slots(tool)?;
    let key = slots.effort.ok_or_else(|| unsupported(tool))?;
    validate_effort(&spec, effort)?;
    let mut original = None;
    if let Some(id) = effective_current(store, tool)? {
        let raw = store.find_raw(tool, &id)?;
        let mut updated = raw.clone();
        set_in_saved_copy(&mut updated, key, effort)?;
        if updated.settings_config != raw.settings_config {
            save_raw(store, tool, &updated)?;
            original = Some(raw);
        }
    }
    write_live_or_restore(store, tool, &slots, key, effort, original.as_ref())?;
    log::info!("model choice: {} effort {:?}", tool.as_str(), effort);
    read(tool)
}

/// A saved endpoint holds the tool's file as JSON: Claude Code's settings
/// object itself, Codex's TOML under `config`, Gemini CLI's `.env` under `env`.
fn set_in_saved_copy(
    raw: &mut UpstreamProvider,
    key: ConfigKey,
    value: Option<&str>,
) -> Result<(), AppError> {
    match key {
        ConfigKey::Json(path) => live_key::json_set(&mut raw.settings_config, path, value),
        ConfigKey::DotEnv(name) => {
            live_key::json_set(&mut raw.settings_config, &["env", name], value)
        }
        ConfigKey::Toml(name) => {
            let Some(root) = raw.settings_config.as_object_mut() else {
                return Ok(());
            };
            let config = root.get("config").and_then(Value::as_str).unwrap_or("");
            let mut doc = config.parse::<toml_edit::DocumentMut>().map_err(|_| {
                AppError::new(ErrorCode::ConfigParseFailed, "error.modelChoice.readFailed")
                    .with_technical("saved Codex config did not parse")
                    .with_remediation("error.remediation.retryOrViewDetails")
            })?;
            live_key::toml_set(&mut doc, name, value);
            root.insert("config".to_string(), Value::String(doc.to_string()));
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "model_choice_tests.rs"]
mod tests;
