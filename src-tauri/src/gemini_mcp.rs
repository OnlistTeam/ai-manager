use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::atomic_write;
use crate::error::AppError;
use crate::gemini_config::get_gemini_settings_path;

/// Return the Gemini MCP config file path (~/.gemini/settings.json)
fn user_config_path() -> PathBuf {
    get_gemini_settings_path()
}

fn read_json_value(path: &Path) -> Result<Value, AppError> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = fs::read_to_string(path).map_err(|e| AppError::io(path, e))?;
    let value: Value = serde_json::from_str(&content).map_err(|e| AppError::json(path, e))?;
    Ok(value)
}

fn write_json_value(path: &Path, value: &Value) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }
    let json =
        serde_json::to_string_pretty(value).map_err(|e| AppError::JsonSerialize { source: e })?;
    atomic_write(path, json.as_bytes())
}

/// Read the mcpServers mapping from the Gemini settings.json
///
/// Performs the reverse format conversion to stay compatible with the unified MCP structure:
/// - httpUrl → url + type: "http"
/// - only a url field -> add type: "sse" (Gemini infers the transport from the field name)
/// - only a command field -> add type: "stdio"
pub fn read_mcp_servers_map() -> Result<std::collections::HashMap<String, Value>, AppError> {
    let path = user_config_path();
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
    }

    let root = read_json_value(&path)?;
    let mut servers: std::collections::HashMap<String, Value> = root
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    // Reverse format conversion: Gemini-specific format -> unified MCP format
    for (_, spec) in servers.iter_mut() {
        if let Some(obj) = spec.as_object_mut() {
            // httpUrl → url + type: "http"
            if let Some(http_url) = obj.remove("httpUrl") {
                obj.insert("url".to_string(), http_url);
                obj.insert("type".to_string(), Value::String("http".to_string()));
            }

            // Gemini CLI does not use a type field: fill it in to match the unified structure, which makes validation and import easier
            if obj.get("type").is_none() {
                if obj.contains_key("command") {
                    obj.insert("type".to_string(), Value::String("stdio".to_string()));
                } else if obj.contains_key("url") {
                    obj.insert("type".to_string(), Value::String("sse".to_string()));
                }
            }
        }
    }

    Ok(servers)
}

/// Write the given mapping of enabled MCP servers into the mcpServers field of the Gemini settings.json
/// Only mcpServers is overwritten; every other field stays unchanged
pub fn set_mcp_servers_map(
    servers: &std::collections::HashMap<String, Value>,
) -> Result<(), AppError> {
    let path = user_config_path();
    let mut root = if path.exists() {
        read_json_value(&path)?
    } else {
        serde_json::json!({})
    };

    // Build the mcpServers object: drop the UI helper fields (enabled/source) and keep only the actual MCP spec
    let mut out: Map<String, Value> = Map::new();
    for (id, spec) in servers.iter() {
        let mut obj = if let Some(map) = spec.as_object() {
            map.clone()
        } else {
            return Err(AppError::McpValidation(format!(
                "MCP server '{id}' is not an object"
            )));
        };

        // Extract the server field (when present)
        if let Some(server_val) = obj.remove("server") {
            let server_obj = server_val.as_object().cloned().ok_or_else(|| {
                AppError::McpValidation(format!(
                    "The server field of MCP server '{id}' is not an object"
                ))
            })?;
            obj = server_obj;
        }

        // Gemini CLI format conversion:
        // - Gemini does not use a "type" field (it infers the transport from the field name)
        // - HTTP uses the "httpUrl" field while SSE uses the "url" field
        let transport_type = obj.get("type").and_then(|v| v.as_str());
        if transport_type == Some("http") {
            // HTTP streaming: rename "url" to "httpUrl"
            if let Some(url_value) = obj.remove("url") {
                obj.insert("httpUrl".to_string(), url_value);
            }
        }
        // SSE keeps the "url" field unchanged

        // Drop the UI helper fields and the type field (Gemini does not need them)
        obj.remove("type");
        obj.remove("enabled");
        obj.remove("source");
        obj.remove("id");
        obj.remove("name");
        obj.remove("description");
        obj.remove("tags");
        obj.remove("homepage");
        obj.remove("docs");

        // Timeout conversion: Claude/Codex use startup_timeout_sec/tool_timeout_sec
        // Gemini CLI only supports timeout (in ms)
        // Defaults: startup=10s, tool=60s
        const DEFAULT_STARTUP_MS: u64 = 10_000;
        const DEFAULT_TOOL_MS: u64 = 60_000;

        let extract_timeout =
            |obj: &mut Map<String, Value>, key: &str, multiplier: u64| -> Option<u64> {
                obj.remove(key).and_then(|val| {
                    val.as_u64()
                        .map(|n| n * multiplier)
                        .or_else(|| val.as_f64().map(|f| (f * multiplier as f64) as u64))
                })
            };

        // Collect the startup and tool timeouts separately, using the defaults when unset
        let startup_ms = extract_timeout(&mut obj, "startup_timeout_sec", 1000)
            .or_else(|| extract_timeout(&mut obj, "startup_timeout_ms", 1))
            .unwrap_or(DEFAULT_STARTUP_MS);
        let tool_ms = extract_timeout(&mut obj, "tool_timeout_sec", 1000)
            .or_else(|| extract_timeout(&mut obj, "tool_timeout_ms", 1))
            .unwrap_or(DEFAULT_TOOL_MS);

        // Take the maximum as the Gemini timeout
        let final_timeout = startup_ms.max(tool_ms);
        obj.insert("timeout".to_string(), Value::Number(final_timeout.into()));

        out.insert(id.clone(), Value::Object(obj));
    }

    {
        let obj = root.as_object_mut().ok_or_else(|| {
            AppError::Config("The root of ~/.gemini/settings.json must be an object".into())
        })?;
        obj.insert("mcpServers".into(), Value::Object(out));
    }

    write_json_value(&path, &root)?;
    Ok(())
}
