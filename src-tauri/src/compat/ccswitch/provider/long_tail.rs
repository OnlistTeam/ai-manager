//! Tool-native provider field adapters for Grok Build, OpenClaw, Hermes and Pi.
//!
//! Persistence and live-file transactions remain owned by upstream `ProviderService`.
//! This module only projects the product's common edit fields into each native shape.

use serde_json::{Map, Value};

use crate::domain::{AppError, ErrorCode, ToolId};
use crate::provider::Provider as UpstreamProvider;

use super::settings_not_object_error;

pub(super) fn settings_shape_supported(tool: ToolId, settings: &Value) -> bool {
    let Some(root) = settings.as_object() else {
        return false;
    };
    match tool {
        ToolId::GrokBuild => root
            .get("config")
            .and_then(Value::as_str)
            .is_some_and(|config| crate::grok_config::validate_config_toml(config).is_ok()),
        ToolId::OpenClaw => {
            optional_string(root, "baseUrl")
                && optional_string(root, "apiKey")
                && optional_string(root, "api")
                && optional_model_array(root)
        }
        ToolId::Hermes => {
            root.get(crate::hermes_config::PROVIDER_SOURCE_FIELD)
                .and_then(Value::as_str)
                != Some(crate::hermes_config::PROVIDER_SOURCE_DICT)
                && optional_string(root, "base_url")
                && optional_string(root, "api_key")
                && optional_string(root, "api_mode")
                && optional_model_array(root)
        }
        ToolId::Pi => {
            optional_string(root, "baseUrl")
                && optional_string(root, "apiKey")
                && optional_string(root, "api")
                && optional_model_array(root)
        }
        ToolId::ClaudeCode
        | ToolId::Codex
        | ToolId::GeminiCli
        | ToolId::OpenCode
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => false,
    }
}

pub(super) fn models(tool: ToolId, raw: &UpstreamProvider) -> Vec<String> {
    if tool == ToolId::GrokBuild {
        return raw
            .settings_config
            .get("config")
            .and_then(Value::as_str)
            .and_then(crate::grok_config::extract_model_config)
            .map(|config| vec![config.model])
            .unwrap_or_default();
    }
    raw.settings_config
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn write_api_key(tool: ToolId, settings: &mut Value, key: &str) -> Result<(), AppError> {
    match tool {
        ToolId::GrokBuild => update_grok_config(settings, |config| {
            crate::grok_config::update_api_key(config, key)
        }),
        ToolId::OpenClaw | ToolId::Pi => write_root_string(settings, "apiKey", key),
        ToolId::Hermes => write_root_string(settings, "api_key", key),
        ToolId::ClaudeCode
        | ToolId::Codex
        | ToolId::GeminiCli
        | ToolId::OpenCode
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => Err(save_failed()),
    }
}

pub(super) fn write_models(
    tool: ToolId,
    raw: &mut UpstreamProvider,
    models: &[String],
) -> Result<(), AppError> {
    if tool == ToolId::GrokBuild {
        let model = models.first().ok_or_else(save_failed)?;
        return update_grok_config(&mut raw.settings_config, |config| {
            crate::grok_config::update_model(config, model)
        });
    }
    if !matches!(tool, ToolId::OpenClaw | ToolId::Hermes | ToolId::Pi) {
        return Err(save_failed());
    }
    let root = raw
        .settings_config
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?;
    let old = root
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let updated = models
        .iter()
        .map(|id| {
            old.iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({ "id": id }))
        })
        .collect();
    root.insert("models".to_string(), Value::Array(updated));
    Ok(())
}

pub(super) fn write_base_url(
    tool: ToolId,
    settings: &mut Value,
    desired: Option<&str>,
) -> Result<(), AppError> {
    match tool {
        ToolId::GrokBuild => {
            let value = desired.ok_or_else(save_failed)?;
            update_grok_config(settings, |config| {
                crate::grok_config::update_base_url(config, value)
            })
        }
        ToolId::OpenClaw | ToolId::Pi => write_optional_root_string(settings, "baseUrl", desired),
        ToolId::Hermes => write_optional_root_string(settings, "base_url", desired),
        ToolId::ClaudeCode
        | ToolId::Codex
        | ToolId::GeminiCli
        | ToolId::OpenCode
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => Err(save_failed()),
    }
}

fn optional_string(root: &Map<String, Value>, key: &str) -> bool {
    root.get(key).is_none_or(Value::is_string)
}

fn optional_model_array(root: &Map<String, Value>) -> bool {
    root.get("models").is_none_or(|models| {
        models.as_array().is_some_and(|models| {
            models.iter().all(|model| {
                model.as_object().is_some_and(|model| {
                    model
                        .get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| !id.trim().is_empty())
                })
            })
        })
    })
}

fn update_grok_config(
    settings: &mut Value,
    update: impl FnOnce(&str) -> Result<String, crate::error::AppError>,
) -> Result<(), AppError> {
    let root = settings
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?;
    let config = root
        .get("config")
        .and_then(Value::as_str)
        .ok_or_else(save_failed)?;
    let updated = update(config).map_err(|_| save_failed())?;
    root.insert("config".to_string(), Value::String(updated));
    Ok(())
}

fn write_root_string(settings: &mut Value, key: &str, value: &str) -> Result<(), AppError> {
    let root = settings
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?;
    root.insert(key.to_string(), Value::String(value.to_string()));
    Ok(())
}

fn write_optional_root_string(
    settings: &mut Value,
    key: &str,
    value: Option<&str>,
) -> Result<(), AppError> {
    if let Some(value) = value {
        return write_root_string(settings, key, value);
    }
    settings
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?
        .remove(key);
    Ok(())
}

fn save_failed() -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.saveFailed")
        .with_remediation("error.remediation.checkServiceSettings")
}
