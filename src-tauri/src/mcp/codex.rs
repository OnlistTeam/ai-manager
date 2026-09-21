//! Codex MCP sync and import module
//!
//! Contains Codex's MCP configuration management:
//! - Import from ~/.codex/config.toml
//! - Sync to ~/.codex/config.toml
//! - JSON-to-TOML conversion logic

use serde_json::{json, Value};
use std::collections::HashMap;

use crate::app_config::{McpApps, McpConfig, McpServer, MultiAppConfig};
use crate::error::AppError;

use super::validation::{extract_server_spec, validate_server_spec};

fn should_sync_codex_mcp() -> bool {
    // Codex not installed/not initialized: ~/.codex directory does not exist.
    // Per user preference: skip writes/deletes when the directory is missing, never create files or directories.
    crate::codex_config::get_codex_config_dir().exists()
}

/// Return enabled MCP servers (filtered by enabled==true)
fn collect_enabled_servers(cfg: &McpConfig) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for (id, entry) in cfg.servers.iter() {
        let enabled = entry
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !enabled {
            continue;
        }
        match extract_server_spec(entry) {
            Ok(spec) => {
                out.insert(id.clone(), spec);
            }
            Err(err) => {
                log::warn!("Skipping invalid MCP entry '{id}': {err}");
            }
        }
    }
    out
}

