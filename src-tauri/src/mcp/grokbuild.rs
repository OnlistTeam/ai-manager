//! Grok Build MCP synchronization and import.
//!
//! Grok Build uses the same top-level `[mcp_servers]` TOML layout as Codex,
//! stored alongside its model configuration in `~/.grok/config.toml`.

use serde_json::{json, Value};

use crate::app_config::{McpApps, McpServer, MultiAppConfig};
use crate::error::AppError;

use super::codex::json_server_to_toml_table;
use super::validation::validate_server_spec;

fn should_sync_grokbuild_mcp() -> bool {
    crate::grok_config::get_grok_config_dir().exists()
}

fn read_config_text() -> Result<String, AppError> {
    let path = crate::grok_config::get_grok_config_path();
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))
}

fn json_server_to_grokbuild_toml_table(server_spec: &Value) -> Result<toml_edit::Table, AppError> {
    let mut table = json_server_to_toml_table(server_spec)?;
    // Grok infers transport from `command` or `url` and uses `headers`, while
    // Codex writes an explicit `type` plus `http_headers`.
    table.remove("type");
    if let Some(headers) = table.remove("http_headers") {
        table.insert("headers", headers);
    }
    Ok(table)
}

fn toml_server_to_json(entry: &toml::value::Table) -> Value {
    fn convert(value: &toml::Value) -> Option<Value> {
        match value {
            toml::Value::String(value) => Some(json!(value)),
            toml::Value::Integer(value) => Some(json!(value)),
            toml::Value::Float(value) => Some(json!(value)),
            toml::Value::Boolean(value) => Some(json!(value)),
            toml::Value::Datetime(value) => Some(json!(value.to_string())),
            toml::Value::Array(values) => Some(Value::Array(
                values.iter().filter_map(convert).collect::<Vec<_>>(),
            )),
            toml::Value::Table(values) => Some(Value::Object(
                values
                    .iter()
                    .filter_map(|(key, value)| convert(value).map(|value| (key.clone(), value)))
                    .collect(),
            )),
        }
    }

    let mut spec = serde_json::Map::new();
    for (key, value) in entry {
        let output_key = if key == "http_headers" {
            "headers"
        } else {
            key
        };
        if let Some(value) = convert(value) {
            spec.insert(output_key.to_string(), value);
        }
    }
    let default_type = if spec.contains_key("url") {
        "http"
    } else {
        "stdio"
    };
    spec.entry("type".to_string())
        .or_insert_with(|| json!(default_type));
    Value::Object(spec)
}

pub fn import_from_grokbuild(config: &mut MultiAppConfig) -> Result<usize, AppError> {
    let text = read_config_text()?;
    if text.trim().is_empty() {
        return Ok(0);
    }
    let root: toml::Table = toml::from_str(&text).map_err(|e| {
        AppError::McpValidation(format!("Failed to parse ~/.grok/config.toml: {e}"))
    })?;
    let Some(entries) = root.get("mcp_servers").and_then(toml::Value::as_table) else {
        return Ok(0);
    };

    let servers = config
        .mcp
        .servers
        .get_or_insert_with(std::collections::HashMap::new);
    let mut changed = 0;
    for (id, entry) in entries {
        let Some(entry) = entry.as_table() else {
            continue;
        };
        let spec = toml_server_to_json(entry);
        if let Err(error) = validate_server_spec(&spec) {
            log::warn!("Skipping invalid Grok Build MCP entry '{id}': {error}");
            continue;
        }

        if let Some(existing) = servers.get_mut(id) {
            if !existing.apps.grokbuild {
                existing.apps.grokbuild = true;
                changed += 1;
            }
        } else {
            servers.insert(
                id.clone(),
                McpServer {
                    id: id.clone(),
                    name: id.clone(),
                    server: spec,
                    apps: McpApps {
                        grokbuild: true,
                        ..Default::default()
                    },
                    description: None,
                    homepage: None,
                    docs: None,
                    tags: Vec::new(),
                },
            );
            changed += 1;
        }
    }
    Ok(changed)
}

