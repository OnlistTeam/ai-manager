use indexmap::IndexMap;
use std::collections::HashMap;

use crate::app_config::{AppType, McpServer};
use crate::error::AppError;
use crate::mcp;
use crate::store::AppState;

/// MCP business logic (unified structure since v3.7.0)
pub struct McpService;

impl McpService {
    /// Get all MCP servers (unified structure)
    pub fn get_all_servers(state: &AppState) -> Result<IndexMap<String, McpServer>, AppError> {
        state.db.get_all_mcp_servers()
    }

    /// Add or update an MCP server
    pub fn upsert_server(state: &AppState, server: McpServer) -> Result<(), AppError> {
        // Read the previous state: handles "an app was unchecked while editing" (it must be removed from that live config)
        let prev_apps = state
            .db
            .get_all_mcp_servers()?
            .get(&server.id)
            .map(|s| s.apps.clone())
            .unwrap_or_default();

        state.db.save_mcp_server(&server)?;

        // Handle disabling: if it was enabled before but unchecked now, remove it from that app's live config
        if prev_apps.claude && !server.apps.claude {
            Self::remove_server_from_app(state, &server.id, &AppType::Claude)?;
        }
        if prev_apps.codex && !server.apps.codex {
            Self::remove_server_from_app(state, &server.id, &AppType::Codex)?;
        }
        if prev_apps.gemini && !server.apps.gemini {
            Self::remove_server_from_app(state, &server.id, &AppType::Gemini)?;
        }
        if prev_apps.grokbuild && !server.apps.grokbuild {
            Self::remove_server_from_app(state, &server.id, &AppType::GrokBuild)?;
        }
        if prev_apps.opencode && !server.apps.opencode {
            Self::remove_server_from_app(state, &server.id, &AppType::OpenCode)?;
        }
        if prev_apps.hermes && !server.apps.hermes {
            Self::remove_server_from_app(state, &server.id, &AppType::Hermes)?;
        }
        if prev_apps.claude_desktop && !server.apps.claude_desktop {
            Self::remove_server_from_app(state, &server.id, &AppType::ClaudeDesktop)?;
        }

        // Sync to every enabled app
        Self::sync_server_to_apps(state, &server)?;

        Ok(())
    }