/// Import MCP from ~/.codex/config.toml into the unified structure (v3.7.0+)
///
/// Supported formats:
/// - Correct format: [mcp_servers.*] (Codex's official standard)
/// - Legacy format: [mcp.servers.*] (tolerant read, for migrating misconfigured entries)
///
/// Existing servers will have the Codex app enabled, without overwriting other fields or app states
pub fn import_from_codex(config: &mut MultiAppConfig) -> Result<usize, AppError> {
    let text = crate::codex_config::read_and_validate_codex_config_text()?;
    if text.trim().is_empty() {
        return Ok(0);
    }

    let root: toml::Table = toml::from_str(&text).map_err(|e| {
        AppError::McpValidation(format!("Failed to parse ~/.codex/config.toml: {e}"))
    })?;

    // Ensure the new structure exists
    let servers = config.mcp.servers.get_or_insert_with(HashMap::new);

    let mut changed_total = 0usize;

    // helper: process a servers table
    let mut import_servers_tbl = |servers_tbl: &toml::value::Table| {
        let mut changed = 0usize;
        for (id, entry_val) in servers_tbl.iter() {
            let Some(entry_tbl) = entry_val.as_table() else {
                continue;
            };

            // Codex's official config omits `type`: `command` denotes a stdio
            // server while a URL-only table denotes remote streamable HTTP.
            // Preserve explicit legacy types, and only infer HTTP when there
            // is no competing command; command-bearing or mixed legacy rows
            // retain the long-standing stdio default.
            let typ = entry_tbl
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or_else(|| {
                    if entry_tbl.get("command").is_none()
                        && entry_tbl
                            .get("url")
                            .and_then(|value| value.as_str())
                            .is_some()
                    {
                        "http"
                    } else {
                        "stdio"
                    }
                });

            // Build JSON spec
            let mut spec = serde_json::Map::new();
            spec.insert("type".into(), json!(typ));

            // Core fields (need manual handling)
            let core_fields = match typ {
                "stdio" => vec!["type", "command", "args", "env", "cwd"],
                // The unified DB spec uses `headers`, Codex TOML uses `http_headers`.
                // Both must be treated as core fields to keep auth values out of the generic logging path.
                "http" | "sse" => vec!["type", "url", "headers", "http_headers"],
                _ => vec!["type"],
            };

            // 1. Process core fields (strongly typed)
            match typ {
                "stdio" => {
                    if let Some(cmd) = entry_tbl.get("command").and_then(|v| v.as_str()) {
                        spec.insert("command".into(), json!(cmd));
                    }
                    if let Some(args) = entry_tbl.get("args").and_then(|v| v.as_array()) {
                        let arr = args
                            .iter()
                            .filter_map(|x| x.as_str())
                            .map(|s| json!(s))
                            .collect::<Vec<_>>();
                        if !arr.is_empty() {
                            spec.insert("args".into(), serde_json::Value::Array(arr));
                        }
                    }
                    if let Some(cwd) = entry_tbl.get("cwd").and_then(|v| v.as_str()) {
                        if !cwd.trim().is_empty() {
                            spec.insert("cwd".into(), json!(cwd));
                        }
                    }
                    if let Some(env_tbl) = entry_tbl.get("env").and_then(|v| v.as_table()) {
                        let mut env_json = serde_json::Map::new();
                        for (k, v) in env_tbl.iter() {
                            if let Some(sv) = v.as_str() {
                                env_json.insert(k.clone(), json!(sv));
                            }
                        }
                        if !env_json.is_empty() {
                            spec.insert("env".into(), serde_json::Value::Object(env_json));
                        }
                    }
                }
                "http" | "sse" => {
                    if let Some(url) = entry_tbl.get("url").and_then(|v| v.as_str()) {
                        spec.insert("url".into(), json!(url));
                    }
                    // Read from http_headers (correct Codex format) or headers (legacy) with priority to http_headers
                    let headers_tbl = entry_tbl
                        .get("http_headers")
                        .and_then(|v| v.as_table())
                        .or_else(|| entry_tbl.get("headers").and_then(|v| v.as_table()));

                    if let Some(headers_tbl) = headers_tbl {
                        let mut headers_json = serde_json::Map::new();
                        for (k, v) in headers_tbl.iter() {
                            if let Some(sv) = v.as_str() {
                                headers_json.insert(k.clone(), json!(sv));
                            }
                        }
                        if !headers_json.is_empty() {
                            spec.insert("headers".into(), serde_json::Value::Object(headers_json));
                        }
                    }
                }
                _ => {
                    log::warn!("Skipping Codex MCP entry '{id}' with unknown type '{typ}'");
                    return changed;
                }
            }

            // 2. Process extended fields and other unknown fields (generic TOML -> JSON conversion)
            for (key, toml_val) in entry_tbl.iter() {
                // Skip already-processed core fields
                if core_fields.contains(&key.as_str()) {
                    continue;
                }

                // Generic TOML value to JSON value conversion
                let json_val = match toml_val {
                    toml::Value::String(s) => Some(json!(s)),
                    toml::Value::Integer(i) => Some(json!(i)),
                    toml::Value::Float(f) => Some(json!(f)),
                    toml::Value::Boolean(b) => Some(json!(b)),
                    toml::Value::Array(arr) => {
                        // Only simple-type arrays are supported
                        let json_arr: Vec<serde_json::Value> = arr
                            .iter()
                            .filter_map(|item| match item {
                                toml::Value::String(s) => Some(json!(s)),
                                toml::Value::Integer(i) => Some(json!(i)),
                                toml::Value::Float(f) => Some(json!(f)),
                                toml::Value::Boolean(b) => Some(json!(b)),
                                _ => None,
                            })
                            .collect();
                        if !json_arr.is_empty() {
                            Some(serde_json::Value::Array(json_arr))
                        } else {
                            log::debug!("Skipping complex array field '{key}' (TOML -> JSON)");
                            None
                        }
                    }
                    toml::Value::Table(tbl) => {
                        // Shallow tables become JSON objects (string values only)
                        let mut json_obj = serde_json::Map::new();
                        for (k, v) in tbl.iter() {
                            if let Some(s) = v.as_str() {
                                json_obj.insert(k.clone(), json!(s));
                            }
                        }
                        if !json_obj.is_empty() {
                            Some(serde_json::Value::Object(json_obj))
                        } else {
                            log::debug!("Skipping complex object field '{key}' (TOML -> JSON)");
                            None
                        }
                    }
                    toml::Value::Datetime(_) => {
                        log::debug!("Skipping datetime field '{key}' (TOML -> JSON)");
                        None
                    }
                };

                if let Some(val) = json_val {
                    spec.insert(key.clone(), val);
                    log::debug!("Imported extended field '{key}' (value omitted)");
                }
            }

            let spec_v = serde_json::Value::Object(spec);

            // Validate: continue processing on a single-item failure
            if let Err(e) = validate_server_spec(&spec_v) {
                log::warn!("Skipping invalid Codex MCP entry '{id}': {e}");
                continue;
            }

            if let Some(existing) = servers.get_mut(id) {
                // Already exists: just enable the Codex app
                if !existing.apps.codex {
                    existing.apps.codex = true;
                    changed += 1;
                    log::info!("MCP server '{id}' now has the Codex app enabled");
                }
            } else {
                // New server: enable only Codex by default
                servers.insert(
                    id.clone(),
                    McpServer {
                        id: id.clone(),
                        name: id.clone(),
                        server: spec_v,
                        apps: McpApps {
                            claude: false,
                            codex: true,
                            gemini: false,
                            grokbuild: false,
                            opencode: false,
                            hermes: false,
                            claude_desktop: false,
                        },
                        description: None,
                        homepage: None,
                        docs: None,
                        tags: Vec::new(),
                    },
                );
                changed += 1;
                log::info!("Imported new MCP server '{id}'");
            }
        }
        changed
    };

    // 1) Process mcp.servers
    if let Some(mcp_val) = root.get("mcp") {
        if let Some(mcp_tbl) = mcp_val.as_table() {
            if let Some(servers_val) = mcp_tbl.get("servers") {
                if let Some(servers_tbl) = servers_val.as_table() {
                    changed_total += import_servers_tbl(servers_tbl);
                }
            }
        }
    }

    // 2) Process mcp_servers
    if let Some(servers_val) = root.get("mcp_servers") {
        if let Some(servers_tbl) = servers_val.as_table() {
            changed_total += import_servers_tbl(servers_tbl);
        }
    }

    Ok(changed_total)
}

