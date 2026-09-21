//! JSON -> SQLite data migration
//!
//! Migrates legacy config.json (MultiAppConfig) data into the SQLite database.

use super::{lock_conn, to_json_string, Database};
use crate::app_config::MultiAppConfig;
use crate::error::AppError;
use rusqlite::{params, Connection};

impl Database {
    /// Migrate data from a MultiAppConfig into the database
    pub fn migrate_from_json(&self, config: &MultiAppConfig) -> Result<(), AppError> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;

        Self::migrate_from_json_tx(&tx, config)?;

        tx.commit()
            .map_err(|e| AppError::Database(format!("Commit migration failed: {e}")))?;
        Ok(())
    }

    /// Run the migration as a dry run (validated in an in-memory database, nothing hits disk)
    ///
    /// Used to verify the migration logic before deployment.
    pub fn migrate_from_json_dry_run(config: &MultiAppConfig) -> Result<(), AppError> {
        let mut conn =
            Connection::open_in_memory().map_err(|e| AppError::Database(e.to_string()))?;
        Self::create_tables_on_conn(&conn)?;
        Self::apply_schema_migrations_on_conn(&conn)?;

        let tx = conn
            .transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;
        Self::migrate_from_json_tx(&tx, config)?;

        // Drop the transaction explicitly without committing (the in-memory database is discarded)
        drop(tx);
        Ok(())
    }

    /// Run the migration inside a transaction
    fn migrate_from_json_tx(
        tx: &rusqlite::Transaction<'_>,
        config: &MultiAppConfig,
    ) -> Result<(), AppError> {
        // 1. Migrate providers
        Self::migrate_providers(tx, config)?;

        // 2. Migrate MCP servers
        Self::migrate_mcp_servers(tx, config)?;

        // 3. Migrate prompts
        Self::migrate_prompts(tx, config)?;

        // 4. Migrate skills
        Self::migrate_skills(tx, config)?;

        // 5. Migrate the common config
        Self::migrate_common_config(tx, config)?;

        Ok(())
    }

    /// Migrate provider data
    fn migrate_providers(
        tx: &rusqlite::Transaction<'_>,
        config: &MultiAppConfig,
    ) -> Result<(), AppError> {
        for (app_key, manager) in &config.apps {
            let app_type = app_key;
            let current_id = &manager.current;

            for (id, provider) in &manager.providers {
                let is_current = if id == current_id { 1 } else { 0 };

                // Handle meta and endpoints
                let mut meta_clone = provider.meta.clone().unwrap_or_default();
                let endpoints = std::mem::take(&mut meta_clone.custom_endpoints);

                tx.execute(
                    "INSERT OR REPLACE INTO providers (
                        id, app_type, name, settings_config, website_url, category,
                        created_at, sort_index, notes, icon, icon_color, meta, is_current
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        id,
                        app_type,
                        provider.name,
                        to_json_string(&provider.settings_config)?,
                        provider.website_url,
                        provider.category,
                        provider.created_at,
                        provider.sort_index,
                        provider.notes,
                        provider.icon,
                        provider.icon_color,
                        to_json_string(&meta_clone)?,
                        is_current,
                    ],
                )
                .map_err(|e| AppError::Database(format!("Migrate provider failed: {e}")))?;

                // Migrate endpoints
                for (url, endpoint) in endpoints {
                    tx.execute(
                        "INSERT INTO provider_endpoints (provider_id, app_type, url, added_at)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![id, app_type, url, endpoint.added_at],
                    )
                    .map_err(|e| AppError::Database(format!("Migrate endpoint failed: {e}")))?;
                }
            }
        }
        Ok(())
    }

    /// Migrate MCP server data
    fn migrate_mcp_servers(
        tx: &rusqlite::Transaction<'_>,
        config: &MultiAppConfig,
    ) -> Result<(), AppError> {
        if let Some(servers) = &config.mcp.servers {
            for (id, server) in servers {
                tx.execute(
                    "INSERT OR REPLACE INTO mcp_servers (
                        id, name, server_config, description, homepage, docs, tags,
                        enabled_claude, enabled_codex, enabled_gemini
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        id,
                        server.name,
                        to_json_string(&server.server)?,
                        server.description,
                        server.homepage,
                        server.docs,
                        to_json_string(&server.tags)?,
                        server.apps.claude,
                        server.apps.codex,
                        server.apps.gemini,
                    ],
                )
                .map_err(|e| AppError::Database(format!("Migrate mcp server failed: {e}")))?;
            }
        }
        Ok(())
    }

    /// Migrate prompt data
    fn migrate_prompts(
        tx: &rusqlite::Transaction<'_>,
        config: &MultiAppConfig,
    ) -> Result<(), AppError> {
        let migrate_app_prompts = |prompts_map: &std::collections::HashMap<
            String,
            crate::prompt::Prompt,
        >,
                                   app_type: &str|
         -> Result<(), AppError> {
            for (id, prompt) in prompts_map {
                tx.execute(
                        "INSERT OR REPLACE INTO prompts (
                            id, app_type, name, content, description, enabled, created_at, updated_at
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            id,
                            app_type,
                            prompt.name,
                            prompt.content,
                            prompt.description,
                            prompt.enabled,
                            prompt.created_at,
                            prompt.updated_at,
                        ],
                    )
                    .map_err(|e| AppError::Database(format!("Migrate prompt failed: {e}")))?;
            }
            Ok(())
        };

        migrate_app_prompts(&config.prompts.claude.prompts, "claude")?;
        migrate_app_prompts(&config.prompts.codex.prompts, "codex")?;
        migrate_app_prompts(&config.prompts.gemini.prompts, "gemini")?;

        Ok(())
    }

    /// Migrate skill data
    fn migrate_skills(
        tx: &rusqlite::Transaction<'_>,
        config: &MultiAppConfig,
    ) -> Result<(), AppError> {
        // The SSOT for skills has moved to the product filesystem directory plus the unified
        // database structure.
        //
        // `skills.skills` in the legacy config.json only recorded install state, without full
        // metadata, and gives no guarantee that the matching skill file exists in the SSOT
        // directory.
        //
        // So the old install state is no longer written straight into the new skills table,
        // which would create the inconsistency of "installed in the database but the file is
        // missing". After migration, installed-skill records can be rebuilt via:
        // - the frontend "import existing" action (scans each app's skills directory and copies
        //   into the SSOT), or
        // - the automatic scan performed on a later startup.

        for repo in &config.skills.repos {
            tx.execute(
                "INSERT OR REPLACE INTO skill_repos (owner, name, branch, enabled) VALUES (?1, ?2, ?3, ?4)",
                params![repo.owner, repo.name, repo.branch, repo.enabled],
            ).map_err(|e| AppError::Database(format!("Migrate skill repo failed: {e}")))?;
        }

        Ok(())
    }

    /// Migrate the common config snippet
    fn migrate_common_config(
        tx: &rusqlite::Transaction<'_>,
        config: &MultiAppConfig,
    ) -> Result<(), AppError> {
        if let Some(snippet) = &config.common_config_snippets.claude {
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params!["common_config_claude", snippet],
            )
            .map_err(|e| AppError::Database(format!("Migrate settings failed: {e}")))?;
        }
        if let Some(snippet) = &config.common_config_snippets.codex {
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params!["common_config_codex", snippet],
            )
            .map_err(|e| AppError::Database(format!("Migrate settings failed: {e}")))?;
        }
        if let Some(snippet) = &config.common_config_snippets.gemini {
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params!["common_config_gemini", snippet],
            )
            .map_err(|e| AppError::Database(format!("Migrate settings failed: {e}")))?;
        }

        Ok(())
    }
}
