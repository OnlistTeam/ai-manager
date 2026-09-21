use crate::config::{get_home_dir, write_text_file};
use crate::error::AppError;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Return the Gemini config directory path (a settings override is supported)
pub fn get_gemini_dir() -> PathBuf {
    if let Some(custom) = crate::settings::get_gemini_override_dir() {
        return custom;
    }

    get_home_dir().join(".gemini")
}

/// Return the path of the Gemini .env file
pub fn get_gemini_env_path() -> PathBuf {
    get_gemini_dir().join(".env")
}

/// Parse the contents of a .env file into key/value pairs
///
/// This parser is lenient and skips invalid lines.
/// Use `parse_env_file_strict` when strict validation is required.
pub fn parse_env_file(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse KEY=VALUE
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();

            // Validate the key (non-empty, letters, digits and underscores only)
            if !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                map.insert(key, value);
            }
        }
    }

    map
}

/// Serialize key/value pairs into .env format
pub fn serialize_env_file(map: &HashMap<String, String>) -> String {
    let mut lines = Vec::new();

    // Sort by key so the output is stable
    let mut keys: Vec<_> = map.keys().collect();
    keys.sort();

    for key in keys {
        if let Some(value) = map.get(key) {
            lines.push(format!("{key}={value}"));
        }
    }

    lines.join("\n")
}

/// Read the Gemini .env file
pub fn read_gemini_env() -> Result<HashMap<String, String>, AppError> {
    let path = get_gemini_env_path();

    if !path.exists() {
        return Ok(HashMap::new());
    }

    let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;

    Ok(parse_env_file(&content))
}

/// Delete lines from the raw .env text by matching both key name and value, keeping
/// everything else verbatim
///
/// This deliberately avoids the `parse_env_file` -> `serialize_env_file` round trip: that
/// pair drops comments, blank lines, unrecognized lines and duplicate definitions, and
/// reorders the whole file by key. That is fine for a full projection (the file is
/// rewritten anyway), but using it for a **targeted** cleanup would delete the user's
/// hand-written content along the way.
///
/// Matching by value rather than by key name removes only the leaked copy and keeps the
/// user's own line with the same key but a different value. When a key is defined more
/// than once, only the matching line is removed and the definition it shadowed becomes
/// effective again — which is exactly what we want, because the shadowing line was the
/// leaked value.
///
/// Returns `None` when no line matched, so the caller can skip writing to disk.
pub fn remove_env_entries_preserving_layout(
    content: &str,
    doomed: &HashMap<String, String>,
) -> Option<String> {
    let mut removed = false;
    let mut kept: Vec<&str> = Vec::new();

    for line in content.split('\n') {
        let trimmed = line.trim();
        let hit = !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && trimmed.split_once('=').is_some_and(|(key, value)| {
                doomed
                    .get(key.trim())
                    .is_some_and(|doomed_value| doomed_value == value.trim())
            });

        if hit {
            removed = true;
        } else {
            kept.push(line);
        }
    }

    removed.then(|| kept.join("\n"))
}

/// Delete lines from `~/.gemini/.env` whose `KEY=VALUE` matches exactly; returns whether the file actually changed
pub fn remove_gemini_env_entries(doomed: &HashMap<String, String>) -> Result<bool, AppError> {
    let path = get_gemini_env_path();
    if !path.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    match remove_env_entries_preserving_layout(&content, doomed) {
        Some(cleaned) => {
            write_gemini_env_text_atomic(&cleaned)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Write the Gemini .env file (atomically)
pub fn write_gemini_env_atomic(map: &HashMap<String, String>) -> Result<(), AppError> {
    write_gemini_env_text_atomic(&serialize_env_file(map))
}

/// Write the Gemini .env file (atomically, content persisted verbatim)
///
/// Shares directory/file permission handling with `write_gemini_env_atomic`; the only
/// difference is that the content is not normalized through `serialize_env_file` — used
/// by the order-preserving targeted deletion.
pub fn write_gemini_env_text_atomic(content: &str) -> Result<(), AppError> {
    let path = get_gemini_env_path();

    // Make sure the directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;

        // Set the directory permissions to 700 (owner read/write/execute only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(parent)
                .map_err(|e| AppError::io(parent, e))?
                .permissions();
            perms.set_mode(0o700);
            fs::set_permissions(parent, perms).map_err(|e| AppError::io(parent, e))?;
        }
    }

    write_text_file(&path, content)?;

    // Set the file permissions to 600 (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)
            .map_err(|e| AppError::io(&path, e))?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&path, perms).map_err(|e| AppError::io(&path, e))?;
    }

    Ok(())
}

/// Convert from .env format into Provider.settings_config (a JSON Value)
pub fn env_to_json(env_map: &HashMap<String, String>) -> Value {
    let mut json_map = serde_json::Map::new();

    for (key, value) in env_map {
        json_map.insert(key.clone(), Value::String(value.clone()));
    }

    serde_json::json!({ "env": json_map })
}