/// Write Codex's enabled==true entries from config.json to ~/.codex/config.toml as TOML
///
/// Format strategy:
/// - Only correct format: top-level [mcp_servers] table (Codex's official standard)
/// - Automatically clean up the legacy format: [mcp.servers] (if present)
/// - Read the existing config.toml; error out on invalid syntax rather than overwriting
/// - Only update the `mcp_servers` table, preserve other keys
/// - Only write enabled entries; clear the mcp_servers table when none are enabled
pub fn sync_enabled_to_codex(config: &MultiAppConfig) -> Result<(), AppError> {
    if !should_sync_codex_mcp() {
        return Ok(());
    }
    use toml_edit::{Item, Table};

    // 1) Collect enabled entries (Codex dimension)
    let enabled = collect_enabled_servers(&config.mcp.codex);

    // 2) Read existing config.toml text; keep returning an error for invalid TOML (don't overwrite the file)
    let base_text = crate::codex_config::read_and_validate_codex_config_text()?;

    // 3) Parse with toml_edit (allow empty file)
    let mut doc = if base_text.trim().is_empty() {
        toml_edit::DocumentMut::default()
    } else {
        base_text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| AppError::McpValidation(format!("Failed to parse config.toml: {e}")))?
    };

    // 4) Clean up the legacy format [mcp.servers] if present
    if let Some(mcp_item) = doc.get_mut("mcp") {
        if let Some(tbl) = mcp_item.as_table_like_mut() {
            if tbl.contains_key("servers") {
                log::warn!("Detected the legacy MCP format [mcp.servers], cleaning it up and migrating to [mcp_servers]");
                tbl.remove("servers");
            }
        }
    }

    // 5) Build the target servers table (stable key order)
    if enabled.is_empty() {
        // No enabled entries: remove the mcp_servers table
        doc.as_table_mut().remove("mcp_servers");
    } else {
        // Build the servers table
        let mut servers_tbl = Table::new();
        let mut ids: Vec<_> = enabled.keys().cloned().collect();
        ids.sort();
        for id in ids {
            let spec = enabled.get(&id).expect("spec must exist");
            // Reuse the generic conversion function (already supports extended fields)
            match json_server_to_toml_table(spec) {
                Ok(table) => {
                    servers_tbl[&id[..]] = Item::Table(table);
                }
                Err(err) => {
                    log::error!("Skipping invalid MCP server '{id}': {err}");
                }
            }
        }
        // Use the only correct format: [mcp_servers]
        doc["mcp_servers"] = Item::Table(servers_tbl);
    }

    // 6) Write back (TOML only, never touches auth.json); toml_edit tries to preserve comments/whitespace/order in untouched regions
    let new_text = doc.to_string();
    let path = crate::codex_config::get_codex_config_path();
    crate::config::write_text_file(&path, &new_text)?;
    Ok(())
}

