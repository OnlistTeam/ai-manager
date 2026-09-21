use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

use crate::config::{get_home_dir, write_text_file};
use crate::error::AppError;
use crate::provider::Provider;

pub const DEFAULT_API_BACKEND: &str = "responses";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrokModelConfig {
    pub profile: String,
    pub model: String,
    pub base_url: String,
    pub name: String,
    pub api_key: Option<String>,
    pub env_key: Option<String>,
    pub api_backend: String,
    pub context_window: i64,
}

/// Grok Build configuration directory (`~/.grok`).
pub fn get_grok_config_dir() -> PathBuf {
    crate::settings::get_grok_override_dir().unwrap_or_else(|| get_home_dir().join(".grok"))
}

/// Grok Build live configuration path (`~/.grok/config.toml`).
pub fn get_grok_config_path() -> PathBuf {
    get_grok_config_dir().join("config.toml")
}

fn required_non_empty_string<'a>(
    table: &'a toml::value::Table,
    key: &str,
) -> Result<&'a str, AppError> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::localized(
                "provider.grokbuild.field.missing",
                format!("Grok Build configuration is missing a valid {key} field"),
            )
        })
}

fn optional_non_empty_string(table: &toml::value::Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

/// Syntax-only validation for a Grok Build config document (empty allowed).
///
/// Official entries use the xAI OAuth login built into the Grok CLI, so config.toml needs
/// no custom model table (and usually has none): an empty document is valid, and a non-empty
/// one only has to be syntactically valid TOML. Both the live layer's reads/writes and the
/// official snapshot validation use this; the stronger "must have a complete custom model
/// table" check lives in `validate_config_toml`.
pub fn validate_config_toml_syntax(config_toml: &str) -> Result<(), AppError> {
    if config_toml.trim().is_empty() {
        return Ok(());
    }
    config_toml
        .parse::<toml::Value>()
        .map(|_| ())
        .map_err(|error| {
            AppError::localized(
                "provider.grokbuild.config.invalid_toml",
                format!("Grok Build config.toml is malformed: {error}"),
            )
        })
}

/// Validate the provider-owned Grok Build TOML document.
pub fn validate_config_toml(config_toml: &str) -> Result<(), AppError> {
    let document = config_toml.parse::<toml::Value>().map_err(|error| {
        AppError::localized(
            "provider.grokbuild.config.invalid_toml",
            format!("Grok Build config.toml is malformed: {error}"),
        )
    })?;

    let root = document.as_table().ok_or_else(|| {
        AppError::localized(
            "provider.grokbuild.config.not_table",
            "Grok Build configuration must be a TOML table",
        )
    })?;
    let models = root
        .get("models")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| {
            AppError::localized(
                "provider.grokbuild.models.missing",
                "Grok Build configuration is missing [models]",
            )
        })?;
    let default_model = required_non_empty_string(models, "default")?;
    let model_entries = root
        .get("model")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| {
            AppError::localized(
                "provider.grokbuild.model.missing",
                "Grok Build configuration is missing [model.<name>]",
            )
        })?;
    let selected_model = model_entries
        .get(default_model)
        .and_then(toml::Value::as_table)
        .ok_or_else(|| {
            AppError::localized(
                "provider.grokbuild.default_model.missing",
                format!("Grok Build configuration is missing [model.\"{default_model}\"]"),
            )
        })?;

    required_non_empty_string(selected_model, "model")?;
    required_non_empty_string(selected_model, "base_url")?;
    required_non_empty_string(selected_model, "name")?;
    if optional_non_empty_string(selected_model, "api_key").is_none()
        && optional_non_empty_string(selected_model, "env_key").is_none()
    {
        return Err(AppError::localized(
            "provider.grokbuild.credentials.missing",
            "Grok Build configuration is missing a valid api_key or env_key field",
        ));
    }
    required_non_empty_string(selected_model, "api_backend")?;

    selected_model
        .get("context_window")
        .and_then(toml::Value::as_integer)
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            AppError::localized(
                "provider.grokbuild.context_window.invalid",
                "Grok Build context_window must be a positive integer",
            )
        })?;

    Ok(())
}