/// Extract the .env format from Provider.settings_config (a JSON Value)
pub fn json_to_env(settings: &Value) -> Result<HashMap<String, String>, AppError> {
    let mut env_map = HashMap::new();

    if let Some(env_obj) = settings.get("env").and_then(|v| v.as_object()) {
        for (key, value) in env_obj {
            if let Some(val_str) = value.as_str() {
                env_map.insert(key.clone(), val_str.to_string());
            }
        }
    }

    Ok(env_map)
}

/// Validate the basic structure of a Gemini config
///
/// This only validates the basic format and does not require GEMINI_API_KEY, so users can
/// create a provider config first and fill in the API key later.
///
/// The API key is validated when switching providers (via `validate_gemini_settings_strict`).
pub fn validate_gemini_settings(settings: &Value) -> Result<(), AppError> {
    // Only validate the basic structure; GEMINI_API_KEY is not required
    // When an env field is present, validate that it is an object
    if let Some(env) = settings.get("env") {
        if !env.is_object() {
            return Err(AppError::localized(
                "gemini.validation.invalid_env",
                "Gemini config invalid: env must be an object",
            ));
        }
    }

    // When a config field is present, validate that it is an object or null
    if let Some(config) = settings.get("config") {
        if !(config.is_object() || config.is_null()) {
            return Err(AppError::localized(
                "gemini.validation.invalid_config",
                "Gemini config invalid: config must be an object",
            ));
        }
    }

    Ok(())
}

/// Validate a Gemini config strictly (required fields enforced)
///
/// Used when switching providers, to ensure the config contains every required field.
/// For providers that need an API key (such as PackyCode), the GEMINI_API_KEY field is validated.
pub fn validate_gemini_settings_strict(settings: &Value) -> Result<(), AppError> {
    // Run the basic format validation first (including the env/config types)
    validate_gemini_settings(settings)?;

    let env_map = json_to_env(settings)?;

    // An empty env means OAuth is used (e.g. Google official), so skip the validation
    if env_map.is_empty() {
        return Ok(());
    }

    // When env is not empty, check the required GEMINI_API_KEY field
    if !env_map.contains_key("GEMINI_API_KEY") {
        return Err(AppError::localized(
            "gemini.validation.missing_api_key",
            "Gemini config missing required field: GEMINI_API_KEY",
        ));
    }

    Ok(())
}

/// Return the path of the Gemini settings.json file
///
/// Returned path: `~/.gemini/settings.json` (next to the `.env` file)
pub fn get_gemini_settings_path() -> PathBuf {
    get_gemini_dir().join("settings.json")
}

