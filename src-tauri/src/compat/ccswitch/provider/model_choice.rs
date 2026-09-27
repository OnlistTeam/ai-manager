//! The model and effort a tool runs, read from and written to its own files
//! (ADR-0055).
//!
//! The model belongs to the endpoint it was picked under: it is the same slot
//! the endpoint edit form writes, saved with the endpoint and written to the
//! live file while that endpoint is in use. The effort belongs to the tool: it
//! is written to the live file and to the in-use endpoint's saved copy, and a
//! switch keeps it (`live_preservation`). Claude Code's effort spans several
//! keys; `claude_effort` reads and writes them.

use std::path::PathBuf;

use serde_json::Value;

use crate::compat::ccswitch::provider_runtime::ToolTerminal;
use crate::domain::{
    normalize_model_name, validate_effort, AppError, EffortInForce, ErrorCode, ModelChoiceSpec,
    ToolId, ToolModelChoice,
};
use crate::provider::Provider as UpstreamProvider;

use super::claude_effort;
use super::live_key::{self, ConfigKey};
use super::{advanced, app_type_for, upstream_detail, ProviderStore};

/// Where one tool keeps its effort.
#[derive(Clone, Copy)]
enum EffortSlot {
    /// One key, which holds the level for every model.
    Key(ConfigKey),
    /// Claude Code's settings file, which keeps a level per model beside the
    /// tool-wide one and an environment variable over both (`claude_effort`).
    ClaudeSettings,
}

/// Where one tool keeps the two settings.
struct Slots {
    file: fn() -> PathBuf,
    model: ConfigKey,
    effort: Option<EffortSlot>,
}

/// The tool-wide effort key, for the switch's settings preservation.
pub(super) const CODEX_EFFORT_KEY: &str = "model_reasoning_effort";

fn slots(tool: ToolId) -> Option<Slots> {
    match tool {
        ToolId::ClaudeCode => Some(Slots {
            file: crate::config::get_claude_settings_path,
            model: ConfigKey::Json(&["env", "ANTHROPIC_MODEL"]),
            effort: Some(EffortSlot::ClaudeSettings),
        }),
        ToolId::Codex => Some(Slots {
            file: crate::codex_config::get_codex_config_path,
            model: ConfigKey::Toml("model"),
            effort: Some(EffortSlot::Key(ConfigKey::Toml(CODEX_EFFORT_KEY))),
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

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

/// The model and effort in force, with what may be chosen.
pub(super) fn read(tool: ToolId, terminal: &ToolTerminal) -> Result<ToolModelChoice, AppError> {
    let (spec, slots) = spec_and_slots(tool)?;
    let text = live_key::read_file(&(slots.file)())?.unwrap_or_default();
    let effort = match slots.effort {
        None => EffortInForce::ToolDefault,
        Some(EffortSlot::Key(key)) => match live_key::get(&text, key)? {
            Some(level) => EffortInForce::Level { level },
            None => EffortInForce::ToolDefault,
        },
        Some(EffortSlot::ClaudeSettings) => {
            claude_effort::resolve(&live_key::parse_json(&text)?, terminal, &spec)
        }
    };
    Ok(ToolModelChoice {
        tool,
        model: live_key::get(&text, slots.model)?,
        effort,
        effort_levels: strings(spec.effort_levels),
        official_models: strings(spec.official_models),
    })
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

/// Writes the live file; when that fails, the saved endpoint goes back to
/// what it was, so the two never disagree.
fn write_live_or_restore(
    store: &ProviderStore,
    tool: ToolId,
    original: Option<&UpstreamProvider>,
    write: impl FnOnce() -> Result<(), AppError>,
) -> Result<(), AppError> {
    let written = write();
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
    terminal: &ToolTerminal,
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
            return read(tool, terminal);
        }
    }
    write_live_or_restore(store, tool, original.as_ref(), || {
        live_key::write(&(slots.file)(), &backups(), slots.model, model.as_deref())
    })?;
    log::info!(
        "model choice: {} model {}",
        tool.as_str(),
        if model.is_some() { "set" } else { "cleared" }
    );
    read(tool, terminal)
}

/// Sets the effort in the live file and in the in-use endpoint's saved copy,
/// which an edit of that endpoint writes back to the file.
pub(super) fn set_effort(
    store: &ProviderStore,
    tool: ToolId,
    effort: Option<&str>,
    terminal: &ToolTerminal,
) -> Result<ToolModelChoice, AppError> {
    let (spec, slots) = spec_and_slots(tool)?;
    let slot = slots.effort.ok_or_else(|| unsupported(tool))?;
    validate_effort(&spec, effort)?;
    let mut original = None;
    if let Some(id) = effective_current(store, tool)? {
        let raw = store.find_raw(tool, &id)?;
        let mut updated = raw.clone();
        match slot {
            EffortSlot::Key(key) => set_in_saved_copy(&mut updated, key, effort)?,
            EffortSlot::ClaudeSettings => {
                claude_effort::apply(&mut updated.settings_config, &spec, effort)?
            }
        }
        if updated.settings_config != raw.settings_config {
            save_raw(store, tool, &updated)?;
            original = Some(raw);
        }
    }
    let path = (slots.file)();
    write_live_or_restore(store, tool, original.as_ref(), || match slot {
        EffortSlot::Key(key) => live_key::write(&path, &backups(), key, effort),
        EffortSlot::ClaudeSettings => live_key::write_json(&path, &backups(), &|settings| {
            claude_effort::apply(settings, &spec, effort)
        }),
    })?;
    log::info!("model choice: {} effort {:?}", tool.as_str(), effort);
    read(tool, terminal)
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
