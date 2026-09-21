//! Proxy feature data access layer
//!
//! Database operations for proxy config, provider health and usage statistics

use std::str::FromStr;

use crate::error::AppError;
use crate::proxy::types::*;
use rust_decimal::Decimal;

use super::super::{lock_conn, Database};

pub(crate) const PRICING_SOURCE_RESPONSE: &str = "response";
pub(crate) const PRICING_SOURCE_REQUEST: &str = "request";

pub(crate) fn validate_cost_multiplier(value: &str) -> Result<Decimal, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::localized(
            "error.multiplierEmpty",
            "Multiplier cannot be empty",
        ));
    }
    let parsed = Decimal::from_str(trimmed).map_err(|e| {
        AppError::localized(
            "error.invalidMultiplier",
            format!("Invalid multiplier: {value} - {e}"),
        )
    })?;
    if parsed < Decimal::ZERO {
        return Err(AppError::localized(
            "error.invalidMultiplier",
            format!("Invalid multiplier: {value} - multiplier cannot be negative"),
        ));
    }
    Ok(parsed)
}

pub(crate) fn validate_pricing_source(value: &str) -> Result<&str, AppError> {
    let trimmed = value.trim();
    if trimmed == PRICING_SOURCE_RESPONSE || trimmed == PRICING_SOURCE_REQUEST {
        Ok(trimmed)
    } else {
        Err(AppError::localized(
            "error.invalidPricingMode",
            format!("Invalid pricing mode: {value}"),
        ))
    }
}

impl Database {
    // ==================== Global Proxy Config ====================