pub fn extract_model_config(config_toml: &str) -> Option<GrokModelConfig> {
    let document = config_toml.parse::<toml::Value>().ok()?;
    let root = document.as_table()?;
    let default_model = root
        .get("models")?
        .as_table()?
        .get("default")?
        .as_str()?
        .trim();
    let selected_model = root
        .get("model")?
        .as_table()?
        .get(default_model)?
        .as_table()?;
    Some(GrokModelConfig {
        profile: default_model.to_string(),
        model: selected_model.get("model")?.as_str()?.trim().to_string(),
        base_url: selected_model
            .get("base_url")?
            .as_str()?
            .trim_end_matches('/')
            .to_string(),
        name: selected_model.get("name")?.as_str()?.trim().to_string(),
        api_key: optional_non_empty_string(selected_model, "api_key"),
        env_key: optional_non_empty_string(selected_model, "env_key"),
        api_backend: selected_model
            .get("api_backend")?
            .as_str()?
            .trim()
            .to_string(),
        context_window: selected_model.get("context_window")?.as_integer()?,
    })
}

pub fn extract_credentials(config_toml: &str) -> Option<(String, String)> {
    let config = extract_model_config(config_toml)?;
    // Credentials only come from two explicit, config-declared sources:
    //   1. an inline `api_key`, or
    //   2. the process env var named by `env_key`.
    //
    // Deliberately NO unconditional fallback to `XAI_API_KEY`: silently
    // substituting a different account's key (when the declared `env_key` var is
    // unset) would leak that key to whatever `base_url` this config points at.
    // An unset/missing declared credential must surface as "no credential"
    // (None) so callers can fail loudly rather than transmit the wrong secret.
    let api_key = config.api_key.or_else(|| {
        config
            .env_key
            .as_deref()
            .and_then(|key| std::env::var(key).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })?;
    Some((config.base_url, api_key))
}

pub fn extract_inline_api_key(config_toml: &str) -> Option<String> {
    extract_model_config(config_toml)?.api_key
}

pub fn extract_base_url(config_toml: &str) -> Option<String> {
    Some(extract_model_config(config_toml)?.base_url)
}

fn update_selected_model_string(
    config_toml: &str,
    field: &str,
    value: &str,
) -> Result<String, AppError> {
    let mut document = config_toml
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| {
            AppError::localized(
                "provider.grokbuild.config.invalid_toml",
                format!("Grok Build config.toml is malformed: {error}"),
            )
        })?;
    let default_model = document
        .get("models")
        .and_then(|item| item.get("default"))
        .and_then(toml_edit::Item::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| {
            AppError::localized(
                "provider.grokbuild.default_model.missing",
                "Grok Build configuration is missing models.default",
            )
        })?
        .to_string();

    let selected_model = document
        .get_mut("model")
        .and_then(|item| item.get_mut(&default_model))
        .and_then(toml_edit::Item::as_table_like_mut)
        .ok_or_else(|| {
            AppError::localized(
                "provider.grokbuild.default_model.missing",
                format!("Grok Build configuration is missing [model.\"{default_model}\"]"),
            )
        })?;
    selected_model.insert(field, toml_edit::value(value));
    Ok(document.to_string())
}

pub fn apply_proxy_takeover(
    config_toml: &str,
    proxy_base_url: &str,
    token_placeholder: &str,
) -> Result<String, AppError> {
    let updated = update_selected_model_string(config_toml, "base_url", proxy_base_url)?;
    let updated = update_selected_model_string(&updated, "api_key", token_placeholder)?;
    update_selected_model_string(&updated, "api_backend", DEFAULT_API_BACKEND)
}

pub fn update_api_key(config_toml: &str, api_key: &str) -> Result<String, AppError> {
    update_selected_model_string(config_toml, "api_key", api_key)
}

pub fn update_base_url(config_toml: &str, base_url: &str) -> Result<String, AppError> {
    update_selected_model_string(config_toml, "base_url", base_url)
}

pub fn update_model(config_toml: &str, model: &str) -> Result<String, AppError> {
    update_selected_model_string(config_toml, "model", model)
}

