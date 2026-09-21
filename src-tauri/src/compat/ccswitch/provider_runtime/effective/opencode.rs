//! Read-only global selection evidence, not a prediction of an arbitrary running session.
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{finish, from_variable, live, unknown, Resolution, ToolEnvironment};
use crate::domain::{EffectiveCredential, EffectiveSelection};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

fn read_optional_json(path: &Path) -> Result<Value, ()> {
    let file = match std::fs::File::open(path) {
        Ok(file) if file.metadata().map_err(|_| ())?.is_file() => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(serde_json::json!({}))
        }
        _ => return Err(()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(());
    }
    serde_json::from_slice::<Value>(&bytes)
        .map_err(|_| ())
        .and_then(|value| {
            if value.is_object() {
                Ok(value)
            } else {
                Err(())
            }
        })
}

fn state_path(environment: &ToolEnvironment) -> PathBuf {
    environment
        .lookup("XDG_STATE_HOME")
        .map(|variable| PathBuf::from(variable.value))
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| crate::config::get_home_dir().join(".local").join("state"))
        .join("opencode")
        .join("model.json")
}

pub(super) fn resolve_live(environment: &ToolEnvironment) -> Resolution {
    // Alternate/inline config requires a full merge. Do not falsely label the standard file active.
    if [
        "OPENCODE_CONFIG",
        "OPENCODE_CONFIG_CONTENT",
        "OPENCODE_CONFIG_DIR",
        "XDG_CONFIG_HOME",
    ]
    .iter()
    .any(|key| environment.lookup(key).is_some())
    {
        return unknown();
    }
    let path = crate::opencode_config::get_opencode_config_path();
    let Ok(config) = crate::opencode_config::read_opencode_config() else {
        return unknown();
    };
    let history = read_optional_json(&state_path(environment));
    let auth_path = environment
        .lookup("XDG_DATA_HOME")
        .map(|variable| PathBuf::from(variable.value))
        .filter(|path| path.is_absolute())
        .map(|path| path.join("opencode"))
        .unwrap_or_else(crate::opencode_config::get_opencode_data_dir)
        .join("auth.json");
    let auth = read_optional_json(&auth_path);
    resolve_with_state(
        &path,
        &config,
        history.as_ref().ok(),
        &auth_path,
        auth.as_ref().ok(),
        environment,
    )
}

fn allowed(config: &Value, provider: &str) -> bool {
    !config
        .get("disabled_providers")
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(provider)))
        && config
            .get("enabled_providers")
            .and_then(Value::as_array)
            .is_none_or(|items| items.iter().any(|item| item.as_str() == Some(provider)))
}

fn valid_model(model: &str) -> bool {
    model.len() <= 256
        && !model.chars().any(char::is_control)
        && model.split_once('/').is_some_and(|(provider, model)| {
            !provider.is_empty() && provider.len() <= 128 && !model.is_empty()
        })
}

pub(super) fn resolve_with_state(
    config_path: &Path,
    config: &Value,
    history: Option<&Value>,
    auth_path: &Path,
    auth: Option<&Value>,
    environment: &ToolEnvironment,
) -> Resolution {
    let (model, selection) = if let Some(model) = config.get("model") {
        let Some(model) = model.as_str().filter(|model| valid_model(model)) else {
            return unknown();
        };
        (model.to_string(), EffectiveSelection::DefaultModel)
    } else {
        let Some(recent) = history
            .and_then(|value| value.get("recent"))
            .and_then(Value::as_array)
        else {
            return unknown();
        };
        let model = recent.iter().find_map(|entry| {
            let provider = entry.get("providerID")?.as_str()?;
            let model = entry.get("modelID")?.as_str()?;
            let full = format!("{provider}/{model}");
            (allowed(config, provider) && valid_model(&full)).then_some(full)
        });
        let Some(model) = model else {
            return unknown();
        };
        (model, EffectiveSelection::RecentModel)
    };
    let (provider_id, _) = model.split_once('/').expect("validated model");
    if !allowed(config, provider_id) {
        return unknown();
    }
    let options = config
        .get("provider")
        .and_then(|value| value.get(provider_id))
        .and_then(|value| value.get("options"));
    let endpoint = match options.and_then(|value| value.get("baseURL")) {
        Some(Value::String(value)) => {
            let (value, source) = resolve_value(value, config_path, environment);
            let Some(value) = value else {
                return unknown();
            };
            Some((value, source))
        }
        Some(_) => return unknown(),
        None => None,
    };
    let mut result = finish(endpoint, None);
    result.selection = selection;
    result.provider_hint = Some(provider_id.to_string());
    result.model = Some(model.clone());
    if let Some(raw) = options.and_then(|value| value.get("apiKey")) {
        let Some(raw) = raw.as_str() else {
            result.credential = EffectiveCredential::Unknown;
            return result;
        };
        let (value, source) = resolve_value(raw, config_path, environment);
        result.credential = if value.is_some() {
            EffectiveCredential::Configured
        } else {
            EffectiveCredential::Unknown
        };
        result.credential_source = source;
    } else if let Some(auth) = auth {
        if let Some(entry) = auth.get(provider_id) {
            result.credential = match entry.get("type").and_then(Value::as_str) {
                Some("api") if nonempty(entry.get("key")) => EffectiveCredential::Configured,
                Some("oauth")
                    if nonempty(entry.get("access")) || nonempty(entry.get("refresh")) =>
                {
                    EffectiveCredential::ToolLogin
                }
                _ => EffectiveCredential::Unknown,
            };
            result.credential_source = live(auth_path);
        } else {
            // Built-in providers, SDK environment credentials and plugins can authenticate too.
            result.credential = EffectiveCredential::Unknown;
        }
    } else {
        result.credential = EffectiveCredential::Unknown;
    }
    result
}

fn nonempty(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}

fn resolve_value(
    raw: &str,
    path: &Path,
    environment: &ToolEnvironment,
) -> (Option<String>, crate::domain::EffectiveConnectionSource) {
    let raw = raw.trim();
    if let Some(name) = raw
        .strip_prefix("{env:")
        .and_then(|value| value.strip_suffix('}'))
    {
        return environment
            .lookup(name)
            .map(|variable| (Some(variable.value.clone()), from_variable(&variable)))
            .unwrap_or((None, live(path)));
    }
    if raw.is_empty() || raw.contains("{file:") || raw.contains("{env:") {
        return (None, live(path));
    }
    (Some(raw.to_string()), live(path))
}

#[cfg(test)]
pub(super) fn resolve_opencode(
    path: &Path,
    config: &Value,
    environment: &ToolEnvironment,
) -> Resolution {
    resolve_with_state(
        path,
        config,
        None,
        Path::new("auth.json"),
        Some(&serde_json::json!({})),
        environment,
    )
}

#[cfg(test)]
mod tests;
