//! General settings data access object
//!
//! Provides key-value storage for general settings.

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use rusqlite::params;

impl Database {
    const LEGACY_COMMON_CONFIG_MIGRATED_KEY: &'static str = "common_config_legacy_migrated_v1";

    fn config_snippet_cleared_key(app_type: &str) -> String {
        format!("common_config_{app_type}_cleared")
    }

    /// Get a setting value
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = ?1")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut rows = stmt
            .query(params![key])
            .map_err(|e| AppError::Database(e.to_string()))?;

        if let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
            Ok(Some(
                row.get(0).map_err(|e| AppError::Database(e.to_string()))?,
            ))
        } else {
            Ok(None)
        }
    }

    /// Read a flag with boolean semantics: `"true"` or `"1"` means true, anything else false.
    ///
    /// Used for one-shot startup flags (`official_providers_seeded`, `first_run_notice_shown`, ...).
    /// **Unlike** legacy helpers such as `is_legacy_common_config_migrated` that only accept
    /// `"true"`, `"1"` is also accepted here to stay compatible with how
    /// `init_default_official_providers` already writes the flag.
    pub fn get_bool_flag(&self, key: &str) -> Result<bool, AppError> {
        Ok(matches!(
            self.get_setting(key)?.as_deref(),
            Some("true") | Some("1")
        ))
    }

    /// Set a setting value
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    // --- Common Config Snippet ---

    /// Get the common config snippet
    pub fn get_config_snippet(&self, app_type: &str) -> Result<Option<String>, AppError> {
        self.get_setting(&format!("common_config_{app_type}"))
    }

    /// Check whether the user explicitly cleared the common config snippet
    pub fn is_config_snippet_cleared(&self, app_type: &str) -> Result<bool, AppError> {
        Ok(self
            .get_setting(&Self::config_snippet_cleared_key(app_type))?
            .as_deref()
            == Some("true"))
    }

    /// Set whether the common config snippet was explicitly cleared
    pub fn set_config_snippet_cleared(
        &self,
        app_type: &str,
        cleared: bool,
    ) -> Result<(), AppError> {
        let key = Self::config_snippet_cleared_key(app_type);
        if cleared {
            self.set_setting(&key, "true")
        } else {
            let conn = lock_conn!(self.conn);
            conn.execute("DELETE FROM settings WHERE key = ?1", params![key])
                .map_err(|e| AppError::Database(e.to_string()))?;
            Ok(())
        }
    }

    /// Whether the common config snippet may currently be auto-extracted from the live config
    pub fn should_auto_extract_config_snippet(&self, app_type: &str) -> Result<bool, AppError> {
        Ok(self.get_config_snippet(app_type)?.is_none()
            && !self.is_config_snippet_cleared(app_type)?)
    }

    /// Check whether the legacy common config migration has already run
    pub fn is_legacy_common_config_migrated(&self) -> Result<bool, AppError> {
        Ok(self
            .get_setting(Self::LEGACY_COMMON_CONFIG_MIGRATED_KEY)?
            .as_deref()
            == Some("true"))
    }

    /// Mark the legacy common config migration as complete
    pub fn set_legacy_common_config_migrated(&self, migrated: bool) -> Result<(), AppError> {
        if migrated {
            self.set_setting(Self::LEGACY_COMMON_CONFIG_MIGRATED_KEY, "true")
        } else {
            let conn = lock_conn!(self.conn);
            conn.execute(
                "DELETE FROM settings WHERE key = ?1",
                params![Self::LEGACY_COMMON_CONFIG_MIGRATED_KEY],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
            Ok(())
        }
    }

    /// Set the common config snippet
    pub fn set_config_snippet(
        &self,
        app_type: &str,
        snippet: Option<String>,
    ) -> Result<(), AppError> {
        let key = format!("common_config_{app_type}");
        if let Some(value) = snippet {
            self.set_setting(&key, &value)
        } else {
            // Delete when None
            let conn = lock_conn!(self.conn);
            conn.execute("DELETE FROM settings WHERE key = ?1", params![key])
                .map_err(|e| AppError::Database(e.to_string()))?;
            Ok(())
        }
    }

    // --- Global outbound proxy ---

    /// Storage key for the global proxy URL
    const GLOBAL_PROXY_URL_KEY: &'static str = "global_proxy_url";

    /// Get the global outbound proxy URL
    ///
    /// None means the proxy is unconfigured or cleared (direct connection)
    /// Some(url) means a proxy is configured
    pub fn get_global_proxy_url(&self) -> Result<Option<String>, AppError> {
        self.get_setting(Self::GLOBAL_PROXY_URL_KEY)
    }

    /// Set the global outbound proxy URL
    ///
    /// - A non-empty string enables the proxy
    /// - An empty string or None clears the proxy setting (direct connection)
    pub fn set_global_proxy_url(&self, url: Option<&str>) -> Result<(), AppError> {
        match url {
            Some(u) if !u.trim().is_empty() => {
                self.set_setting(Self::GLOBAL_PROXY_URL_KEY, u.trim())
            }
            _ => {
                // Clear the proxy setting
                let conn = lock_conn!(self.conn);
                conn.execute(
                    "DELETE FROM settings WHERE key = ?1",
                    params![Self::GLOBAL_PROXY_URL_KEY],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
                Ok(())
            }
        }
    }

    // --- Proxy takeover state (deprecated, replaced by proxy_config.enabled) ---

    /// Get the proxy takeover state of an app
    ///
    /// **Deprecated**: use the `proxy_config.enabled` field instead.
    /// This method only exists to read legacy data during database migration.
    #[deprecated(
        since = "3.9.0",
        note = "use get_proxy_config_for_app().enabled instead"
    )]
    pub fn get_proxy_takeover_enabled(&self, app_type: &str) -> Result<bool, AppError> {
        let key = format!("proxy_takeover_{app_type}");
        match self.get_setting(&key)? {
            Some(value) => Ok(value == "true"),
            None => Ok(false),
        }
    }

    /// Set the proxy takeover state of an app
    ///
    /// **Deprecated**: use the `proxy_config.enabled` field instead.
    #[deprecated(
        since = "3.9.0",
        note = "use update_proxy_config_for_app() to change the enabled field"
    )]
    pub fn set_proxy_takeover_enabled(
        &self,
        app_type: &str,
        enabled: bool,
    ) -> Result<(), AppError> {
        let key = format!("proxy_takeover_{app_type}");
        let value = if enabled { "true" } else { "false" };
        self.set_setting(&key, value)
    }

    /// Check whether any app has proxy takeover enabled
    ///
    /// **Deprecated**: use `is_live_takeover_active()` instead.
    #[deprecated(since = "3.9.0", note = "use is_live_takeover_active() instead")]
    pub fn has_any_proxy_takeover(&self) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key LIKE 'proxy_takeover_%' AND value = 'true'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(count > 0)
    }

    /// Clear all proxy takeover state (sets every proxy_takeover_* entry to false)
    ///
    /// **Deprecated**: the settings table no longer stores proxy state.
    #[deprecated(
        since = "3.9.0",
        note = "use update_proxy_config_for_app() to clear the enabled field of each app"
    )]
    pub fn clear_all_proxy_takeover(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE settings SET value = 'false' WHERE key LIKE 'proxy_takeover_%'",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        log::info!("Cleared all proxy takeover state");
        Ok(())
    }

    // --- Rectifier config ---

    /// Get the rectifier config
    ///
    /// Returns the rectifier config, falling back to the defaults (everything enabled)
    pub fn get_rectifier_config(&self) -> Result<crate::proxy::types::RectifierConfig, AppError> {
        match self.get_setting("rectifier_config")? {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| AppError::Database(format!("Failed to parse rectifier config: {e}"))),
            None => Ok(crate::proxy::types::RectifierConfig::default()),
        }
    }

    /// Update the rectifier config
    pub fn set_rectifier_config(
        &self,
        config: &crate::proxy::types::RectifierConfig,
    ) -> Result<(), AppError> {
        let json = serde_json::to_string(config).map_err(|e| {
            AppError::Database(format!("Failed to serialize rectifier config: {e}"))
        })?;
        self.set_setting("rectifier_config", &json)
    }

    // --- Optimizer config ---

    /// Get the optimizer config
    ///
    /// Returns the optimizer config, falling back to the defaults (disabled)
    pub fn get_optimizer_config(&self) -> Result<crate::proxy::types::OptimizerConfig, AppError> {
        match self.get_setting("optimizer_config")? {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| AppError::Database(format!("Failed to parse optimizer config: {e}"))),
            None => Ok(crate::proxy::types::OptimizerConfig::default()),
        }
    }

    /// Update the optimizer config
    pub fn set_optimizer_config(
        &self,
        config: &crate::proxy::types::OptimizerConfig,
    ) -> Result<(), AppError> {
        let json = serde_json::to_string(config).map_err(|e| {
            AppError::Database(format!("Failed to serialize optimizer config: {e}"))
        })?;
        self.set_setting("optimizer_config", &json)
    }

    // --- Copilot optimizer config ---

    /// Get the Copilot optimizer config
    ///
    /// Returns the config, falling back to the defaults (enabled)
    pub fn get_copilot_optimizer_config(
        &self,
    ) -> Result<crate::proxy::types::CopilotOptimizerConfig, AppError> {
        match self.get_setting("copilot_optimizer_config")? {
            Some(json) => serde_json::from_str(&json).map_err(|e| {
                AppError::Database(format!("Failed to parse Copilot optimizer config: {e}"))
            }),
            None => Ok(crate::proxy::types::CopilotOptimizerConfig::default()),
        }
    }

    /// Update the Copilot optimizer config
    pub fn set_copilot_optimizer_config(
        &self,
        config: &crate::proxy::types::CopilotOptimizerConfig,
    ) -> Result<(), AppError> {
        let json = serde_json::to_string(config).map_err(|e| {
            AppError::Database(format!("Failed to serialize Copilot optimizer config: {e}"))
        })?;
        self.set_setting("copilot_optimizer_config", &json)
    }

    // --- Logging config ---

    /// Get the logging config
    pub fn get_log_config(&self) -> Result<crate::proxy::types::LogConfig, AppError> {
        match self.get_setting("log_config")? {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| AppError::Database(format!("Failed to parse logging config: {e}"))),
            None => Ok(crate::proxy::types::LogConfig::default()),
        }
    }

    /// Update the logging config
    pub fn set_log_config(&self, config: &crate::proxy::types::LogConfig) -> Result<(), AppError> {
        let json = serde_json::to_string(config)
            .map_err(|e| AppError::Database(format!("Failed to serialize logging config: {e}")))?;
        self.set_setting("log_config", &json)
    }
}