pub fn has_proxy_placeholder(config_toml: &str, token_placeholder: &str) -> bool {
    extract_model_config(config_toml)
        .and_then(|config| config.api_key)
        .is_some_and(|api_key| api_key == token_placeholder)
}

pub fn base_url_matches(config_toml: &str, predicate: impl FnOnce(&str) -> bool) -> bool {
    extract_model_config(config_toml).is_some_and(|config| predicate(&config.base_url))
}

/// Remove MCP projections from a provider-owned Grok Build settings snapshot.
/// MCP servers are owned by the database and projected into live config.toml.
pub fn strip_grok_mcp_servers_from_settings(settings: &mut Value) -> Result<(), AppError> {
    let Some(config_text) = settings
        .get("config")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(());
    };
    if !config_text.contains("mcp") {
        return Ok(());
    }

    let mut document = config_text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| AppError::Message(format!("Invalid Grok Build config.toml: {error}")))?;
    let mut changed = document.as_table_mut().remove("mcp_servers").is_some();
    if let Some(mcp_table) = document
        .get_mut("mcp")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        if mcp_table.remove("servers").is_some() {
            changed = true;
        }
        if mcp_table.is_empty() {
            document.as_table_mut().remove("mcp");
        }
    }

    if changed {
        if let Some(object) = settings.as_object_mut() {
            object.insert("config".to_string(), Value::String(document.to_string()));
        }
    }
    Ok(())
}

/// Read the live `~/.grok/config.toml` as a provider settings snapshot.
///
/// TOML syntax validation only: when live is in the official state (no custom model table)
/// it must still be readable, for switch write-back and UI display. Import paths that need a
/// "complete custom model config" add `validate_config_toml` themselves.
pub fn read_grok_live_settings() -> Result<Value, AppError> {
    let path = get_grok_config_path();
    if !path.exists() {
        return Err(AppError::localized(
            "grokbuild.config.missing",
            "Grok Build config file does not exist",
        ));
    }

    let config = fs::read_to_string(&path).map_err(|error| AppError::io(&path, error))?;
    validate_config_toml_syntax(&config)?;
    Ok(json!({ "config": config }))
}

pub fn write_grok_provider_live(provider: &Provider) -> Result<(), AppError> {
    let settings = provider.settings_config.as_object().ok_or_else(|| {
        AppError::localized(
            "provider.grokbuild.settings.not_object",
            "Grok Build configuration must be a JSON object",
        )
    })?;
    let config = settings
        .get("config")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::localized(
                "provider.grokbuild.config.missing",
                "Grok Build configuration is missing the config field",
            )
        })?;

    // Official entries inject no custom model table: the snapshot is written back as is (an
    // empty file the first time), Grok CLI falls back to its built-in official models plus
    // its own OAuth login, and the MCP projection is rewritten afterwards by the switch flow.
    // Non-official providers must carry a complete custom model config.
    if provider.category.as_deref() != Some("official") {
        validate_config_toml(config)?;
    }

    write_grok_live_settings(&json!({ "config": config }))
}

/// Raw live-file writer, mirroring `read_grok_live_settings` (syntax-only).
///
/// Proxy takeover backup/restore goes through here too: official-state live (no custom model
/// table) must be writable back as is. The full shape validation is the job of the
/// non-official branch of `write_grok_provider_live`.
pub fn write_grok_live_settings(settings: &Value) -> Result<(), AppError> {
    let config = settings
        .get("config")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::localized(
                "provider.grokbuild.config.missing",
                "Grok Build configuration is missing the config field",
            )
        })?;
    validate_config_toml_syntax(config)?;
    write_text_file(&get_grok_config_path(), config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn valid_config() -> &'static str {
        r#"[models]
default = "grok-4.5"

[model."grok-4.5"]
model = "grok-4.5"
base_url = "https://example.com/v1"
name = "Example"
api_key = "secret"
api_backend = "responses"
context_window = 500000
"#
    }

    fn valid_env_key_config() -> &'static str {
        r#"[models]
default = "grok-env"

