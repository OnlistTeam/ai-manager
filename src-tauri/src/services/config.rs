use super::provider::{sanitize_claude_settings_for_live, ProviderService};
use crate::app_config::{AppType, MultiAppConfig};
use crate::error::AppError;
use crate::provider::Provider;
use chrono::Utc;
use serde_json::Value;
use std::fs;
use std::path::Path;

const MAX_BACKUPS: usize = 10;

/// Config import/export business logic
pub struct ConfigService;

impl ConfigService {
    /// Create a backup of the current config.json and return the backup ID (empty string if the file does not exist).
    pub fn create_backup(config_path: &Path) -> Result<String, AppError> {
        if !config_path.exists() {
            return Ok(String::new());
        }

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_id = format!("backup_{timestamp}");

        let backup_dir = config_path
            .parent()
            .ok_or_else(|| AppError::Config("Invalid config path".into()))?
            .join("backups");

        fs::create_dir_all(&backup_dir).map_err(|e| AppError::io(&backup_dir, e))?;

        let backup_path = backup_dir.join(format!("{backup_id}.json"));
        let contents = fs::read(config_path).map_err(|e| AppError::io(config_path, e))?;
        fs::write(&backup_path, contents).map_err(|e| AppError::io(&backup_path, e))?;

        Self::cleanup_old_backups(&backup_dir, MAX_BACKUPS)?;

        Ok(backup_id)
    }

    fn cleanup_old_backups(backup_dir: &Path, retain: usize) -> Result<(), AppError> {
        if retain == 0 {
            return Ok(());
        }

        let entries = match fs::read_dir(backup_dir) {
            Ok(iter) => iter
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .map(|ext| ext == "json")
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>(),
            Err(_) => return Ok(()),
        };

        if entries.len() <= retain {
            return Ok(());
        }

        let remove_count = entries.len().saturating_sub(retain);
        let mut sorted = entries;

        sorted.sort_by(|a, b| {
            let a_time = a.metadata().and_then(|m| m.modified()).ok();
            let b_time = b.metadata().and_then(|m| m.modified()).ok();
            a_time.cmp(&b_time)
        });

        for entry in sorted.into_iter().take(remove_count) {
            if let Err(err) = fs::remove_file(entry.path()) {
                log::warn!(
                    "Failed to remove old backup {}: {}",
                    entry.path().display(),
                    err
                );
            }
        }

        Ok(())
    }

    /// Sync the current provider into the matching live config.
    pub fn sync_current_providers_to_live(config: &mut MultiAppConfig) -> Result<(), AppError> {
        Self::sync_current_provider_for_app(config, &AppType::Claude)?;
        Self::sync_current_provider_for_app(config, &AppType::Codex)?;
        Self::sync_current_provider_for_app(config, &AppType::Gemini)?;
        Self::sync_current_provider_for_app(config, &AppType::GrokBuild)?;
        Ok(())
    }

    fn sync_current_provider_for_app(
        config: &mut MultiAppConfig,
        app_type: &AppType,
    ) -> Result<(), AppError> {
        let (current_id, provider) = {
            let manager = match config.get_manager(app_type) {
                Some(manager) => manager,
                None => return Ok(()),
            };

            if manager.current.is_empty() {
                return Ok(());
            }

            let current_id = manager.current.clone();
            let provider = match manager.providers.get(&current_id) {
                Some(provider) => provider.clone(),
                None => {
                    log::warn!(
                        "provider {current_id} of current app {app_type:?} not found, skipping live sync"
                    );
                    return Ok(());
                }
            };
            (current_id, provider)
        };

        match app_type {
            AppType::Codex => Self::sync_codex_live(config, &current_id, &provider)?,
            AppType::Claude => Self::sync_claude_live(config, &current_id, &provider)?,
            AppType::ClaudeDesktop => {
                // Claude Desktop 3P profiles are managed by claude_desktop_config.
            }
            AppType::Gemini => Self::sync_gemini_live(config, &current_id, &provider)?,
            AppType::GrokBuild => crate::grok_config::write_grok_provider_live(&provider)?,
            AppType::OpenCode => {
                // OpenCode uses additive mode, no live sync needed
                // OpenCode providers are managed directly in the config file
            }
            AppType::OpenClaw => {
                // OpenClaw uses additive mode, no live sync needed
                // OpenClaw providers are managed directly in the config file
            }
            AppType::Hermes => {
                // Hermes uses additive mode, no live sync needed
            }
            AppType::Pi => {
                // Pi owns its shared models/settings documents; this legacy
                // single-provider live-sync path must not rewrite them.
            }
        }

        Ok(())
    }

