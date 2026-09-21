//! Schema definitions and migrations
//!
//! Responsible for creating the database tables and migrating between versions.

use super::{lock_conn, Database, SCHEMA_VERSION};
use crate::error::AppError;
use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Serialize)]
struct LegacySkillMigrationRow {
    directory: String,
    app_type: String,
}

impl Database {
    /// Create all database tables
    pub(crate) fn create_tables(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        Self::create_tables_on_conn(&conn)
    }

    /// Create the tables on a given connection (used by migrations and tests)
    pub(crate) fn create_tables_on_conn(conn: &Connection) -> Result<(), AppError> {
        // 1. Providers table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS providers (
                id TEXT NOT NULL,
                app_type TEXT NOT NULL,
                name TEXT NOT NULL,
                settings_config TEXT NOT NULL,
                website_url TEXT,
                category TEXT,
                created_at INTEGER,
                sort_index INTEGER,
                notes TEXT,
                icon TEXT,
                icon_color TEXT,
                meta TEXT NOT NULL DEFAULT '{}',
                is_current BOOLEAN NOT NULL DEFAULT 0,
                in_failover_queue BOOLEAN NOT NULL DEFAULT 0,
                PRIMARY KEY (id, app_type)
            )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // 2. Provider Endpoints table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS provider_endpoints (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider_id TEXT NOT NULL,
                app_type TEXT NOT NULL,
                url TEXT NOT NULL,
                added_at INTEGER,
                FOREIGN KEY (provider_id, app_type) REFERENCES providers(id, app_type) ON DELETE CASCADE
            )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // 3. MCP Servers table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS mcp_servers (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, server_config TEXT NOT NULL,
            description TEXT, homepage TEXT, docs TEXT, tags TEXT NOT NULL DEFAULT '[]',
            enabled_claude BOOLEAN NOT NULL DEFAULT 0, enabled_codex BOOLEAN NOT NULL DEFAULT 0,
            enabled_gemini BOOLEAN NOT NULL DEFAULT 0, enabled_grokbuild BOOLEAN NOT NULL DEFAULT 0,
            enabled_opencode BOOLEAN NOT NULL DEFAULT 0,
            enabled_hermes BOOLEAN NOT NULL DEFAULT 0,
            enabled_claude_desktop BOOLEAN NOT NULL DEFAULT 0
        )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // 4. Prompts table
        conn.execute("CREATE TABLE IF NOT EXISTS prompts (
            id TEXT NOT NULL, app_type TEXT NOT NULL, name TEXT NOT NULL, content TEXT NOT NULL,
            description TEXT, enabled BOOLEAN NOT NULL DEFAULT 1, created_at INTEGER, updated_at INTEGER,
            PRIMARY KEY (id, app_type)
        )", []).map_err(|e| AppError::Database(e.to_string()))?;

        // 5. Skills table (unified structure since v3.10.0)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS skills (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            directory TEXT NOT NULL,
            repo_owner TEXT,
            repo_name TEXT,
            repo_branch TEXT DEFAULT 'main',
            readme_url TEXT,
            enabled_claude BOOLEAN NOT NULL DEFAULT 0,
            enabled_codex BOOLEAN NOT NULL DEFAULT 0,
            enabled_gemini BOOLEAN NOT NULL DEFAULT 0,
            enabled_grokbuild BOOLEAN NOT NULL DEFAULT 0,
            enabled_opencode BOOLEAN NOT NULL DEFAULT 0,
            enabled_hermes BOOLEAN NOT NULL DEFAULT 0,
            installed_at INTEGER NOT NULL DEFAULT 0,
            content_hash TEXT,
            updated_at INTEGER NOT NULL DEFAULT 0
        )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // 6. Skill Repos table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS skill_repos (
            owner TEXT NOT NULL, name TEXT NOT NULL, branch TEXT NOT NULL DEFAULT 'main',
            enabled BOOLEAN NOT NULL DEFAULT 1, PRIMARY KEY (owner, name)
        )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // 7. Settings table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // 8. Proxy Config table (three rows, keyed by app_type)
        conn.execute("CREATE TABLE IF NOT EXISTS proxy_config (
            app_type TEXT PRIMARY KEY CHECK (app_type IN ('claude','codex','gemini','grokbuild')),
            proxy_enabled INTEGER NOT NULL DEFAULT 0, listen_address TEXT NOT NULL DEFAULT '127.0.0.1',
            listen_port INTEGER NOT NULL DEFAULT 15721, enable_logging INTEGER NOT NULL DEFAULT 1,
            enabled INTEGER NOT NULL DEFAULT 0, auto_failover_enabled INTEGER NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL DEFAULT 3, streaming_first_byte_timeout INTEGER NOT NULL DEFAULT 60,
            streaming_idle_timeout INTEGER NOT NULL DEFAULT 120, non_streaming_timeout INTEGER NOT NULL DEFAULT 600,
            circuit_failure_threshold INTEGER NOT NULL DEFAULT 4, circuit_success_threshold INTEGER NOT NULL DEFAULT 2,
            circuit_timeout_seconds INTEGER NOT NULL DEFAULT 60, circuit_error_rate_threshold REAL NOT NULL DEFAULT 0.6,
            circuit_min_requests INTEGER NOT NULL DEFAULT 10,
            default_cost_multiplier TEXT NOT NULL DEFAULT '1',
            pricing_model_source TEXT NOT NULL DEFAULT 'response',
            created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )", []).map_err(|e| AppError::Database(e.to_string()))?;

        // Seed the three rows (different defaults per app)
        //
        // Legacy database compatibility:
        // - older proxy_config was a singleton table (no app_type column), where the three-row seed insert must not run;
        // - such a table is converted to the three-row layout in apply_schema_migrations() and seeded afterwards.
        if Self::has_column(conn, "proxy_config", "app_type")? {
            conn.execute(
                "INSERT OR IGNORE INTO proxy_config (app_type, max_retries,
                streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                circuit_error_rate_threshold, circuit_min_requests)
                VALUES ('claude', 6, 90, 180, 600, 8, 3, 90, 0.7, 15)",
                [],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
            conn.execute(
                "INSERT OR IGNORE INTO proxy_config (app_type, max_retries,
                streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                circuit_error_rate_threshold, circuit_min_requests)
                VALUES ('codex', 3, 60, 120, 600, 4, 2, 60, 0.6, 10)",
                [],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
            conn.execute(
                "INSERT OR IGNORE INTO proxy_config (app_type, max_retries,
                streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                circuit_error_rate_threshold, circuit_min_requests)
                VALUES ('gemini', 5, 60, 120, 600, 4, 2, 60, 0.6, 10)",
                [],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
            conn.execute(
                "INSERT OR IGNORE INTO proxy_config (app_type, max_retries,
                streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                circuit_error_rate_threshold, circuit_min_requests)
                VALUES ('grokbuild', 3, 60, 120, 600, 4, 2, 60, 0.6, 10)",
                [],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        // 9. Provider Health table
        conn.execute("CREATE TABLE IF NOT EXISTS provider_health (
            provider_id TEXT NOT NULL, app_type TEXT NOT NULL, is_healthy INTEGER NOT NULL DEFAULT 1,
            consecutive_failures INTEGER NOT NULL DEFAULT 0, last_success_at TEXT, last_failure_at TEXT,
            last_error TEXT, updated_at TEXT NOT NULL,
            PRIMARY KEY (provider_id, app_type),
            FOREIGN KEY (provider_id, app_type) REFERENCES providers(id, app_type) ON DELETE CASCADE
        )", []).map_err(|e| AppError::Database(e.to_string()))?;

        // 10. Proxy Request Logs table
        // pricing_model = the model name actually used for pricing at write time (the resolved
        // pricing_model_source); backfill reprices from it. NULL marks rows from before v11, '' marks unpriced error rows.
        conn.execute("CREATE TABLE IF NOT EXISTS proxy_request_logs (
            request_id TEXT PRIMARY KEY, provider_id TEXT NOT NULL, app_type TEXT NOT NULL, model TEXT NOT NULL,
            request_model TEXT,
            pricing_model TEXT,
            input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0, cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
            input_token_semantics INTEGER NOT NULL DEFAULT 0,
            input_cost_usd TEXT NOT NULL DEFAULT '0', output_cost_usd TEXT NOT NULL DEFAULT '0',
            cache_read_cost_usd TEXT NOT NULL DEFAULT '0', cache_creation_cost_usd TEXT NOT NULL DEFAULT '0',
            total_cost_usd TEXT NOT NULL DEFAULT '0', latency_ms INTEGER NOT NULL, first_token_ms INTEGER,
            duration_ms INTEGER, status_code INTEGER NOT NULL, error_message TEXT, session_id TEXT,
            provider_type TEXT, is_streaming INTEGER NOT NULL DEFAULT 0,
            cost_multiplier TEXT NOT NULL DEFAULT '1.0', created_at INTEGER NOT NULL,
            data_source TEXT NOT NULL DEFAULT 'proxy'
        )", []).map_err(|e| AppError::Database(e.to_string()))?;

        conn.execute("CREATE INDEX IF NOT EXISTS idx_request_logs_provider ON proxy_request_logs(provider_id, app_type)", [])
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_request_logs_created_at ON proxy_request_logs(created_at)", [])
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_logs_model ON proxy_request_logs(model)",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_logs_session ON proxy_request_logs(session_id)",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_logs_status ON proxy_request_logs(status_code)",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Self::create_request_logs_usage_indexes_if_supported(conn)?;

        // 11. Model Pricing table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS model_pricing (
            model_id TEXT PRIMARY KEY, display_name TEXT NOT NULL,
            input_cost_per_million TEXT NOT NULL, output_cost_per_million TEXT NOT NULL,
            cache_read_cost_per_million TEXT NOT NULL DEFAULT '0',
            cache_creation_cost_per_million TEXT NOT NULL DEFAULT '0'
        )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // 12. Stream Check Logs table
        conn.execute("CREATE TABLE IF NOT EXISTS stream_check_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT, provider_id TEXT NOT NULL, provider_name TEXT NOT NULL,
            app_type TEXT NOT NULL, status TEXT NOT NULL, success INTEGER NOT NULL, message TEXT NOT NULL,
            response_time_ms INTEGER, http_status INTEGER, model_used TEXT,
            retry_count INTEGER DEFAULT 0, tested_at INTEGER NOT NULL
        )", []).map_err(|e| AppError::Database(e.to_string()))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_stream_check_logs_provider
             ON stream_check_logs(app_type, provider_id, tested_at DESC)",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Note: circuit_breaker_config has been merged into the proxy_config table

        // 16. Proxy Live Backup table (live config backups)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS proxy_live_backup (
            app_type TEXT PRIMARY KEY, original_config TEXT NOT NULL, backed_up_at TEXT NOT NULL
        )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // 17. Usage Daily Rollups table (daily aggregates)
        // request_model preserves the "client alias -> real model" mapping dimension of route takeover,
        // pricing_model preserves the pricing basis at write time (which diverges from model in request pricing mode);
        // without them, takeover billing is unauditable once detail rows are pruned. Migrated legacy rows get '' (unknown).
        conn.execute(
            "CREATE TABLE IF NOT EXISTS usage_daily_rollups (
                date TEXT NOT NULL,
                app_type TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                model TEXT NOT NULL,
                request_model TEXT NOT NULL DEFAULT '',
                pricing_model TEXT NOT NULL DEFAULT '',
                request_count INTEGER NOT NULL DEFAULT 0,
                success_count INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                input_token_semantics INTEGER NOT NULL DEFAULT 0,
                total_cost_usd TEXT NOT NULL DEFAULT '0',
                avg_latency_ms INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (date, app_type, provider_id, model, request_model, pricing_model)
            )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // 18. Session Log Sync table (session log sync state)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS session_log_sync (
                file_path TEXT PRIMARY KEY,
                last_modified INTEGER NOT NULL,
                last_line_offset INTEGER NOT NULL DEFAULT 0,
                last_synced_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Session detail rows are pruned after rollup, so request IDs needed
        // for fork/rewrite deduplication live in a compact durable ledger.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS session_usage_dedup (
                data_source TEXT NOT NULL,
                request_id TEXT NOT NULL,
                semantic_id TEXT NOT NULL,
                has_entry_id INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (data_source, request_id)
            )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_session_usage_dedup_semantic
             ON session_usage_dedup(data_source, semantic_id, has_entry_id)",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Product-owned, post-verification tool version transitions (ADR-0019).
        Self::ensure_tool_version_events_table(conn)?;

        // 20. Profiles table (project entities shared across all apps; payload holds per-app slotted
        //     snapshots of providers/MCP/skills/prompts, while each app group's current marker lives in settings)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                payload TEXT NOT NULL,
                sort_order INTEGER,
                created_at INTEGER,
                updated_at INTEGER
            )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Repair databases that ran an unreleased dev build: the current marker used to be a global
        // key and is now per app group (finalized as current_profile_id_<scope> in v12, no separate version bump)
        if conn
            .execute(
                "INSERT OR REPLACE INTO settings (key, value)
                 SELECT 'current_profile_id_claude', value FROM settings
                 WHERE key = 'current_profile_id'",
                [],
            )
            .is_ok()
        {
            let _ = conn.execute("DELETE FROM settings WHERE key = 'current_profile_id'", []);
        }

        // Try adding the live_takeover_active column to proxy_config
        let _ = conn.execute(
            "ALTER TABLE proxy_config ADD COLUMN live_takeover_active INTEGER NOT NULL DEFAULT 0",
            [],
        );

        // Try adding the base config columns to proxy_config (for upgrades from v3.9.0-2)
        let _ = conn.execute(
            "ALTER TABLE proxy_config ADD COLUMN proxy_enabled INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE proxy_config ADD COLUMN listen_address TEXT NOT NULL DEFAULT '127.0.0.1'",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE proxy_config ADD COLUMN listen_port INTEGER NOT NULL DEFAULT 15721",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE proxy_config ADD COLUMN enable_logging INTEGER NOT NULL DEFAULT 1",
            [],
        );

        // Try adding the timeout config columns to proxy_config
        let _ = conn.execute(
            "ALTER TABLE proxy_config ADD COLUMN streaming_first_byte_timeout INTEGER NOT NULL DEFAULT 60",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE proxy_config ADD COLUMN streaming_idle_timeout INTEGER NOT NULL DEFAULT 120",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE proxy_config ADD COLUMN non_streaming_timeout INTEGER NOT NULL DEFAULT 600",
            [],
        );

        // Compatibility: if an old proxy_config is still a singleton (no app_type), convert it to the three-row layout at startup.
        // Note: with user_version=2 the v1->v2 migration no longer runs, but the new query code depends on the app_type column.
        if Self::table_exists(conn, "proxy_config")?
            && !Self::has_column(conn, "proxy_config", "app_type")?
        {
            Self::migrate_proxy_config_to_per_app(conn)?;
        }

        // Make sure the in_failover_queue column exists (for existing v2 databases)
        Self::add_column_if_missing(
            conn,
            "providers",
            "in_failover_queue",
            "BOOLEAN NOT NULL DEFAULT 0",
        )?;

        // Drop the old failover_queue table if present
        let _ = conn.execute("DROP INDEX IF EXISTS idx_failover_queue_order", []);
        let _ = conn.execute("DROP TABLE IF EXISTS failover_queue", []);

        // Create the failover queue index (on the providers table)
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_providers_failover
             ON providers(app_type, in_failover_queue, sort_index)",
            [],
        );

        Ok(())
    }

    /// Apply schema migrations
    pub(crate) fn apply_schema_migrations(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        Self::apply_schema_migrations_on_conn(&conn)
    }

    /// Apply schema migrations on a given connection
    pub(crate) fn apply_schema_migrations_on_conn(conn: &Connection) -> Result<(), AppError> {
        conn.execute("SAVEPOINT schema_migration;", [])
            .map_err(|e| {
                AppError::Database(format!("Failed to open the migration savepoint: {e}"))
            })?;

        let mut version = Self::get_user_version(conn)?;

        if version > SCHEMA_VERSION {
            conn.execute("ROLLBACK TO schema_migration;", []).ok();
            conn.execute("RELEASE schema_migration;", []).ok();
            return Err(AppError::Database(format!(
                "Database version {version} is newer than this app supports ({SCHEMA_VERSION}); please upgrade the app and try again."
            )));
        }

        let result = (|| {
            while version < SCHEMA_VERSION {
                match version {
                    0 => {
                        log::info!("Detected user_version=0, migrating to 1 (adding missing columns and setting the version)");
                        Self::migrate_v0_to_v1(conn)?;
                        Self::set_user_version(conn, 1)?;
                    }
                    1 => {
                        log::info!(
                            "Migrating the database from v1 to v2 (usage statistics tables, full field set, skills table rebuild)"
                        );
                        Self::migrate_v1_to_v2(conn)?;
                        Self::set_user_version(conn, 2)?;
                    }
                    2 => {
                        log::info!(
                            "Migrating the database from v2 to v3 (unified skills management)"
                        );
                        Self::migrate_v2_to_v3(conn)?;
                        Self::set_user_version(conn, 3)?;
                    }
                    3 => {
                        log::info!("Migrating the database from v3 to v4 (OpenCode support)");
                        Self::migrate_v3_to_v4(conn)?;
                        Self::set_user_version(conn, 4)?;
                    }
                    4 => {
                        log::info!("Migrating the database from v4 to v5 (pricing mode support)");
                        Self::migrate_v4_to_v5(conn)?;
                        Self::set_user_version(conn, 5)?;
                    }
                    5 => {
                        log::info!("Migrating the database from v5 to v6 (usage rollup table + unified Copilot template type)");
                        Self::migrate_v5_to_v6(conn)?;
                        Self::set_user_version(conn, 6)?;
                    }
                    6 => {
                        log::info!(
                            "Migrating the database from v6 to v7 (skills update detection)"
                        );
                        Self::migrate_v6_to_v7(conn)?;
                        Self::set_user_version(conn, 7)?;
                    }
                    7 => {
                        log::info!("Migrating the database from v7 to v8 (session log usage tracking + model pricing fixes)");
                        Self::migrate_v7_to_v8(conn)?;
                        Self::set_user_version(conn, 8)?;
                    }
                    8 => {
                        log::info!("Migrating the database from v8 to v9 (comprehensive model pricing refresh)");
                        Self::migrate_v8_to_v9(conn)?;
                        Self::set_user_version(conn, 9)?;
                    }
                    9 => {
                        log::info!("Migrating the database from v9 to v10 (Hermes Agent support)");
                        Self::migrate_v9_to_v10(conn)?;
                        Self::set_user_version(conn, 10)?;
                    }
                    10 => {
                        log::info!("Migrating the database from v10 to v11 (usage_daily_rollups keeps the request_model dimension)");
                        Self::migrate_v10_to_v11(conn)?;
                        Self::set_user_version(conn, 11)?;
                    }
                    11 => {
                        log::info!(
                            "Migrating the database from v11 to v12 (project profiles table)"
                        );
                        Self::migrate_v11_to_v12(conn)?;
                        Self::set_user_version(conn, 12)?;
                    }
                    12 => {
                        log::info!("Migrating the database from v12 to v13 (record input token cache semantics)");
                        Self::migrate_v12_to_v13(conn)?;
                        Self::set_user_version(conn, 13)?;
                    }
                    13 => {
                        log::info!(
                            "Migrating the database from v13 to v14 (Grok Build proxy config)"
                        );
                        Self::migrate_v13_to_v14(conn)?;
                        Self::set_user_version(conn, 14)?;
                    }
                    14 => {
                        log::info!("Migrating the database from v14 to v15 (Grok Build support for skills/MCP)");
                        Self::migrate_v14_to_v15(conn)?;
                        Self::set_user_version(conn, 15)?;
                    }
                    15 => {
                        log::info!(
                            "Migrating the database from v15 to v16 (rebuild Codex session usage)"
                        );
                        Self::migrate_v15_to_v16(conn)?;
                        Self::set_user_version(conn, 16)?;
                    }
                    16 => {
                        log::info!("Migrating the database from v16 to v17 (persistent session usage dedup ledger)");
                        Self::migrate_v16_to_v17(conn)?;
                        Self::set_user_version(conn, 17)?;
                    }
                    17 => {
                        log::info!("Migrating the database from v17 to v18 (tool version history)");
                        Self::migrate_v17_to_v18(conn)?;
                        Self::set_user_version(conn, 18)?;
                    }
                    18 => {
                        log::info!("Migrating the database from v18 to v19 (uv/pipx sources in tool version history)");
                        Self::migrate_v18_to_v19(conn)?;
                        Self::set_user_version(conn, 19)?;
                    }
                    19 => {
                        log::info!(
                            "Migrating the database from v19 to v20 (Claude Desktop MCP scope)"
                        );
                        Self::migrate_v19_to_v20(conn)?;
                        Self::set_user_version(conn, 20)?;
                    }
                    _ => {
                        return Err(AppError::Database(format!(
                            "Unknown database version {version}, cannot migrate to {SCHEMA_VERSION}"
                        )));
                    }
                }
                version = Self::get_user_version(conn)?;
            }
            Ok(())
        })();

        match result {
            Ok(_) => {
                conn.execute("RELEASE schema_migration;", []).map_err(|e| {
                    AppError::Database(format!("Failed to release the migration savepoint: {e}"))
                })?;
                Ok(())
            }
            Err(e) => {
                conn.execute("ROLLBACK TO schema_migration;", []).ok();
                conn.execute("RELEASE schema_migration;", []).ok();
                Err(e)
            }
        }
    }

    /// v0 -> v1 migration: add every missing column
    fn migrate_v0_to_v1(conn: &Connection) -> Result<(), AppError> {
        // providers table
        Self::add_column_if_missing(conn, "providers", "category", "TEXT")?;
        Self::add_column_if_missing(conn, "providers", "created_at", "INTEGER")?;
        Self::add_column_if_missing(conn, "providers", "sort_index", "INTEGER")?;
        Self::add_column_if_missing(conn, "providers", "notes", "TEXT")?;
        Self::add_column_if_missing(conn, "providers", "icon", "TEXT")?;
        Self::add_column_if_missing(conn, "providers", "icon_color", "TEXT")?;
        Self::add_column_if_missing(conn, "providers", "meta", "TEXT NOT NULL DEFAULT '{}'")?;
        Self::add_column_if_missing(
            conn,
            "providers",
            "is_current",
            "BOOLEAN NOT NULL DEFAULT 0",
        )?;

        // provider_endpoints table
        Self::add_column_if_missing(conn, "provider_endpoints", "added_at", "INTEGER")?;

        // mcp_servers table
        Self::add_column_if_missing(conn, "mcp_servers", "description", "TEXT")?;
        Self::add_column_if_missing(conn, "mcp_servers", "homepage", "TEXT")?;
        Self::add_column_if_missing(conn, "mcp_servers", "docs", "TEXT")?;
        Self::add_column_if_missing(conn, "mcp_servers", "tags", "TEXT NOT NULL DEFAULT '[]'")?;
        Self::add_column_if_missing(
            conn,
            "mcp_servers",
            "enabled_codex",
            "BOOLEAN NOT NULL DEFAULT 0",
        )?;
        Self::add_column_if_missing(
            conn,
            "mcp_servers",
            "enabled_gemini",
            "BOOLEAN NOT NULL DEFAULT 0",
        )?;

        // prompts table
        Self::add_column_if_missing(conn, "prompts", "description", "TEXT")?;
        Self::add_column_if_missing(conn, "prompts", "enabled", "BOOLEAN NOT NULL DEFAULT 1")?;
        Self::add_column_if_missing(conn, "prompts", "created_at", "INTEGER")?;
        Self::add_column_if_missing(conn, "prompts", "updated_at", "INTEGER")?;

        // skills table
        Self::add_column_if_missing(conn, "skills", "installed_at", "INTEGER NOT NULL DEFAULT 0")?;

        // skill_repos table
        Self::add_column_if_missing(
            conn,
            "skill_repos",
            "branch",
            "TEXT NOT NULL DEFAULT 'main'",
        )?;
        Self::add_column_if_missing(conn, "skill_repos", "enabled", "BOOLEAN NOT NULL DEFAULT 1")?;
        // Note: the skills_path column was removed because whole-repo recursive scanning is supported now

        Ok(())
    }

    /// v1 -> v2 migration: add usage statistics tables and the full field set, rebuild the skills table
    fn migrate_v1_to_v2(conn: &Connection) -> Result<(), AppError> {
        // providers table columns
        Self::add_column_if_missing(
            conn,
            "providers",
            "cost_multiplier",
            "TEXT NOT NULL DEFAULT '1.0'",
        )?;
        Self::add_column_if_missing(conn, "providers", "limit_daily_usd", "TEXT")?;
        Self::add_column_if_missing(conn, "providers", "limit_monthly_usd", "TEXT")?;
        Self::add_column_if_missing(conn, "providers", "provider_type", "TEXT")?;
        Self::add_column_if_missing(
            conn,
            "providers",
            "in_failover_queue",
            "BOOLEAN NOT NULL DEFAULT 0",
        )?;

        // Add the proxy timeout config columns
        if Self::table_exists(conn, "proxy_config")? {
            // Base columns missing in older versions
            Self::add_column_if_missing(
                conn,
                "proxy_config",
                "proxy_enabled",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            Self::add_column_if_missing(
                conn,
                "proxy_config",
                "listen_address",
                "TEXT NOT NULL DEFAULT '127.0.0.1'",
            )?;
            Self::add_column_if_missing(
                conn,
                "proxy_config",
                "listen_port",
                "INTEGER NOT NULL DEFAULT 15721",
            )?;
            Self::add_column_if_missing(
                conn,
                "proxy_config",
                "enable_logging",
                "INTEGER NOT NULL DEFAULT 1",
            )?;

            Self::add_column_if_missing(
                conn,
                "proxy_config",
                "streaming_first_byte_timeout",
                "INTEGER NOT NULL DEFAULT 60",
            )?;
            Self::add_column_if_missing(
                conn,
                "proxy_config",
                "streaming_idle_timeout",
                "INTEGER NOT NULL DEFAULT 120",
            )?;
            Self::add_column_if_missing(
                conn,
                "proxy_config",
                "non_streaming_timeout",
                "INTEGER NOT NULL DEFAULT 600",
            )?;
        }

        // Drop the old failover_queue table if present
        conn.execute("DROP INDEX IF EXISTS idx_failover_queue_order", [])
            .map_err(|e| {
                AppError::Database(format!("Failed to drop the failover_queue index: {e}"))
            })?;
        conn.execute("DROP TABLE IF EXISTS failover_queue", [])
            .map_err(|e| {
                AppError::Database(format!("Failed to drop the failover_queue table: {e}"))
            })?;

        // Create the failover index
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_providers_failover
             ON providers(app_type, in_failover_queue, sort_index)",
            [],
        )
        .map_err(|e| AppError::Database(format!("Failed to create the failover index: {e}")))?;

        // proxy_request_logs table
        conn.execute("CREATE TABLE IF NOT EXISTS proxy_request_logs (
            request_id TEXT PRIMARY KEY, provider_id TEXT NOT NULL, app_type TEXT NOT NULL, model TEXT NOT NULL,
            request_model TEXT,
            input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0, cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
            input_token_semantics INTEGER NOT NULL DEFAULT 0,
            input_cost_usd TEXT NOT NULL DEFAULT '0', output_cost_usd TEXT NOT NULL DEFAULT '0',
            cache_read_cost_usd TEXT NOT NULL DEFAULT '0', cache_creation_cost_usd TEXT NOT NULL DEFAULT '0',
            total_cost_usd TEXT NOT NULL DEFAULT '0', latency_ms INTEGER NOT NULL, first_token_ms INTEGER,
            duration_ms INTEGER, status_code INTEGER NOT NULL, error_message TEXT, session_id TEXT,
            provider_type TEXT, is_streaming INTEGER NOT NULL DEFAULT 0,
            cost_multiplier TEXT NOT NULL DEFAULT '1.0', created_at INTEGER NOT NULL
        )", [])?;

        // Add the new columns to an existing table
        Self::add_column_if_missing(conn, "proxy_request_logs", "provider_type", "TEXT")?;
        Self::add_column_if_missing(
            conn,
            "proxy_request_logs",
            "is_streaming",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        Self::add_column_if_missing(
            conn,
            "proxy_request_logs",
            "cost_multiplier",
            "TEXT NOT NULL DEFAULT '1.0'",
        )?;
        Self::add_column_if_missing(conn, "proxy_request_logs", "first_token_ms", "INTEGER")?;
        Self::add_column_if_missing(conn, "proxy_request_logs", "duration_ms", "INTEGER")?;

        // model_pricing table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS model_pricing (
            model_id TEXT PRIMARY KEY, display_name TEXT NOT NULL,
            input_cost_per_million TEXT NOT NULL, output_cost_per_million TEXT NOT NULL,
            cache_read_cost_per_million TEXT NOT NULL DEFAULT '0',
            cache_creation_cost_per_million TEXT NOT NULL DEFAULT '0'
        )",
            [],
        )?;

        // Clear and re-insert the model pricing
        conn.execute("DELETE FROM model_pricing", [])
            .map_err(|e| AppError::Database(format!("Failed to clear the model pricing: {e}")))?;
        Self::seed_model_pricing(conn)?;

        // Rebuild the skills table (adds the app_type column)
        Self::migrate_skills_table(conn)?;

        // Rebuild proxy_config into the three-row layout (per-app configuration)
        Self::migrate_proxy_config_to_per_app(conn)?;

        Ok(())
    }

    /// Convert proxy_config to the three-row layout (per-app configuration)
    fn migrate_proxy_config_to_per_app(conn: &Connection) -> Result<(), AppError> {
        // Check whether the new layout is already in place (idempotency)
        if !Self::table_exists(conn, "proxy_config")? {
            // Table missing, skip the migration (fresh install)
            return Ok(());
        }

        if Self::has_column(conn, "proxy_config", "app_type")? {
            // Already the three-row layout, skip the migration
            log::info!("proxy_config already uses the three-row layout, skipping the migration");
            return Ok(());
        }

        // Read the old config
        let old_config = conn
            .query_row(
                "SELECT listen_address, listen_port, max_retries, enable_logging,
                    streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout
             FROM proxy_config WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i32>(1)?,
                        row.get::<_, i32>(2)?,
                        row.get::<_, i32>(3)?,
                        row.get::<_, i32>(4).unwrap_or(30),
                        row.get::<_, i32>(5).unwrap_or(60),
                        row.get::<_, i32>(6).unwrap_or(300),
                    ))
                },
            )
            .unwrap_or_else(|_| ("127.0.0.1".to_string(), 5000, 3, 1, 30, 60, 300));

        let old_cb = conn.query_row(
            "SELECT failure_threshold, success_threshold, timeout_seconds, error_rate_threshold, min_requests
             FROM circuit_breaker_config WHERE id = 1", [],
            |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?, row.get::<_, i64>(2)?,
                      row.get::<_, f64>(3)?, row.get::<_, i32>(4)?))
        ).unwrap_or((5, 2, 60, 0.5, 10));

        let get_bool = |key: &str| -> bool {
            conn.query_row("SELECT value FROM settings WHERE key = ?", [key], |r| {
                r.get::<_, String>(0)
            })
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false)
        };

        let apps = [
            (
                "claude",
                get_bool("proxy_takeover_claude"),
                get_bool("auto_failover_enabled_claude"),
                6,
                45,
                90,
                8,
                3,
                90,
                0.6,
                15,
            ),
            (
                "codex",
                get_bool("proxy_takeover_codex"),
                get_bool("auto_failover_enabled_codex"),
                3,
                old_config.4,
                old_config.5,
                old_cb.0,
                old_cb.1,
                old_cb.2,
                old_cb.3,
                old_cb.4,
            ),
            (
                "gemini",
                get_bool("proxy_takeover_gemini"),
                get_bool("auto_failover_enabled_gemini"),
                5,
                old_config.4,
                old_config.5,
                old_cb.0,
                old_cb.1,
                old_cb.2,
                old_cb.3,
                old_cb.4,
            ),
            (
                "grokbuild",
                false,
                false,
                3,
                old_config.4,
                old_config.5,
                old_cb.0,
                old_cb.1,
                old_cb.2,
                old_cb.3,
                old_cb.4,
            ),
        ];

        // Create the new table
        conn.execute("DROP TABLE IF EXISTS proxy_config_new", [])?;
        conn.execute("CREATE TABLE proxy_config_new (
            app_type TEXT PRIMARY KEY CHECK (app_type IN ('claude','codex','gemini','grokbuild')),
            proxy_enabled INTEGER NOT NULL DEFAULT 0, listen_address TEXT NOT NULL DEFAULT '127.0.0.1',
            listen_port INTEGER NOT NULL DEFAULT 15721, enable_logging INTEGER NOT NULL DEFAULT 1,
            enabled INTEGER NOT NULL DEFAULT 0, auto_failover_enabled INTEGER NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL DEFAULT 3, streaming_first_byte_timeout INTEGER NOT NULL DEFAULT 60,
            streaming_idle_timeout INTEGER NOT NULL DEFAULT 120, non_streaming_timeout INTEGER NOT NULL DEFAULT 600,
            circuit_failure_threshold INTEGER NOT NULL DEFAULT 4, circuit_success_threshold INTEGER NOT NULL DEFAULT 2,
            circuit_timeout_seconds INTEGER NOT NULL DEFAULT 60, circuit_error_rate_threshold REAL NOT NULL DEFAULT 0.6,
            circuit_min_requests INTEGER NOT NULL DEFAULT 10,
            default_cost_multiplier TEXT NOT NULL DEFAULT '1',
            pricing_model_source TEXT NOT NULL DEFAULT 'response',
            live_takeover_active INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )", [])?;

        // Insert the three config rows
        for (app, takeover, failover, retries, fb, idle, cb_f, cb_s, cb_t, cb_r, cb_m) in apps {
            conn.execute(
                "INSERT INTO proxy_config_new (app_type, proxy_enabled, listen_address, listen_port, enable_logging,
                 enabled, auto_failover_enabled, max_retries, streaming_first_byte_timeout, streaming_idle_timeout,
                 non_streaming_timeout, circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                 circuit_error_rate_threshold, circuit_min_requests)
                 VALUES (?1, 0, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                rusqlite::params![app, old_config.0, old_config.1, old_config.3,
                    if takeover { 1 } else { 0 }, if failover { 1 } else { 0 },
                    retries, fb, idle, old_config.6, cb_f, cb_s, cb_t, cb_r, cb_m]
            ).map_err(|e| AppError::Database(format!("Failed to insert the {app} config: {e}")))?;
        }

        // Swap the table in and clean up
        conn.execute("DROP TABLE IF EXISTS proxy_config", [])?;
        conn.execute("ALTER TABLE proxy_config_new RENAME TO proxy_config", [])?;
        conn.execute("DROP TABLE IF EXISTS circuit_breaker_config", [])?;
        conn.execute("DELETE FROM settings WHERE key LIKE 'proxy_takeover_%'", [])?;
        conn.execute(
            "DELETE FROM settings WHERE key LIKE 'auto_failover_enabled_%'",
            [],
        )?;

        log::info!("proxy_config has been converted to the three-row layout");
        Ok(())
    }

    /// Migrate the skills table: from a single-key primary key to the composite (directory, app_type) key
    fn migrate_skills_table(conn: &Connection) -> Result<(), AppError> {
        // The v3 layout (unified management) is already a newer skills table:
        // - the primary key is id
        // - it has the enabled_claude / enabled_codex / enabled_gemini columns
        // In that case the v1 -> v2 migration must not run, or it would fail on mismatched columns.
        if Self::has_column(conn, "skills", "enabled_claude")?
            || Self::has_column(conn, "skills", "id")?
        {
            log::info!("skills table already uses the v3 layout, skipping the v1 -> v2 migration");
            return Ok(());
        }

        // Check whether the new layout is already in place
        if Self::has_column(conn, "skills", "app_type")? {
            log::info!("skills table already has the app_type column, skipping the migration");
            return Ok(());
        }

        log::info!("Starting the skills table migration...");

        // 1. Rename the old table
        conn.execute("ALTER TABLE skills RENAME TO skills_old", [])
            .map_err(|e| {
                AppError::Database(format!("Failed to rename the old skills table: {e}"))
            })?;

        // 2. Create the new table
        conn.execute(
            "CREATE TABLE skills (
                directory TEXT NOT NULL,
                app_type TEXT NOT NULL,
                installed BOOLEAN NOT NULL DEFAULT 0,
                installed_at INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (directory, app_type)
            )",
            [],
        )
        .map_err(|e| AppError::Database(format!("Failed to create the new skills table: {e}")))?;

        // 3. Migrate the data: parse the key format (e.g. "claude:my-skill" or "codex:foo").
        //    Legacy data without a prefix defaults to claude.
        let mut stmt = conn
            .prepare("SELECT key, installed, installed_at FROM skills_old")
            .map_err(|e| AppError::Database(format!("Failed to query the old skills data: {e}")))?;

        let old_skills: Vec<(String, bool, i64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| AppError::Database(format!("Failed to read the old skills data: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(format!("Failed to parse the old skills data: {e}")))?;

        let count = old_skills.len();

        for (key, installed, installed_at) in old_skills {
            // Parse the key: "app:directory" or "directory" (defaults to claude)
            let (app_type, directory) = if let Some(idx) = key.find(':') {
                let (app, dir) = key.split_at(idx);
                (app.to_string(), dir[1..].to_string()) // skip the colon
            } else {
                ("claude".to_string(), key.clone())
            };

            conn.execute(
                "INSERT INTO skills (directory, app_type, installed, installed_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![directory, app_type, installed, installed_at],
            )
            .map_err(|e| {
                AppError::Database(format!("Failed to migrate skill {key} into the new table: {e}"))
            })?;
        }

        // 4. Drop the old table
        conn.execute("DROP TABLE skills_old", [])
            .map_err(|e| AppError::Database(format!("Failed to drop the old skills table: {e}")))?;

        log::info!("skills table migration finished, {count} records migrated");
        Ok(())
    }

    /// v2 -> v3 migration: unified skills management
    ///
    /// Migrate the skills table from the composite (directory, app_type) key to a unified id primary
    /// key with per-app enable flags (enabled_claude, enabled_codex, enabled_gemini).
    ///
    /// Strategy:
    /// 1. The old database only stored install records; the real skill files live on the filesystem.
    /// 2. Rebuild the table outright and let SkillService rescan the filesystem on the next startup.
    fn migrate_v2_to_v3(conn: &Connection) -> Result<(), AppError> {
        // Check whether the new layout is in place (by looking for the enabled_claude column)
        if Self::has_column(conn, "skills", "enabled_claude")? {
            log::info!("skills table already uses the v3 layout, skipping the migration");
            return Ok(());
        }

        log::info!("Starting the skills table migration to the v3 layout (unified management)...");

        // 1. Back up the old data (for logging and the follow-up startup migration)
        let old_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM skills", [], |row| row.get(0))
            .unwrap_or(0);
        log::info!("The old skills table holds {old_count} records");

        let mut stmt = conn
            .prepare(
                "SELECT directory, app_type FROM skills
                 WHERE installed = 1",
            )
            .map_err(|e| {
                AppError::Database(format!("Failed to query the old skills snapshot: {e}"))
            })?;
        let snapshot_rows: Vec<LegacySkillMigrationRow> = stmt
            .query_map([], |row| {
                Ok(LegacySkillMigrationRow {
                    directory: row.get(0)?,
                    app_type: row.get(1)?,
                })
            })
            .map_err(|e| {
                AppError::Database(format!("Failed to read the old skills snapshot: {e}"))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                AppError::Database(format!("Failed to parse the old skills snapshot: {e}"))
            })?;
        let snapshot_json = serde_json::to_string(&snapshot_rows).map_err(|e| {
            AppError::Database(format!("Failed to serialize the old skills snapshot: {e}"))
        })?;

        // Flag: after startup, scan the filesystem and rebuild the skills data
        // Rationale: the v3 layout moves the skills SSOT to the product filesystem directory, while
        // the old table only stored install records, so a lossless migration is impossible; the app
        // directories are scanned and imported after startup instead.
        let _ = conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('skills_ssot_migration_pending', 'true')",
            [],
        );
        let _ = conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('skills_ssot_migration_snapshot', ?1)",
            [snapshot_json],
        );

        // 2. Drop the old table
        conn.execute("DROP TABLE IF EXISTS skills", [])
            .map_err(|e| AppError::Database(format!("Failed to drop the old skills table: {e}")))?;

        // 3. Create the new table
        conn.execute(
            "CREATE TABLE skills (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                directory TEXT NOT NULL,
                repo_owner TEXT,
                repo_name TEXT,
                repo_branch TEXT DEFAULT 'main',
                readme_url TEXT,
                enabled_claude BOOLEAN NOT NULL DEFAULT 0,
                enabled_codex BOOLEAN NOT NULL DEFAULT 0,
                enabled_gemini BOOLEAN NOT NULL DEFAULT 0,
                installed_at INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .map_err(|e| AppError::Database(format!("Failed to create the new skills table: {e}")))?;

        log::info!(
            "The skills table has been migrated to the v3 layout.\n\
             Note: old install records were cleared; the filesystem will be rescanned on first startup."
        );

        Ok(())
    }

    /// v3 -> v4 migration: OpenCode support
    ///
    /// Adds the enabled_opencode column to the mcp_servers and skills tables.
    fn migrate_v3_to_v4(conn: &Connection) -> Result<(), AppError> {
        // Add enabled_opencode to mcp_servers
        Self::add_column_if_missing(
            conn,
            "mcp_servers",
            "enabled_opencode",
            "BOOLEAN NOT NULL DEFAULT 0",
        )?;

        // Add enabled_opencode to skills
        Self::add_column_if_missing(
            conn,
            "skills",
            "enabled_opencode",
            "BOOLEAN NOT NULL DEFAULT 0",
        )?;

        log::info!("v3 -> v4 migration finished: OpenCode support added");
        Ok(())
    }

    /// v4 -> v5 migration: pricing mode configuration and request model columns
    fn migrate_v4_to_v5(conn: &Connection) -> Result<(), AppError> {
        if Self::table_exists(conn, "proxy_config")? {
            Self::add_column_if_missing(
                conn,
                "proxy_config",
                "default_cost_multiplier",
                "TEXT NOT NULL DEFAULT '1'",
            )?;
            Self::add_column_if_missing(
                conn,
                "proxy_config",
                "pricing_model_source",
                "TEXT NOT NULL DEFAULT 'response'",
            )?;
        }
        if Self::table_exists(conn, "proxy_request_logs")? {
            Self::add_column_if_missing(conn, "proxy_request_logs", "request_model", "TEXT")?;
        }

        log::info!("v4 -> v5 migration finished: pricing mode and request model columns added");
        Ok(())
    }

    /// v5 -> v6 migration: usage daily rollup table + unified Copilot template type
    fn migrate_v5_to_v6(conn: &Connection) -> Result<(), AppError> {
        // 1. Add the usage daily rollup table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS usage_daily_rollups (
                date TEXT NOT NULL,
                app_type TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                model TEXT NOT NULL,
                request_count INTEGER NOT NULL DEFAULT 0,
                success_count INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                total_cost_usd TEXT NOT NULL DEFAULT '0',
                avg_latency_ms INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (date, app_type, provider_id, model)
            )",
            [],
        )
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to create the usage_daily_rollups table: {e}"
            ))
        })?;

        // 2. Unify the Copilot template type as github_copilot
        let mut stmt = conn
            .prepare("SELECT id, app_type, meta FROM providers")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut updates = Vec::new();
        for row in rows {
            let (id, app_type, meta_str) = row.map_err(|e| AppError::Database(e.to_string()))?;

            if let Ok(mut meta) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                let mut updated = false;

                if let Some(usage_script) = meta.get_mut("usage_script") {
                    if let Some(template_type) = usage_script.get_mut("template_type") {
                        if template_type == "copilot" {
                            *template_type =
                                serde_json::Value::String("github_copilot".to_string());
                            updated = true;
                        }
                    }
                }

                if updated {
                    let new_meta_str = serde_json::to_string(&meta)
                        .map_err(|e| AppError::Database(e.to_string()))?;
                    updates.push((id, app_type, new_meta_str));
                }
            }
        }

        for (id, app_type, new_meta) in updates {
            conn.execute(
                "UPDATE providers SET meta = ?1 WHERE id = ?2 AND app_type = ?3",
                params![new_meta, id, app_type],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        log::info!(
            "v5 -> v6 migration finished: usage rollup table added, copilot template type unified"
        );
        Ok(())
    }

    /// v6 -> v7: skills update detection (content_hash + updated_at)
    fn migrate_v6_to_v7(conn: &Connection) -> Result<(), AppError> {
        if Self::table_exists(conn, "skills")? {
            Self::add_column_if_missing(conn, "skills", "content_hash", "TEXT")?;
            Self::add_column_if_missing(
                conn,
                "skills",
                "updated_at",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
        }
        log::info!("v6 -> v7 migration finished: content_hash and updated_at columns added");
        Ok(())
    }

    /// v7 -> v8: session log usage tracking (statistics without proxy mode)
    fn migrate_v7_to_v8(conn: &Connection) -> Result<(), AppError> {
        // 1. Add the data_source column to proxy_request_logs to distinguish the data origin
        if Self::table_exists(conn, "proxy_request_logs")? {
            Self::add_column_if_missing(
                conn,
                "proxy_request_logs",
                "data_source",
                "TEXT NOT NULL DEFAULT 'proxy'",
            )?;
            Self::create_request_logs_usage_indexes_if_supported(conn)?;
        }

        // 2. Create the session log sync state table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS session_log_sync (
                file_path TEXT PRIMARY KEY,
                last_modified INTEGER NOT NULL,
                last_line_offset INTEGER NOT NULL DEFAULT 0,
                last_synced_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| {
            AppError::Database(format!("Failed to create the session_log_sync table: {e}"))
        })?;

        // 3. Fix Chinese-vendor model pricing: CNY values had been stored in the USD columns; convert to USD
        if Self::table_exists(conn, "model_pricing")? {
            let pricing_fixes: &[(&str, &str, &str, &str, &str)] = &[
                ("deepseek-v3.2", "0.28", "0.42", "0.028", "0"),
                ("deepseek-v3.1", "0.55", "1.67", "0.055", "0"),
                ("deepseek-v3", "0.28", "1.11", "0.028", "0"),
                ("doubao-seed-code", "0.17", "1.11", "0.02", "0"),
                ("kimi-k2-thinking", "0.55", "2.20", "0.10", "0"),
                ("kimi-k2-0905", "0.55", "2.20", "0.10", "0"),
                ("kimi-k2-turbo", "1.11", "8.06", "0.14", "0"),
                ("minimax-m2.1", "0.27", "0.95", "0.03", "0"),
                ("minimax-m2.1-lightning", "0.27", "2.33", "0.03", "0"),
                ("minimax-m2", "0.27", "0.95", "0.03", "0"),
                ("glm-4.7", "0.39", "1.75", "0.04", "0"),
                ("glm-4.6", "0.28", "1.11", "0.03", "0"),
                ("mimo-v2-flash", "0.09", "0.29", "0.009", "0"),
            ];
            for (model_id, input, output, cache_read, cache_creation) in pricing_fixes {
                conn.execute(
                    "UPDATE model_pricing SET
                        input_cost_per_million = ?2,
                        output_cost_per_million = ?3,
                        cache_read_cost_per_million = ?4,
                        cache_creation_cost_per_million = ?5
                     WHERE model_id = ?1",
                    rusqlite::params![model_id, input, output, cache_read, cache_creation],
                )
                .map_err(|e| {
                    AppError::Database(format!(
                        "Failed to update the pricing of model {model_id}: {e}"
                    ))
                })?;
            }
        }

        log::info!("v7 -> v8 migration finished: data_source column, session_log_sync table, 13 model prices corrected");
        Ok(())
    }

    /// v8 -> v9: comprehensive model pricing refresh (clear + re-seed)
    fn migrate_v8_to_v9(conn: &Connection) -> Result<(), AppError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS model_pricing (
                model_id TEXT PRIMARY KEY, display_name TEXT NOT NULL,
                input_cost_per_million TEXT NOT NULL, output_cost_per_million TEXT NOT NULL,
                cache_read_cost_per_million TEXT NOT NULL DEFAULT '0',
                cache_creation_cost_per_million TEXT NOT NULL DEFAULT '0'
            )",
            [],
        )
        .map_err(|e| {
            AppError::Database(format!("Failed to create the model_pricing table: {e}"))
        })?;
        conn.execute("DELETE FROM model_pricing", [])
            .map_err(|e| AppError::Database(format!("Failed to clear the model pricing: {e}")))?;
        Self::seed_model_pricing(conn)?;
        log::info!("v8 -> v9 migration finished: all model pricing data refreshed");
        Ok(())
    }

    /// v9 -> v10 migration: Hermes Agent support
    fn migrate_v9_to_v10(conn: &Connection) -> Result<(), AppError> {
        Self::add_column_if_missing(
            conn,
            "mcp_servers",
            "enabled_hermes",
            "BOOLEAN NOT NULL DEFAULT 0",
        )?;

        // skills table may not exist in databases migrated from very old versions
        if Self::table_exists(conn, "skills")? {
            Self::add_column_if_missing(
                conn,
                "skills",
                "enabled_hermes",
                "BOOLEAN NOT NULL DEFAULT 0",
            )?;
        }

        log::info!("v9 -> v10 migration finished: Hermes Agent support added");
        Ok(())
    }

    /// v10 -> v11: usage_daily_rollups gains the request_model dimension (part of the primary key),
    /// and proxy_request_logs gains a pricing_model column (the pricing basis at write time, used by backfill).
    ///
    /// Under route takeover, model (the real upstream model) differs from request_model (the client alias);
    /// the old rollup aggregated by model only, so the mapping was lost forever once detail rows were pruned
    /// and billing became unauditable. Changing the primary key requires rebuilding the table in SQLite; the
    /// request_model of legacy rows is unknowable and is filled with ''.
    fn migrate_v10_to_v11(conn: &Connection) -> Result<(), AppError> {
        // proxy_request_logs.pricing_model: NULL = rows from before v11 (backfill uses the old
        // model -> placeholder -> request_model fallback), '' = unpriced error rows
        if Self::table_exists(conn, "proxy_request_logs")? {
            Self::add_column_if_missing(conn, "proxy_request_logs", "pricing_model", "TEXT")?;
        }

        if !Self::table_exists(conn, "usage_daily_rollups")? {
            log::info!("v10 -> v11: usage_daily_rollups does not exist, skipping the rebuild");
            return Ok(());
        }

        conn.execute_batch(
            "ALTER TABLE usage_daily_rollups RENAME TO usage_daily_rollups_v10;
             CREATE TABLE usage_daily_rollups (
                 date TEXT NOT NULL,
                 app_type TEXT NOT NULL,
                 provider_id TEXT NOT NULL,
                 model TEXT NOT NULL,
                 request_model TEXT NOT NULL DEFAULT '',
                 pricing_model TEXT NOT NULL DEFAULT '',
                 request_count INTEGER NOT NULL DEFAULT 0,
                 success_count INTEGER NOT NULL DEFAULT 0,
                 input_tokens INTEGER NOT NULL DEFAULT 0,
                 output_tokens INTEGER NOT NULL DEFAULT 0,
                 cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                 cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                 total_cost_usd TEXT NOT NULL DEFAULT '0',
                 avg_latency_ms INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (date, app_type, provider_id, model, request_model, pricing_model)
             );
             INSERT INTO usage_daily_rollups
                 (date, app_type, provider_id, model, request_model, pricing_model,
                  request_count, success_count, input_tokens, output_tokens,
                  cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms)
             SELECT date, app_type, provider_id, model, '', '',
                  request_count, success_count, input_tokens, output_tokens,
                  cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
             FROM usage_daily_rollups_v10;
             DROP TABLE usage_daily_rollups_v10;",
        )
        .map_err(|e| {
            AppError::Database(format!(
                "v10 -> v11 failed to rebuild usage_daily_rollups: {e}"
            ))
        })?;

        log::info!(
            "v10 -> v11 migration finished: usage_daily_rollups keeps the request_model/pricing_model dimensions"
        );
        Ok(())
    }

    /// v11 -> v12 migration: add the project profiles table
    /// Kept identical to the CREATE statement in create_tables_on_conn (IF NOT EXISTS makes it idempotent)
    fn migrate_v11_to_v12(conn: &Connection) -> Result<(), AppError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                payload TEXT NOT NULL,
                sort_order INTEGER,
                created_at INTEGER,
                updated_at INTEGER
            )",
            [],
        )
        .map_err(|e| {
            AppError::Database(format!(
                "v11 -> v12 failed to create the profiles table: {e}"
            ))
        })?;
        Ok(())
    }

    /// v12 -> v13: record whether input_tokens includes cache writes.
    ///
    /// The default 0 means legacy/unknown semantics; old Codex rows include cache reads only, not
    /// cache creation. New proxy rows explicitly write 1 (total-inclusive) or 2 (fresh).
    fn migrate_v12_to_v13(conn: &Connection) -> Result<(), AppError> {
        if Self::table_exists(conn, "proxy_request_logs")? {
            Self::add_column_if_missing(
                conn,
                "proxy_request_logs",
                "input_token_semantics",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
        }
        if Self::table_exists(conn, "usage_daily_rollups")? {
            Self::add_column_if_missing(
                conn,
                "usage_daily_rollups",
                "input_token_semantics",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
        }
        Ok(())
    }

    /// v13 -> v14: allow Grok Build to own an independent proxy configuration row.
    fn migrate_v13_to_v14(conn: &Connection) -> Result<(), AppError> {
        if !Self::table_exists(conn, "proxy_config")? {
            return Ok(());
        }

        conn.execute("DROP TABLE IF EXISTS proxy_config_v14", [])
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "CREATE TABLE proxy_config_v14 (
                app_type TEXT PRIMARY KEY CHECK (app_type IN ('claude','codex','gemini','grokbuild')),
                proxy_enabled INTEGER NOT NULL DEFAULT 0,
                listen_address TEXT NOT NULL DEFAULT '127.0.0.1',
                listen_port INTEGER NOT NULL DEFAULT 15721,
                enable_logging INTEGER NOT NULL DEFAULT 1,
                enabled INTEGER NOT NULL DEFAULT 0,
                auto_failover_enabled INTEGER NOT NULL DEFAULT 0,
                max_retries INTEGER NOT NULL DEFAULT 3,
                streaming_first_byte_timeout INTEGER NOT NULL DEFAULT 60,
                streaming_idle_timeout INTEGER NOT NULL DEFAULT 120,
                non_streaming_timeout INTEGER NOT NULL DEFAULT 600,
                circuit_failure_threshold INTEGER NOT NULL DEFAULT 4,
                circuit_success_threshold INTEGER NOT NULL DEFAULT 2,
                circuit_timeout_seconds INTEGER NOT NULL DEFAULT 60,
                circuit_error_rate_threshold REAL NOT NULL DEFAULT 0.6,
                circuit_min_requests INTEGER NOT NULL DEFAULT 10,
                default_cost_multiplier TEXT NOT NULL DEFAULT '1',
                pricing_model_source TEXT NOT NULL DEFAULT 'response',
                live_takeover_active INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        let copied_columns = [
            ("app_type", "'claude'"),
            ("proxy_enabled", "0"),
            ("listen_address", "'127.0.0.1'"),
            ("listen_port", "15721"),
            ("enable_logging", "1"),
            ("enabled", "0"),
            ("auto_failover_enabled", "0"),
            ("max_retries", "3"),
            ("streaming_first_byte_timeout", "60"),
            ("streaming_idle_timeout", "120"),
            ("non_streaming_timeout", "600"),
            ("circuit_failure_threshold", "4"),
            ("circuit_success_threshold", "2"),
            ("circuit_timeout_seconds", "60"),
            ("circuit_error_rate_threshold", "0.6"),
            ("circuit_min_requests", "10"),
            ("default_cost_multiplier", "'1'"),
            ("pricing_model_source", "'response'"),
            ("live_takeover_active", "0"),
            ("created_at", "datetime('now')"),
            ("updated_at", "datetime('now')"),
        ]
        .into_iter()
        .map(|(column, fallback)| {
            Self::has_column(conn, "proxy_config", column).map(|exists| {
                if exists {
                    format!("\"{column}\"")
                } else {
                    fallback.into()
                }
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?
        .join(", ");

        let copy_sql = format!(
            "INSERT INTO proxy_config_v14 (
                app_type, proxy_enabled, listen_address, listen_port, enable_logging,
                enabled, auto_failover_enabled, max_retries,
                streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout,
                circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds,
                circuit_error_rate_threshold, circuit_min_requests,
                default_cost_multiplier, pricing_model_source, live_takeover_active,
                created_at, updated_at
            )
            SELECT {copied_columns} FROM proxy_config"
        );
        conn.execute(&copy_sql, [])
            .map_err(|e| AppError::Database(e.to_string()))?;

        conn.execute("DROP TABLE proxy_config", [])
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute("ALTER TABLE proxy_config_v14 RENAME TO proxy_config", [])
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "INSERT OR IGNORE INTO proxy_config (app_type) VALUES ('grokbuild')",
            [],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// v14 -> v15: persist Grok Build enablement for unified Skills and MCP.
    fn migrate_v14_to_v15(conn: &Connection) -> Result<(), AppError> {
        if Self::table_exists(conn, "mcp_servers")? {
            Self::add_column_if_missing(
                conn,
                "mcp_servers",
                "enabled_grokbuild",
                "BOOLEAN NOT NULL DEFAULT 0",
            )?;
        }
        if Self::table_exists(conn, "skills")? {
            Self::add_column_if_missing(
                conn,
                "skills",
                "enabled_grokbuild",
                "BOOLEAN NOT NULL DEFAULT 0",
            )?;
        }
        Ok(())
    }

    /// v15 -> v16: remove Codex session rows and cursors so startup sync can
    /// rebuild them with fork-history alignment. Must stay connection-level:
    /// schema migration already owns the Database connection mutex.
    fn migrate_v15_to_v16(conn: &Connection) -> Result<(), AppError> {
        let codex_dir = crate::codex_config::get_codex_config_dir();
        crate::services::session_usage_codex::reset_codex_usage_on_conn(conn, &codex_dir)
    }

    /// v16 -> v17: preserve session request identities after detail rollup.
    fn migrate_v16_to_v17(conn: &Connection) -> Result<(), AppError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS session_usage_dedup (
                data_source TEXT NOT NULL,
                request_id TEXT NOT NULL,
                semantic_id TEXT NOT NULL,
                has_entry_id INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (data_source, request_id)
             );
             CREATE INDEX IF NOT EXISTS idx_session_usage_dedup_semantic
             ON session_usage_dedup(data_source, semantic_id, has_entry_id);",
        )
        .map_err(|error| {
            AppError::Database(format!(
                "Failed to create the session usage dedup ledger: {error}"
            ))
        })?;
        Ok(())
    }

    /// v17 -> v18: retain only product-owned, verified tool transitions.
    fn migrate_v17_to_v18(conn: &Connection) -> Result<(), AppError> {
        Self::ensure_tool_version_events_table(conn)
    }

    /// v18 -> v19: SQLite cannot alter a CHECK constraint in place. Rebuild
    /// the table inside the migration savepoint, retaining ids and the stable
    /// history index before admitting the two newly proven Python owners.
    fn migrate_v18_to_v19(conn: &Connection) -> Result<(), AppError> {
        if !Self::table_exists(conn, "tool_version_events")? {
            return Self::ensure_tool_version_events_table(conn);
        }
        conn.execute_batch(
            "DROP TABLE IF EXISTS tool_version_events_v19;
             DROP INDEX IF EXISTS idx_tool_version_events_history;
             CREATE TABLE tool_version_events_v19 (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_id TEXT NOT NULL CHECK (tool_id IN (
                    'claude-code', 'codex', 'opencode', 'gemini-cli',
                    'grok-build', 'openclaw', 'hermes', 'pi'
                )),
                from_version TEXT CHECK (
                    from_version IS NULL OR length(from_version) BETWEEN 1 AND 64
                ),
                to_version TEXT NOT NULL CHECK (length(to_version) BETWEEN 1 AND 64),
                source TEXT NOT NULL CHECK (source IN (
                    'npm', 'pnpm', 'bun', 'volta', 'uv', 'pipx', 'brew',
                    'nativeInstaller', 'unmanaged'
                )),
                operation_kind TEXT NOT NULL CHECK (operation_kind IN (
                    'install', 'update', 'changeVersion'
                )),
                occurred_at INTEGER NOT NULL CHECK (occurred_at >= 0)
             );
             INSERT INTO tool_version_events_v19
                (id, tool_id, from_version, to_version, source, operation_kind, occurred_at)
             SELECT id, tool_id, from_version, to_version, source, operation_kind, occurred_at
             FROM tool_version_events;
             DROP TABLE tool_version_events;
             ALTER TABLE tool_version_events_v19 RENAME TO tool_version_events;
             CREATE INDEX idx_tool_version_events_history
             ON tool_version_events(tool_id, occurred_at DESC, id DESC);",
        )
        .map_err(|error| {
            AppError::Database(format!(
                "Failed to extend the tool version history sources: {error}"
            ))
        })?;
        Ok(())
    }

    /// v19 -> v20: product-owned Claude Desktop MCP projection. Existing and
    /// imported CC Switch rows remain disabled until explicit adoption.
    fn migrate_v19_to_v20(conn: &Connection) -> Result<(), AppError> {
        // Some recovery/fixture databases legitimately do not carry the MCP
        // table. A product-owned optional projection must not prevent the
        // remaining schema chain from reaching the current version.
        if !Self::table_exists(conn, "mcp_servers")? {
            return Ok(());
        }
        Self::add_column_if_missing(
            conn,
            "mcp_servers",
            "enabled_claude_desktop",
            "BOOLEAN NOT NULL DEFAULT 0",
        )
        .map(|_| ())
    }

    fn ensure_tool_version_events_table(conn: &Connection) -> Result<(), AppError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tool_version_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_id TEXT NOT NULL CHECK (tool_id IN (
                    'claude-code', 'codex', 'opencode', 'gemini-cli',
                    'grok-build', 'openclaw', 'hermes', 'pi'
                )),
                from_version TEXT CHECK (
                    from_version IS NULL OR length(from_version) BETWEEN 1 AND 64
                ),
                to_version TEXT NOT NULL CHECK (length(to_version) BETWEEN 1 AND 64),
                source TEXT NOT NULL CHECK (source IN (
                    'npm', 'pnpm', 'bun', 'volta', 'uv', 'pipx', 'brew',
                    'nativeInstaller', 'unmanaged'
                )),
                operation_kind TEXT NOT NULL CHECK (operation_kind IN (
                    'install', 'update', 'changeVersion'
                )),
                occurred_at INTEGER NOT NULL CHECK (occurred_at >= 0)
             );
             CREATE INDEX IF NOT EXISTS idx_tool_version_events_history
             ON tool_version_events(tool_id, occurred_at DESC, id DESC);",
        )
        .map_err(|error| {
            AppError::Database(format!(
                "Failed to create the tool version history: {error}"
            ))
        })?;
        Ok(())
    }

    /// Startup cleanup for product preferences nothing reads anymore: the removed "pause AI
    /// Manager updates" feature wrote a per-tool `aimgr.toolVersionPin.<tool>` key, and the
    /// removed first-run guide wrote `aimgr.onboardingCompleted`. The leftovers are deleted here
    /// to keep backups and exports free of orphaned preferences.
    /// Idempotent; returns the number of deleted rows.
    pub(crate) fn remove_retired_product_settings(&self) -> Result<usize, AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM settings
             WHERE key LIKE 'aimgr.toolVersionPin.%' OR key = 'aimgr.onboardingCompleted'",
            [],
        )
        .map_err(|error| {
            AppError::Database(format!(
                "Failed to clean up the retired product settings: {error}"
            ))
        })
    }

    /// Insert the default model pricing data
    /// Format: (model_id, display_name, input, output, cache_read, cache_creation)
    /// Note: model_id uses the hyphenated form (e.g. claude-haiku-4-5), matching normalized API model names
    fn seed_model_pricing(conn: &Connection) -> Result<(), AppError> {
        let pricing_data = [
            // Claude Fable 5.1 / Mythos 5.1 (released 2026-09-01; same price as Fable 5, but cache
            // read is 0.025x = $0.25 rather than Fable 5's $1)
            (
                "claude-fable-5-1",
                "Claude Fable 5.1",
                "10",
                "50",
                "0.25",
                "12.50",
            ),
            (
                "claude-mythos-5-1",
                "Claude Mythos 5.1",
                "10",
                "50",
                "0.25",
                "12.50",
            ),
            // Claude Fable 5 (a new tier above Opus)
            (
                "claude-fable-5",
                "Claude Fable 5",
                "10",
                "50",
                "1.00",
                "12.50",
            ),
            (
                "claude-mythos-5",
                "Claude Mythos 5",
                "10",
                "50",
                "1.00",
                "12.50",
            ),
            // Claude Opus 5 (same price point as Opus 4.8; fast mode $10/$50 is not listed here)
            ("claude-opus-5", "Claude Opus 5", "5", "25", "0.50", "6.25"),
            // Claude 4.8 family
            (
                "claude-opus-4-8",
                "Claude Opus 4.8",
                "5",
                "25",
                "0.50",
                "6.25",
            ),
            // Claude Sonnet 5 (confirmed on the official pricing page 2026-09: the $2/$10 introductory
            // price became the list price, and the planned 09-01 increase to $3/$15 was cancelled)
            (
                "claude-sonnet-5",
                "Claude Sonnet 5",
                "2",
                "10",
                "0.20",
                "2.50",
            ),
            // Claude 4.7 family
            (
                "claude-opus-4-7",
                "Claude Opus 4.7",
                "5",
                "25",
                "0.50",
                "6.25",
            ),
            // Claude 4.6 family (bare-id rows cover log variants without a date suffix, priced like the dated rows)
            (
                "claude-opus-4-6",
                "Claude Opus 4.6",
                "5",
                "25",
                "0.50",
                "6.25",
            ),
            (
                "claude-sonnet-4-6",
                "Claude Sonnet 4.6",
                "3",
                "15",
                "0.30",
                "3.75",
            ),
            (
                "claude-opus-4-6-20260206",
                "Claude Opus 4.6",
                "5",
                "25",
                "0.50",
                "6.25",
            ),
            (
                "claude-sonnet-4-6-20260217",
                "Claude Sonnet 4.6",
                "3",
                "15",
                "0.30",
                "3.75",
            ),
            // Claude 4.5 family
            (
                "claude-opus-4-5-20251101",
                "Claude Opus 4.5",
                "5",
                "25",
                "0.50",
                "6.25",
            ),
            (
                "claude-sonnet-4-5-20250929",
                "Claude Sonnet 4.5",
                "3",
                "15",
                "0.30",
                "3.75",
            ),
            (
                "claude-haiku-4-5-20251001",
                "Claude Haiku 4.5",
                "1",
                "5",
                "0.10",
                "1.25",
            ),
            // Claude 4 family (legacy models)
            (
                "claude-opus-4-20250514",
                "Claude Opus 4",
                "15",
                "75",
                "1.50",
                "18.75",
            ),
            (
                "claude-opus-4-1-20250805",
                "Claude Opus 4.1",
                "15",
                "75",
                "1.50",
                "18.75",
            ),
            (
                "claude-sonnet-4-20250514",
                "Claude Sonnet 4",
                "3",
                "15",
                "0.30",
                "3.75",
            ),
            // Claude 3.5 family
            (
                "claude-3-5-haiku-20241022",
                "Claude 3.5 Haiku",
                "0.80",
                "4",
                "0.08",
                "1",
            ),
            (
                "claude-3-5-sonnet-20241022",
                "Claude 3.5 Sonnet",
                "3",
                "15",
                "0.30",
                "3.75",
            ),
            // GPT-6 family (Astra, released 2026-09-04, 1.05M context window)
            // Official pricing page + model page + models.dev agree: 10/50, cache read 1, cache write 1.25x input = 12.50.
            // The >272K long-context tier (20/75/2/25) cannot be expressed in this table and is ignored, as for gpt-5.5.
            // The low/medium/high/xhigh effort tiers fall back to this row after the price lookup strips the suffix;
            //   max is not in the strip list (it would collide with real *-max ids), so no extra suffix row is added.
            ("gpt-6-astra", "GPT-6 Astra", "10", "50", "1", "12.5"),
            // GPT-5.6 family (Sol / Terra / Luna, released 2026-06)
            // From the 5.6 family on, cache write costs 1.25x the input price (earlier GPT models wrote cache for free; do not backfill older families)
            // 2026-09-06 audit: Sol switched to the promotional price 4/20/0.40/5 (the OpenAI pricing page says "at least through 2026-11-21"),
            // list price 5/30/0.50/6.25. The promotional price is recorded and deliberately not exempted: once the promo ends and models.dev updates, the audit reports it automatically.
            ("gpt-5.6-sol", "GPT-5.6 Sol", "4", "20", "0.40", "5"),
            // 2026-07-30 OpenAI price cut: luna -80%, terra -20%, sol unchanged (fast mode 2x pricing is not listed here)
            ("gpt-5.6-terra", "GPT-5.6 Terra", "2", "12", "0.20", "2.50"),
            (
                "gpt-5.6-luna",
                "GPT-5.6 Luna",
                "0.20",
                "1.20",
                "0.02",
                "0.25",
            ),
            // The bare name gpt-5.6 is the official alias of sol; the effort suffixes mirror the accounting shape of the gpt-5.5 family.
            // The price lookup matches the id exactly first and only then strips the effort suffix, so these rows must be repriced together with sol, otherwise a stale price shadows the base row.
            ("gpt-5.6", "GPT-5.6 Sol", "4", "20", "0.40", "5"),
            ("gpt-5.6-low", "GPT-5.6 Sol", "4", "20", "0.40", "5"),
            ("gpt-5.6-medium", "GPT-5.6 Sol", "4", "20", "0.40", "5"),
            ("gpt-5.6-high", "GPT-5.6 Sol", "4", "20", "0.40", "5"),
            ("gpt-5.6-xhigh", "GPT-5.6 Sol", "4", "20", "0.40", "5"),
            ("gpt-5.6-minimal", "GPT-5.6 Sol", "4", "20", "0.40", "5"),
            // GPT-5.5 family
            ("gpt-5.5", "GPT-5.5", "5", "30", "0.50", "0"),
            ("gpt-5.5-low", "GPT-5.5", "5", "30", "0.50", "0"),
            ("gpt-5.5-medium", "GPT-5.5", "5", "30", "0.50", "0"),
            ("gpt-5.5-high", "GPT-5.5", "5", "30", "0.50", "0"),
            ("gpt-5.5-xhigh", "GPT-5.5", "5", "30", "0.50", "0"),
            ("gpt-5.5-minimal", "GPT-5.5", "5", "30", "0.50", "0"),
            // GPT-5.4 family
            ("gpt-5.4", "GPT-5.4", "2.50", "15", "0.25", "0"),
            ("gpt-5.4-mini", "GPT-5.4 Mini", "0.75", "4.50", "0.075", "0"),
            ("gpt-5.4-nano", "GPT-5.4 Nano", "0.20", "1.25", "0.02", "0"),
            // GPT-5.2 family
            ("gpt-5.2", "GPT-5.2", "1.75", "14", "0.175", "0"),
            ("gpt-5.2-low", "GPT-5.2", "1.75", "14", "0.175", "0"),
            ("gpt-5.2-medium", "GPT-5.2", "1.75", "14", "0.175", "0"),
            ("gpt-5.2-high", "GPT-5.2", "1.75", "14", "0.175", "0"),
            ("gpt-5.2-xhigh", "GPT-5.2", "1.75", "14", "0.175", "0"),
            ("gpt-5.2-codex", "GPT-5.2 Codex", "1.75", "14", "0.175", "0"),
            (
                "gpt-5.2-codex-low",
                "GPT-5.2 Codex",
                "1.75",
                "14",
                "0.175",
                "0",
            ),
            (
                "gpt-5.2-codex-medium",
                "GPT-5.2 Codex",
                "1.75",
                "14",
                "0.175",
                "0",
            ),
            (
                "gpt-5.2-codex-high",
                "GPT-5.2 Codex",
                "1.75",
                "14",
                "0.175",
                "0",
            ),
            (
                "gpt-5.2-codex-xhigh",
                "GPT-5.2 Codex",
                "1.75",
                "14",
                "0.175",
                "0",
            ),
            // GPT-5.3 Codex family
            ("gpt-5.3-codex", "GPT-5.3 Codex", "1.75", "14", "0.175", "0"),
            (
                "gpt-5.3-codex-spark",
                "GPT-5.3 Codex Spark",
                "1.75",
                "14",
                "0.175",
                "0",
            ),
            (
                "gpt-5.3-codex-low",
                "GPT-5.3 Codex",
                "1.75",
                "14",
                "0.175",
                "0",
            ),
            (
                "gpt-5.3-codex-medium",
                "GPT-5.3 Codex",
                "1.75",
                "14",
                "0.175",
                "0",
            ),
            (
                "gpt-5.3-codex-high",
                "GPT-5.3 Codex",
                "1.75",
                "14",
                "0.175",
                "0",
            ),
            (
                "gpt-5.3-codex-xhigh",
                "GPT-5.3 Codex",
                "1.75",
                "14",
                "0.175",
                "0",
            ),
            // GPT-5.1 family
            ("gpt-5.1", "GPT-5.1", "1.25", "10", "0.125", "0"),
            ("gpt-5.1-low", "GPT-5.1", "1.25", "10", "0.125", "0"),
            ("gpt-5.1-medium", "GPT-5.1", "1.25", "10", "0.125", "0"),
            ("gpt-5.1-high", "GPT-5.1", "1.25", "10", "0.125", "0"),
            ("gpt-5.1-minimal", "GPT-5.1", "1.25", "10", "0.125", "0"),
            ("gpt-5.1-codex", "GPT-5.1 Codex", "1.25", "10", "0.125", "0"),
            (
                "gpt-5.1-codex-mini",
                "GPT-5.1 Codex",
                "1.25",
                "10",
                "0.125",
                "0",
            ),
            (
                "gpt-5.1-codex-max",
                "GPT-5.1 Codex",
                "1.25",
                "10",
                "0.125",
                "0",
            ),
            (
                "gpt-5.1-codex-max-high",
                "GPT-5.1 Codex",
                "1.25",
                "10",
                "0.125",
                "0",
            ),
            (
                "gpt-5.1-codex-max-xhigh",
                "GPT-5.1 Codex",
                "1.25",
                "10",
                "0.125",
                "0",
            ),
            // GPT-5 family
            ("gpt-5", "GPT-5", "1.25", "10", "0.125", "0"),
            ("gpt-5-low", "GPT-5", "1.25", "10", "0.125", "0"),
            ("gpt-5-medium", "GPT-5", "1.25", "10", "0.125", "0"),
            ("gpt-5-high", "GPT-5", "1.25", "10", "0.125", "0"),
            ("gpt-5-minimal", "GPT-5", "1.25", "10", "0.125", "0"),
            ("gpt-5-codex", "GPT-5 Codex", "1.25", "10", "0.125", "0"),
            ("gpt-5-codex-low", "GPT-5 Codex", "1.25", "10", "0.125", "0"),
            (
                "gpt-5-codex-medium",
                "GPT-5 Codex",
                "1.25",
                "10",
                "0.125",
                "0",
            ),
            (
                "gpt-5-codex-high",
                "GPT-5 Codex",
                "1.25",
                "10",
                "0.125",
                "0",
            ),
            (
                "gpt-5-codex-mini",
                "GPT-5 Codex",
                "1.25",
                "10",
                "0.125",
                "0",
            ),
            (
                "gpt-5-codex-mini-medium",
                "GPT-5 Codex",
                "1.25",
                "10",
                "0.125",
                "0",
            ),
            (
                "gpt-5-codex-mini-high",
                "GPT-5 Codex",
                "1.25",
                "10",
                "0.125",
                "0",
            ),
            // OpenAI Reasoning family
            ("o3", "OpenAI o3", "2", "8", "0.50", "0"),
            ("o4-mini", "OpenAI o4-mini", "1.10", "4.40", "0.275", "0"),
            // GPT-4.1 family
            ("gpt-4.1", "GPT-4.1", "2", "8", "0.50", "0"),
            ("gpt-4.1-mini", "GPT-4.1 Mini", "0.40", "1.60", "0.10", "0"),
            ("gpt-4.1-nano", "GPT-4.1 Nano", "0.10", "0.40", "0.025", "0"),
            // Gemini 3.8 family (released 2026-09-02, 1M context window)
            // Introductory price 0.75/3.75/0.075 through 2026-12-31, list price 1.50/7.50/0.15 from 2027-01-01; same treatment as 3.7 Flash, no exemption.
            (
                "gemini-3.8-flash",
                "Gemini 3.8 Flash",
                "0.75",
                "3.75",
                "0.075",
                "0",
            ),
            // Gemini 3.6 family
            // 2026-09-06 audit: Google's pricing page moved 3.6 Flash to the introductory price 0.75/3.75/0.075 as well (through 2026-12-31),
            // returning to the list price 1.50/7.50/0.15 on 2027-01-01. Same treatment as 3.7/3.8 Flash, deliberately kept out of audit-ignore.json.
            (
                "gemini-3.6-flash",
                "Gemini 3.6 Flash",
                "0.75",
                "3.75",
                "0.075",
                "0",
            ),
            // Gemini 3.5 family
            (
                "gemini-3.5-flash",
                "Gemini 3.5 Flash",
                "1.50",
                "9.00",
                "0.15",
                "0",
            ),
            (
                "gemini-3.5-flash-lite",
                "Gemini 3.5 Flash Lite",
                "0.30",
                "2.50",
                "0.03",
                "0",
            ),
            // Gemini 3.1 family
            (
                "gemini-3.1-pro-preview",
                "Gemini 3.1 Pro Preview",
                "2",
                "12",
                "0.20",
                "0",
            ),
            (
                "gemini-3.1-flash-lite",
                "Gemini 3.1 Flash Lite",
                "0.25",
                "1.50",
                "0.025",
                "0",
            ),
            (
                "gemini-3.1-flash-lite-preview",
                "Gemini 3.1 Flash Lite Preview",
                "0.25",
                "1.50",
                "0.025",
                "0",
            ),
            // Gemini 3 family
            (
                "gemini-3-pro-preview",
                "Gemini 3 Pro Preview",
                "2",
                "12",
                "0.2",
                "0",
            ),
            (
                "gemini-3-flash-preview",
                "Gemini 3 Flash Preview",
                "0.5",
                "3",
                "0.05",
                "0",
            ),
            // Gemini 2.5 family
            (
                "gemini-2.5-pro",
                "Gemini 2.5 Pro",
                "1.25",
                "10",
                "0.125",
                "0",
            ),
            (
                "gemini-2.5-flash",
                "Gemini 2.5 Flash",
                "0.3",
                "2.5",
                "0.03",
                "0",
            ),
            (
                "gemini-2.5-flash-lite",
                "Gemini 2.5 Flash Lite",
                "0.10",
                "0.40",
                "0.01",
                "0",
            ),
            // Gemini 2.0 family
            (
                "gemini-2.0-flash",
                "Gemini 2.0 Flash",
                "0.10",
                "0.40",
                "0.025",
                "0",
            ),
            // StepFun family
            (
                "step-3.7-flash",
                "Step 3.7 Flash",
                "0.19",
                "1.13",
                "0.04",
                "0",
            ),
            (
                "step-3.5-flash",
                "Step 3.5 Flash",
                "0.10",
                "0.30",
                "0.02",
                "0",
            ),
            (
                "step-3.5-flash-2603",
                "Step 3.5 Flash 2603",
                "0.10",
                "0.30",
                "0.02",
                "0",
            ),
            // ====== Chinese-vendor models (USD/1M tokens) ======
            // Doubao (ByteDance)
            // Seed 2.1 family (2026-06 Volcano Engine official list price, CNY converted at ~7.14):
            //   pro   input CNY 6 / output CNY 30 / cache hit CNY 1.2
            //   turbo input CNY 3 / output CNY 15 / cache hit CNY 0.6
            // The "cache storage CNY 0.017/M/hour" charge is time-based storage, a different basis from this table's cache_creation (per written token), so it is set to 0.
            (
                "doubao-seed-2-1-pro",
                "Doubao Seed 2.1 Pro",
                "0.84",
                "4.2",
                "0.17",
                "0",
            ),
            (
                "doubao-seed-2-1-turbo",
                "Doubao Seed 2.1 Turbo",
                "0.42",
                "2.1",
                "0.08",
                "0",
            ),
            (
                "doubao-seed-code",
                "Doubao Seed Code",
                "0.17",
                "1.11",
                "0.02",
                "0",
            ),
            (
                "doubao-seed-2-0-pro",
                "Doubao Seed 2.0 Pro",
                "0.47",
                "2.37",
                "0.09",
                "0",
            ),
            (
                "doubao-seed-2-0-code",
                "Doubao Seed 2.0 Code",
                "0.47",
                "2.37",
                "0.09",
                "0",
            ),
            (
                "doubao-seed-2-0-code-preview-latest",
                "Doubao Seed 2.0 Code Preview",
                "0.47",
                "2.37",
                "0.09",
                "0",
            ),
            (
                "doubao-seed-2-0-lite",
                "Doubao Seed 2.0 Lite",
                "0.08",
                "0.50",
                "0.017",
                "0",
            ),
            (
                "doubao-seed-2-0-mini",
                "Doubao Seed 2.0 Mini",
                "0.03",
                "0.31",
                "0.0056",
                "0",
            ),
            // DeepSeek family
            (
                "deepseek-v3.2",
                "DeepSeek V3.2",
                "0.28",
                "0.42",
                "0.028",
                "0",
            ),
            (
                "deepseek-v3.1",
                "DeepSeek V3.1",
                "0.55",
                "1.67",
                "0.055",
                "0",
            ),
            ("deepseek-v3", "DeepSeek V3", "0.28", "1.11", "0.028", "0"),
            // input = cache-miss price, cache_read = cache-hit price; DeepSeek does not charge for cache writes -> 0.
            //
            // -- 2026-09-11: V4 Flash retired; all three ids are now served by DeepSeek-V4.1-Flash --
            // From the official pricing page: the legacy names `deepseek-v4-flash` / `deepseek-v4-flash-vision-exp`
            // are still accepted, but "the corresponding models have been retired"; requests are served by V4.1-Flash
            // and billed at the Flash price -> all three share one price. V4.1 Flash peak tier is 0.3/1.2/0.006 (the
            // off-peak tier is exactly half; models.dev records the off-peak tier, so audit section A flags these rows long-term, as expected).
            // deepseek-flash is the only officially recommended name today and must be listed separately: the price lookup's prefix fallback is LIKE '{id}-%',
            // which only matches longer rows, so the short id never matches deepseek-v4-flash and a missing row silently bills at 0.
            //
            // NOTE: deepseek-chat / deepseek-reasoner keep the 2026-07 V4 Flash alias price unchanged (rechecked
            // 2026-09-11): the official docs site no longer lists either id and the first-party models.dev entries were
            // removed too — with no authoritative source for either "follows the V4.1 Flash cut" or "retired", the old
            // values stay under the no-source-no-change rule.
            // This repository never adopted the upstream 2026-08-16 peak/off-peak split, so DeepSeek rows move from the
            // 2026-07 price straight to the V4.1 Flash tier.
            (
                "deepseek-chat",
                "DeepSeek Chat",
                "0.14",
                "0.28",
                "0.0028",
                "0",
            ),
            (
                "deepseek-reasoner",
                "DeepSeek Reasoner",
                "0.14",
                "0.28",
                "0.0028",
                "0",
            ),
            // DeepSeek V4 family (official CNY converted at 1 USD ~ 7.14)
            (
                "deepseek-flash",
                "DeepSeek V4.1 Flash",
                "0.3",
                "1.2",
                "0.006",
                "0",
            ),
            (
                "deepseek-v4-flash",
                "DeepSeek V4 Flash",
                "0.3",
                "1.2",
                "0.006",
                "0",
            ),
            // Some upstreams (e.g. Alibaba Bailian) return a 4-digit MMDD date variant. The price lookup's
            // strip_model_date_suffix only strips ISO / 8-digit YYYYMMDD / 6-digit YYMMDD, never reaching the
            // bare id, and the prefix fallback only matches longer rows — without the alias the request silently bills at 0
            (
                "deepseek-v4-flash-0731",
                "DeepSeek V4 Flash",
                "0.3",
                "1.2",
                "0.006",
                "0",
            ),
            // Legacy vision experiment name; the official pricing page states it is "still accepted, served by V4.1-Flash and billed at the Flash price".
            // Official install scripts <= 1.2.0 wrote this id and existing providers still use it; the prefix fallback cannot match the shorter
            // deepseek-v4-flash, so without a dedicated row it silently bills at 0
            (
                "deepseek-v4-flash-vision-exp",
                "DeepSeek V4 Flash Vision Exp",
                "0.3",
                "1.2",
                "0.006",
                "0",
            ),
            // NOTE: from 2026-09-14 12:00 Beijing time the official announcement retires V4 Pro in an orderly way; until V4.1 Pro ships,
            // all deepseek-v4-pro requests "are all routed to V4.1 Flash and billed at the V4.1
            // Flash price" -> this row drops to the Flash tier, matching the four rows above (the repair guard copies
            // this repository's previous 2026-07 price 0.435/0.87/0.003625).
            (
                "deepseek-v4-pro",
                "DeepSeek V4 Pro",
                "0.3",
                "1.2",
                "0.006",
                "0",
            ),
            // Kimi (Moonshot AI)
            (
                "kimi-k2-thinking",
                "Kimi K2 Thinking",
                "0.55",
                "2.20",
                "0.10",
                "0",
            ),
            ("kimi-k2-0905", "Kimi K2", "0.55", "2.20", "0.10", "0"),
            (
                "kimi-k2-turbo",
                "Kimi K2 Turbo",
                "1.11",
                "8.06",
                "0.14",
                "0",
            ),
            ("kimi-k2.5", "Kimi K2.5", "0.60", "3.00", "0.10", "0"),
            ("kimi-k2.6", "Kimi K2.6", "0.95", "4.00", "0.16", "0"),
            (
                "kimi-k2.7-code",
                "Kimi K2.7 Code",
                "0.95",
                "4.00",
                "0.19",
                "0",
            ),
            // The HighSpeed tier costs 2x the base model (the usual Kimi pattern, same as K2 Turbo)
            (
                "kimi-k2.7-code-highspeed",
                "Kimi K2.7 Code HighSpeed",
                "1.90",
                "8.00",
                "0.38",
                "0",
            ),
            ("kimi-k3", "Kimi K3", "3.00", "15.00", "0.30", "0"),
            // The bare K3 name used by the Kimi For Coding plan (no kimi- prefix), same standard list price
            ("k3", "Kimi K3", "3.00", "15.00", "0.30", "0"),
            // Tencent Hunyuan (official CNY 1/4/0.25 converted at 1 USD ~ 7.14; the lowest tier of the Hy3 tiered pricing)
            ("hunyuan-hy3", "Hunyuan Hy3", "0.14", "0.56", "0.035", "0"),
            ("hy3", "Hunyuan Hy3", "0.14", "0.56", "0.035", "0"),
            // MiniMax family
            // 2026-09-06 audit: the official pay-as-you-go page (platform.minimax.io/docs/guides/pricing-paygo)
            // M2 / M2.1 / M2.5 are all 0.3/1.2/0.03/0.375, matching models.dev; the old values 0.27/0.95 and 0.15 were early mistakes.
            (
                "minimax-m2.1",
                "MiniMax M2.1",
                "0.30",
                "1.20",
                "0.03",
                "0.375",
            ),
            (
                "minimax-m2.1-lightning",
                "MiniMax M2.1 Lightning",
                "0.27",
                "2.33",
                "0.03",
                "0",
            ),
            ("minimax-m2", "MiniMax M2", "0.30", "1.20", "0.03", "0.375"),
            (
                "minimax-m2.5",
                "MiniMax M2.5",
                "0.30",
                "1.20",
                "0.03",
                "0.375",
            ),
            (
                "minimax-m2.5-lightning",
                "MiniMax M2.5 Lightning",
                "0.30",
                "2.40",
                "0.03",
                "0",
            ),
            (
                "minimax-m2.7",
                "MiniMax M2.7",
                "0.30",
                "1.20",
                "0.06",
                "0.375",
            ),
            (
                "minimax-m2.7-highspeed",
                "MiniMax M2.7 Highspeed",
                "0.60",
                "2.40",
                "0.06",
                "0.375",
            ),
            ("minimax-m3", "MiniMax M3", "0.30", "1.20", "0.06", "0"),
            // GLM (Zhipu)
            ("glm-4.7", "GLM-4.7", "0.6", "2.2", "0.11", "0"),
            ("glm-4.6", "GLM-4.6", "0.6", "2.2", "0.11", "0"),
            ("glm-5", "GLM-5", "1", "3.2", "0.2", "0"),
            ("glm-5.1", "GLM-5.1", "1.4", "4.4", "0.26", "0"),
            ("glm-5.2", "GLM-5.2", "1.4", "4.4", "0.26", "0"),
            ("glm-5.3", "GLM-5.3", "1.4", "4.4", "0.26", "0"),
            (
                "glm-5.3-flash",
                "GLM-5.3-Flash",
                "0.15",
                "0.50",
                "0.03",
                "0",
            ),
            ("glm-5-turbo", "GLM-5-Turbo", "1.2", "4", "0.24", "0"),
            ("glm-5v-turbo", "GLM-5V-Turbo", "1.2", "4", "0.24", "0"),
            // MiMo (Xiaomi)
            (
                "mimo-v2-flash",
                "MiMo V2 Flash",
                "0.09",
                "0.29",
                "0.009",
                "0",
            ),
            ("mimo-v2-pro", "MiMo V2 Pro", "0.435", "0.87", "0.0036", "0"),
            ("mimo-v2.5", "MiMo V2.5", "0.14", "0.29", "0.0028", "0"),
            (
                "mimo-v2.5-pro",
                "MiMo V2.5 Pro",
                "0.435",
                "0.87",
                "0.0036",
                "0",
            ),
            // Qwen family (Alibaba)
            ("qwen3.8-max", "Qwen3.8 Max", "2", "6", "0.25", "2.50"),
            // 2026-09-06: the Alibaba international pricing page lists a flat 0.15/0.47 across the whole range (0 < tokens <= 1M) with no tiers;
            // the two cache columns are only described as an "unusual ratio" without numbers, so models.dev values are used (same basis as qwen3.8-max)
            (
                "qwen3.8-flash",
                "Qwen3.8 Flash",
                "0.15",
                "0.47",
                "0.016",
                "0.20",
            ),
            ("qwen3.7-max", "Qwen3.7 Max", "2.50", "7.50", "0.25", "0"),
            ("qwen3.7-plus", "Qwen3.7 Plus", "0.40", "1.60", "0.08", "0"),
            (
                "qwen3.6-plus",
                "Qwen3.6 Plus",
                "0.325",
                "1.95",
                "0.065",
                "0",
            ),
            (
                "qwen3.6-flash",
                "Qwen3.6 Flash",
                "0.1875",
                "1.125",
                "0.0375",
                "0",
            ),
            ("qwen3.5-plus", "Qwen3.5 Plus", "0.26", "1.56", "0.052", "0"),
            ("qwen3-max", "Qwen3 Max", "0.78", "3.90", "0", "0"),
            (
                "qwen3-235b-a22b",
                "Qwen3 235B-A22B",
                "0.70",
                "8.40",
                "0",
                "0",
            ),
            (
                "qwen3-coder-plus",
                "Qwen3 Coder Plus",
                "0.65",
                "3.25",
                "0.13",
                "0",
            ),
            (
                "qwen3-coder-480b",
                "Qwen3 Coder 480B",
                "0.65",
                "3.25",
                "0",
                "0",
            ),
            (
                "qwen3-coder-480b-a35b-instruct",
                "Qwen3 Coder 480B-A35B Instruct",
                "0.65",
                "3.25",
                "0",
                "0",
            ),
            (
                "qwen3-coder-flash",
                "Qwen3 Coder Flash",
                "0.195",
                "0.975",
                "0.039",
                "0",
            ),
            (
                "qwen3-coder-next",
                "Qwen3 Coder Next",
                "0.12",
                "0.75",
                "0",
                "0",
            ),
            ("qwq-plus", "QwQ Plus", "0.80", "2.40", "0", "0"),
            ("qwq-32b", "QwQ 32B", "0.20", "0.60", "0", "0"),
            ("qwen3-32b", "Qwen3 32B", "0.16", "0.64", "0", "0"),
            // Grok family (xAI)
            // 4.5/4.6 both use tiered pricing: the unit price doubles for prompts >= 200K (4/12, cached doubles too).
            // This table has no tier column, so the base tier (<200K) is used, consistent with other tiered vendors
            ("grok-4.6", "Grok 4.6", "2", "6", "0.50", "0"),
            ("grok-4.5", "Grok 4.5", "2", "6", "0.30", "0"),
            // Internal alias reported by modelUsage when Grok CLI runs in official OAuth mode. The pricing was
            // derived from two rounds of measured costUsdTicks (1 tick = 1e-10 USD): input/output match
            // grok-4.5 at 2/6, and cache read matches at 0.30
            ("grok-4.5-build", "Grok 4.5 Build", "2", "6", "0.30", "0"),
            ("grok-4.3", "Grok 4.3", "1.25", "2.50", "0.20", "0"),
            (
                "grok-4.20-0309-reasoning",
                "Grok 4.20 Reasoning",
                "1.25",
                "2.50",
                "0.20",
                "0",
            ),
            (
                "grok-4.20-0309-non-reasoning",
                "Grok 4.20",
                "1.25",
                "2.50",
                "0.20",
                "0",
            ),
            (
                "grok-4-1-fast-reasoning",
                "Grok 4.1 Fast Reasoning",
                "0.20",
                "0.50",
                "0.05",
                "0",
            ),
            (
                "grok-4-1-fast-non-reasoning",
                "Grok 4.1 Fast",
                "0.20",
                "0.50",
                "0.05",
                "0",
            ),
            ("grok-4", "Grok 4", "3", "15", "0.75", "0"),
            (
                "grok-code-fast-1",
                "Grok Build 0.1 (Code Fast Alias)",
                "1",
                "2",
                "0.20",
                "0",
            ),
            ("grok-build-0.1", "Grok Build 0.1", "1", "2", "0.20", "0"),
            ("grok-3", "Grok 3", "3", "15", "0.75", "0"),
            ("grok-3-mini", "Grok 3 Mini", "0.25", "0.50", "0.075", "0"),
            // Mistral family
            (
                "mistral-medium-3.5",
                "Mistral Medium 3.5",
                "1.50",
                "7.50",
                "0",
                "0",
            ),
            (
                "mistral-small-4",
                "Mistral Small 4",
                "0.10",
                "0.30",
                "0.01",
                "0",
            ),
            (
                "devstral-small-2-2512",
                "Devstral Small 2",
                "0.10",
                "0.30",
                "0.01",
                "0",
            ),
            (
                "magistral-small",
                "Magistral Small",
                "0.50",
                "1.50",
                "0",
                "0",
            ),
            ("codestral-2508", "Codestral", "0.30", "0.90", "0.03", "0"),
            (
                "devstral-small-1.1",
                "Devstral Small 1.1",
                "0.07",
                "0.28",
                "0.01",
                "0",
            ),
            ("devstral-2-2512", "Devstral 2", "0.40", "2", "0.04", "0"),
            (
                "devstral-medium",
                "Devstral Medium",
                "0.40",
                "2",
                "0.04",
                "0",
            ),
            (
                "mistral-large-3-2512",
                "Mistral Large 3",
                "0.50",
                "1.50",
                "0.05",
                "0",
            ),
            (
                "mistral-medium-3.1",
                "Mistral Medium 3.1",
                "0.40",
                "2",
                "0.04",
                "0",
            ),
            (
                "mistral-small-3.2-24b",
                "Mistral Small 3.2",
                "0.075",
                "0.20",
                "0.01",
                "0",
            ),
            ("magistral-medium", "Magistral Medium", "2", "5", "0", "0"),
            // Cohere family
            ("command-a", "Cohere Command A", "2.50", "10", "0", "0"),
            (
                "command-r-plus",
                "Cohere Command R+",
                "2.50",
                "10",
                "0",
                "0",
            ),
            ("command-r", "Cohere Command R", "0.15", "0.60", "0", "0"),
            // Additional OpenAI models
            ("o3-pro", "OpenAI o3-pro", "20", "80", "0", "0"),
            ("o3-mini", "OpenAI o3-mini", "0.55", "2.20", "0.55", "0"),
            ("o1", "OpenAI o1", "15", "60", "7.50", "0"),
            ("o1-mini", "OpenAI o1-mini", "0.55", "2.20", "0.55", "0"),
            ("codex-mini", "Codex Mini", "0.75", "3", "0.025", "0"),
            ("gpt-5-mini", "GPT-5 Mini", "0.25", "2", "0.025", "0"),
            ("gpt-5-nano", "GPT-5 Nano", "0.05", "0.40", "0.005", "0"),
        ];

        let mut stmt = conn
            .prepare(
                "INSERT OR IGNORE INTO model_pricing (
                    model_id, display_name, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|e| {
                AppError::Database(format!(
                    "Failed to prepare the model pricing statement: {e}"
                ))
            })?;
        for (model_id, display_name, input, output, cache_read, cache_creation) in pricing_data {
            stmt.execute(rusqlite::params![
                model_id,
                display_name,
                input,
                output,
                cache_read,
                cache_creation
            ])
            .map_err(|e| AppError::Database(format!("Failed to insert the model pricing: {e}")))?;
        }

        log::info!("Inserted {} default model pricing rows", pricing_data.len());
        Ok(())
    }

    fn repair_current_model_pricing(conn: &Connection) -> Result<(), AppError> {
        let pricing_fixes = [
            // 2026-09-02: the official pricing page confirms the Sonnet 5 $2/$10 introductory price became the
            // list price and the planned 09-01 increase to $3/$15 was cancelled; rows seeded at the old list price
            // are moved back (rows the user edited do not match the old values and stay untouched)
            (
                "claude-sonnet-5",
                "Claude Sonnet 5",
                "2",
                "10",
                "0.20",
                "2.50",
                "3",
                "15",
                "0.30",
                "3.75",
            ),
            // 2026-08-13 models.dev pricing audit: the official cached input price for grok-4.5 is 0.30
            // (current docs.x.ai price table), matching the measured billing of grok-4.5-build; rows entered
            // earlier at 0.50 are corrected here. Note 0.50 is the cached price of grok-4.6 — do not mix them up
            (
                "grok-4.5", "Grok 4.5", "2", "6", "0.30", "0", "2", "6", "0.50", "0",
            ),
            // 2026-07-30 OpenAI GPT-5.6 price cut: luna -80%, terra -20% (sol unchanged).
            // Two guards per tier: the main guard matches >= v3.19 (already ran the 07-12 cache_write fix),
            // the zero-state guard matches users upgrading directly from < v3.19 (cache_write still the old seed 0)
            (
                "gpt-5.6-luna",
                "GPT-5.6 Luna",
                "0.20",
                "1.20",
                "0.02",
                "0.25",
                "1",
                "6",
                "0.10",
                "1.25",
            ),
            (
                "gpt-5.6-luna",
                "GPT-5.6 Luna",
                "0.20",
                "1.20",
                "0.02",
                "0.25",
                "1",
                "6",
                "0.10",
                "0",
            ),
            (
                "gpt-5.6-terra",
                "GPT-5.6 Terra",
                "2",
                "12",
                "0.20",
                "2.50",
                "2.50",
                "15",
                "0.25",
                "3.125",
            ),
            (
                "gpt-5.6-terra",
                "GPT-5.6 Terra",
                "2",
                "12",
                "0.20",
                "2.50",
                "2.50",
                "15",
                "0.25",
                "0",
            ),
            // 2026-07-31 models.dev pricing audit: after the V4 release, DeepSeek chat/reasoner dropped to the
            // V4 Flash alias price; the official MiniMax M3 standard tier is 0.3/1.2 (the old value looks like the accelerated tier)
            (
                "deepseek-chat",
                "DeepSeek Chat",
                "0.14",
                "0.28",
                "0.0028",
                "0",
                "0.27",
                "1.10",
                "0.07",
                "0",
            ),
            (
                "deepseek-reasoner",
                "DeepSeek Reasoner",
                "0.14",
                "0.28",
                "0.0028",
                "0",
                "0.55",
                "2.19",
                "0.14",
                "0",
            ),
            (
                "minimax-m3",
                "MiniMax M3",
                "0.30",
                "1.20",
                "0.06",
                "0",
                "0.60",
                "2.40",
                "0.12",
                "0",
            ),
            // 2026-07-12: the GPT-5.6 family charges cache write at 1.25x the input price (the new rule from OpenAI 5.6 on),
            // correcting the early seed value of 0; only rows the user has not edited are matched
            (
                "gpt-5.6-sol",
                "GPT-5.6 Sol",
                "5",
                "30",
                "0.50",
                "6.25",
                "5",
                "30",
                "0.50",
                "0",
            ),
            (
                "gpt-5.6-terra",
                "GPT-5.6 Terra",
                "2.50",
                "15",
                "0.25",
                "3.125",
                "2.50",
                "15",
                "0.25",
                "0",
            ),
            (
                "gpt-5.6-luna",
                "GPT-5.6 Luna",
                "1",
                "6",
                "0.10",
                "1.25",
                "1",
                "6",
                "0.10",
                "0",
            ),
            // 2026-06-10 full pricing audit (vendor official list prices; CNY converted at ~7.14)
            // GLM 4.6/4.7: the old values were relay/OpenRouter discount prices, aligned to the official Z.ai price (like glm-5/5.1)
            (
                "glm-4.7", "GLM-4.7", "0.6", "2.2", "0.11", "0", "0.39", "1.75", "0.04", "0",
            ),
            (
                "glm-4.6", "GLM-4.6", "0.6", "2.2", "0.11", "0", "0.28", "1.11", "0.03", "0",
            ),
            // Grok 4.20: xAI cut the price from 2/6 to 1.25/2.50
            (
                "grok-4.20-0309-reasoning",
                "Grok 4.20 Reasoning",
                "1.25",
                "2.50",
                "0.20",
                "0",
                "2",
                "6",
                "0.20",
                "0",
            ),
            (
                "grok-4.20-0309-non-reasoning",
                "Grok 4.20",
                "1.25",
                "2.50",
                "0.20",
                "0",
                "2",
                "6",
                "0.20",
                "0",
            ),
            // Kimi K2.5 official output 3.00
            (
                "kimi-k2.5",
                "Kimi K2.5",
                "0.60",
                "3.00",
                "0.10",
                "0",
                "0.60",
                "2.50",
                "0.10",
                "0",
            ),
            // MiniMax M2.5 input 0.15
            (
                "minimax-m2.5",
                "MiniMax M2.5",
                "0.15",
                "0.95",
                "0.03",
                "0",
                "0.12",
                "0.95",
                "0.03",
                "0",
            ),
            // Mistral Devstral 2 output 0.90 -> 2 (consistent with devstral-medium in the same table)
            (
                "devstral-2-2512",
                "Devstral 2",
                "0.40",
                "2",
                "0.04",
                "0",
                "0.40",
                "0.90",
                "0.04",
                "0",
            ),
            // Doubao Seed 2.0: the old lite price was 3-4x too high, plus cache hit prices added across the family
            (
                "doubao-seed-2-0-lite",
                "Doubao Seed 2.0 Lite",
                "0.08",
                "0.50",
                "0.017",
                "0",
                "0.25",
                "2",
                "0",
                "0",
            ),
            (
                "doubao-seed-2-0-pro",
                "Doubao Seed 2.0 Pro",
                "0.47",
                "2.37",
                "0.09",
                "0",
                "0.47",
                "2.37",
                "0",
                "0",
            ),
            (
                "doubao-seed-2-0-code",
                "Doubao Seed 2.0 Code",
                "0.47",
                "2.37",
                "0.09",
                "0",
                "0.47",
                "2.37",
                "0",
                "0",
            ),
            (
                "doubao-seed-2-0-code-preview-latest",
                "Doubao Seed 2.0 Code Preview",
                "0.47",
                "2.37",
                "0.09",
                "0",
                "0.47",
                "2.37",
                "0",
                "0",
            ),
            (
                "doubao-seed-2-0-mini",
                "Doubao Seed 2.0 Mini",
                "0.03",
                "0.31",
                "0.0056",
                "0",
                "0.03",
                "0.31",
                "0",
                "0",
            ),
            // MiMo: permanent price cut on 5/27, the old values are the pre-cut prices
            (
                "mimo-v2-pro",
                "MiMo V2 Pro",
                "0.435",
                "0.87",
                "0.0036",
                "0",
                "1",
                "3",
                "0",
                "0",
            ),
            (
                "mimo-v2.5",
                "MiMo V2.5",
                "0.14",
                "0.29",
                "0.0028",
                "0",
                "0.09",
                "0.29",
                "0.009",
                "0",
            ),
            (
                "mimo-v2.5-pro",
                "MiMo V2.5 Pro",
                "0.435",
                "0.87",
                "0.0036",
                "0",
                "1",
                "3",
                "0",
                "0",
            ),
            // Qwen: the official "implicit cache = 20% of input" rule fills in the cache hit price
            (
                "qwen3.6-plus",
                "Qwen3.6 Plus",
                "0.325",
                "1.95",
                "0.065",
                "0",
                "0.325",
                "1.95",
                "0",
                "0",
            ),
            (
                "qwen3.5-plus",
                "Qwen3.5 Plus",
                "0.26",
                "1.56",
                "0.052",
                "0",
                "0.26",
                "1.56",
                "0",
                "0",
            ),
            (
                "qwen3-coder-plus",
                "Qwen3 Coder Plus",
                "0.65",
                "3.25",
                "0.13",
                "0",
                "0.65",
                "3.25",
                "0",
                "0",
            ),
            (
                "qwen3-coder-flash",
                "Qwen3 Coder Flash",
                "0.195",
                "0.975",
                "0.039",
                "0",
                "0.195",
                "0.975",
                "0",
                "0",
            ),
            (
                "deepseek-v4-flash",
                "DeepSeek V4 Flash",
                "0.14",
                "0.28",
                "0.0028",
                "0",
                "0.14",
                "0.28",
                "0.028",
                "0",
            ),
            (
                "deepseek-v4-pro",
                "DeepSeek V4 Pro",
                "0.435",
                "0.87",
                "0.003625",
                "0",
                "1.68",
                "3.36",
                "0.14",
                "0",
            ),
            (
                "glm-5", "GLM-5", "1", "3.2", "0.2", "0", "0.72", "2.30", "0", "0",
            ),
            (
                "glm-5.1", "GLM-5.1", "1.4", "4.4", "0.26", "0", "0.95", "3.15", "0", "0",
            ),
            (
                "grok-code-fast-1",
                "Grok Build 0.1 (Code Fast Alias)",
                "1",
                "2",
                "0.20",
                "0",
                "0.20",
                "1.50",
                "0.02",
                "0",
            ),
            // 2026-09-06 audit. The entries below must come after all older entries above (chained guards; the
            // order is pinned by tests.rs::model_pricing_seed_repairs_known_outdated_builtin_prices):
            // - gpt-5.6-sol: on databases older than v3.19, the 07-12 entry first raises cache_write from 0 to 6.25, then this entry lowers it to the promo price
            // - minimax-m2.5: first goes through the 0.12 -> 0.15 entry, then this one to 0.30
            // Google 3.6 Flash moves to the introductory price 0.75/3.75/0.075 (through 2026-12-31, list 1.50/7.50/0.15)
            (
                "gemini-3.6-flash",
                "Gemini 3.6 Flash",
                "0.75",
                "3.75",
                "0.075",
                "0",
                "1.50",
                "7.50",
                "0.15",
                "0",
            ),
            // OpenAI GPT-5.6 Sol promotional price 4/20/0.40/5 (at least through 2026-11-21); the bare name and the effort suffix rows move together
            (
                "gpt-5.6-sol",
                "GPT-5.6 Sol",
                "4",
                "20",
                "0.40",
                "5",
                "5",
                "30",
                "0.50",
                "6.25",
            ),
            (
                "gpt-5.6",
                "GPT-5.6 Sol",
                "4",
                "20",
                "0.40",
                "5",
                "5",
                "30",
                "0.50",
                "6.25",
            ),
            (
                "gpt-5.6-low",
                "GPT-5.6 Sol",
                "4",
                "20",
                "0.40",
                "5",
                "5",
                "30",
                "0.50",
                "6.25",
            ),
            (
                "gpt-5.6-medium",
                "GPT-5.6 Sol",
                "4",
                "20",
                "0.40",
                "5",
                "5",
                "30",
                "0.50",
                "6.25",
            ),
            (
                "gpt-5.6-high",
                "GPT-5.6 Sol",
                "4",
                "20",
                "0.40",
                "5",
                "5",
                "30",
                "0.50",
                "6.25",
            ),
            (
                "gpt-5.6-xhigh",
                "GPT-5.6 Sol",
                "4",
                "20",
                "0.40",
                "5",
                "5",
                "30",
                "0.50",
                "6.25",
            ),
            (
                "gpt-5.6-minimal",
                "GPT-5.6 Sol",
                "4",
                "20",
                "0.40",
                "5",
                "5",
                "30",
                "0.50",
                "6.25",
            ),
            // Official MiniMax pay-as-you-go prices: M2 / M2.1 / M2.5 = 0.3/1.2/0.03/0.375
            (
                "minimax-m2",
                "MiniMax M2",
                "0.30",
                "1.20",
                "0.03",
                "0.375",
                "0.27",
                "0.95",
                "0.03",
                "0",
            ),
            (
                "minimax-m2.1",
                "MiniMax M2.1",
                "0.30",
                "1.20",
                "0.03",
                "0.375",
                "0.27",
                "0.95",
                "0.03",
                "0",
            ),
            (
                "minimax-m2.5",
                "MiniMax M2.5",
                "0.30",
                "1.20",
                "0.03",
                "0.375",
                "0.15",
                "0.95",
                "0.03",
                "0",
            ),
            // 2026-09-11 audit: DeepSeek V4 Flash is retired, and requests to deepseek-v4-flash / -0731 are now
            // served by V4.1-Flash and billed at the Flash price (official pricing page quick_start/pricing).
            // This repository never adopted the upstream 2026-08-16 peak/off-peak split, so the guard uses the 2026-07 price
            // 0.14/0.28/0.0028 → 0.3/1.2/0.006。
            //
            // NOTE: this must come after the 2026-07 cache_read fix above — an old database has to be pushed to
            // 0.0028 first for this group's guard to match; moved before it, an old database would stall at 0.028.
            // deepseek-chat / deepseek-reasoner are deliberately excluded here — they are fully delisted officially and no
            // authoritative source shows they followed the cut; see the DeepSeek V4 comment in seed_model_pricing.
            (
                "deepseek-v4-flash",
                "DeepSeek V4 Flash",
                "0.3",
                "1.2",
                "0.006",
                "0",
                "0.14",
                "0.28",
                "0.0028",
                "0",
            ),
            (
                "deepseek-v4-flash-0731",
                "DeepSeek V4 Flash",
                "0.3",
                "1.2",
                "0.006",
                "0",
                "0.14",
                "0.28",
                "0.0028",
                "0",
            ),
            // From 2026-09-14 12:00 Beijing time all deepseek-v4-pro requests are routed to V4.1 Flash and billed at
            // the Flash price (note (2) on the official pricing page), so this row drops to the Flash tier.
            //
            // NOTE: the guard is this repository's 2026-07 V4 Pro price 0.435/0.87/0.003625, produced by the entry
            // above — this one must come after it. Chain: 1.68/3.36/0.14 -> (2026-07) -> 0.435/0.87/0.003625
            // -> (this entry) -> 0.3/1.2/0.006.
            (
                "deepseek-v4-pro",
                "DeepSeek V4 Pro",
                "0.3",
                "1.2",
                "0.006",
                "0",
                "0.435",
                "0.87",
                "0.003625",
                "0",
            ),
        ];

        for (
            model_id,
            display_name,
            input,
            output,
            cache_read,
            cache_creation,
            old_input,
            old_output,
            old_cache_read,
            old_cache_creation,
        ) in pricing_fixes
        {
            conn.execute(
                "UPDATE model_pricing SET
                    display_name = ?2,
                    input_cost_per_million = ?3,
                    output_cost_per_million = ?4,
                    cache_read_cost_per_million = ?5,
                    cache_creation_cost_per_million = ?6
                 WHERE model_id = ?1
                   AND input_cost_per_million = ?7
                   AND output_cost_per_million = ?8
                   AND cache_read_cost_per_million = ?9
                   AND cache_creation_cost_per_million = ?10",
                rusqlite::params![
                    model_id,
                    display_name,
                    input,
                    output,
                    cache_read,
                    cache_creation,
                    old_input,
                    old_output,
                    old_cache_read,
                    old_cache_creation
                ],
            )
            .map_err(|e| {
                AppError::Database(format!(
                    "Failed to repair the pricing of model {model_id}: {e}"
                ))
            })?;
        }

        Ok(())
    }

    /// Make sure the model pricing table has its default data
    pub fn ensure_model_pricing_seeded(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        Self::ensure_model_pricing_seeded_on_conn(&conn)
    }

    pub(crate) fn ensure_model_pricing_seeded_on_conn(conn: &Connection) -> Result<(), AppError> {
        // INSERT OR IGNORE runs on every startup to append new models; only prices still equal to the old built-in values are repaired.
        Self::seed_model_pricing(conn)?;
        Self::repair_current_model_pricing(conn)
    }

    // --- Helpers ---

    pub(crate) fn get_user_version(conn: &Connection) -> Result<i32, AppError> {
        conn.query_row("PRAGMA user_version;", [], |row| row.get(0))
            .map_err(|e| AppError::Database(format!("Failed to read user_version: {e}")))
    }

    pub(crate) fn set_user_version(conn: &Connection, version: i32) -> Result<(), AppError> {
        if version < 0 {
            return Err(AppError::Database(
                "user_version must not be negative".to_string(),
            ));
        }
        let sql = format!("PRAGMA user_version = {version};");
        conn.execute(&sql, [])
            .map_err(|e| AppError::Database(format!("Failed to write user_version: {e}")))?;
        Ok(())
    }

    fn create_request_logs_usage_indexes_if_supported(conn: &Connection) -> Result<(), AppError> {
        if !Self::table_exists(conn, "proxy_request_logs")? {
            return Ok(());
        }

        let has_app_type = Self::has_column(conn, "proxy_request_logs", "app_type")?;
        let has_created_at = Self::has_column(conn, "proxy_request_logs", "created_at")?;
        if has_app_type && has_created_at {
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_request_logs_app_created_at
                 ON proxy_request_logs(app_type, created_at DESC)",
                [],
            )
            .map_err(|e| {
                AppError::Database(format!("Failed to create the usage app/time index: {e}"))
            })?;
        }

        let required_columns = [
            "app_type",
            "data_source",
            "input_tokens",
            "output_tokens",
            "cache_read_tokens",
            "created_at",
            "cache_creation_tokens",
        ];
        for column in required_columns {
            if !Self::has_column(conn, "proxy_request_logs", column)? {
                return Ok(());
            }
        }

        conn.execute("DROP INDEX IF EXISTS idx_request_logs_dedup_lookup", [])
            .map_err(|e| {
                AppError::Database(format!("Failed to drop the old usage dedup index: {e}"))
            })?;

        // To stay compatible with legacy NULL data_source rows, the query layer uses
        // COALESCE(data_source, 'proxy'). A plain data_source index cannot match that expression and would
        // degrade the cross-source dedup subquery into large scans; an expression index lets SQLite look it up directly.
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_logs_dedup_lookup_expr
             ON proxy_request_logs(app_type, COALESCE(data_source, 'proxy'), input_tokens,
                                   output_tokens, cache_read_tokens, created_at,
                                   cache_creation_tokens)",
            [],
        )
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to create the usage dedup expression index: {e}"
            ))
        })?;
        Ok(())
    }

    fn validate_identifier(s: &str, kind: &str) -> Result<(), AppError> {
        if s.is_empty() {
            return Err(AppError::Database(format!("{kind} must not be empty")));
        }
        if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(AppError::Database(format!(
                "Invalid {kind}: {s}; only letters, digits and underscores are allowed"
            )));
        }
        Ok(())
    }

    pub(crate) fn table_exists(conn: &Connection, table: &str) -> Result<bool, AppError> {
        Self::validate_identifier(table, "table name")?;

        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .map_err(|e| AppError::Database(format!("Failed to read the table name: {e}")))?;
        let mut rows = stmt
            .query([])
            .map_err(|e| AppError::Database(format!("Failed to query the table name: {e}")))?;
        while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
            let name: String = row
                .get(0)
                .map_err(|e| AppError::Database(format!("Failed to parse the table name: {e}")))?;
            if name.eq_ignore_ascii_case(table) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn has_column(
        conn: &Connection,
        table: &str,
        column: &str,
    ) -> Result<bool, AppError> {
        Self::validate_identifier(table, "table name")?;
        Self::validate_identifier(column, "column name")?;

        let sql = format!("PRAGMA table_info(\"{table}\");");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::Database(format!("Failed to read the table schema: {e}")))?;
        let mut rows = stmt
            .query([])
            .map_err(|e| AppError::Database(format!("Failed to query the table schema: {e}")))?;
        while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
            let name: String = row
                .get(1)
                .map_err(|e| AppError::Database(format!("Failed to read the column name: {e}")))?;
            if name.eq_ignore_ascii_case(column) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn add_column_if_missing(
        conn: &Connection,
        table: &str,
        column: &str,
        definition: &str,
    ) -> Result<bool, AppError> {
        Self::validate_identifier(table, "table name")?;
        Self::validate_identifier(column, "column name")?;

        if !Self::table_exists(conn, table)? {
            return Err(AppError::Database(format!(
                "table {table} does not exist, cannot add column {column}"
            )));
        }
        if Self::has_column(conn, table, column)? {
            return Ok(false);
        }

        let sql = format!("ALTER TABLE \"{table}\" ADD COLUMN \"{column}\" {definition};");
        conn.execute(&sql, []).map_err(|e| {
            AppError::Database(format!(
                "Failed to add column {column} to table {table}: {e}"
            ))
        })?;
        log::info!("Added missing column {column} to table {table}");
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_v12_to_v13_adds_input_token_semantics_columns() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        conn.execute(
            "CREATE TABLE proxy_request_logs (request_id TEXT PRIMARY KEY)",
            [],
        )?;
        conn.execute(
            "CREATE TABLE usage_daily_rollups (date TEXT PRIMARY KEY)",
            [],
        )?;
        Database::set_user_version(&conn, 12)?;

        Database::apply_schema_migrations_on_conn(&conn)?;

        assert_eq!(Database::get_user_version(&conn)?, SCHEMA_VERSION);
        assert!(Database::has_column(
            &conn,
            "proxy_request_logs",
            "input_token_semantics"
        )?);
        assert!(Database::has_column(
            &conn,
            "usage_daily_rollups",
            "input_token_semantics"
        )?);
        let log_default: i64 = conn.query_row(
            "SELECT dflt_value = '0' FROM pragma_table_info('proxy_request_logs')
             WHERE name = 'input_token_semantics'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(log_default, 1);

        Ok(())
    }

    #[test]
    fn migrate_v13_to_v14_adds_grokbuild_proxy_row_and_preserves_values() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        Database::create_tables_on_conn(&conn)?;
        conn.execute("DELETE FROM proxy_config WHERE app_type = 'grokbuild'", [])?;
        conn.execute(
            "UPDATE proxy_config SET enabled = 1, max_retries = 9 WHERE app_type = 'codex'",
            [],
        )?;
        Database::set_user_version(&conn, 13)?;

        Database::apply_schema_migrations_on_conn(&conn)?;

        assert_eq!(Database::get_user_version(&conn)?, SCHEMA_VERSION);
        let grok_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM proxy_config WHERE app_type = 'grokbuild'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(grok_rows, 1);
        let codex_values: (i64, i64) = conn.query_row(
            "SELECT enabled, max_retries FROM proxy_config WHERE app_type = 'codex'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(codex_values, (1, 9));

        Ok(())
    }

    #[test]
    fn migrate_v14_to_v15_adds_grokbuild_skill_and_mcp_flags() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE mcp_servers (
                id TEXT PRIMARY KEY,
                enabled_codex BOOLEAN NOT NULL DEFAULT 0
            );
            CREATE TABLE skills (
                id TEXT PRIMARY KEY,
                enabled_codex BOOLEAN NOT NULL DEFAULT 0
            );",
        )?;
        conn.execute(
            "INSERT INTO mcp_servers (id, enabled_codex) VALUES ('mcp-1', 1)",
            [],
        )?;
        conn.execute(
            "INSERT INTO skills (id, enabled_codex) VALUES ('skill-1', 1)",
            [],
        )?;
        Database::set_user_version(&conn, 14)?;

        Database::apply_schema_migrations_on_conn(&conn)?;

        assert_eq!(Database::get_user_version(&conn)?, SCHEMA_VERSION);
        assert!(Database::has_column(
            &conn,
            "mcp_servers",
            "enabled_grokbuild"
        )?);
        assert!(Database::has_column(&conn, "skills", "enabled_grokbuild")?);
        let mcp_values: (i64, i64) = conn.query_row(
            "SELECT enabled_codex, enabled_grokbuild FROM mcp_servers WHERE id = 'mcp-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let skill_values: (i64, i64) = conn.query_row(
            "SELECT enabled_codex, enabled_grokbuild FROM skills WHERE id = 'skill-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(mcp_values, (1, 0));
        assert_eq!(skill_values, (1, 0));

        Ok(())
    }

    #[test]
    fn migrate_v15_to_v16_resets_only_codex_session_usage() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        Database::create_tables_on_conn(&conn)?;
        conn.execute_batch(
            "INSERT INTO proxy_request_logs (
                request_id, provider_id, app_type, model, input_tokens,
                output_tokens, cache_read_tokens, latency_ms, status_code,
                created_at, data_source
             ) VALUES
                ('codex-row', '_codex_session', 'codex', 'gpt', 1, 1, 0, 0, 200, 1, 'codex_session'),
                ('gemini-row', '_gemini_session', 'gemini', 'gemini', 1, 1, 0, 0, 200, 1, 'gemini_session');
             INSERT INTO usage_daily_rollups (date, app_type, provider_id, model)
             VALUES
                ('2026-07-10', 'codex', '_codex_session', 'gpt'),
                ('2026-07-10', 'gemini', '_gemini_session', 'gemini');
             INSERT INTO session_log_sync
                (file_path, last_modified, last_line_offset, last_synced_at)
             VALUES
                ('/old/sessions/rollout-old-00000000-0000-4000-8000-000000000001.jsonl', 1, 1, 1),
                ('/gemini/tmp/session-123.json', 1, 1, 1);",
        )?;
        Database::set_user_version(&conn, 15)?;

        Database::apply_schema_migrations_on_conn(&conn)?;

        assert_eq!(Database::get_user_version(&conn)?, SCHEMA_VERSION);
        assert!(Database::table_exists(&conn, "session_usage_dedup")?);
        let counts: (i64, i64, i64, i64) = conn.query_row(
            "SELECT
                (SELECT COUNT(*) FROM proxy_request_logs WHERE data_source = 'codex_session'),
                (SELECT COUNT(*) FROM proxy_request_logs WHERE data_source = 'gemini_session'),
                (SELECT COUNT(*) FROM usage_daily_rollups WHERE provider_id = '_codex_session'),
                (SELECT COUNT(*) FROM session_log_sync)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        assert_eq!(counts, (0, 1, 0, 1));
        Ok(())
    }

    #[test]
    fn migrate_v16_to_v17_creates_session_usage_dedup_ledger() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        Database::set_user_version(&conn, 16)?;

        Database::apply_schema_migrations_on_conn(&conn)?;

        assert_eq!(Database::get_user_version(&conn)?, SCHEMA_VERSION);
        assert!(Database::table_exists(&conn, "session_usage_dedup")?);
        conn.execute(
            "INSERT INTO session_usage_dedup
             (data_source, request_id, semantic_id, has_entry_id)
             VALUES ('pi_session', 'request', 'semantic', 1)",
            [],
        )?;
        Ok(())
    }

    #[test]
    fn migrate_v17_to_v18_creates_bounded_tool_version_history() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        Database::set_user_version(&conn, 17)?;

        Database::apply_schema_migrations_on_conn(&conn)?;

        assert_eq!(Database::get_user_version(&conn)?, SCHEMA_VERSION);
        assert!(Database::table_exists(&conn, "tool_version_events")?);
        conn.execute(
            "INSERT INTO tool_version_events
             (tool_id, from_version, to_version, source, operation_kind, occurred_at)
             VALUES ('codex', '1.0.0', '1.1.0', 'npm', 'update', 1)",
            [],
        )?;
        assert!(conn
            .execute(
                "INSERT INTO tool_version_events
                 (tool_id, from_version, to_version, source, operation_kind, occurred_at)
                 VALUES ('codex', '1.1.0', '1.2.0', 'npm', 'repair', 2)",
                [],
            )
            .is_err());
        Ok(())
    }

    #[test]
    fn migrate_v18_to_v19_preserves_history_and_admits_only_new_python_owners(
    ) -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE tool_version_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_id TEXT NOT NULL,
                from_version TEXT,
                to_version TEXT NOT NULL,
                source TEXT NOT NULL CHECK (source IN (
                    'npm', 'pnpm', 'bun', 'volta', 'brew',
                    'nativeInstaller', 'unmanaged'
                )),
                operation_kind TEXT NOT NULL,
                occurred_at INTEGER NOT NULL
             );
             CREATE INDEX idx_tool_version_events_history
             ON tool_version_events(tool_id, occurred_at DESC, id DESC);
             INSERT INTO tool_version_events
                (tool_id, from_version, to_version, source, operation_kind, occurred_at)
             VALUES ('codex', '1.0.0', '1.1.0', 'npm', 'update', 10);",
        )?;
        Database::set_user_version(&conn, 18)?;

        Database::apply_schema_migrations_on_conn(&conn)?;

        assert_eq!(Database::get_user_version(&conn)?, SCHEMA_VERSION);
        let preserved: (i64, String) = conn.query_row(
            "SELECT id, source FROM tool_version_events WHERE tool_id = 'codex'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(preserved, (1, "npm".to_string()));
        for (occurred_at, source) in [(20, "uv"), (30, "pipx")] {
            conn.execute(
                "INSERT INTO tool_version_events
                 (tool_id, from_version, to_version, source, operation_kind, occurred_at)
                 VALUES ('hermes', '0.18.0', '0.19.0', ?1, 'changeVersion', ?2)",
                params![source, occurred_at],
            )?;
        }
        assert!(conn
            .execute(
                "INSERT INTO tool_version_events
                 (tool_id, to_version, source, operation_kind, occurred_at)
                 VALUES ('hermes', '0.20.0', 'pip', 'update', 40)",
                [],
            )
            .is_err());
        let index_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_tool_version_events_history'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(index_count, 1);
        Ok(())
    }

    #[test]
    fn migrate_v19_to_v20_adds_a_disabled_claude_desktop_scope_without_drift(
    ) -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE mcp_servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                server_config TEXT NOT NULL,
                enabled_claude BOOLEAN NOT NULL DEFAULT 0,
                enabled_codex BOOLEAN NOT NULL DEFAULT 0,
                enabled_gemini BOOLEAN NOT NULL DEFAULT 0,
                enabled_grokbuild BOOLEAN NOT NULL DEFAULT 0,
                enabled_opencode BOOLEAN NOT NULL DEFAULT 0,
                enabled_hermes BOOLEAN NOT NULL DEFAULT 0
             );
             INSERT INTO mcp_servers
                (id, name, server_config, enabled_claude, enabled_codex)
             VALUES ('filesystem', 'Filesystem', '{}', 1, 1);",
        )?;
        Database::set_user_version(&conn, 19)?;

        Database::apply_schema_migrations_on_conn(&conn)?;

        assert_eq!(Database::get_user_version(&conn)?, SCHEMA_VERSION);
        assert!(Database::has_column(
            &conn,
            "mcp_servers",
            "enabled_claude_desktop"
        )?);
        let flags: (bool, bool, bool) = conn.query_row(
            "SELECT enabled_claude, enabled_codex, enabled_claude_desktop
             FROM mcp_servers WHERE id = 'filesystem'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(flags, (true, true, false));
        Ok(())
    }

    #[test]
    fn startup_cleanup_removes_only_retired_product_settings() -> Result<(), AppError> {
        let db = Database::memory()?;
        db.set_setting("aimgr.toolVersionPin.codex", "1.2.3")?;
        db.set_setting("aimgr.toolVersionPin.claude-code", "")?;
        db.set_setting("aimgr.onboardingCompleted", "true")?;
        db.set_setting("aimgr.importPromptSeen", "true")?;
        db.set_setting("global_proxy_url", "http://127.0.0.1:7890")?;

        assert_eq!(db.remove_retired_product_settings()?, 3);

        assert_eq!(db.get_setting("aimgr.toolVersionPin.codex")?, None);
        assert_eq!(db.get_setting("aimgr.toolVersionPin.claude-code")?, None);
        assert_eq!(db.get_setting("aimgr.onboardingCompleted")?, None);
        assert_eq!(
            db.get_setting("aimgr.importPromptSeen")?.as_deref(),
            Some("true")
        );
        assert_eq!(
            db.get_setting("global_proxy_url")?.as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(db.remove_retired_product_settings()?, 0);
        Ok(())
    }
}
