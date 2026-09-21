//! Gemini MCP sync and import module

use serde_json::Value;
use std::collections::HashMap;

use crate::app_config::{McpApps, McpConfig, McpServer, MultiAppConfig};
use crate::error::AppError;

use super::validation::{extract_server_spec, validate_server_spec};

fn should_sync_gemini_mcp() -> bool {
    // When Gemini is not installed/initialized the ~/.gemini directory does not exist.
    // Per user preference: skip writes/removals when it is missing, never create files or dirs.
    crate::gemini_config::get_gemini_dir().exists()
}

/// Return the enabled MCP servers (filtered by enabled == true)
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

/// Write the entries enabled for Gemini in config.json into the Gemini MCP config
pub fn sync_enabled_to_gemini(config: &MultiAppConfig) -> Result<(), AppError> {
    if !should_sync_gemini_mcp() {
        return Ok(());
    }
    let enabled = collect_enabled_servers(&config.mcp.gemini);
    crate::gemini_mcp::set_mcp_servers_map(&enabled)
}

/// Import from the Gemini MCP config into the unified structure (v3.7.0+)
/// Existing servers get the Gemini app enabled without overwriting other fields or app states
pub fn import_from_gemini(config: &mut MultiAppConfig) -> Result<usize, AppError> {
    let map = crate::gemini_mcp::read_mcp_servers_map()?;
    if map.is_empty() {
        return Ok(0);
    }

    // Make sure the new structure exists
    let servers = config.mcp.servers.get_or_insert_with(HashMap::new);

    let mut changed = 0;
    let mut errors = Vec::new();

    for (id, spec) in map.iter() {
        // Validation: a single failure does not abort; collect errors and keep going
        if let Err(e) = validate_server_spec(spec) {
            log::warn!("Skipping invalid MCP server '{id}': {e}");
            errors.push(format!("{id}: {e}"));
            continue;
        }

        if let Some(existing) = servers.get_mut(id) {
            // Already exists: only enable the Gemini app
            if !existing.apps.gemini {
                existing.apps.gemini = true;
                changed += 1;
                log::info!("Enabled the Gemini app for MCP server '{id}'");
            }
        } else {
            // New server: enable only Gemini by default
            servers.insert(
                id.clone(),
                McpServer {
                    id: id.clone(),
                    name: id.clone(),
                    server: spec.clone(),
                    apps: McpApps {
                        claude: false,
                        codex: false,
                        gemini: true,
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

    if !errors.is_empty() {
        log::warn!(
            "Import finished with {} failures: {:?}",
            errors.len(),
            errors
        );
    }

    Ok(changed)
}

/// Sync a single MCP server into the Gemini live config
pub fn sync_single_server_to_gemini(
    _config: &MultiAppConfig,
    id: &str,
    server_spec: &Value,
) -> Result<(), AppError> {
    if !should_sync_gemini_mcp() {
        return Ok(());
    }
    // Read the existing MCP config
    let mut current = crate::gemini_mcp::read_mcp_servers_map()?;

    // Insert or update the current server
    current.insert(id.to_string(), server_spec.clone());

    // Write back
    crate::gemini_mcp::set_mcp_servers_map(&current)
}

/// Remove a single MCP server from the Gemini live config
pub fn remove_server_from_gemini(id: &str) -> Result<(), AppError> {
    if !should_sync_gemini_mcp() {
        return Ok(());
    }
    // Read the existing MCP config
    let mut current = crate::gemini_mcp::read_mcp_servers_map()?;

    // Remove the requested server
    current.remove(id);

    // Write back
    crate::gemini_mcp::set_mcp_servers_map(&current)
}