pub fn sync_single_server_to_grokbuild(
    _config: &MultiAppConfig,
    id: &str,
    server_spec: &Value,
) -> Result<(), AppError> {
    if !should_sync_grokbuild_mcp() {
        return Ok(());
    }
    use toml_edit::Item;

    let path = crate::grok_config::get_grok_config_path();
    let text = read_config_text()?;
    let mut doc = if text.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        text.parse::<toml_edit::DocumentMut>().map_err(|e| {
            AppError::McpValidation(format!("Failed to parse Grok Build config.toml: {e}"))
        })?
    };
    // If mcp_servers is missing, or present but not a table (e.g. `mcp_servers = "x"` / `[]`),
    // normalize it to an empty table so the later `doc["mcp_servers"][id] = …` does not
    // index a non-table and trigger toml_edit's IndexMut panic. A hand-written config.toml
    // cannot be trusted. The check uses the immutable `as_table_like`: borrowing mutably
    // just to test emptiness would force another get_mut below.
    if doc
        .get("mcp_servers")
        .is_none_or(|item| item.as_table_like().is_none())
    {
        // Normalizing discards the non-table value the user wrote, so leave a trace.
        if doc.get("mcp_servers").is_some_and(|item| !item.is_none()) {
            log::warn!(
                "mcp_servers in Grok Build config.toml is not a table; reset to an empty table"
            );
        }
        doc["mcp_servers"] = toml_edit::table();
    }
    let servers = doc
        .get_mut("mcp_servers")
        .and_then(toml_edit::Item::as_table_like_mut)
        .ok_or_else(|| {
            AppError::McpValidation(
                "mcp_servers in Grok Build config.toml is not a table".to_string(),
            )
        })?;
    servers.insert(
        id,
        Item::Table(json_server_to_grokbuild_toml_table(server_spec)?),
    );
    crate::config::write_text_file(&path, &doc.to_string())
}

pub fn remove_server_from_grokbuild(id: &str) -> Result<(), AppError> {
    if !should_sync_grokbuild_mcp() {
        return Ok(());
    }
    let path = crate::grok_config::get_grok_config_path();
    if !path.exists() {
        return Ok(());
    }
    let text = read_config_text()?;
    let mut doc = match text.parse::<toml_edit::DocumentMut>() {
        Ok(doc) => doc,
        Err(error) => {
            log::warn!("Failed to parse Grok Build config.toml: {error}; skipping removal");
            return Ok(());
        }
    };
    // Mirror the write path by using as_table_like_mut: for an inline table as_table_mut
    // returns None and the removal would silently no-op — the UI shows it removed while
    // Grok Build still loads it on the next start.
    if let Some(item) = doc.get_mut("mcp_servers") {
        // `Item::None` is toml_edit's placeholder, not a value the user wrote, so warning
        // about it would be noise. Must be computed before taking the mutable borrow.
        let user_authored = !item.is_none();
        match item.as_table_like_mut() {
            Some(servers) => {
                servers.remove(id);
            }
            None if user_authored => {
                log::warn!("mcp_servers in Grok Build config.toml is not a table; cannot remove server '{id}'");
            }
            None => {}
        }
    }
    crate::config::write_text_file(&path, &doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_http_headers_to_unified_headers() {
        let entry: toml::value::Table = toml::from_str(
            r#"type = "http"
url = "https://example.com/mcp"
http_headers = { Authorization = "Bearer token" }
"#,
        )
        .expect("parse table");
        let spec = toml_server_to_json(&entry);
        assert_eq!(spec["type"], "http");
        assert_eq!(spec["headers"]["Authorization"], "Bearer token");
    }

    #[test]
    fn writes_grokbuild_remote_server_without_codex_only_fields() {
        let table = json_server_to_grokbuild_toml_table(&json!({
            "type": "http",
            "url": "https://example.com/mcp",
            "headers": { "Authorization": "Bearer token" }
        }))
        .expect("convert server");

        assert!(!table.contains_key("type"));
        assert!(!table.contains_key("http_headers"));
        assert_eq!(
            table
                .get("headers")
                .and_then(toml_edit::Item::as_table)
                .and_then(|headers| headers.get("Authorization"))
                .and_then(toml_edit::Item::as_str),
            Some("Bearer token")
        );
    }
}
