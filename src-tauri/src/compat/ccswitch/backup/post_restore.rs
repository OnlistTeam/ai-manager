//! What the product projects to the tools after the managed database has been
//! replaced (backup restore or configuration-archive import).
//!
//! Upstream `commands::sync_support::run_post_import_sync` re-projects
//! *everything* it manages: the current providers, every MCP server (deleting
//! live entries the database marks disabled), every Skill directory (deleting
//! tool copies the database marks disabled) and the first enabled Prompt
//! (overwriting CLAUDE.md / AGENTS.md / … with the stored content, no backup).
//! After a restore those rows are by definition an old snapshot, while the live
//! files are what the user kept editing since. Projecting the snapshot over
//! them destroys work the backup never contained.
//!
//! The product therefore projects only what a restore is *for*: which service
//! each tool talks to. Extensions stay as they are on disk until the user
//! changes them from the Extensions page. The housekeeping steps (pricing
//! table, settings cache, log level, usage cache) are kept because they read
//! the restored database instead of writing to tool files.
//!
//! The two provider projections are upstream `services/provider/live.rs`
//! functions opened from `fn` to `pub(crate) fn` (visibility only). They are
//! used instead of `ProviderService::sync_current_provider_for_app`, which
//! finishes with a per-app MCP projection that deletes live servers.

use crate::app_config::AppType;
use crate::error::AppError;
use crate::services::model_pricing;
use crate::services::provider::{
    sync_all_providers_to_live, sync_current_provider_for_app_respecting_takeover,
};
use crate::settings;
use crate::store::AppState;

/// Best-effort like the upstream routine: every step runs, failures are
/// collected, and the caller turns an `Err` into `tools_out_of_sync = true`.
pub(super) fn project_restored_database(state: &AppState) -> Result<(), AppError> {
    let mut failures = Vec::new();

    for app_type in AppType::all() {
        if matches!(app_type, AppType::Pi) {
            continue;
        }
        // Additive-mode tools keep every provider in their live file, and the
        // upstream writer only upserts (`opencode_config::set_provider` and
        // friends), so a stale snapshot cannot delete a connection there.
        let result = if app_type.is_additive_mode() {
            sync_all_providers_to_live(state, &app_type)
        } else {
            sync_current_provider_for_app_respecting_takeover(state, &app_type)
        };
        if let Err(error) = result {
            log::warn!(
                "[Restore] projecting the restored {} provider to its live config failed: {error}",
                app_type.as_str()
            );
            failures.push(format!("provider/{}: {error}", app_type.as_str()));
        }
    }

    if let Err(error) = model_pricing::sync_local_model_pricing(&state.db) {
        failures.push(format!("model pricing: {error}"));
    }
    if let Err(error) = settings::reload_settings() {
        failures.push(format!("settings cache: {error}"));
    }
    match state.db.get_log_config() {
        Ok(log_config) => log::set_max_level(log_config.to_level_filter()),
        Err(error) => {
            log::set_max_level(log::LevelFilter::Info);
            failures.push(format!("runtime log level: {error}"));
        }
    }
    state.usage_cache.invalidate_all();

    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppError::Message(format!(
            "post-restore projection failed: {}",
            failures.join("; ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use super::project_restored_database;
    use crate::app_config::{AppType, InstalledSkill, McpApps, McpServer, SkillApps};
    use crate::database::Database;
    use crate::prompt::Prompt;
    use crate::provider::Provider as UpstreamProvider;
    use crate::store::AppState;

    struct TestHome(Option<std::ffi::OsString>);

    impl TestHome {
        fn set(path: &std::path::Path) -> Self {
            let previous = std::env::var_os("AI_MANAGER_TEST_HOME");
            std::env::set_var("AI_MANAGER_TEST_HOME", path);
            Self(previous)
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
                None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
            }
        }
    }

    const HAND_EDITED: &str = "# Hand-edited long after the backup was taken\n";
    const LIVE_MCP: &str = r#"{"mcpServers":{"browser":{"command":"npx","args":["browser-mcp"]}}}"#;

    #[test]
    #[serial_test::serial]
    fn a_restored_database_projects_the_service_but_never_its_extension_snapshot() {
        let temp = tempfile::tempdir().expect("temp home");
        let _home = TestHome::set(temp.path());
        let claude_dir = temp.path().join(".claude");
        let skill_doc = claude_dir
            .join("skills")
            .join("local-skill")
            .join("SKILL.md");
        fs::create_dir_all(skill_doc.parent().expect("skill directory")).expect("skill dir");
        fs::write(&skill_doc, "---\nname: Local\n---\n").expect("seed tool Skill copy");
        let instructions = claude_dir.join("CLAUDE.md");
        fs::write(&instructions, HAND_EDITED).expect("seed hand-edited instructions");
        let live_mcp = temp.path().join(".claude.json");
        fs::write(&live_mcp, LIVE_MCP).expect("seed live MCP config");

        let state = AppState::new(Arc::new(Database::memory().expect("memory database")));
        let app = AppType::Claude.as_str();
        // The restored snapshot: an enabled Prompt whose content is months old,
        // plus an MCP server and a Skill the snapshot marks *disabled* for Claude
        // although the user has since re-enabled both directly in the tool.
        state
            .db
            .save_prompt(
                app,
                &Prompt {
                    id: "stale".to_string(),
                    name: "Stale".to_string(),
                    content: "# Stale snapshot".to_string(),
                    description: None,
                    enabled: true,
                    created_at: Some(1),
                    updated_at: Some(1),
                },
            )
            .expect("seed stale Prompt");
        state
            .db
            .save_mcp_server(&McpServer {
                id: "browser".to_string(),
                name: "browser".to_string(),
                server: json!({"command": "npx", "args": ["browser-mcp"]}),
                apps: McpApps::default(),
                description: None,
                homepage: None,
                docs: None,
                tags: Vec::new(),
            })
            .expect("seed disabled MCP row");
        state
            .db
            .save_skill(&InstalledSkill {
                id: "local-skill".to_string(),
                name: "Local".to_string(),
                description: None,
                directory: "local-skill".to_string(),
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                readme_url: None,
                apps: SkillApps::default(),
                installed_at: 1,
                content_hash: None,
                updated_at: 0,
            })
            .expect("seed disabled Skill row");
        // The one thing a restore is for: which service the tool talks to.
        let provider = UpstreamProvider::with_id(
            "primary".to_string(),
            "Primary".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.test",
                    "ANTHROPIC_API_KEY": "sk-primary-0123456789ABCD"
                }
            }),
            None,
        );
        state
            .db
            .save_provider(app, &provider)
            .expect("seed restored provider");
        state
            .db
            .set_current_provider(app, &provider.id)
            .expect("seed restored current provider");

        project_restored_database(&state).expect("projection succeeds");

        assert_eq!(
            fs::read_to_string(&instructions).expect("read instructions"),
            HAND_EDITED,
            "the stale Prompt snapshot was projected over the hand-edited file"
        );
        assert!(
            skill_doc.is_file(),
            "the tool's own Skill copy was removed because the snapshot marked it disabled"
        );
        let mcp: Value =
            serde_json::from_slice(&fs::read(&live_mcp).expect("read live MCP")).expect("json");
        assert!(
            mcp["mcpServers"]["browser"].is_object(),
            "the live MCP server was removed because the snapshot marked it disabled"
        );
        let settings: Value = serde_json::from_slice(
            &fs::read(crate::config::get_claude_settings_path()).expect("read live settings"),
        )
        .expect("valid live settings");
        assert_eq!(
            settings["env"]["ANTHROPIC_BASE_URL"],
            "https://primary.example.test"
        );
    }
}
