//! OpenClaw config file read/write module
//!
//! Handles reads and writes of the `~/.openclaw/openclaw.json` config file (JSON5 format).
//! OpenClaw manages providers additively, so every provider config lives in the same file.

use crate::config::atomic_write;
use crate::error::AppError;
use crate::settings::{effective_backup_retain_count, get_openclaw_override_dir};
use chrono::Local;
use indexmap::IndexMap;
use json_five::rt::parser::{
    from_str as rt_from_str, JSONKeyValuePair as RtJSONKeyValuePair,
    JSONObjectContext as RtJSONObjectContext, JSONText as RtJSONText, JSONValue as RtJSONValue,
    KeyValuePairContext as RtKeyValuePairContext,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const OPENCLAW_DEFAULT_SOURCE: &str =
    "{\n  models: {\n    mode: 'merge',\n    providers: {},\n  },\n}\n";
const OPENCLAW_TOOLS_PROFILES: &[&str] = &["minimal", "coding", "messaging", "full"];

// ============================================================================
// Path Functions
// ============================================================================

/// Return the OpenClaw config directory
///
/// Default path: `~/.openclaw/`
/// Can be overridden through settings.openclaw_config_dir
pub fn get_openclaw_dir() -> PathBuf {
    if let Some(override_dir) = get_openclaw_override_dir() {
        return override_dir;
    }

    crate::config::get_home_dir().join(".openclaw")
}

/// Return the OpenClaw config file path
///
/// Returns `~/.openclaw/openclaw.json`
pub fn get_openclaw_config_path() -> PathBuf {
    get_openclaw_dir().join("openclaw.json")
}

fn default_openclaw_config_value() -> Value {
    json!({
        "models": {
            "mode": "merge",
            "providers": {}
        }
    })
}

fn openclaw_write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// ============================================================================
// Type Definitions
// ============================================================================

/// OpenClaw health check warning
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawHealthWarning {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// OpenClaw write result
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawWriteOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<OpenClawHealthWarning>,
}

/// OpenClaw provider config (an entry under models.providers)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<OpenClawModelEntry>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// OpenClaw model entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawModelEntry {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<OpenClawModelCost>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// OpenClaw model cost config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawModelCost {
    pub input: f64,
    pub output: f64,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

// ============================================================================
// Core Read/Write Functions
// ============================================================================

/// Read the OpenClaw config file
///
/// Supports JSON5 and returns the full config JSON object
pub fn read_openclaw_config() -> Result<Value, AppError> {
    let path = get_openclaw_config_path();
    if !path.exists() {
        return Ok(default_openclaw_config_value());
    }

    let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    json5::from_str(&content)
        .map_err(|e| AppError::Config(format!("Failed to parse OpenClaw config as JSON5: {e}")))
}

struct OpenClawConfigDocument {
    path: PathBuf,
    original_source: Option<String>,
    text: RtJSONText,
}

impl OpenClawConfigDocument {
    fn load() -> Result<Self, AppError> {
        let path = get_openclaw_config_path();
        let original_source = if path.exists() {
            Some(fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?)
        } else {
            None
        };

        let source = original_source
            .clone()
            .unwrap_or_else(|| OPENCLAW_DEFAULT_SOURCE.to_string());
        let text = rt_from_str(&source).map_err(|e| {
            AppError::Config(format!(
                "Failed to parse OpenClaw config as round-trip JSON5 document: {}",
                e.message
            ))
        })?;

        Ok(Self {
            path,
            original_source,
            text,
        })
    }

    fn set_root_section(&mut self, key: &str, value: &Value) -> Result<(), AppError> {
        let RtJSONValue::JSONObject {
            key_value_pairs,
            context,
        } = &mut self.text.value
        else {
            return Err(AppError::Config(
                "OpenClaw config root must be a JSON5 object".to_string(),
            ));
        };

        if key_value_pairs.is_empty()
            && context
                .as_ref()
                .map(|ctx| ctx.wsc.0.is_empty())
                .unwrap_or(true)
        {
            *context = Some(RtJSONObjectContext {
                wsc: ("\n  ".to_string(),),
            });
        }

        let leading_ws = context
            .as_ref()
            .map(|ctx| ctx.wsc.0.clone())
            .unwrap_or_default();
        let entry_separator_ws = derive_entry_separator(&leading_ws);
        let child_indent = extract_trailing_indent(&leading_ws);
        let new_value = value_to_rt_value(value, &child_indent)?;

        if let Some(existing) = key_value_pairs
            .iter_mut()
            .find(|pair| json5_key_name(&pair.key) == Some(key))
        {
            existing.value = new_value;
            return Ok(());
        }

        let new_pair = if let Some(last_pair) = key_value_pairs.last_mut() {
            let last_ctx = ensure_kvp_context(last_pair);
            let closing_ws = if let Some(after_comma) = last_ctx.wsc.3.clone() {
                last_ctx.wsc.3 = Some(entry_separator_ws.clone());
                after_comma
            } else {
                let closing_ws = std::mem::take(&mut last_ctx.wsc.2);
                last_ctx.wsc.3 = Some(entry_separator_ws.clone());
                closing_ws
            };

            make_root_pair(key, new_value, closing_ws)
        } else {
            make_root_pair(
                key,
                new_value,
                derive_closing_ws_from_separator(&leading_ws),
            )
        };

        key_value_pairs.push(new_pair);
        Ok(())
    }

    fn save(self) -> Result<OpenClawWriteOutcome, AppError> {
        let _guard = openclaw_write_lock().lock()?;

        let current_source = if self.path.exists() {
            Some(fs::read_to_string(&self.path).map_err(|e| AppError::io(&self.path, e))?)
        } else {
            None
        };

        if current_source != self.original_source {
            return Err(AppError::Config(
                "OpenClaw config changed on disk. Please reload and try again.".to_string(),
            ));
        }

        let next_source = self.text.to_string();
        if current_source.as_deref() == Some(next_source.as_str()) {
            let warnings = scan_openclaw_health_from_value(
                &json5::from_str::<Value>(&next_source).map_err(|e| {
                    AppError::Config(format!(
                        "Failed to parse unchanged OpenClaw config as JSON5: {e}"
                    ))
                })?,
            );

            return Ok(OpenClawWriteOutcome {
                backup_path: None,
                warnings,
            });
        }

        let backup_path = current_source
            .as_ref()
            .map(|source| create_openclaw_backup(source))
            .transpose()?
            .map(|path| path.display().to_string());

        atomic_write(&self.path, next_source.as_bytes())?;

        let warnings = scan_openclaw_health_from_value(
            &json5::from_str::<Value>(&next_source).map_err(|e| {
                AppError::Config(format!(
                    "Failed to parse newly written OpenClaw config as JSON5: {e}"
                ))
            })?,
        );

        log::debug!("OpenClaw config written to {:?}", self.path);
        Ok(OpenClawWriteOutcome {
            backup_path,
            warnings,
        })
    }
}

fn write_root_section(section: &str, value: &Value) -> Result<OpenClawWriteOutcome, AppError> {
    let mut document = OpenClawConfigDocument::load()?;
    document.set_root_section(section, value)?;
    document.save()
}

fn create_openclaw_backup(source: &str) -> Result<PathBuf, AppError> {
    let backup_dir = crate::infrastructure::paths::product_data_dir()
        .join("backups")
        .join("openclaw");
    fs::create_dir_all(&backup_dir).map_err(|e| AppError::io(&backup_dir, e))?;

    let base_id = format!("openclaw_{}", Local::now().format("%Y%m%d_%H%M%S"));
    let mut filename = format!("{base_id}.json5");
    let mut backup_path = backup_dir.join(&filename);
    let mut counter = 1;

    while backup_path.exists() {
        filename = format!("{base_id}_{counter}.json5");
        backup_path = backup_dir.join(&filename);
        counter += 1;
    }

    atomic_write(&backup_path, source.as_bytes())?;
    cleanup_openclaw_backups(&backup_dir)?;
    Ok(backup_path)
}

fn cleanup_openclaw_backups(dir: &Path) -> Result<(), AppError> {
    let retain = effective_backup_retain_count();
    let mut entries = fs::read_dir(dir)
        .map_err(|e| AppError::io(dir, e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "json5" || ext == "json")
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    if entries.len() <= retain {
        return Ok(());
    }

    entries.sort_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok());
    let remove_count = entries.len().saturating_sub(retain);
    for entry in entries.into_iter().take(remove_count) {
        if let Err(err) = fs::remove_file(entry.path()) {
            log::warn!(
                "Failed to remove old OpenClaw config backup {}: {err}",
                entry.path().display()
            );
        }
    }

    Ok(())
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value
        .as_object_mut()
        .expect("value should be object after normalization")
}

fn ensure_kvp_context(pair: &mut RtJSONKeyValuePair) -> &mut RtKeyValuePairContext {
    pair.context.get_or_insert_with(|| RtKeyValuePairContext {
        wsc: (String::new(), " ".to_string(), String::new(), None),
    })
}

fn extract_trailing_indent(separator_ws: &str) -> String {
    separator_ws
        .rsplit_once('\n')
        .map(|(_, tail)| tail.to_string())
        .unwrap_or_default()
}

fn derive_closing_ws_from_separator(separator_ws: &str) -> String {
    let Some((prefix, indent)) = separator_ws.rsplit_once('\n') else {
        return String::new();
    };

    let reduced_indent = if indent.ends_with('\t') {
        &indent[..indent.len().saturating_sub(1)]
    } else if indent.ends_with("  ") {
        &indent[..indent.len().saturating_sub(2)]
    } else if indent.ends_with(' ') {
        &indent[..indent.len().saturating_sub(1)]
    } else {
        indent
    };

    format!("{prefix}\n{reduced_indent}")
}

fn derive_entry_separator(leading_ws: &str) -> String {
    if leading_ws.is_empty() {
        return String::new();
    }

    if leading_ws.contains('\n') {
        return format!("\n{}", extract_trailing_indent(leading_ws));
    }

    String::new()
}

fn value_to_rt_value(value: &Value, parent_indent: &str) -> Result<RtJSONValue, AppError> {
    // `json-five` 0.3.1 can panic when pretty-printing nested empty maps/arrays.
    // Serialize with `serde_json` instead; the resulting JSON is valid JSON5 and
    // can still be parsed back into the round-trip AST we use for insertion.
    let source = serde_json::to_string_pretty(value)
        .map_err(|e| AppError::Config(format!("Failed to serialize JSON section: {e}")))?;

    let adjusted = reindent_json5_block(&source, parent_indent);
    let text = rt_from_str(&adjusted).map_err(|e| {
        AppError::Config(format!(
            "Failed to parse generated JSON5 section: {}",
            e.message
        ))
    })?;
    Ok(text.value)
}

fn reindent_json5_block(source: &str, parent_indent: &str) -> String {
    let normalized = normalize_json_five_output(source);
    if parent_indent.is_empty() || !normalized.contains('\n') {
        return normalized;
    }

    let mut lines = normalized.lines();
    let Some(first_line) = lines.next() else {
        return String::new();
    };

    let mut result = String::from(first_line);
    for line in lines {
        result.push('\n');
        result.push_str(parent_indent);
        result.push_str(line);
    }
    result
}

fn normalize_json_five_output(source: &str) -> String {
    source.replace("\\/", "/")
}

fn make_root_pair(key: &str, value: RtJSONValue, closing_ws: String) -> RtJSONKeyValuePair {
    RtJSONKeyValuePair {
        key: make_json5_key(key),
        value,
        context: Some(RtKeyValuePairContext {
            wsc: (String::new(), " ".to_string(), closing_ws, None),
        }),
    }
}

fn make_json5_key(key: &str) -> RtJSONValue {
    if is_identifier_key(key) {
        RtJSONValue::Identifier(key.to_string())
    } else {
        RtJSONValue::DoubleQuotedString(key.to_string())
    }
}

fn is_identifier_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    matches!(first, 'a'..='z' | 'A'..='Z' | '_' | '$')
        && chars.all(|ch| matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '$'))
}