[model."grok-env"]
model = "grok-4.5"
base_url = "https://example.com/v1"
name = "Example Env"
env_key = "GROK_TEST_API_KEY"
api_backend = "responses"
context_window = 500000
"#
    }

    #[test]
    fn validates_expected_config_shape() {
        validate_config_toml(valid_config()).expect("valid Grok Build config");
        validate_config_toml(valid_env_key_config()).expect("valid env_key configuration");
    }

    #[test]
    fn syntax_validation_accepts_official_snapshots() {
        validate_config_toml_syntax("").expect("empty official snapshot");
        validate_config_toml_syntax("[mcp_servers.echo]\ncommand = \"echo\"\n")
            .expect("official-mode config without model tables");
        assert!(validate_config_toml_syntax("not = [valid").is_err());
    }

    #[test]
    fn rejects_missing_selected_model_table() {
        let error = validate_config_toml("[models]\ndefault = \"grok-4.5\"\n")
            .expect_err("missing model table should fail");
        assert!(error.to_string().contains("model"));
    }

    #[test]
    fn rejects_config_without_api_key_or_env_key() {
        let config = valid_config().replace("api_key = \"secret\"\n", "");
        let error = validate_config_toml(&config).expect_err("credentials should be required");
        assert!(error.to_string().contains("api_key"));
        assert!(error.to_string().contains("env_key"));
    }

    #[test]
    fn extracts_selected_model_and_updates_takeover_fields() {
        let selected = extract_model_config(valid_config()).expect("selected model");
        assert_eq!(selected.profile, "grok-4.5");
        assert_eq!(selected.model, "grok-4.5");
        assert_eq!(selected.base_url, "https://example.com/v1");

        let updated = apply_proxy_takeover(
            valid_config(),
            "http://127.0.0.1:15721/grokbuild/v1",
            "PROXY_MANAGED",
        )
        .expect("takeover config");
        let selected = extract_model_config(&updated).expect("updated selected model");
        assert_eq!(selected.base_url, "http://127.0.0.1:15721/grokbuild/v1");
        assert_eq!(selected.api_key.as_deref(), Some("PROXY_MANAGED"));
        assert!(has_proxy_placeholder(&updated, "PROXY_MANAGED"));
    }

    #[test]
    fn takeover_preserves_env_key_profile_and_injects_inline_placeholder() {
        let direct_config = valid_env_key_config().replace(
            "api_backend = \"responses\"",
            "api_backend = \"chat_completions\"",
        );
        let updated = apply_proxy_takeover(
            &direct_config,
            "http://127.0.0.1:15721/grokbuild/v1",
            "PROXY_MANAGED",
        )
        .expect("takeover config");
        let selected = extract_model_config(&updated).expect("updated selected model");

        assert_eq!(selected.profile, "grok-env");
        assert_eq!(selected.env_key.as_deref(), Some("GROK_TEST_API_KEY"));
        assert_eq!(selected.api_key.as_deref(), Some("PROXY_MANAGED"));
        assert_eq!(selected.api_backend, DEFAULT_API_BACKEND);
    }

    #[test]
    #[serial]
    fn resolves_api_key_from_configured_environment_variable() {
        let original = std::env::var_os("GROK_TEST_API_KEY");
        std::env::set_var("GROK_TEST_API_KEY", "env-secret");

        let credentials = extract_credentials(valid_env_key_config()).expect("credentials");

        assert_eq!(credentials.0, "https://example.com/v1");
        assert_eq!(credentials.1, "env-secret");
        match original {
            Some(value) => std::env::set_var("GROK_TEST_API_KEY", value),
            None => std::env::remove_var("GROK_TEST_API_KEY"),
        }
    }

    /// Build a config whose `env_key` points at an unset environment variable — the case of
    /// "an indirect reference is declared but the variable does not exist", which before the
    /// fix silently fell back to `XAI_API_KEY`.
    fn env_key_unset_config() -> &'static str {
        r#"[models]
default = "grok-env"