/// Sync a single MCP server to the live Codex configuration
/// Always use Codex's official format [mcp_servers], and clean up the legacy format [mcp.servers] if present
/// Write a single MCP server table into `[mcp_servers]`, ensuring the key is a "table".
///
/// `~/.codex/config.toml` can be hand-edited by the user: if `mcp_servers` exists but is not a table
/// (e.g. `mcp_servers = "x"` / `[]`), a mere `contains_key` check would skip rebuilding it, and the
/// subsequent `doc["mcp_servers"][id] = …` would trigger toml_edit's `IndexMut` panic
/// (the panic happens inside a Tauri command, unwinding across FFI). We normalize first, then insert.
fn upsert_mcp_server_table(
    doc: &mut toml_edit::DocumentMut,
    id: &str,
    table: toml_edit::Table,
) -> Result<(), AppError> {
    if doc
        .get_mut("mcp_servers")
        .and_then(toml_edit::Item::as_table_like_mut)
        .is_none()
    {
        // If the key exists but isn't a table, normalizing it discards whatever the user hand-wrote — this must leave a trace,
        // otherwise the user would just see their edit vanish without explanation.
        if doc.get("mcp_servers").is_some_and(|item| !item.is_none()) {
            log::warn!("config.toml's mcp_servers is not a table; reset it to an empty table");
        }
        doc["mcp_servers"] = toml_edit::table();
    }
    let servers = doc
        .get_mut("mcp_servers")
        .and_then(toml_edit::Item::as_table_like_mut)
        .ok_or_else(|| {
            AppError::McpValidation("config.toml's mcp_servers is not a table".to_string())
        })?;
    servers.insert(id, toml_edit::Item::Table(table));
    Ok(())
}

/// Remove a single MCP server from `[mcp_servers]` (and the historical legacy format `[mcp.servers]`).
///
/// Symmetric with `upsert_mcp_server_table` in using `as_table_like_mut`: if the user wrote the config as an
/// inline table (`mcp_servers = { foo = {...} }`, which is valid TOML), `as_table_mut` would return
/// None, causing the removal to **silently fail** — the UI says it was removed, but the entry is still in the
/// file, and Codex loads it again on its next launch. This is more insidious than a panic, since users usually
/// come here to disable an MCP precisely because they found a problem with it.
///
/// Kept separate from the write path as a pure doc-level function, so the guard can be unit-tested without a real `~/.codex/config.toml`.
fn remove_mcp_server_from_doc(doc: &mut toml_edit::DocumentMut, id: &str) {
    if let Some(item) = doc.get_mut("mcp_servers") {
        // `Item::None` is toml_edit's placeholder state, not a value the user wrote — warning about it would be noise.
        // Must be computed before taking the mutable borrow.
        let user_authored = !item.is_none();
        match item.as_table_like_mut() {
            Some(mcp_servers) => {
                mcp_servers.remove(id);
            }
            None if user_authored => {
                log::warn!("config.toml's mcp_servers is not a table; cannot remove server '{id}'");
            }
            None => {}
        }
    }

    // Also clean up data that may exist in the legacy location: [mcp.servers] (if present)
    if let Some(mcp_table) = doc.get_mut("mcp").and_then(|t| t.as_table_like_mut()) {
        if let Some(servers) = mcp_table
            .get_mut("servers")
            .and_then(|s| s.as_table_like_mut())
        {
            if servers.remove(id).is_some() {
                log::warn!("Cleaned up server '{id}' from the legacy MCP format [mcp.servers]");
            }
        }
    }
}