fn json5_key_name(key: &RtJSONValue) -> Option<&str> {
    match key {
        RtJSONValue::Identifier(name)
        | RtJSONValue::DoubleQuotedString(name)
        | RtJSONValue::SingleQuotedString(name) => Some(name),
        _ => None,
    }
}

fn warning(code: &str, message: impl Into<String>, path: Option<&str>) -> OpenClawHealthWarning {
    OpenClawHealthWarning {
        code: code.to_string(),
        message: message.into(),
        path: path.map(|value| value.to_string()),
    }
}

fn scan_openclaw_health_from_value(config: &Value) -> Vec<OpenClawHealthWarning> {
    let mut warnings = Vec::new();

    if let Some(profile) = config
        .get("tools")
        .and_then(|tools| tools.get("profile"))
        .and_then(Value::as_str)
    {
        if !OPENCLAW_TOOLS_PROFILES.contains(&profile) {
            warnings.push(warning(
                "invalid_tools_profile",
                format!("tools.profile uses unsupported value '{profile}'."),
                Some("tools.profile"),
            ));
        }
    }

    if config
        .get("agents")
        .and_then(|agents| agents.get("defaults"))
        .and_then(|defaults| defaults.get("timeout"))
        .is_some()
    {
        warnings.push(warning(
            "legacy_agents_timeout",
            "agents.defaults.timeout is deprecated; use agents.defaults.timeoutSeconds.",
            Some("agents.defaults.timeout"),
        ));
    }

    if let Some(value) = config.get("env").and_then(|env| env.get("vars")) {
        if !value.is_object() {
            warnings.push(warning(
                "stringified_env_vars",
                "env.vars should be an object. The current value looks stringified or malformed.",
                Some("env.vars"),
            ));
        }
    }

    if let Some(value) = config.get("env").and_then(|env| env.get("shellEnv")) {
        if !value.is_object() {
            warnings.push(warning(
                "stringified_env_shell_env",
                "env.shellEnv should be an object. The current value looks stringified or malformed.",
                Some("env.shellEnv"),
            ));
        }
    }

    warnings
}