[model."grok-env"]
model = "grok-4.5"
base_url = "https://attacker.example/v1"
name = "Attacker Env"
env_key = "GROK_TEST_DEFINITELY_UNSET_VAR"
api_backend = "responses"
context_window = 500000
"#
    }

    #[test]
    #[serial]
    fn does_not_fall_back_to_xai_api_key_when_declared_env_key_is_unset() {
        // Even when XAI_API_KEY happens to be set in the process, it must not be silently borrowed for a different base_url.
        let original_xai = std::env::var_os("XAI_API_KEY");
        let original_unset = std::env::var_os("GROK_TEST_DEFINITELY_UNSET_VAR");
        std::env::set_var("XAI_API_KEY", "xai-secret-should-not-leak");
        std::env::remove_var("GROK_TEST_DEFINITELY_UNSET_VAR");

        let credentials = extract_credentials(env_key_unset_config());

        assert!(
            credentials.is_none(),
            "declared env_key unset must yield None, never a borrowed XAI_API_KEY; got {credentials:?}"
        );

        match original_xai {
            Some(value) => std::env::set_var("XAI_API_KEY", value),
            None => std::env::remove_var("XAI_API_KEY"),
        }
        match original_unset {
            Some(value) => std::env::set_var("GROK_TEST_DEFINITELY_UNSET_VAR", value),
            None => std::env::remove_var("GROK_TEST_DEFINITELY_UNSET_VAR"),
        }
    }

    #[test]
    fn strips_projected_mcp_servers_without_touching_model_config() {
        let mut settings = json!({
            "config": format!(
                "{}\n[mcp_servers.echo]\ncommand = \"echo\"\n",
                valid_config()
            )
        });

        strip_grok_mcp_servers_from_settings(&mut settings).expect("strip MCP servers");

        let config = settings.get("config").and_then(Value::as_str).unwrap();
        assert!(!config.contains("mcp_servers"));
        assert!(config.contains("model = \"grok-4.5\""));
        validate_config_toml(config).expect("stripped config remains valid");
    }

    #[test]
    #[serial]
    fn official_provider_roundtrips_without_custom_model_tables() {
        let temp = TempDir::new().expect("temp dir");
        let original_test_home = std::env::var_os("AI_MANAGER_TEST_HOME");
        std::env::set_var("AI_MANAGER_TEST_HOME", temp.path());

        // Official entry: an empty config is writable (it clears the custom model table and hands control back to the Grok CLI official login)
        let mut official = Provider::with_id(
            "grokbuild-official".to_string(),
            "Grok Official".to_string(),
            json!({ "config": "" }),
            None,
        );
        official.category = Some("official".to_string());
        write_grok_provider_live(&official).expect("official empty config is writable");
        assert_eq!(
            fs::read_to_string(get_grok_config_path()).expect("read config"),
            ""
        );

        // Official-state live (e.g. after the MCP projection was rewritten) has no custom model table; reading and writing it back as is must both work
        let official_live = "[mcp_servers.echo]\ncommand = \"echo\"\n";
        write_grok_live_settings(&json!({ "config": official_live }))
            .expect("official-mode live is writable for backup restore");
        let settings = read_grok_live_settings().expect("official-mode live is readable");
        assert_eq!(
            settings.get("config").and_then(Value::as_str),
            Some(official_live)
        );

        // Non-official providers still require a complete custom model config
        let custom = Provider::with_id(
            "custom".to_string(),
            "Custom".to_string(),
            json!({ "config": "" }),
            None,
        );
        assert!(write_grok_provider_live(&custom).is_err());

        match original_test_home {
            Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
            None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
        }
    }

    #[test]
    #[serial]
    fn writes_and_reads_live_config() {
        let temp = TempDir::new().expect("temp dir");
        let original_test_home = std::env::var_os("AI_MANAGER_TEST_HOME");
        std::env::set_var("AI_MANAGER_TEST_HOME", temp.path());

        let provider = Provider::with_id(
            "grok".to_string(),
            "Example".to_string(),
            json!({ "config": valid_config() }),
            None,
        );
        write_grok_provider_live(&provider).expect("write live config");

        let path = get_grok_config_path();
        assert_eq!(path, temp.path().join(".grok").join("config.toml"));
        assert_eq!(
            fs::read_to_string(path).expect("read config"),
            valid_config()
        );
        assert_eq!(
            read_grok_live_settings()
                .expect("read live settings")
                .get("config")
                .and_then(Value::as_str),
            Some(valid_config())
        );

        match original_test_home {
            Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
            None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
        }
    }
}