pub fn sync_single_server_to_codex(
    _config: &MultiAppConfig,
    id: &str,
    server_spec: &Value,
) -> Result<(), AppError> {
    if !should_sync_codex_mcp() {
        return Ok(());
    }

    // Read the existing config.toml
    let config_path = crate::codex_config::get_codex_config_path();

    let mut doc = if config_path.exists() {
        let content =
            std::fs::read_to_string(&config_path).map_err(|e| AppError::io(&config_path, e))?;
        // A parse failure must be an error, not silently replaced with an empty document: writing back an empty
        // document would wipe out the user's other sections in config.toml (model/model_providers/comments, etc.) entirely
        content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| AppError::McpValidation(format!("Failed to parse config.toml: {e}")))?
    } else {
        toml_edit::DocumentMut::new()
    };

    // Clean up the legacy format [mcp.servers] if present
    if let Some(mcp_item) = doc.get_mut("mcp") {
        if let Some(tbl) = mcp_item.as_table_like_mut() {
            if tbl.contains_key("servers") {
                log::warn!("Detected the legacy MCP format [mcp.servers], cleaning it up and migrating to [mcp_servers]");
                tbl.remove("servers");
            }
        }
    }

    // Convert the JSON server spec into a TOML table
    let toml_table = json_server_to_toml_table(server_spec)?;
    upsert_mcp_server_table(&mut doc, id, toml_table)?;

    // Write back the file
    let new_text = doc.to_string();
    crate::config::write_text_file(&config_path, &new_text)?;

    Ok(())
}

/// Remove a single MCP server from the live Codex configuration
/// Delete from the correct [mcp_servers] table, and also clean up data that may exist in the legacy location [mcp.servers]
pub fn remove_server_from_codex(id: &str) -> Result<(), AppError> {
    if !should_sync_codex_mcp() {
        return Ok(());
    }
    let config_path = crate::codex_config::get_codex_config_path();

    if !config_path.exists() {
        return Ok(()); // File doesn't exist, nothing to delete
    }

    let content =
        std::fs::read_to_string(&config_path).map_err(|e| AppError::io(&config_path, e))?;

    // Try to parse the existing config; return directly on failure (nothing to delete anyway)
    let mut doc = match content.parse::<toml_edit::DocumentMut>() {
        Ok(doc) => doc,
        Err(e) => {
            log::warn!("Failed to parse Codex config.toml: {e}, skipping the delete operation");
            return Ok(());
        }
    };

    remove_mcp_server_from_doc(&mut doc, id);

    // Write back the file
    let new_text = doc.to_string();
    crate::config::write_text_file(&config_path, &new_text)?;

    Ok(())
}

// ============================================================================
// TOML conversion helpers
// ============================================================================

/// Generic JSON-value-to-TOML-value converter (supports simple types and shallow nesting)
///
/// Supported type conversions:
/// - String → TOML String
/// - Number (i64) → TOML Integer
/// - Number (f64) → TOML Float
/// - Boolean → TOML Boolean
/// - Array[simple type] -> TOML Array
/// - Object -> TOML Inline Table (string values only)
///
/// Unsupported types (returns None):
/// - null
/// - deeply nested objects
/// - mixed-type arrays
fn json_value_to_toml_item(value: &Value, field_name: &str) -> Option<toml_edit::Item> {
    use toml_edit::{Array, InlineTable, Item};

    match value {
        Value::String(s) => Some(toml_edit::value(s.as_str())),

        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(toml_edit::value(i))
            } else if let Some(f) = n.as_f64() {
                Some(toml_edit::value(f))
            } else {
                log::warn!("Skipping field '{field_name}': unconvertible number type {n}");
                None
            }
        }

        Value::Bool(b) => Some(toml_edit::value(*b)),

        Value::Array(arr) => {
            // Only simple-type arrays are supported (strings, numbers, booleans)
            let mut toml_arr = Array::default();
            let mut all_same_type = true;

            for item in arr {
                match item {
                    Value::String(s) => toml_arr.push(s.as_str()),
                    Value::Number(n) if n.is_i64() => {
                        if let Some(i) = n.as_i64() {
                            toml_arr.push(i);
                        } else {
                            all_same_type = false;
                            break;
                        }
                    }
                    Value::Number(n) if n.is_f64() => {
                        if let Some(f) = n.as_f64() {
                            toml_arr.push(f);
                        } else {
                            all_same_type = false;
                            break;
                        }
                    }
                    Value::Bool(b) => toml_arr.push(*b),
                    _ => {
                        all_same_type = false;
                        break;
                    }
                }
            }

            if all_same_type && !toml_arr.is_empty() {
                Some(Item::Value(toml_edit::Value::Array(toml_arr)))
            } else {
                log::warn!("Skipping field '{field_name}': unsupported array type (mixed types or nested structures)");
                None
            }
        }

        Value::Object(obj) => {
            // Only shallow objects are supported (all values must be strings) -> TOML Inline Table
            let mut inline_table = InlineTable::new();
            let mut all_strings = true;

            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    // InlineTable needs a Value type; toml_edit::value() returns an Item, so we need to extract the inner Value
                    inline_table.insert(k, s.into());
                } else {
                    all_strings = false;
                    break;
                }
            }

            if all_strings && !inline_table.is_empty() {
                Some(Item::Value(toml_edit::Value::InlineTable(inline_table)))
            } else {
                log::warn!("Skipping field '{field_name}': object value contains a non-string type, consider using sub-table syntax");
                None
            }
        }

        Value::Null => {
            log::debug!("Skipping field '{field_name}': TOML does not support null values");
            None
        }
    }
}