    fn sync_codex_live(
        config: &mut MultiAppConfig,
        provider_id: &str,
        provider: &Provider,
    ) -> Result<(), AppError> {
        let settings = provider.settings_config.as_object().ok_or_else(|| {
            AppError::Config(format!(
                "Codex config of provider {provider_id} must be an object"
            ))
        })?;
        let auth = settings.get("auth").ok_or_else(|| {
            AppError::Config(format!(
                "Codex config of provider {provider_id} is missing the auth field"
            ))
        })?;
        if !auth.is_object() {
            return Err(AppError::Config(format!(
                "Codex auth config of provider {provider_id} must be a JSON object"
            )));
        }
        let cfg_text = settings.get("config").and_then(Value::as_str);

        let profile = crate::proxy::providers::resolve_codex_catalog_tool_profile(provider);

        crate::codex_config::write_codex_provider_live_with_catalog(
            &provider.settings_config,
            provider.category.as_deref(),
            auth,
            cfg_text,
            profile,
        )?;
        // Note: since v3.7.0 MCP sync goes through McpService and is no longer called here
        // sync_enabled_to_codex uses the old config.mcp.codex structure, which is empty in the new architecture
        // Enabling/disabling MCP should go through McpService::toggle_app

        let cfg_text_after = crate::codex_config::read_and_validate_codex_config_text()?;
        if let Some(manager) = config.get_manager_mut(&AppType::Codex) {
            if let Some(target) = manager.providers.get_mut(provider_id) {
                if let Some(obj) = target.settings_config.as_object_mut() {
                    let mut restored = serde_json::json!({
                        "auth": auth.clone(),
                        "config": cfg_text_after,
                    });
                    let restore_provider_token =
                        crate::codex_config::should_restore_codex_provider_token_for_backfill(
                            provider.category.as_deref(),
                            &provider.settings_config,
                        );
                    crate::codex_config::restore_codex_settings_for_backfill(
                        &mut restored,
                        &provider.settings_config,
                        restore_provider_token,
                    )?;
                    // Both auth and config must be written back: backfill moves the live
                    // experimental_bearer_token into restored.auth.OPENAI_API_KEY.
                    if let Some(restored_obj) = restored.as_object() {
                        if let Some(auth_value) = restored_obj.get("auth") {
                            obj.insert("auth".to_string(), auth_value.clone());
                        }
                        if let Some(config_value) = restored_obj.get("config") {
                            obj.insert("config".to_string(), config_value.clone());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn sync_claude_live(
        config: &mut MultiAppConfig,
        provider_id: &str,
        provider: &Provider,
    ) -> Result<(), AppError> {
        use crate::config::{read_json_file, write_json_file};

        let settings_path = crate::config::get_claude_settings_path();
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }

        let settings = sanitize_claude_settings_for_live(&provider.settings_config);
        write_json_file(&settings_path, &settings)?;

        let live_after = read_json_file::<serde_json::Value>(&settings_path)?;
        if let Some(manager) = config.get_manager_mut(&AppType::Claude) {
            if let Some(target) = manager.providers.get_mut(provider_id) {
                target.settings_config = live_after;
            }
        }

        Ok(())
    }

    fn sync_gemini_live(
        config: &mut MultiAppConfig,
        provider_id: &str,
        provider: &Provider,
    ) -> Result<(), AppError> {
        use crate::gemini_config::{env_to_json, read_gemini_env};

        ProviderService::write_gemini_live(provider)?;

        // Read back what was actually written and update the config (including settings.json)
        let live_after_env = read_gemini_env()?;
        let settings_path = crate::gemini_config::get_gemini_settings_path();
        let live_after_config = if settings_path.exists() {
            crate::config::read_json_file(&settings_path)?
        } else {
            serde_json::json!({})
        };
        let mut live_after = env_to_json(&live_after_env);
        if let Some(obj) = live_after.as_object_mut() {
            obj.insert("config".to_string(), live_after_config);
        }

        if let Some(manager) = config.get_manager_mut(&AppType::Gemini) {
            if let Some(target) = manager.providers.get_mut(provider_id) {
                target.settings_config = live_after;
            }
        }

        Ok(())
    }
}