    /// Get the global proxy config (shared fields)
    ///
    /// Read from the claude row (all three rows are kept mirrored)
    pub async fn get_global_proxy_config(&self) -> Result<GlobalProxyConfig, AppError> {
        // Scope conn to a block so the lock is not held across an await
        let result = {
            let conn = lock_conn!(self.conn);
            conn.query_row(
                "SELECT proxy_enabled, listen_address, listen_port, enable_logging
                 FROM proxy_config WHERE app_type = 'claude'",
                [],
                |row| {
                    Ok(GlobalProxyConfig {
                        proxy_enabled: row.get::<_, i32>(0)? != 0,
                        listen_address: row.get(1)?,
                        listen_port: row.get::<_, i32>(2)? as u16,
                        enable_logging: row.get::<_, i32>(3)? != 0,
                    })
                },
            )
        };
        // conn was released at the end of the block

        match result {
            Ok(config) => Ok(config),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Create the default config when the row is missing
                self.init_proxy_config_rows().await?;
                Ok(GlobalProxyConfig {
                    proxy_enabled: false,
                    listen_address: "127.0.0.1".to_string(),
                    listen_port: 15721,
                    enable_logging: true,
                })
            }
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// Update the global proxy config (mirrored across all three rows)
    pub async fn update_global_proxy_config(
        &self,
        config: GlobalProxyConfig,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        conn.execute(
            "UPDATE proxy_config SET
                proxy_enabled = ?1,
                listen_address = ?2,
                listen_port = ?3,
                enable_logging = ?4,
                updated_at = datetime('now')",
            rusqlite::params![
                if config.proxy_enabled { 1 } else { 0 },
                config.listen_address,
                config.listen_port as i32,
                if config.enable_logging { 1 } else { 0 },
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Get the default cost multiplier
    pub async fn get_default_cost_multiplier(&self, app_type: &str) -> Result<String, AppError> {
        let result = {
            let conn = lock_conn!(self.conn);
            conn.query_row(
                "SELECT default_cost_multiplier FROM proxy_config WHERE app_type = ?1",
                [app_type],
                |row| row.get(0),
            )
        };

        match result {
            Ok(value) => Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                self.init_proxy_config_rows().await?;
                Ok("1".to_string())
            }
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// Set the default cost multiplier
    pub async fn set_default_cost_multiplier(
        &self,
        app_type: &str,
        value: &str,
    ) -> Result<(), AppError> {
        validate_cost_multiplier(value)?;
        let trimmed = value.trim();

        // Make sure the row exists
        self.ensure_proxy_config_row_exists(app_type)?;

        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE proxy_config SET
                default_cost_multiplier = ?2,
                updated_at = datetime('now')
             WHERE app_type = ?1",
            rusqlite::params![app_type, trimmed],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Get the pricing mode source
    pub async fn get_pricing_model_source(&self, app_type: &str) -> Result<String, AppError> {
        let result = {
            let conn = lock_conn!(self.conn);
            conn.query_row(
                "SELECT pricing_model_source FROM proxy_config WHERE app_type = ?1",
                [app_type],
                |row| row.get(0),
            )
        };

        match result {
            Ok(value) => Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                self.init_proxy_config_rows().await?;
                Ok(PRICING_SOURCE_RESPONSE.to_string())
            }
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// Set the pricing mode source
    pub async fn set_pricing_model_source(
        &self,
        app_type: &str,
        value: &str,
    ) -> Result<(), AppError> {
        let trimmed = validate_pricing_source(value)?;

        // Make sure the row exists
        self.ensure_proxy_config_row_exists(app_type)?;

        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE proxy_config SET
                pricing_model_source = ?2,
                updated_at = datetime('now')
             WHERE app_type = ?1",
            rusqlite::params![app_type, trimmed],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Get the per-app proxy config
    pub async fn get_proxy_config_for_app(
        &self,
        app_type: &str,
    ) -> Result<AppProxyConfig, AppError> {
        // Scope conn to a block so the lock is not held across an await
        let app_type_owned = app_type.to_string();
        let result = {
            let conn = lock_conn!(self.conn);
            conn.query_row(
                "SELECT app_type, enabled, auto_failover_enabled,
                        max_retries, streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                        circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                        circuit_error_rate_threshold, circuit_min_requests
                 FROM proxy_config WHERE app_type = ?1",
                [app_type],
                |row| {
                    Ok(AppProxyConfig {
                        app_type: row.get(0)?,
                        enabled: row.get::<_, i32>(1)? != 0,
                        auto_failover_enabled: row.get::<_, i32>(2)? != 0,
                        max_retries: row.get::<_, i32>(3)? as u32,
                        streaming_first_byte_timeout: row.get::<_, i32>(4)? as u32,
                        streaming_idle_timeout: row.get::<_, i32>(5)? as u32,
                        non_streaming_timeout: row.get::<_, i32>(6)? as u32,
                        circuit_failure_threshold: row.get::<_, i32>(7)? as u32,
                        circuit_success_threshold: row.get::<_, i32>(8)? as u32,
                        circuit_timeout_seconds: row.get::<_, i32>(9)? as u32,
                        circuit_error_rate_threshold: row.get(10)?,
                        circuit_min_requests: row.get::<_, i32>(11)? as u32,
                    })
                },
            )
        };
        // conn was released at the end of the block

        match result {
            Ok(config) => Ok(config),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Create the default config when the row is missing
                self.init_proxy_config_rows().await?;
                Ok(AppProxyConfig {
                    app_type: app_type_owned,
                    enabled: false,
                    auto_failover_enabled: false,
                    max_retries: 3,
                    streaming_first_byte_timeout: 60,
                    streaming_idle_timeout: 120,
                    non_streaming_timeout: 600,
                    circuit_failure_threshold: 4,
                    circuit_success_threshold: 2,
                    circuit_timeout_seconds: 60,
                    circuit_error_rate_threshold: 0.6,
                    circuit_min_requests: 10,
                })
            }
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// Update the per-app proxy config
    pub async fn update_proxy_config_for_app(
        &self,
        config: AppProxyConfig,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        conn.execute(
            "UPDATE proxy_config SET
                enabled = ?2,
                auto_failover_enabled = ?3,
                max_retries = ?4,
                streaming_first_byte_timeout = ?5,
                streaming_idle_timeout = ?6,
                non_streaming_timeout = ?7,
                circuit_failure_threshold = ?8,
                circuit_success_threshold = ?9,
                circuit_timeout_seconds = ?10,
                circuit_error_rate_threshold = ?11,
                circuit_min_requests = ?12,
                updated_at = datetime('now')
             WHERE app_type = ?1",
            rusqlite::params![
                config.app_type,
                if config.enabled { 1 } else { 0 },
                if config.auto_failover_enabled { 1 } else { 0 },
                config.max_retries as i32,
                config.streaming_first_byte_timeout as i32,
                config.streaming_idle_timeout as i32,
                config.non_streaming_timeout as i32,
                config.circuit_failure_threshold as i32,
                config.circuit_success_threshold as i32,
                config.circuit_timeout_seconds as i32,
                config.circuit_error_rate_threshold,
                config.circuit_min_requests as i32,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Ensure the proxy_config row for the given app_type exists (sync version, used by set_* functions)
    ///
    /// Uses the same per-app defaults as the schema.rs seed
    fn ensure_proxy_config_row_exists(&self, app_type: &str) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Lock(e.to_string()))?;

        // Defaults vary by app_type (kept in sync with the schema.rs seed)
        let (retries, fb_timeout, idle_timeout, cb_fail, cb_succ, cb_timeout, cb_rate, cb_min) =
            match app_type {
                "claude" => (6, 90, 180, 8, 3, 90, 0.7, 15),
                "codex" => (3, 60, 120, 4, 2, 60, 0.6, 10),
                "gemini" => (5, 60, 120, 4, 2, 60, 0.6, 10),
                "grokbuild" => (3, 60, 120, 4, 2, 60, 0.6, 10),
                _ => (3, 60, 120, 4, 2, 60, 0.6, 10), // defaults
            };

        conn.execute(
            "INSERT OR IGNORE INTO proxy_config (
                app_type, max_retries,
                streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                circuit_error_rate_threshold, circuit_min_requests
            ) VALUES (?1, ?2, ?3, ?4, 600, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                app_type,
                retries,
                fb_timeout,
                idle_timeout,
                cb_fail,
                cb_succ,
                cb_timeout,
                cb_rate,
                cb_min
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Initialize the three rows of the proxy_config table
    ///
    /// Uses the same per-app defaults as the schema.rs seed
    async fn init_proxy_config_rows(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        // Same per-app defaults as the schema.rs seed
        // claude: more aggressive retry and timeout settings
        conn.execute(
            "INSERT OR IGNORE INTO proxy_config (
                app_type, max_retries,
                streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                circuit_error_rate_threshold, circuit_min_requests
            ) VALUES ('claude', 6, 90, 180, 600, 8, 3, 90, 0.7, 15)",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // codex: default settings
        conn.execute(
            "INSERT OR IGNORE INTO proxy_config (
                app_type, max_retries,
                streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                circuit_error_rate_threshold, circuit_min_requests
            ) VALUES ('codex', 3, 60, 120, 600, 4, 2, 60, 0.6, 10)",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // gemini: slightly higher retry count
        conn.execute(
            "INSERT OR IGNORE INTO proxy_config (
                app_type, max_retries,
                streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                circuit_error_rate_threshold, circuit_min_requests
            ) VALUES ('gemini', 5, 60, 120, 600, 4, 2, 60, 0.6, 10)",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // grokbuild: Responses protocol, same timeout defaults as Codex.
        conn.execute(
            "INSERT OR IGNORE INTO proxy_config (
                app_type, max_retries,
                streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                circuit_error_rate_threshold, circuit_min_requests
            ) VALUES ('grokbuild', 3, 60, 120, 600, 4, 2, 60, 0.6, 10)",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    // ==================== Legacy Proxy Config (compatibility) ====================

    /// Get the proxy config (legacy API, returns the claude row)
    pub async fn get_proxy_config(&self) -> Result<ProxyConfig, AppError> {
        // Scope conn to a block so the lock is not held across an await
        let result = {
            let conn = lock_conn!(self.conn);
            conn.query_row(
                "SELECT listen_address, listen_port, max_retries,
                        enable_logging,
                        streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout
                 FROM proxy_config WHERE app_type = 'claude'",
                [],
                |row| {
                    Ok(ProxyConfig {
                        listen_address: row.get(0)?,
                        listen_port: row.get::<_, i32>(1)? as u16,
                        max_retries: row.get::<_, i32>(2)? as u8,
                        request_timeout: 600, // deprecated field, returns the default
                        enable_logging: row.get::<_, i32>(3)? != 0,
                        live_takeover_active: false, // deprecated field
                        streaming_first_byte_timeout: row.get::<_, i32>(4).unwrap_or(60) as u64,
                        streaming_idle_timeout: row.get::<_, i32>(5).unwrap_or(120) as u64,
                        non_streaming_timeout: row.get::<_, i32>(6).unwrap_or(600) as u64,
                    })
                },
            )
        };
        // conn was released at the end of the block

        match result {
            Ok(config) => Ok(config),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Initialize the default config when the row is missing
                self.init_proxy_config_rows().await?;
                Ok(ProxyConfig::default())
            }
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// Update the proxy config (legacy API, updates the shared fields of all three rows)
    pub async fn update_proxy_config(&self, config: ProxyConfig) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        // Update the shared fields of all three rows
        conn.execute(
            "UPDATE proxy_config SET
                listen_address = ?1,
                listen_port = ?2,
                max_retries = ?3,
                enable_logging = ?4,
                streaming_first_byte_timeout = ?5,
                streaming_idle_timeout = ?6,
                non_streaming_timeout = ?7,
                updated_at = datetime('now')",
            rusqlite::params![
                config.listen_address,
                config.listen_port as i32,
                config.max_retries as i32,
                if config.enable_logging { 1 } else { 0 },
                config.streaming_first_byte_timeout as i32,
                config.streaming_idle_timeout as i32,
                config.non_streaming_timeout as i32,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Set the live takeover state (legacy API, superseded by the enabled field)
    pub async fn set_live_takeover_active(&self, _active: bool) -> Result<(), AppError> {
        // This field is no longer used; the enabled field replaces it.
        // The empty implementation is kept for compatibility with old code.
        Ok(())
    }

    /// Check whether live takeover mode is active
    ///
    /// Checks whether any app has enabled = true
    pub async fn is_live_takeover_active(&self) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM proxy_config WHERE enabled = 1",
                [],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(count > 0)
    }

    /// Sync version: check whether any app has enabled = true
    ///
    /// Used by sync paths such as `ProfileService::apply` to decide whether to stop the proxy service.
    pub fn is_live_takeover_active_sync(&self) -> bool {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM proxy_config WHERE enabled = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
            > 0
    }

    // ==================== Provider Health ====================

    /// Get the provider health state
    pub async fn get_provider_health(
        &self,
        provider_id: &str,
        app_type: &str,
    ) -> Result<ProviderHealth, AppError> {
        let result = {
            let conn = lock_conn!(self.conn);

            conn.query_row(
                "SELECT provider_id, app_type, is_healthy, consecutive_failures,
                        last_success_at, last_failure_at, last_error, updated_at
                 FROM provider_health
                 WHERE provider_id = ?1 AND app_type = ?2",
                rusqlite::params![provider_id, app_type],
                |row| {
                    Ok(ProviderHealth {
                        provider_id: row.get(0)?,
                        app_type: row.get(1)?,
                        is_healthy: row.get::<_, i64>(2)? != 0,
                        consecutive_failures: row.get::<_, i64>(3)? as u32,
                        last_success_at: row.get(4)?,
                        last_failure_at: row.get(5)?,
                        last_error: row.get(6)?,
                        updated_at: row.get(7)?,
                    })
                },
            )
        };

        match result {
            Ok(health) => Ok(health),
            // A missing record counts as healthy (state is cleared on shutdown, so a restart starts clean)
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ProviderHealth {
                provider_id: provider_id.to_string(),
                app_type: app_type.to_string(),
                is_healthy: true,
                consecutive_failures: 0,
                last_success_at: None,
                last_failure_at: None,
                last_error: None,
                updated_at: chrono::Utc::now().to_rfc3339(),
            }),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// Update the provider health state
    ///
    /// Uses the default threshold (5); prefer `update_provider_health_with_threshold` to pass the configured threshold
    pub async fn update_provider_health(
        &self,
        provider_id: &str,
        app_type: &str,
        success: bool,
        error_msg: Option<String>,
    ) -> Result<(), AppError> {
        // The default threshold matches CircuitBreakerConfig::default()
        self.update_provider_health_with_threshold(provider_id, app_type, success, error_msg, 5)
            .await
    }

    /// Update the provider health state (with an explicit threshold)
    ///
    /// # Arguments
    /// * `failure_threshold` - how many consecutive failures mark the provider unhealthy
    pub async fn update_provider_health_with_threshold(
        &self,
        provider_id: &str,
        app_type: &str,
        success: bool,
        error_msg: Option<String>,
        failure_threshold: u32,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        let now = chrono::Utc::now().to_rfc3339();

        // Read the current state first
        let current = conn.query_row(
            "SELECT consecutive_failures FROM provider_health
             WHERE provider_id = ?1 AND app_type = ?2",
            rusqlite::params![provider_id, app_type],
            |row| Ok(row.get::<_, i64>(0)? as u32),
        );

        let (is_healthy, consecutive_failures) = if success {
            // Success: reset the failure counter
            (1, 0)
        } else {
            // Failure: increment the failure counter
            let failures = current.unwrap_or(0) + 1;
            // Use the supplied threshold instead of a hardcoded one
            let healthy = if failures >= failure_threshold { 0 } else { 1 };
            (healthy, failures)
        };

        let (last_success_at, last_failure_at) = if success {
            (Some(now.clone()), None)
        } else {
            (None, Some(now.clone()))
        };

        // UPSERT
        conn.execute(
            "INSERT OR REPLACE INTO provider_health
             (provider_id, app_type, is_healthy, consecutive_failures,
              last_success_at, last_failure_at, last_error, updated_at)
             VALUES (?1, ?2, ?3, ?4,
                     COALESCE(?5, (SELECT last_success_at FROM provider_health
                                   WHERE provider_id = ?1 AND app_type = ?2)),
                     COALESCE(?6, (SELECT last_failure_at FROM provider_health
                                   WHERE provider_id = ?1 AND app_type = ?2)),
                     ?7, ?8)",
            rusqlite::params![
                provider_id,
                app_type,
                is_healthy,
                consecutive_failures as i64,
                last_success_at,
                last_failure_at,
                error_msg,
                &now,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Reset the provider health state
    pub async fn reset_provider_health(
        &self,
        provider_id: &str,
        app_type: &str,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        conn.execute(
            "DELETE FROM provider_health WHERE provider_id = ?1 AND app_type = ?2",
            rusqlite::params![provider_id, app_type],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        log::debug!("Reset health status for provider {provider_id} (app: {app_type})");

        Ok(())
    }

    /// Clear the health state of one app (used when turning off a single proxy)
    pub async fn clear_provider_health_for_app(&self, app_type: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        conn.execute(
            "DELETE FROM provider_health WHERE app_type = ?1",
            [app_type],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        log::debug!("Cleared provider health records for app {app_type}");
        Ok(())
    }

    /// Clear all provider health state (called when the proxy stops)
    pub async fn clear_all_provider_health(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        conn.execute("DELETE FROM provider_health", [])
            .map_err(|e| AppError::Database(e.to_string()))?;

        log::debug!("Cleared all provider health records");
        Ok(())
    }

    // ==================== Circuit Breaker Config (Legacy Compatibility) ====================

    /// Get the circuit breaker config (legacy API, read from the claude row)
    ///
    /// Circuit breaker settings now live in the proxy_config table, one row per app.
    /// This method is kept for compatibility; prefer get_proxy_config_for_app.
    pub async fn get_circuit_breaker_config(
        &self,
    ) -> Result<crate::proxy::circuit_breaker::CircuitBreakerConfig, AppError> {
        // Scope conn to a block so the lock is not held across an await
        let result = {
            let conn = lock_conn!(self.conn);
            conn.query_row(
                "SELECT circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                        circuit_error_rate_threshold, circuit_min_requests
                 FROM proxy_config WHERE app_type = 'claude'",
                [],
                |row| {
                    Ok(crate::proxy::circuit_breaker::CircuitBreakerConfig {
                        failure_threshold: row.get::<_, i32>(0)? as u32,
                        success_threshold: row.get::<_, i32>(1)? as u32,
                        timeout_seconds: row.get::<_, i64>(2)? as u64,
                        error_rate_threshold: row.get(3)?,
                        min_requests: row.get::<_, i32>(4)? as u32,
                    })
                },
            )
        };
        // conn was released at the end of the block

        match result {
            Ok(config) => Ok(config),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Initialize the default config when the row is missing
                self.init_proxy_config_rows().await?;
                Ok(crate::proxy::circuit_breaker::CircuitBreakerConfig::default())
            }
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// Update the circuit breaker config (legacy API, updates all three rows)
    ///
    /// Circuit breaker settings now live in the proxy_config table.
    /// This method is kept for compatibility; prefer update_proxy_config_for_app.
    pub async fn update_circuit_breaker_config(
        &self,
        config: &crate::proxy::circuit_breaker::CircuitBreakerConfig,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        // Update the circuit breaker settings of all three rows
        conn.execute(
            "UPDATE proxy_config SET
                circuit_failure_threshold = ?1,
                circuit_success_threshold = ?2,
                circuit_timeout_seconds = ?3,
                circuit_error_rate_threshold = ?4,
                circuit_min_requests = ?5,
                updated_at = datetime('now')",
            rusqlite::params![
                config.failure_threshold as i32,
                config.success_threshold as i32,
                config.timeout_seconds as i64,
                config.error_rate_threshold,
                config.min_requests as i32,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    // ==================== Live Backup ====================

    /// Save a live config backup
    pub async fn save_live_backup(
        &self,
        app_type: &str,
        config_json: &str,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO proxy_live_backup (app_type, original_config, backed_up_at)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![app_type, config_json, now],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        log::info!("Backed up the {app_type} live config");
        Ok(())
    }

    /// Check whether any live config backup exists
    pub async fn has_any_live_backup(&self) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM proxy_live_backup", [], |row| {
                row.get(0)
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(count > 0)
    }

    /// Get a live config backup
    pub async fn get_live_backup(&self, app_type: &str) -> Result<Option<LiveBackup>, AppError> {
        let conn = lock_conn!(self.conn);

        let result = conn.query_row(
            "SELECT app_type, original_config, backed_up_at FROM proxy_live_backup WHERE app_type = ?1",
            rusqlite::params![app_type],
            |row| {
                Ok(LiveBackup {
                    app_type: row.get(0)?,
                    original_config: row.get(1)?,
                    backed_up_at: row.get(2)?,
                })
            },
        );

        match result {
            Ok(backup) => Ok(Some(backup)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// Delete a live config backup
    pub async fn delete_live_backup(&self, app_type: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        conn.execute(
            "DELETE FROM proxy_live_backup WHERE app_type = ?1",
            rusqlite::params![app_type],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        log::info!("Deleted the {app_type} live config backup");
        Ok(())
    }

    /// Delete all live config backups
    pub async fn delete_all_live_backups(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);

        conn.execute("DELETE FROM proxy_live_backup", [])
            .map_err(|e| AppError::Database(e.to_string()))?;

        log::info!("Deleted all live config backups");
        Ok(())
    }

    // ==================== Sync Methods for Tray Menu ====================

    /// Synchronously read the proxy enabled state and auto-failover state of an app
    ///
    /// Used by sync call sites such as tray menu construction
    /// Returns (enabled, auto_failover_enabled)
    pub fn get_proxy_flags_sync(&self, app_type: &str) -> (bool, bool) {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return (false, false),
        };

        conn.query_row(
            "SELECT enabled, auto_failover_enabled FROM proxy_config WHERE app_type = ?1",
            [app_type],
            |row| Ok((row.get::<_, i32>(0)? != 0, row.get::<_, i32>(1)? != 0)),
        )
        .unwrap_or((false, false))
    }

    /// Synchronously set the proxy enabled state and auto-failover state of an app
    ///
    /// Used by sync call sites such as tray menu clicks
    pub fn set_proxy_flags_sync(
        &self,
        app_type: &str,
        enabled: bool,
        auto_failover_enabled: bool,
    ) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(format!("Mutex lock failed: {e}")))?;

        conn.execute(
            "UPDATE proxy_config SET enabled = ?2, auto_failover_enabled = ?3, updated_at = datetime('now') WHERE app_type = ?1",
            rusqlite::params![
                app_type,
                if enabled { 1 } else { 0 },
                if auto_failover_enabled { 1 } else { 0 },
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::database::Database;
    use crate::error::AppError;

    #[tokio::test]
    async fn test_default_cost_multiplier_round_trip() -> Result<(), AppError> {
        let db = Database::memory()?;

        let default = db.get_default_cost_multiplier("claude").await?;
        assert_eq!(default, "1");

        db.set_default_cost_multiplier("claude", "1.5").await?;
        let updated = db.get_default_cost_multiplier("claude").await?;
        assert_eq!(updated, "1.5");

        Ok(())
    }

    #[tokio::test]
    async fn test_default_cost_multiplier_validation() -> Result<(), AppError> {
        let db = Database::memory()?;

        let err = db
            .set_default_cost_multiplier("claude", "not-a-number")
            .await
            .unwrap_err();
        // AppError::localized returns AppError::Localized variant
        assert!(matches!(
            err,
            AppError::Localized {
                key: "error.invalidMultiplier",
                ..
            }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_pricing_model_source_round_trip_and_validation() -> Result<(), AppError> {
        let db = Database::memory()?;

        let default = db.get_pricing_model_source("claude").await?;
        assert_eq!(default, "response");

        db.set_pricing_model_source("claude", "request").await?;
        let updated = db.get_pricing_model_source("claude").await?;
        assert_eq!(updated, "request");

        let err = db
            .set_pricing_model_source("claude", "invalid")
            .await
            .unwrap_err();
        // AppError::localized returns AppError::Localized variant
        assert!(matches!(
            err,
            AppError::Localized {
                key: "error.invalidPricingMode",
                ..
            }
        ));

        let err = db
            .set_default_cost_multiplier("claude", "-0.5")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            AppError::Localized {
                key: "error.invalidMultiplier",
                ..
            }
        ));

        Ok(())
    }
}