/// Update the security.auth.selectedType field in the Gemini directory's settings.json
///
/// This function:
/// 1. Reads the existing settings.json (when present)
/// 2. Updates only the `security.auth.selectedType` field, keeping every other field
/// 3. Writes the file atomically
///
/// # Parameters
/// - `selected_type`: the selectedType value to set (e.g. "gemini-api-key" or "oauth-personal")
fn update_selected_type(selected_type: &str) -> Result<(), AppError> {
    let settings_path = get_gemini_settings_path();

    // Make sure the directory exists
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }

    // Read the existing settings.json (when present)
    let mut settings_content = if settings_path.exists() {
        let content =
            fs::read_to_string(&settings_path).map_err(|e| AppError::io(&settings_path, e))?;
        serde_json::from_str::<Value>(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Update only the security.auth.selectedType field
    if let Some(obj) = settings_content.as_object_mut() {
        let security = obj
            .entry("security")
            .or_insert_with(|| serde_json::json!({}));

        if let Some(security_obj) = security.as_object_mut() {
            let auth = security_obj
                .entry("auth")
                .or_insert_with(|| serde_json::json!({}));

            if let Some(auth_obj) = auth.as_object_mut() {
                auth_obj.insert(
                    "selectedType".to_string(),
                    Value::String(selected_type.to_string()),
                );
            }
        }
    }

    // Write the file
    crate::config::write_json_file(&settings_path, &settings_content)?;

    Ok(())
}

/// Write settings.json for a Packycode Gemini provider
///
/// Sets the following in `~/.gemini/settings.json`:
/// ```json
/// {
///   "security": {
///     "auth": {
///       "selectedType": "gemini-api-key"
///     }
///   }
/// }
/// ```
///
/// Every other field in the file is preserved.
pub fn write_packycode_settings() -> Result<(), AppError> {
    update_selected_type("gemini-api-key")
}

/// Write settings.json for the official Google Gemini provider (OAuth mode)
///
/// Sets the following in `~/.gemini/settings.json`:
/// ```json
/// {
///   "security": {
///     "auth": {
///       "selectedType": "oauth-personal"
///     }
///   }
/// }
/// ```
///
/// Every other field in the file is preserved.
pub fn write_google_oauth_settings() -> Result<(), AppError> {
    update_selected_type("oauth-personal")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_file() {
        let content = r#"
# Comment line
GOOGLE_GEMINI_BASE_URL=https://example.com
GEMINI_API_KEY=sk-test123
GEMINI_MODEL=gemini-3.5-flash

# Another comment
"#;

        let map = parse_env_file(content);

        assert_eq!(map.len(), 3);
        assert_eq!(
            map.get("GOOGLE_GEMINI_BASE_URL"),
            Some(&"https://example.com".to_string())
        );
        assert_eq!(map.get("GEMINI_API_KEY"), Some(&"sk-test123".to_string()));
        assert_eq!(
            map.get("GEMINI_MODEL"),
            Some(&"gemini-3.5-flash".to_string())
        );
    }

    #[test]
    fn test_serialize_env_file() {
        let mut map = HashMap::new();
        map.insert("GEMINI_API_KEY".to_string(), "sk-test".to_string());
        map.insert("GEMINI_MODEL".to_string(), "gemini-3.5-flash".to_string());

        let content = serialize_env_file(&map);

        assert!(content.contains("GEMINI_API_KEY=sk-test"));
        assert!(content.contains("GEMINI_MODEL=gemini-3.5-flash"));
    }

    #[test]
    fn test_env_json_conversion() {
        let mut env_map = HashMap::new();
        env_map.insert("GEMINI_API_KEY".to_string(), "test-key".to_string());

        let json = env_to_json(&env_map);
        let converted = json_to_env(&json).unwrap();

        assert_eq!(
            converted.get("GEMINI_API_KEY"),
            Some(&"test-key".to_string())
        );
    }

    #[test]
    fn test_packycode_settings_structure() {
        // Verify the structure of the Packycode settings.json
        let settings_content = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "gemini-api-key"
                }
            }
        });

        assert_eq!(
            settings_content["security"]["auth"]["selectedType"],
            "gemini-api-key"
        );
    }

    #[test]
    fn test_packycode_settings_merge() {
        // Test the merge logic: other fields must be preserved
        let mut existing_settings = serde_json::json!({
            "otherField": "should-be-kept",
            "security": {
                "otherSetting": "also-kept",
                "auth": {
                    "otherAuth": "preserved"
                }
            }
        });

        // Simulate updating selectedType
        if let Some(obj) = existing_settings.as_object_mut() {
            let security = obj
                .entry("security")
                .or_insert_with(|| serde_json::json!({}));

            if let Some(security_obj) = security.as_object_mut() {
                let auth = security_obj
                    .entry("auth")
                    .or_insert_with(|| serde_json::json!({}));

                if let Some(auth_obj) = auth.as_object_mut() {
                    auth_obj.insert(
                        "selectedType".to_string(),
                        Value::String("gemini-api-key".to_string()),
                    );
                }
            }
        }

        // Verify every field was preserved
        assert_eq!(existing_settings["otherField"], "should-be-kept");
        assert_eq!(existing_settings["security"]["otherSetting"], "also-kept");
        assert_eq!(
            existing_settings["security"]["auth"]["otherAuth"],
            "preserved"
        );
        assert_eq!(
            existing_settings["security"]["auth"]["selectedType"],
            "gemini-api-key"
        );
    }

    #[test]
    fn test_google_oauth_settings_structure() {
        // Verify the structure of the Google OAuth settings.json
        let settings_content = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "oauth-personal"
                }
            }
        });

        assert_eq!(
            settings_content["security"]["auth"]["selectedType"],
            "oauth-personal"
        );
    }

    #[test]
    fn test_validate_empty_env_for_oauth() {
        // An empty env (official Google OAuth) passes the basic validation
        let settings = serde_json::json!({
            "env": {}
        });

        assert!(validate_gemini_settings(&settings).is_ok());
        // Strict validation must pass as well (an empty env means OAuth)
        assert!(validate_gemini_settings_strict(&settings).is_ok());
    }

    #[test]
    fn test_validate_env_with_api_key() {
        // A config with an API key passes validation
        let settings = serde_json::json!({
            "env": {
                "GEMINI_API_KEY": "sk-test123",
                "GEMINI_MODEL": "gemini-3.5-flash"
            }
        });

        assert!(validate_gemini_settings(&settings).is_ok());
        assert!(validate_gemini_settings_strict(&settings).is_ok());
    }

    #[test]
    fn test_validate_env_without_api_key_relaxed() {
        // A non-empty config missing the API key still passes the basic validation (the user fills it in later)
        let settings = serde_json::json!({
            "env": {
                "GEMINI_MODEL": "gemini-3.5-flash"
            }
        });

        // Basic validation must pass (the API key may be filled in later)
        assert!(validate_gemini_settings(&settings).is_ok());
        // Strict validation must fail (a complete config is required when switching)
        assert!(validate_gemini_settings_strict(&settings).is_err());
    }

    #[test]
    fn test_validate_invalid_env_type() {
        // A non-object env must fail
        let settings = serde_json::json!({
            "env": "invalid_string"
        });

        assert!(validate_gemini_settings(&settings).is_err());
    }
}