/// Helper: convert a JSON MCP server spec into a toml_edit::Table
///
/// Strategy:
/// 1. Core fields (type, command, args, url, headers, env, cwd) are strongly typed
/// 2. Extended fields (timeout, retry, etc.) are auto-converted via an allowlist
/// 3. Other unknown fields are attempted via the generic converter
pub(super) fn json_server_to_toml_table(spec: &Value) -> Result<toml_edit::Table, AppError> {
    use toml_edit::{Array, Item, Table};

    let mut t = Table::new();
    let typ = spec.get("type").and_then(|v| v.as_str()).unwrap_or("stdio");
    t["type"] = toml_edit::value(typ);

    // Define core fields (already handled below, skip generic conversion)
    let core_fields = match typ {
        "stdio" => vec!["type", "command", "args", "env", "cwd"],
        "http" | "sse" => vec!["type", "url", "headers", "http_headers"],
        _ => vec!["type"],
    };

    // Define the extended field allowlist (common optional Codex fields)
    let extended_fields = [
        // Common fields
        "timeout",
        "timeout_ms",
        "startup_timeout_ms",
        "startup_timeout_sec",
        "connection_timeout",
        "read_timeout",
        "debug",
        "log_level",
        "disabled",
        // stdio-specific
        "shell",
        "encoding",
        "working_dir",
        "restart_on_exit",
        "max_restart_count",
        // http/sse-specific
        "retry_count",
        "max_retry_attempts",
        "retry_delay",
        "cache_tools_list",
        "verify_ssl",
        "insecure",
        "proxy",
    ];

    // 1. Process core fields (strongly typed)
    match typ {
        "stdio" => {
            let cmd = spec.get("command").and_then(|v| v.as_str()).unwrap_or("");
            t["command"] = toml_edit::value(cmd);

            if let Some(args) = spec.get("args").and_then(|v| v.as_array()) {
                let mut arr_v = Array::default();
                for a in args.iter().filter_map(|x| x.as_str()) {
                    arr_v.push(a);
                }
                if !arr_v.is_empty() {
                    t["args"] = Item::Value(toml_edit::Value::Array(arr_v));
                }
            }

            if let Some(cwd) = spec.get("cwd").and_then(|v| v.as_str()) {
                if !cwd.trim().is_empty() {
                    t["cwd"] = toml_edit::value(cwd);
                }
            }

            if let Some(env) = spec.get("env").and_then(|v| v.as_object()) {
                let mut env_tbl = Table::new();
                for (k, v) in env.iter() {
                    if let Some(s) = v.as_str() {
                        env_tbl[&k[..]] = toml_edit::value(s);
                    }
                }
                if !env_tbl.is_empty() {
                    t["env"] = Item::Table(env_tbl);
                }
            }
        }
        "http" | "sse" => {
            let url = spec.get("url").and_then(|v| v.as_str()).unwrap_or("");
            t["url"] = toml_edit::value(url);

            if let Some(headers) = spec.get("headers").and_then(|v| v.as_object()) {
                let mut h_tbl = Table::new();
                for (k, v) in headers.iter() {
                    if let Some(s) = v.as_str() {
                        h_tbl[&k[..]] = toml_edit::value(s);
                    }
                }
                if !h_tbl.is_empty() {
                    t["http_headers"] = Item::Table(h_tbl);
                }
            }
        }
        _ => {}
    }

    // 2. Process extended fields and other unknown fields
    if let Some(obj) = spec.as_object() {
        for (key, value) in obj {
            // Skip already-processed core fields
            if core_fields.contains(&key.as_str()) {
                continue;
            }

            // Try the generic converter
            if let Some(toml_item) = json_value_to_toml_item(value, key) {
                t[&key[..]] = toml_item;

                // Only log the field name: unknown fields may also carry a token / secret.
                if extended_fields.contains(&key.as_str()) {
                    log::debug!("Converted extended field '{key}' (value omitted)");
                } else {
                    log::debug!("Converted custom field '{key}' (value omitted)");
                }
            }
        }
    }

    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_normalizes_non_table_mcp_servers_without_panicking() {
        // A hand-edited config.toml: mcp_servers is a string instead of a table.
        // Before the fix, `doc["mcp_servers"][id] = …` would panic.
        for malformed in [
            "mcp_servers = \"x\"\n",
            "mcp_servers = []\n",
            "mcp_servers = 42\n",
        ] {
            let mut doc = malformed
                .parse::<toml_edit::DocumentMut>()
                .expect("fixture parses");
            let table = json_server_to_toml_table(&json!({
                "type": "stdio",
                "command": "npx"
            }))
            .expect("server table");

            upsert_mcp_server_table(&mut doc, "echo", table)
                .unwrap_or_else(|e| panic!("upsert must not fail for {malformed:?}: {e}"));

            let servers = doc
                .get("mcp_servers")
                .and_then(|item| item.as_table_like())
                .unwrap_or_else(|| panic!("mcp_servers must be normalized to a table"));
            assert!(servers.contains_key("echo"));
        }
    }

    #[test]
    fn upsert_preserves_existing_servers_in_a_valid_table() {
        let mut doc = "[mcp_servers.keep]\ncommand = \"keep\"\n"
            .parse::<toml_edit::DocumentMut>()
            .expect("fixture parses");
        let table = json_server_to_toml_table(&json!({
            "type": "stdio",
            "command": "npx"
        }))
        .expect("server table");

        upsert_mcp_server_table(&mut doc, "added", table).expect("upsert");

        let servers = doc
            .get("mcp_servers")
            .and_then(|item| item.as_table_like())
            .expect("table");
        assert!(servers.contains_key("keep"), "existing server must survive");
        assert!(servers.contains_key("added"));
    }

    #[test]
    fn remove_deletes_from_inline_table_form_too() {
        // Inline tables are valid TOML, but as_table_mut() returns None for them — using it as the guard
        // would make removal silently fail: the UI reports success, but the entry is still there, and Codex loads it again on its next launch.
        let mut doc = "mcp_servers = { drop = { command = \"x\" }, keep = { command = \"y\" } }\n"
            .parse::<toml_edit::DocumentMut>()
            .expect("fixture parses");

        remove_mcp_server_from_doc(&mut doc, "drop");

        let servers = doc
            .get("mcp_servers")
            .and_then(|item| item.as_table_like())
            .expect("mcp_servers must still be table-like");
        assert!(
            !servers.contains_key("drop"),
            "removal must work on the inline-table form"
        );
        assert!(servers.contains_key("keep"), "siblings must survive");
    }

    #[test]
    fn remove_is_a_noop_on_non_table_mcp_servers() {
        // Must neither panic nor silently wipe out a value the user hand-wrote
        let mut doc = "mcp_servers = 42\n"
            .parse::<toml_edit::DocumentMut>()
            .expect("fixture parses");

        remove_mcp_server_from_doc(&mut doc, "whatever");

        assert_eq!(doc.to_string(), "mcp_servers = 42\n");
    }

    #[test]
    fn http_headers_are_only_written_to_codex_http_headers() {
        let table = json_server_to_toml_table(&json!({
            "type": "http",
            "url": "https://mcp.example.com",
            "headers": {
                "Authorization": "Bearer top-secret",
                "X-Api-Key": "also-secret"
            },
            "timeout": 30
        }))
        .unwrap();

        let headers = table
            .get("http_headers")
            .and_then(|item| item.as_table())
            .expect("Codex http_headers table should be written");
        assert_eq!(
            headers.get("Authorization").and_then(|item| item.as_str()),
            Some("Bearer top-secret")
        );
        assert!(
            table.get("headers").is_none(),
            "legacy headers must not be emitted a second time"
        );
        assert_eq!(
            table.get("timeout").and_then(|item| item.as_integer()),
            Some(30)
        );
    }
}