    /// Delete an MCP server
    pub fn delete_server(state: &AppState, id: &str) -> Result<bool, AppError> {
        let server = state.db.get_all_mcp_servers()?.shift_remove(id);

        if let Some(server) = server {
            state.db.delete_mcp_server(id)?;

            // Remove from the live config of every app
            Self::remove_server_from_all_apps(state, id, &server)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Toggle the enabled state for a given app
    pub fn toggle_app(
        state: &AppState,
        server_id: &str,
        app: AppType,
        enabled: bool,
    ) -> Result<(), AppError> {
        if let Some(server) = state
            .db
            .update_mcp_server_app_enabled(server_id, &app, enabled)?
        {
            // Sync to the matching app
            if enabled {
                Self::sync_server_to_app(state, &server, &app)?;
            } else {
                Self::remove_server_from_app(state, server_id, &app)?;
            }
        }

        Ok(())
    }

    /// Sync an MCP server to every enabled app
    fn sync_server_to_apps(_state: &AppState, server: &McpServer) -> Result<(), AppError> {
        for app in server.apps.enabled_apps() {
            Self::sync_server_to_app_no_config(server, &app)?;
        }

        Ok(())
    }

    /// Sync an MCP server to a given app
    fn sync_server_to_app(
        _state: &AppState,
        server: &McpServer,
        app: &AppType,
    ) -> Result<(), AppError> {
        Self::sync_server_to_app_no_config(server, app)
    }

    fn sync_server_to_app_no_config(server: &McpServer, app: &AppType) -> Result<(), AppError> {
        match app {
            AppType::Claude => {
                mcp::sync_single_server_to_claude(&Default::default(), &server.id, &server.server)?;
            }
            AppType::ClaudeDesktop => {
                mcp::sync_single_server_to_claude_desktop(
                    &Default::default(),
                    &server.id,
                    &server.server,
                )?;
            }
            AppType::Codex => {
                // Codex uses TOML format, must use the correct function
                mcp::sync_single_server_to_codex(&Default::default(), &server.id, &server.server)?;
            }
            AppType::Gemini => {
                mcp::sync_single_server_to_gemini(&Default::default(), &server.id, &server.server)?;
            }
            AppType::GrokBuild => {
                mcp::sync_single_server_to_grokbuild(
                    &Default::default(),
                    &server.id,
                    &server.server,
                )?;
            }
            AppType::OpenCode => {
                mcp::sync_single_server_to_opencode(
                    &Default::default(),
                    &server.id,
                    &server.server,
                )?;
            }
            AppType::OpenClaw => {
                // OpenClaw MCP support is still in development (Issue #4834)
                // Skip for now
                log::debug!("OpenClaw MCP support is still in development, skipping sync");
            }
            AppType::Hermes => {
                mcp::sync_single_server_to_hermes(&Default::default(), &server.id, &server.server)?;
            }
            AppType::Pi => {}
        }
        Ok(())
    }

    /// Remove the server from every app it was ever enabled for
    fn remove_server_from_all_apps(
        state: &AppState,
        id: &str,
        server: &McpServer,
    ) -> Result<(), AppError> {
        // Remove from every app it was enabled for
        for app in server.apps.enabled_apps() {
            Self::remove_server_from_app(state, id, &app)?;
        }
        Ok(())
    }

    fn remove_server_from_app(_state: &AppState, id: &str, app: &AppType) -> Result<(), AppError> {
        match app {
            AppType::Claude => mcp::remove_server_from_claude(id)?,
            AppType::ClaudeDesktop => {
                mcp::remove_server_from_claude_desktop(id)?;
            }
            AppType::Codex => mcp::remove_server_from_codex(id)?,
            AppType::Gemini => mcp::remove_server_from_gemini(id)?,
            AppType::GrokBuild => mcp::remove_server_from_grokbuild(id)?,
            AppType::OpenCode => {
                mcp::remove_server_from_opencode(id)?;
            }
            AppType::OpenClaw => {
                // OpenClaw MCP support is still in development
                log::debug!("OpenClaw MCP support is still in development, skipping remove");
            }
            AppType::Hermes => {
                mcp::remove_server_from_hermes(id)?;
            }
            AppType::Pi => {}
        }
        Ok(())
    }

    /// Manually sync all enabled MCP servers to their apps.
    ///
    /// Best-effort: a failed projection for one app (e.g. a broken ~/.claude.json) does not block the
    /// others - each app's live file is independent, and one corrupt file is no reason to leave the MCP
    /// state of other apps stale. If anything failed after the full run, the failures are aggregated into
    /// a single reported error to keep them visible to the caller.
    pub fn sync_all_enabled(state: &AppState) -> Result<(), AppError> {
        let servers = Self::get_all_servers(state)?;

        let mut failures: Vec<String> = Vec::new();
        for app in AppType::all() {
            if let Err(err) = Self::project_servers_to_app(state, &servers, &app) {
                log::warn!("Failed to sync MCP to {app:?}: {err}");
                failures.push(format!("{}: {err}", app.as_str()));
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(AppError::Message(format!(
                "MCP sync failed for some apps: {}",
                failures.join("; ")
            )))
        }
    }

    /// Project the enabled state to a single app only. Used for a targeted re-projection after one app's
    /// live config was rewritten wholesale, so the failure surface of unrelated apps (e.g. a broken
    /// ~/.claude.json) is kept out of the target app's critical path.
    pub fn sync_enabled_for_app(state: &AppState, app: &AppType) -> Result<(), AppError> {
        let servers = Self::get_all_servers(state)?;
        Self::project_servers_to_app(state, &servers, app)
    }

    fn project_servers_to_app(
        state: &AppState,
        servers: &IndexMap<String, McpServer>,
        app: &AppType,
    ) -> Result<(), AppError> {
        if matches!(app, AppType::OpenClaw | AppType::Pi) {
            return Ok(());
        }

        for server in servers.values() {
            if server.apps.is_enabled_for(app) {
                Self::sync_server_to_app(state, server, app)?;
            } else {
                Self::remove_server_from_app(state, &server.id, app)?;
            }
        }

        Ok(())
    }

    // ========================================================================
    // Compatibility layer: supports the old v3.6.x commands (deprecated, to be removed in v4.0)
    // ========================================================================

    /// [Deprecated] Get the MCP servers of a given app (legacy API compatibility)
    #[deprecated(since = "3.7.0", note = "Use get_all_servers instead")]
    pub fn get_servers(
        state: &AppState,
        app: AppType,
    ) -> Result<HashMap<String, serde_json::Value>, AppError> {
        let all_servers = Self::get_all_servers(state)?;
        let mut result = HashMap::new();

        for (id, server) in all_servers {
            if server.apps.is_enabled_for(&app) {
                result.insert(id, server.server);
            }
        }

        Ok(result)
    }

    /// [Deprecated] Set the enabled state of an MCP server for a given app (legacy API compatibility)
    #[deprecated(since = "3.7.0", note = "Use toggle_app instead")]
    pub fn set_enabled(
        state: &AppState,
        app: AppType,
        id: &str,
        enabled: bool,
    ) -> Result<bool, AppError> {
        Self::toggle_app(state, id, app, enabled)?;
        Ok(true)
    }

    /// [Deprecated] Sync enabled MCP servers to a given app (legacy API compatibility)
    #[deprecated(since = "3.7.0", note = "Use sync_all_enabled instead")]
    pub fn sync_enabled(state: &AppState, app: AppType) -> Result<(), AppError> {
        let servers = Self::get_all_servers(state)?;

        for server in servers.values() {
            if server.apps.is_enabled_for(&app) {
                Self::sync_server_to_app(state, server, &app)?;
            }
        }

        Ok(())
    }

    /// Import MCP from Claude (updated to the unified structure in v3.7.0)
    pub fn import_from_claude(state: &AppState) -> Result<usize, AppError> {
        // Build a temporary MultiAppConfig for the import
        let mut temp_config = crate::app_config::MultiAppConfig::default();

        // Call the existing import logic (from mcp.rs)
        let count = crate::mcp::import_from_claude(&mut temp_config)?;

        let mut new_count = 0;

        // Persist imported servers to the database
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    // Already exists: only enable Claude, do not overwrite other fields (consistent with the import module)
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.claude = true;
                        merged
                    } else {
                        // A genuinely new server
                        new_count += 1;
                        server.clone()
                    };

                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save.clone());

                    // Import reads existing config and must not write back to any app's live config.
                    // Write-back happens on explicit edit, enable/disable, or manual sync.
                }
            }
        }

        Ok(new_count)
    }

    /// Import Claude Desktop's existing `mcpServers` without writing the live
    /// file back. Existing global rows keep their metadata and other flags.
    pub fn import_from_claude_desktop(state: &AppState) -> Result<usize, AppError> {
        let mut temp_config = crate::app_config::MultiAppConfig::default();
        let count = crate::mcp::import_from_claude_desktop(&mut temp_config)?;
        let mut new_count = 0;
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.claude_desktop = true;
                        merged
                    } else {
                        new_count += 1;
                        server.clone()
                    };
                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save);
                }
            }
        }
        Ok(new_count)
    }

    /// Import MCP from Codex (updated to the unified structure in v3.7.0)
    pub fn import_from_codex(state: &AppState) -> Result<usize, AppError> {
        // Build a temporary MultiAppConfig for the import
        let mut temp_config = crate::app_config::MultiAppConfig::default();

        // Call the existing import logic (from mcp.rs)
        let count = crate::mcp::import_from_codex(&mut temp_config)?;

        let mut new_count = 0;

        // Persist imported servers to the database
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    // Already exists: only enable Codex, do not overwrite other fields (consistent with the import module)
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.codex = true;
                        merged
                    } else {
                        // A genuinely new server
                        new_count += 1;
                        server.clone()
                    };

                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save.clone());

                    // Import reads existing config and must not write back to any app's live config.
                    // Write-back happens on explicit edit, enable/disable, or manual sync.
                }
            }
        }

        Ok(new_count)
    }

    /// Import MCP from Gemini (updated to the unified structure in v3.7.0)
    pub fn import_from_gemini(state: &AppState) -> Result<usize, AppError> {
        // Build a temporary MultiAppConfig for the import
        let mut temp_config = crate::app_config::MultiAppConfig::default();

        // Call the existing import logic (from mcp.rs)
        let count = crate::mcp::import_from_gemini(&mut temp_config)?;

        let mut new_count = 0;

        // Persist imported servers to the database
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    // Already exists: only enable Gemini, do not overwrite other fields (consistent with the import module)
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.gemini = true;
                        merged
                    } else {
                        // A genuinely new server
                        new_count += 1;
                        server.clone()
                    };

                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save.clone());

                    // Import reads existing config and must not write back to any app's live config.
                    // Write-back happens on explicit edit, enable/disable, or manual sync.
                }
            }
        }

        Ok(new_count)
    }

    /// Import MCP from the `[mcp_servers]` section of Grok Build.
    pub fn import_from_grokbuild(state: &AppState) -> Result<usize, AppError> {
        let mut temp_config = crate::app_config::MultiAppConfig::default();
        let count = crate::mcp::import_from_grokbuild(&mut temp_config)?;
        let mut new_count = 0;

        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.grokbuild = true;
                        merged
                    } else {
                        new_count += 1;
                        server.clone()
                    };
                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save);
                }
            }
        }
        Ok(new_count)
    }

    /// Import MCP from OpenCode (added in v3.9.2+)
    pub fn import_from_opencode(state: &AppState) -> Result<usize, AppError> {
        // Build a temporary MultiAppConfig for the import
        let mut temp_config = crate::app_config::MultiAppConfig::default();

        // Call the existing import logic (from mcp/opencode.rs)
        let count = crate::mcp::import_from_opencode(&mut temp_config)?;

        let mut new_count = 0;

        // Persist imported servers to the database
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    // Already exists: only enable OpenCode, do not overwrite other fields (consistent with the import module)
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.opencode = true;
                        merged
                    } else {
                        // A genuinely new server
                        new_count += 1;
                        server.clone()
                    };

                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save.clone());

                    // Import reads existing config and must not write back to any app's live config.
                    // Write-back happens on explicit edit, enable/disable, or manual sync.
                }
            }
        }

        Ok(new_count)
    }

    /// Import MCP from Hermes
    pub fn import_from_hermes(state: &AppState) -> Result<usize, AppError> {
        // Build a temporary MultiAppConfig for the import
        let mut temp_config = crate::app_config::MultiAppConfig::default();

        // Call the import logic (from mcp/hermes.rs)
        let count = crate::mcp::import_from_hermes(&mut temp_config)?;

        let mut new_count = 0;

        // Persist imported servers to the database
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    // Already exists: only enable Hermes, do not overwrite other fields (consistent with the import module)
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.hermes = true;
                        merged
                    } else {
                        // A genuinely new server
                        new_count += 1;
                        server.clone()
                    };

                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save.clone());

                    // Import reads existing config and must not write back to any app's live config.
                    // Write-back happens on explicit edit, enable/disable, or manual sync.
                }
            }
        }

        Ok(new_count)
    }

    /// Import servers from every MCP-capable app; returns the number of newly imported servers.
    ///
    /// Best-effort: a failed import for one app (e.g. a broken config.toml) does not block the others;
    /// if anything failed after the full run the failures are aggregated into a single reported error -
    /// the historical implementation swallowed errors with a per-app `unwrap_or(0)`, so a broken file
    /// only showed up as "imported 0" and the user could not tell which app was at fault.
    pub fn import_from_all_apps(state: &AppState) -> Result<usize, AppError> {
        let mut total = 0;
        let mut failures: Vec<String> = Vec::new();

        let results: [(&str, Result<usize, AppError>); 6] = [
            ("claude", Self::import_from_claude(state)),
            ("codex", Self::import_from_codex(state)),
            ("gemini", Self::import_from_gemini(state)),
            ("grokbuild", Self::import_from_grokbuild(state)),
            ("opencode", Self::import_from_opencode(state)),
            ("hermes", Self::import_from_hermes(state)),
        ];
        for (app, result) in results {
            match result {
                Ok(count) => total += count,
                Err(err) => {
                    log::warn!("Failed to import MCP from {app}: {err}");
                    failures.push(format!("{app}: {err}"));
                }
            }
        }

        if failures.is_empty() {
            Ok(total)
        } else {
            Err(AppError::Message(format!(
                "Imported {total}, but some apps failed to import: {}",
                failures.join("; ")
            )))
        }
    }
}