// ============================================================================
// Provider Functions (Untyped - for raw JSON operations)
// ============================================================================

/// Return every provider config (raw JSON)
///
/// Read from `models.providers`
pub fn get_providers() -> Result<Map<String, Value>, AppError> {
    let config = read_openclaw_config()?;
    Ok(config
        .get("models")
        .and_then(|m| m.get("providers"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default())
}

/// Return a single provider config (raw JSON)
pub fn get_provider(id: &str) -> Result<Option<Value>, AppError> {
    Ok(get_providers()?.get(id).cloned())
}

/// Set a provider config (raw JSON)
///
/// Written to `models.providers`
pub fn set_provider(id: &str, provider_config: Value) -> Result<OpenClawWriteOutcome, AppError> {
    let mut full_config = read_openclaw_config()?;
    let root = ensure_object(&mut full_config);
    let models = root.entry("models".to_string()).or_insert_with(|| {
        json!({
            "mode": "merge",
            "providers": {}
        })
    });
    let providers = ensure_object(models)
        .entry("providers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    ensure_object(providers).insert(id.to_string(), provider_config);

    let models_value = root.get("models").cloned().unwrap_or_else(|| {
        json!({
            "mode": "merge",
            "providers": {}
        })
    });
    write_root_section("models", &models_value)
}

/// Delete a provider config
pub fn remove_provider(id: &str) -> Result<OpenClawWriteOutcome, AppError> {
    let mut config = read_openclaw_config()?;
    let mut removed = false;

    if let Some(providers) = config
        .get_mut("models")
        .and_then(|models| models.get_mut("providers"))
        .and_then(Value::as_object_mut)
    {
        removed = providers.remove(id).is_some();
    }

    if !removed {
        return Ok(OpenClawWriteOutcome::default());
    }

    let models_value = config.get("models").cloned().unwrap_or_else(|| {
        json!({
            "mode": "merge",
            "providers": {}
        })
    });
    write_root_section("models", &models_value)
}

// ============================================================================
// Provider Functions (Typed)
// ============================================================================

/// Return every provider config (typed)
pub fn get_typed_providers() -> Result<IndexMap<String, OpenClawProviderConfig>, AppError> {
    let providers = get_providers()?;
    let mut result = IndexMap::new();

    for (id, value) in providers {
        match serde_json::from_value::<OpenClawProviderConfig>(value.clone()) {
            Ok(config) => {
                result.insert(id, config);
            }
            Err(e) => {
                log::warn!("Failed to parse OpenClaw provider '{id}': {e}");
            }
        }
    }

    Ok(result)
}

/// Set the provider configs (typed)
pub fn set_typed_provider(
    id: &str,
    config: &OpenClawProviderConfig,
) -> Result<OpenClawWriteOutcome, AppError> {
    let value = serde_json::to_value(config).map_err(|e| AppError::JsonSerialize { source: e })?;
    set_provider(id, value)
}

// ============================================================================
// Agents Configuration Functions
// ============================================================================

// ============================================================================
// Full Agents Defaults Functions
// ============================================================================

// ============================================================================
// Env Configuration
// ============================================================================

// ============================================================================
// Tools Configuration
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::{Mutex, OnceLock};

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    fn with_test_paths<T>(source: &str, test: impl FnOnce(&Path) -> T) -> T {
        let _guard = test_guard();
        let temp = tempfile::tempdir().unwrap();
        let openclaw_dir = temp.path().join(".openclaw");
        fs::create_dir_all(&openclaw_dir).unwrap();
        let config_path = openclaw_dir.join("openclaw.json");
        fs::write(&config_path, source).unwrap();
        let old_test_home = std::env::var_os("AI_MANAGER_TEST_HOME");
        let old_home = std::env::var_os("HOME");
        std::env::set_var("AI_MANAGER_TEST_HOME", temp.path());
        std::env::set_var("HOME", temp.path());
        let result = test(&config_path);
        match old_test_home {
            Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
            None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
        }
        match old_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        result
    }

    #[test]
    fn scan_health_detects_known_openclaw_issues() {
        let config = json!({
            "tools": { "profile": "default" },
            "agents": { "defaults": { "timeout": 30 } },
            "env": { "vars": "[object Object]", "shellEnv": "oops" }
        });

        let warnings = scan_openclaw_health_from_value(&config);
        let codes = warnings
            .into_iter()
            .map(|warning| warning.code)
            .collect::<Vec<_>>();
        assert!(codes.contains(&"invalid_tools_profile".to_string()));
        assert!(codes.contains(&"legacy_agents_timeout".to_string()));
        assert!(codes.contains(&"stringified_env_vars".to_string()));
        assert!(codes.contains(&"stringified_env_shell_env".to_string()));
    }

    #[test]
    #[serial]
    fn save_detects_external_conflict() {
        let source = r#"{
  models: {
    mode: 'merge',
    providers: {},
  },
}
"#;

        with_test_paths(source, |config_path| {
            let mut document = OpenClawConfigDocument::load().unwrap();
            document
                .set_root_section("env", &json!({ "TOKEN": "value" }))
                .unwrap();

            fs::write(config_path, "{ changedExternally: true }\n").unwrap();
            let err = document.save().unwrap_err();
            assert!(err.to_string().contains("OpenClaw config changed on disk"));
        });
    }

    #[test]
    #[serial]
    fn remove_last_provider_writes_empty_providers_without_panic() {
        let source = r#"{
  models: {
    mode: 'merge',
    providers: {
      '1-copy': {
        api: 'anthropic-messages',
      },
    },
  },
}
"#;

        with_test_paths(source, |_| {
            let outcome = remove_provider("1-copy").unwrap();
            assert!(outcome.backup_path.is_some());

            let config = read_openclaw_config().unwrap();
            let providers = config
                .get("models")
                .and_then(|models| models.get("providers"))
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();

            assert!(providers.is_empty());

            let written = fs::read_to_string(get_openclaw_config_path()).unwrap();
            assert!(written.contains("\"providers\": {}"));
        });
    }
}
