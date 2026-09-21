//! Provider service module
//!
//! Handles provider CRUD operations, switching, and configuration management.

mod endpoints;
mod gemini_auth;
mod live;
mod pi;
mod usage;

use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

use crate::app_config::AppType;
use crate::database::{validate_cost_multiplier, validate_pricing_source};
use crate::error::AppError;
use crate::provider::{Provider, UsageResult};
use crate::services::mcp::McpService;
use crate::settings::CustomEndpoint;
use crate::store::AppState;

// Re-export sub-module functions for external access
pub use live::{
    import_default_config, import_hermes_providers_from_live, import_openclaw_providers_from_live,
    import_opencode_providers_from_live, read_live_settings,
    should_import_default_config_on_startup, sync_current_to_live,
};

pub fn import_pi_providers_from_live(state: &AppState) -> Result<usize, AppError> {
    pi::import_from_live(state)
}

// Internal re-exports (pub(crate))
pub(crate) use live::sanitize_claude_settings_for_live;
pub(crate) use live::{
    build_effective_provider_for_live_with_codex_oauth_manager,
    build_effective_settings_with_common_config, normalize_provider_common_config_for_storage,
    provider_exists_in_live_config, strip_common_config_from_live_settings,
    sync_all_providers_to_live, sync_current_provider_for_app_respecting_takeover,
    sync_current_provider_for_app_to_live, write_live_with_common_config_for_codex_oauth_manager,
    write_live_with_common_config_for_state, LiveSyncOutcome,
};

// Internal re-exports
use live::{
    remove_hermes_provider_from_live, remove_openclaw_provider_from_live,
    remove_opencode_provider_from_live, write_gemini_live,
};
use usage::validate_usage_script;

/// Codex official providers are safe to select during takeover: Codex keeps
/// ownership of the active ChatGPT login and the proxy only forwards the
/// authenticated request. Other apps' official providers retain the block.
pub fn official_provider_supports_proxy_takeover(app_type: &AppType, provider: &Provider) -> bool {
    matches!(app_type, AppType::Codex)
        && crate::proxy::providers::is_codex_official_provider(provider)
}

/// When the unified session toggle changes, immediately rewrite the current
/// official Codex provider's live config to the new toggle state, so the
/// toggle takes effect right away (no need to wait for the next switch).
/// No-op when the current provider is not official (or doesn't exist): the
/// injection only applies to official configs, third-party live configs are
/// unaffected by the toggle.
pub fn reapply_current_codex_official_live(state: &AppState) -> Result<bool, AppError> {
    let current_id = ProviderService::current(state, AppType::Codex)?;
    if current_id.is_empty() {
        return Ok(false);
    }
    let providers = state.db.get_all_providers(AppType::Codex.as_str())?;
    let Some(provider) = providers.get(&current_id) else {
        return Ok(false);
    };
    if provider.category.as_deref() != Some("official")
        && !crate::proxy::providers::is_codex_official_provider(provider)
    {
        return Ok(false);
    }

    // During takeover, live is owned by the proxy; a leftover backup alone no
    // longer counts as takeover evidence.
    let outcome =
        live::sync_live_for_provider_respecting_takeover(state, &AppType::Codex, provider)?;
    if outcome == LiveSyncOutcome::BackupOnly {
        return Ok(true);
    }
    // Rewriting live replaces config.toml wholesale (by design), so
    // [mcp_servers] is lost along with it — we must immediately re-project
    // enabled MCPs from the DB. Only project Codex, not sync_all_enabled:
    // the latter walks AppType::all() and short-circuits on the first
    // failure, so a broken live for an unrelated app ahead of Codex (e.g. a
    // malformed ~/.claude.json) would block Codex's re-projection, leaving
    // the just-cleared [mcp_servers] never refilled.
    // A projection failure is downgraded to a warning: by this point live
    // has already been persisted with the new toggle state, so the toggle
    // has effectively taken effect. Propagating the error would make
    // save_settings roll back the toggle setting, producing a split state of
    // "setting = old value, live = new bucket" — exactly what that rollback
    // is meant to prevent. MCP projection is self-healing (the next switch,
    // or any MCP enable/disable, re-projects it).
    if let Err(err) = McpService::sync_enabled_for_app(state, &AppType::Codex) {
        log::warn!(
            "Re-projecting Codex MCP after unified session toggle rewrote live failed (will self-heal on next sync): {err}"
        );
    }
    Ok(true)
}

/// Provider business logic service
pub struct ProviderService;

/// Result of a provider switch operation, including any non-fatal warnings
#[derive(Debug, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SwitchResult {
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(target_os = "macos", windows))]
    use crate::claude_desktop_config::PROFILE_ID;
    use crate::config::{get_claude_settings_path, read_json_file, write_json_file};
    use crate::database::Database;
    use crate::provider::{
        AuthBinding, AuthBindingSource, ProviderMeta, UniversalProvider, UsageScript,
    };
    #[cfg(any(target_os = "macos", windows))]
    use crate::provider::{ClaudeDesktopMode, ClaudeDesktopModelRoute};
    use crate::proxy::types::ProxyConfig;
    use crate::store::AppState;
    use serde_json::json;
    use serial_test::serial;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, OnceLock};
    use tempfile::TempDir;

    struct TempHome {
        dir: TempDir,
        original_home: Option<String>,
        #[cfg(windows)]
        original_local_app_data: Option<String>,
        original_userprofile: Option<String>,
        original_test_home: Option<String>,
    }

    impl TempHome {
        fn new() -> Self {
            let dir = TempDir::new().expect("failed to create temp home");
            let original_home = env::var("HOME").ok();
            #[cfg(windows)]
            let original_local_app_data = env::var("LOCALAPPDATA").ok();
            let original_userprofile = env::var("USERPROFILE").ok();
            let original_test_home = env::var("AI_MANAGER_TEST_HOME").ok();

            env::set_var("HOME", dir.path());
            #[cfg(windows)]
            env::set_var("LOCALAPPDATA", dir.path().join("AppData").join("Local"));
            env::set_var("USERPROFILE", dir.path());
            env::set_var("AI_MANAGER_TEST_HOME", dir.path());

            Self {
                dir,
                original_home,
                #[cfg(windows)]
                original_local_app_data,
                original_userprofile,
                original_test_home,
            }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match &self.original_home {
                Some(value) => env::set_var("HOME", value),
                None => env::remove_var("HOME"),
            }

            #[cfg(windows)]
            {
                match &self.original_local_app_data {
                    Some(value) => env::set_var("LOCALAPPDATA", value),
                    None => env::remove_var("LOCALAPPDATA"),
                }
            }

            match &self.original_userprofile {
                Some(value) => env::set_var("USERPROFILE", value),
                None => env::remove_var("USERPROFILE"),
            }

            match &self.original_test_home {
                Some(value) => env::set_var("AI_MANAGER_TEST_HOME", value),
                None => env::remove_var("AI_MANAGER_TEST_HOME"),
            }
        }
    }

    #[cfg(windows)]
    fn claude_desktop_profile_path(home: &Path) -> PathBuf {
        home.join("AppData")
            .join("Local")
            .join("Claude-3p")
            .join("configLibrary")
            .join(format!("{PROFILE_ID}.json"))
    }

    #[cfg(target_os = "macos")]
    fn claude_desktop_profile_path(home: &Path) -> PathBuf {
        home.join("Library")
            .join("Application Support")
            .join("Claude-3p")
            .join("configLibrary")
            .join(format!("{PROFILE_ID}.json"))
    }

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    fn with_test_home<T>(test: impl FnOnce(&AppState, &Path) -> T) -> T {
        let _guard = test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let old_test_home = std::env::var_os("AI_MANAGER_TEST_HOME");
        let old_home = std::env::var_os("HOME");
        std::env::set_var("AI_MANAGER_TEST_HOME", temp.path());
        std::env::set_var("HOME", temp.path());
        // `get_hermes_dir()` consults HERMES_HOME first and falls back to %LOCALAPPDATA%
        // on Windows, so neither is covered by the test home. Left in place, the Hermes
        // cases below would read and write the machine's real Hermes config.
        let old_hermes_home = std::env::var_os("HERMES_HOME");
        let old_local_appdata = std::env::var_os("LOCALAPPDATA");
        std::env::remove_var("HERMES_HOME");
        std::env::remove_var("LOCALAPPDATA");

        let db = Arc::new(Database::memory().expect("in-memory database"));
        let state = AppState::new(db);
        let result = test(&state, temp.path());

        match old_local_appdata {
            Some(value) => std::env::set_var("LOCALAPPDATA", value),
            None => std::env::remove_var("LOCALAPPDATA"),
        }
        match old_hermes_home {
            Some(value) => std::env::set_var("HERMES_HOME", value),
            None => std::env::remove_var("HERMES_HOME"),
        }
        match old_test_home {
            Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
            None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
        }
        match old_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }

        result
    }

    fn codex_settings(base_url: &str, api_key: &str) -> Value {
        json!({
            "auth": {
                "OPENAI_API_KEY": api_key
            },
            "config": format!(
                "model_provider = \"custom\"\n\
                 [model_providers.custom]\n\
                 name = \"custom\"\n\
                 base_url = \"{base_url}\"\n\
                 wire_api = \"chat\"\n"
            )
        })
    }

    fn usage_script_with_credentials(
        api_key: Option<&str>,
        base_url: Option<&str>,
        template_type: Option<&str>,
    ) -> UsageScript {
        UsageScript {
            enabled: true,
            language: "javascript".to_string(),
            code: "return { remaining: 1, unit: 'USD' };".to_string(),
            timeout: Some(10),
            api_key: api_key.map(str::to_string),
            base_url: base_url.map(str::to_string),
            access_token: None,
            user_id: None,
            template_type: template_type.map(str::to_string),
            auto_query_interval: None,
            coding_plan_provider: None,
            access_key_id: Some("ak-test".to_string()),
            secret_access_key: Some("sk-test".to_string()),
            team_organization_id: None,
            team_project_id: None,
        }
    }

    fn codex_provider_with_usage(
        id: &str,
        base_url: &str,
        api_key: &str,
        usage_api_key: Option<&str>,
        usage_base_url: Option<&str>,
        template_type: Option<&str>,
    ) -> Provider {
        let mut provider = Provider::with_id(
            id.to_string(),
            format!("Provider {id}"),
            codex_settings(base_url, api_key),
            None,
        );
        provider.meta = Some(ProviderMeta {
            usage_script: Some(usage_script_with_credentials(
                usage_api_key,
                usage_base_url,
                template_type,
            )),
            ..Default::default()
        });
        provider
    }

    fn managed_codex_provider(id: &str, account_id: &str) -> Provider {
        let mut provider = Provider::with_id(
            id.to_string(),
            format!("Managed {id}"),
            json!({
                "auth": {},
                "config": ""
            }),
            None,
        );
        provider.category = Some("official".to_string());
        provider.meta = Some(ProviderMeta {
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("codex_oauth".to_string()),
                account_id: Some(account_id.to_string()),
            }),
            ..Default::default()
        });
        provider
    }

    fn openclaw_provider(id: &str) -> Provider {
        Provider {
            id: id.to_string(),
            name: format!("Provider {id}"),
            settings_config: json!({
                "baseUrl": "https://api.deepseek.com",
                "apiKey": "test-key",
                "api": "openai-completions",
                "models": [],
            }),
            website_url: None,
            category: Some("custom".to_string()),
            created_at: Some(1),
            sort_index: Some(0),
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    fn hermes_provider(id: &str) -> Provider {
        Provider {
            id: id.to_string(),
            name: format!("Provider {id}"),
            settings_config: json!({
                "api": "openai-chat",
                "base_url": "https://api.example.com/v1",
                "api_key": "test-key",
                "models": {
                    "gpt-4o": {
                        "name": "GPT-4o"
                    }
                }
            }),
            website_url: None,
            category: Some("custom".to_string()),
            created_at: Some(1),
            sort_index: Some(0),
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    fn opencode_provider(id: &str) -> Provider {
        Provider {
            id: id.to_string(),
            name: format!("Provider {id}"),
            settings_config: json!({
                "npm": "@ai-sdk/openai-compatible",
                "name": format!("Provider {id}"),
                "options": {
                    "baseURL": "https://api.example.com/v1",
                    "apiKey": "test-key"
                },
                "models": {
                    "gpt-4o": {
                        "name": "GPT-4o"
                    }
                }
            }),
            website_url: None,
            category: Some("custom".to_string()),
            created_at: Some(1),
            sort_index: Some(0),
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    fn opencode_omo_provider(id: &str, category: &str) -> Provider {
        let mut settings = serde_json::Map::new();
        settings.insert(
            "agents".to_string(),
            json!({
                "writer": {
                    "model": "gpt-4o-mini"
                }
            }),
        );
        if category == "omo" {
            settings.insert(
                "categories".to_string(),
                json!({
                    "default": ["writer"]
                }),
            );
        }
        settings.insert(
            "otherFields".to_string(),
            json!({
                "theme": "dark"
            }),
        );

        Provider {
            id: id.to_string(),
            name: format!("Provider {id}"),
            settings_config: Value::Object(settings),
            website_url: None,
            category: Some(category.to_string()),
            created_at: Some(1),
            sort_index: Some(0),
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    fn omo_config_path(home: &Path, category: &str) -> PathBuf {
        home.join(".config").join("opencode").join(match category {
            "omo" => crate::services::omo::STANDARD.preferred_filename,
            "omo-slim" => crate::services::omo::SLIM.preferred_filename,
            other => panic!("unexpected OMO category in test: {other}"),
        })
    }

    #[test]
    #[serial]
    fn add_clears_usage_credentials_that_match_provider_config() {
        with_test_home(|state, _| {
            let provider = codex_provider_with_usage(
                "codex-a",
                "https://api.a.example/v1/",
                "sk-a",
                Some(" sk-a "),
                Some(" https://api.a.example/v1/ "),
                None,
            );

            ProviderService::add(state, AppType::Codex, provider, false).expect("add provider");

            let saved = state
                .db
                .get_provider_by_id("codex-a", AppType::Codex.as_str())
                .expect("query saved provider")
                .expect("saved provider should exist");
            let script = saved
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script.api_key, None);
            assert_eq!(script.base_url, None);
        });
    }

    #[test]
    #[serial]
    fn update_preserves_usage_credentials_that_only_match_previous_config() {
        with_test_home(|state, _| {
            let provider = codex_provider_with_usage(
                "codex-usage-old",
                "https://api.a.example/v1/",
                "sk-a",
                Some("sk-a"),
                Some("https://api.a.example/v1/"),
                None,
            );
            state
                .db
                .save_provider(AppType::Codex.as_str(), &provider)
                .expect("seed provider with explicit usage credentials");

            let mut updated = provider.clone();
            updated.settings_config = codex_settings("https://api.b.example/v1/", "sk-b");

            ProviderService::update(state, AppType::Codex, None, updated)
                .expect("update provider main credentials");

            let saved = state
                .db
                .get_provider_by_id("codex-usage-old", AppType::Codex.as_str())
                .expect("query updated provider")
                .expect("updated provider should exist");
            let script = saved
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script.api_key.as_deref(), Some("sk-a"));
            assert_eq!(
                script.base_url.as_deref(),
                Some("https://api.a.example/v1/")
            );
            assert_eq!(
                saved.resolve_usage_credentials(&AppType::Codex),
                ("https://api.b.example/v1".to_string(), "sk-b".to_string())
            );
        });
    }

    #[test]
    #[serial]
    fn copied_provider_uses_edited_credentials_after_add_clears_mirrored_usage_credentials() {
        with_test_home(|state, _| {
            let copied_provider = codex_provider_with_usage(
                "codex-copy",
                "https://api.a.example/v1/",
                "sk-a",
                Some("sk-a"),
                Some("https://api.a.example/v1/"),
                None,
            );

            ProviderService::add(state, AppType::Codex, copied_provider, false)
                .expect("add copied provider");

            let saved_after_add = state
                .db
                .get_provider_by_id("codex-copy", AppType::Codex.as_str())
                .expect("query copied provider")
                .expect("copied provider should exist");
            let script_after_add = saved_after_add
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");
            assert_eq!(script_after_add.api_key, None);
            assert_eq!(script_after_add.base_url, None);

            let mut edited_provider = saved_after_add.clone();
            edited_provider.settings_config = codex_settings("https://api.b.example/v1/", "sk-b");

            ProviderService::update(state, AppType::Codex, None, edited_provider)
                .expect("edit copied provider credentials");

            let saved_after_update = state
                .db
                .get_provider_by_id("codex-copy", AppType::Codex.as_str())
                .expect("query edited provider")
                .expect("edited provider should exist");
            let script_after_update = saved_after_update
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script_after_update.api_key, None);
            assert_eq!(script_after_update.base_url, None);
            assert_eq!(
                saved_after_update.resolve_usage_credentials(&AppType::Codex),
                ("https://api.b.example/v1".to_string(), "sk-b".to_string())
            );
        });
    }

    #[test]
    #[serial]
    fn update_clears_usage_credentials_that_match_current_config() {
        with_test_home(|state, _| {
            let provider = codex_provider_with_usage(
                "codex-current",
                "https://api.a.example/v1",
                "sk-a",
                Some("sk-usage"),
                Some("https://usage.example/api"),
                None,
            );
            state
                .db
                .save_provider(AppType::Codex.as_str(), &provider)
                .expect("seed provider with distinct usage credentials");

            let mut updated = provider.clone();
            updated.settings_config = codex_settings("https://api.b.example/v1/", "sk-b");
            updated.meta = Some(ProviderMeta {
                usage_script: Some(usage_script_with_credentials(
                    Some(" sk-b "),
                    Some(" https://api.b.example/v1/ "),
                    None,
                )),
                ..Default::default()
            });

            ProviderService::update(state, AppType::Codex, None, updated)
                .expect("update provider with redundant usage credentials");

            let saved = state
                .db
                .get_provider_by_id("codex-current", AppType::Codex.as_str())
                .expect("query updated provider")
                .expect("updated provider should exist");
            let script = saved
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script.api_key, None);
            assert_eq!(script.base_url, None);
        });
    }

    #[test]
    #[serial]
    fn add_preserves_distinct_usage_credentials() {
        with_test_home(|state, _| {
            let provider = codex_provider_with_usage(
                "codex-distinct",
                "https://api.main.example/v1",
                "sk-main",
                Some("sk-usage"),
                Some("https://usage.example/api"),
                None,
            );

            ProviderService::add(state, AppType::Codex, provider, false).expect("add provider");

            let saved = state
                .db
                .get_provider_by_id("codex-distinct", AppType::Codex.as_str())
                .expect("query saved provider")
                .expect("saved provider should exist");
            let script = saved
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script.api_key.as_deref(), Some("sk-usage"));
            assert_eq!(
                script.base_url.as_deref(),
                Some("https://usage.example/api")
            );
        });
    }

    #[test]
    #[serial]
    fn add_does_not_clear_token_plan_credentials() {
        with_test_home(|state, _| {
            let provider = codex_provider_with_usage(
                "codex-token-plan",
                "https://api.plan.example/v1",
                "sk-plan",
                Some("sk-plan"),
                Some("https://api.plan.example/v1"),
                Some("token_plan"),
            );

            ProviderService::add(state, AppType::Codex, provider, false).expect("add provider");

            let saved = state
                .db
                .get_provider_by_id("codex-token-plan", AppType::Codex.as_str())
                .expect("query saved provider")
                .expect("saved provider should exist");
            let script = saved
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script.api_key.as_deref(), Some("sk-plan"));
            assert_eq!(
                script.base_url.as_deref(),
                Some("https://api.plan.example/v1")
            );
            assert_eq!(script.access_key_id.as_deref(), Some("ak-test"));
            assert_eq!(script.secret_access_key.as_deref(), Some("sk-test"));
        });
    }

    #[test]
    fn validate_provider_settings_rejects_missing_auth() {
        let provider = Provider::with_id(
            "codex".into(),
            "Codex".into(),
            json!({ "config": "base_url = \"https://example.com\"" }),
            None,
        );
        let err = ProviderService::validate_provider_settings(&AppType::Codex, &provider)
            .expect_err("missing auth should be rejected");
        assert!(
            err.to_string().contains("auth"),
            "expected auth error, got {err:?}"
        );
    }

    #[test]
    #[serial]
    fn add_accepts_multiple_unbound_codex_official_cards() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            state
                .db
                .init_default_official_providers()
                .expect("seed official providers");
            let fixed_id = crate::database::CODEX_OFFICIAL_PROVIDER_ID;
            state
                .db
                .set_current_provider(AppType::Codex.as_str(), fixed_id)
                .expect("set database current");
            crate::settings::set_current_provider(&AppType::Codex, Some(fixed_id))
                .expect("set local current");

            for id in ["follow-login-a", "follow-login-b"] {
                let mut provider = Provider::with_id(
                    id.to_string(),
                    id.to_string(),
                    json!({ "auth": {}, "config": "" }),
                    None,
                );
                provider.category = Some("official".to_string());
                ProviderService::add(state, AppType::Codex, provider, false)
                    .expect("add unbound Official card");
            }

            let providers = state
                .db
                .get_all_providers(AppType::Codex.as_str())
                .expect("read providers");
            assert!(providers.contains_key("follow-login-a"));
            assert!(providers.contains_key("follow-login-b"));
        });
    }

    #[test]
    #[serial]
    fn update_keeps_official_provider_id_when_binding_and_unbinding() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed",
                        "managed-access-token",
                        Some("managed-id-token"),
                    )
                    .await
                    .expect("seed managed account");
            });

            let provider_id = crate::database::CODEX_OFFICIAL_PROVIDER_ID;
            let mut unbound = Provider::with_id(
                provider_id.to_string(),
                "OpenAI Official".to_string(),
                json!({ "auth": {}, "config": "" }),
                None,
            );
            unbound.category = Some("official".to_string());
            state
                .db
                .save_provider(AppType::Codex.as_str(), &unbound)
                .expect("save unbound card");
            state
                .db
                .set_current_provider(AppType::Codex.as_str(), provider_id)
                .expect("set database current");
            crate::settings::set_current_provider(&AppType::Codex, Some(provider_id))
                .expect("set local current");

            let mut bound = managed_codex_provider(provider_id, "acct-managed");
            bound.name = unbound.name.clone();
            ProviderService::update(state, AppType::Codex, Some(provider_id), bound)
                .expect("bind managed account");

            let saved_bound = state
                .db
                .get_provider_by_id(provider_id, AppType::Codex.as_str())
                .expect("query bound card")
                .expect("bound card should keep its ID");
            assert_eq!(
                ProviderService::managed_codex_oauth_account_id(&saved_bound).as_deref(),
                Some("acct-managed")
            );
            assert_eq!(
                state
                    .db
                    .get_current_provider(AppType::Codex.as_str())
                    .expect("read database current")
                    .as_deref(),
                Some(provider_id)
            );
            assert_eq!(
                crate::settings::get_current_provider(&AppType::Codex).as_deref(),
                Some(provider_id)
            );

            unbound.settings_config["config"] = Value::String(
                crate::codex_config::inject_codex_unified_session_bucket("")
                    .expect("inject live-only unified session route"),
            );
            ProviderService::update(state, AppType::Codex, Some(provider_id), unbound)
                .expect("unbind managed account");

            let saved_unbound = state
                .db
                .get_provider_by_id(provider_id, AppType::Codex.as_str())
                .expect("query unbound card")
                .expect("unbound card should keep its ID");
            assert!(ProviderService::managed_codex_oauth_account_id(&saved_unbound).is_none());
            assert_eq!(saved_unbound.settings_config["config"], json!(""));
            assert_eq!(
                state
                    .db
                    .get_current_provider(AppType::Codex.as_str())
                    .expect("read database current")
                    .as_deref(),
                Some(provider_id)
            );
            assert_eq!(
                crate::settings::get_current_provider(&AppType::Codex).as_deref(),
                Some(provider_id)
            );
        });
    }

    #[test]
    fn extract_gemini_common_config_strips_credentials_keeps_shareable() {
        // Gemini's shared snippet is deep-merged back into **other** Gemini
        // providers' env (live.rs::apply_common_config_to_settings), so no
        // credential may ever enter the snippet.
        // This used to hard-code skipping only GEMINI_API_KEY/GOOGLE_GEMINI_BASE_URL,
        // but GOOGLE_API_KEY is a first-class Gemini credential recognized by
        // provider.rs -> it would leak into other providers.
        let settings = json!({
            "env": {
                "GEMINI_API_KEY": "g-gem",
                "GOOGLE_API_KEY": "g-legacy-real-key",
                "GOOGLE_GEMINI_BASE_URL": "https://gemini.example",
                "GOOGLE_APPLICATION_CREDENTIALS": "/path/creds.json",
                "SOME_PROXY_AUTH_TOKEN": "tok-proxy",
                // shareable non-secret config must be preserved
                "GEMINI_TIMEOUT_MS": "30000"
            }
        });

        let snippet =
            ProviderService::extract_gemini_common_config(&settings).expect("extract should work");
        let value: Value = serde_json::from_str(&snippet).expect("snippet is valid JSON");

        for leaked in [
            "GEMINI_API_KEY",
            "GOOGLE_API_KEY",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "SOME_PROXY_AUTH_TOKEN",
        ] {
            assert!(
                value.get(leaked).is_none(),
                "credential {leaked} must not leak into the shared Gemini snippet"
            );
        }
        assert_eq!(
            value.get("GEMINI_TIMEOUT_MS").and_then(|v| v.as_str()),
            Some("30000"),
            "shareable non-secret config must be preserved"
        );
    }

    /// Build an "already contaminated" scene: the snippet carries account
    /// A's credential plus one legitimately shareable key.
    #[test]
    fn sensitive_key_matcher_covers_common_credential_namings() {
        for key in [
            // Bare `_KEY`: the most common naming, but used to slip through
            // when only `_API_KEY`-style subtypes were enumerated
            "OPENAI_KEY",
            "GROQ_KEY",
            "XAI_KEY",
            // Compound naming without a separator
            "VOLC_ACCESSKEY",
            "ALIYUN_SECRETKEY",
            "SOME_APITOKEN",
            // personal access token: contains neither TOKEN nor KEY
            "GITHUB_PAT",
            "gitlab_pat",
            // Password-style abbreviations
            "MYSQL_PWD",
            "DB_PASS",
            "GPG_PASSPHRASE",
            "AWS_CREDS",
        ] {
            assert!(
                ProviderService::is_sensitive_config_key(key),
                "{key} must be treated as a credential"
            );
        }

        // The suffix must be preceded by an underscore, so ordinary config
        // isn't swept up too
        for key in [
            "PATH",
            "OLDPWD",
            "GEMINI_COMPAT",
            "SSL_BYPASS",
            "GEMINI_TIMEOUT_MS",
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
        ] {
            assert!(
                !ProviderService::is_sensitive_config_key(key),
                "{key} is ordinary shareable config and must not be stripped"
            );
        }
    }

    fn seed_leaked_gemini_state(db: &Arc<Database>) {
        db.set_config_snippet(
            "gemini",
            Some(
                json!({
                    "GOOGLE_API_KEY": "key-A-leaked",
                    "SOME_PROXY_AUTH_TOKEN": "tok-A-leaked",
                    "GEMINI_TIMEOUT_MS": "30000"
                })
                .to_string(),
            ),
        )
        .expect("seed snippet");

        // Victim B: the leaked credential has already been merged into its env
        let victim = Provider::with_id(
            "b".into(),
            "Relay B".into(),
            json!({ "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://relay-b.example",
                "GOOGLE_API_KEY": "key-A-leaked",
                "GEMINI_TIMEOUT_MS": "30000"
            }}),
            None,
        );
        db.save_provider("gemini", &victim).expect("save victim");

        // Provider C: wrote the same key name itself but with a different
        // value, must not be accidentally deleted
        let unrelated = Provider::with_id(
            "c".into(),
            "Own Key C".into(),
            json!({ "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://c.example",
                "GOOGLE_API_KEY": "key-C-owned"
            }}),
            None,
        );
        db.save_provider("gemini", &unrelated).expect("save c");
    }

    /// Saving the active provider while takeover has never been enabled must
    /// rewrite the real live file immediately.
    #[tokio::test]
    #[serial]
    async fn update_current_claude_provider_writes_live_when_proxy_never_enabled() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        let original = Provider::with_id(
            "p1".into(),
            "Claude A".into(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token-a",
                    "ANTHROPIC_BASE_URL": "https://api.old.example"
                }
            }),
            None,
        );
        db.save_provider("claude", &original)
            .expect("save provider");
        db.set_current_provider("claude", "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::Claude, Some("p1"))
            .expect("set local current provider");
        write_live_with_common_config_for_state(&state, &AppType::Claude, &original)
            .expect("seed live file");

        let mut updated = original.clone();
        updated.settings_config["env"]["ANTHROPIC_BASE_URL"] =
            Value::String("https://api.new.example".into());
        ProviderService::update(&state, AppType::Claude, None, updated)
            .expect("update current provider");

        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        assert_eq!(
            live["env"]["ANTHROPIC_BASE_URL"].as_str(),
            Some("https://api.new.example")
        );
    }

    /// A stale backup row must be refreshed but must not divert the live write.
    #[tokio::test]
    #[serial]
    async fn update_current_claude_provider_writes_live_when_backup_row_is_stale() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        let original = Provider::with_id(
            "p1".into(),
            "Claude A".into(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token-a",
                    "ANTHROPIC_BASE_URL": "https://api.old.example"
                }
            }),
            None,
        );
        db.save_provider("claude", &original)
            .expect("save provider");
        db.set_current_provider("claude", "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::Claude, Some("p1"))
            .expect("set local current provider");
        write_live_with_common_config_for_state(&state, &AppType::Claude, &original)
            .expect("seed live file");
        db.save_live_backup(
            "claude",
            &serde_json::to_string(&original.settings_config).expect("serialize backup"),
        )
        .await
        .expect("seed stale backup");
        assert!(!state.proxy_service.is_running().await);

        let mut updated = original.clone();
        updated.settings_config["env"]["ANTHROPIC_BASE_URL"] =
            Value::String("https://api.new.example".into());
        ProviderService::update(&state, AppType::Claude, None, updated)
            .expect("update current provider");

        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        assert_eq!(
            live["env"]["ANTHROPIC_BASE_URL"].as_str(),
            Some("https://api.new.example")
        );
        let backup = db
            .get_live_backup("claude")
            .await
            .expect("read backup")
            .expect("backup remains");
        assert!(backup.original_config.contains("https://api.new.example"));
    }

    /// An enabled flag left behind by an interrupted teardown is not enough to
    /// suppress a live write when neither placeholder nor backup evidence exists.
    #[tokio::test]
    #[serial]
    async fn update_current_claude_provider_ignores_enabled_flag_without_evidence() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        let original = Provider::with_id(
            "p1".into(),
            "Claude A".into(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token-a",
                    "ANTHROPIC_BASE_URL": "https://api.old.example"
                }
            }),
            None,
        );
        db.save_provider("claude", &original)
            .expect("save provider");
        db.set_current_provider("claude", "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::Claude, Some("p1"))
            .expect("set local current provider");
        write_live_with_common_config_for_state(&state, &AppType::Claude, &original)
            .expect("seed live file");
        let mut config = db
            .get_proxy_config_for_app("claude")
            .await
            .expect("read proxy config");
        config.enabled = true;
        db.update_proxy_config_for_app(config)
            .await
            .expect("leave enabled flag set");
        assert!(!state.proxy_service.is_running().await);

        let mut updated = original.clone();
        updated.settings_config["env"]["ANTHROPIC_BASE_URL"] =
            Value::String("https://api.new.example".into());
        ProviderService::update(&state, AppType::Claude, None, updated)
            .expect("update current provider");

        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        assert_eq!(
            live["env"]["ANTHROPIC_BASE_URL"].as_str(),
            Some("https://api.new.example")
        );
    }

    #[tokio::test]
    #[serial]
    async fn scrub_gemini_removes_leaked_credentials_from_snippet_and_providers() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");
        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        seed_leaked_gemini_state(&db);

        ProviderService::scrub_leaked_gemini_common_config(&state)
            .await
            .expect("scrub must succeed");

        // Snippet: credentials scrubbed, shareable config preserved
        let snippet = db
            .get_config_snippet("gemini")
            .expect("read snippet")
            .expect("snippet must still exist");
        let snippet: Value = serde_json::from_str(&snippet).expect("valid json");
        assert!(snippet.get("GOOGLE_API_KEY").is_none());
        assert!(snippet.get("SOME_PROXY_AUTH_TOKEN").is_none());
        assert_eq!(
            snippet.get("GEMINI_TIMEOUT_MS").and_then(Value::as_str),
            Some("30000"),
            "shareable config must survive the scrub"
        );

        // Victim B: the copy that spread to it gets cleaned up
        let providers = db.get_all_providers("gemini").expect("providers");
        let victim_env = &providers["b"].settings_config["env"];
        assert!(
            victim_env.get("GOOGLE_API_KEY").is_none(),
            "leaked key must be removed from the victim provider"
        );
        assert_eq!(
            victim_env.get("GEMINI_TIMEOUT_MS").and_then(Value::as_str),
            Some("30000"),
            "non-credential config must not be touched"
        );
    }

    #[tokio::test]
    #[serial]
    async fn scrub_gemini_keeps_a_providers_own_differently_valued_key() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");
        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        seed_leaked_gemini_state(&db);

        ProviderService::scrub_leaked_gemini_common_config(&state)
            .await
            .expect("scrub must succeed");

        // This case is the easiest to get wrong as "delete anything matching
        // the key name": C's own key value differs from the snippet's, it's
        // its own credential
        let providers = db.get_all_providers("gemini").expect("providers");
        assert_eq!(
            providers["c"].settings_config["env"]
                .get("GOOGLE_API_KEY")
                .and_then(Value::as_str),
            Some("key-C-owned"),
            "a provider's own key must not be deleted by name matching"
        );
    }

    #[tokio::test]
    #[serial]
    async fn scrub_gemini_audit_records_key_names_but_never_values() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");
        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        seed_leaked_gemini_state(&db);

        ProviderService::scrub_leaked_gemini_common_config(&state)
            .await
            .expect("scrub must succeed");

        let audit_text = db
            .get_setting("gemini_common_config_scrub_audit_v1")
            .expect("read audit")
            .expect("an audit record must exist so the deletion is not silent");

        // Values must never enter this record: `settings` gets synced up via
        // WebDAV/S3, so keeping a value would turn a single cleanup into a
        // plaintext copy that spreads across devices, has no UI entry point,
        // and never expires.
        assert!(
            !audit_text.contains("key-A-leaked") && !audit_text.contains("tok-A-leaked"),
            "the audit record must never carry credential values: {audit_text}"
        );

        // But it must clearly say what was deleted and from where, otherwise
        // the user has no way to know short of digging through logs
        let audit: Value = serde_json::from_str(&audit_text).expect("audit is JSON");
        let removed: Vec<&str> = audit["removedFromSnippet"]
            .as_array()
            .expect("removedFromSnippet array")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(
            removed.contains(&"GOOGLE_API_KEY") && removed.contains(&"SOME_PROXY_AUTH_TOKEN"),
            "every key removed from the snippet must be named: {audit}"
        );
        let victim = audit["providers"]
            .as_array()
            .expect("providers array")
            .iter()
            .find(|entry| entry["id"] == json!("b"))
            .expect("every provider whose config gets rewritten must be recorded");
        assert_eq!(
            victim["removedKeys"],
            json!(["GOOGLE_API_KEY"]),
            "the record must name what was taken from each provider: {audit}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn scrub_gemini_never_overwrites_an_existing_audit_record() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");
        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        seed_leaked_gemini_state(&db);

        // Simulates a previous run that aborted halfway through: the
        // completion flag was never set, so the next startup will re-run it,
        // but by then the "original state" it reads is already incomplete.
        // Overwriting unconditionally would clobber the first run's complete
        // record with this incomplete one.
        db.set_setting(
            "gemini_common_config_scrub_audit_v1",
            "{\"from\":\"an earlier, complete run\"}",
        )
        .expect("seed an existing audit record");

        ProviderService::scrub_leaked_gemini_common_config(&state)
            .await
            .expect("scrub must succeed");

        assert_eq!(
            db.get_setting("gemini_common_config_scrub_audit_v1")
                .expect("read audit")
                .as_deref(),
            Some("{\"from\":\"an earlier, complete run\"}"),
            "an audit record from an earlier run must survive a retry"
        );
    }

    #[tokio::test]
    #[serial]
    async fn scrub_gemini_cleans_the_live_env_without_a_current_provider() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");
        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        seed_leaked_gemini_state(&db);

        // No current provider — this is exactly the branch where
        // sync_current_provider_for_app returns Ok immediately without
        // writing any file. If live can't be cleaned here and the snippet has
        // already been cleared, the next switch's backfill would permanently
        // write the leftover into the victim provider's config.
        crate::gemini_config::write_gemini_env_atomic(&HashMap::from([
            ("GOOGLE_API_KEY".to_string(), "key-A-leaked".to_string()),
            ("GEMINI_TIMEOUT_MS".to_string(), "30000".to_string()),
            // A hand-added edit that exists only in live: targeted removal
            // must preserve it, a full re-projection would wipe it out
            (
                "HTTPS_PROXY".to_string(),
                "http://127.0.0.1:7890".to_string(),
            ),
        ]))
        .expect("seed live env");

        ProviderService::scrub_leaked_gemini_common_config(&state)
            .await
            .expect("scrub must succeed");

        let live = crate::gemini_config::read_gemini_env().expect("read live env");
        assert!(
            !live.contains_key("GOOGLE_API_KEY"),
            "the leaked credential must be gone from ~/.gemini/.env: {live:?}"
        );
        assert_eq!(
            live.get("HTTPS_PROXY").map(String::as_str),
            Some("http://127.0.0.1:7890"),
            "a hand-added live-only var must survive targeted removal: {live:?}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn scrub_gemini_live_cleanup_preserves_the_rest_of_the_env_file() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");
        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        seed_leaked_gemini_state(&db);

        // This is a startup-time cleanup the user never actively triggered,
        // so it shouldn't incidentally rewrite content unrelated to the leak.
        // A read -> HashMap -> write round trip would drop all comments,
        // blank lines, and unrecognized lines, and reorder everything by key
        // name.
        let original = "\
# my own notes
GOOGLE_API_KEY=key-C-owned

GOOGLE_API_KEY=key-A-leaked
this line is not KEY=VALUE at all
GEMINI_TIMEOUT_MS=30000
";
        crate::gemini_config::write_gemini_env_text_atomic(original).expect("seed live env");

        ProviderService::scrub_leaked_gemini_common_config(&state)
            .await
            .expect("scrub must succeed");

        let raw = std::fs::read_to_string(crate::gemini_config::get_gemini_env_path())
            .expect("read live env");
        assert!(
            !raw.contains("key-A-leaked"),
            "the leaked line must be gone: {raw:?}"
        );
        assert!(
            raw.contains("# my own notes"),
            "comments must survive a targeted removal: {raw:?}"
        );
        assert!(
            raw.contains("this line is not KEY=VALUE at all"),
            "unparseable lines must survive a targeted removal: {raw:?}"
        );
        // The line masked by the leaked value takes effect again -- exactly
        // the desired outcome, since what was masking it was precisely the
        // leaked value
        assert_eq!(
            crate::gemini_config::read_gemini_env()
                .expect("read live env")
                .get("GOOGLE_API_KEY")
                .map(String::as_str),
            Some("key-C-owned"),
            "only the matching line may be dropped: {raw:?}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn scrub_gemini_aborts_before_clearing_the_snippet_when_the_live_backup_fails() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");
        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        seed_leaked_gemini_state(&db);

        // This snapshot gets written back to live verbatim when the proxy is
        // turned off. If it can't be cleaned yet we go ahead and clear the
        // snippet and set the completion flag anyway, the credential would
        // come back to life the moment the proxy stops, and the one-shot flag
        // guarantees it won't be cleaned a second time.
        db.save_live_backup("gemini", "}not json{")
            .await
            .expect("seed backup");

        let result = ProviderService::scrub_leaked_gemini_common_config(&state).await;
        assert!(
            result.is_err(),
            "a backup that cannot be cleaned must abort the scrub"
        );

        // The snippet is the sole source of knowledge for "which keys to
        // strip"; after an abort it must be left exactly as it was,
        // otherwise the next retry would short-circuit on an empty poison
        // set and set the flag anyway
        let snippet = db
            .get_config_snippet("gemini")
            .expect("read snippet")
            .expect("snippet must still exist");
        assert!(
            snippet.contains("key-A-leaked"),
            "the snippet must be left intact so the next boot can retry: {snippet}"
        );
        assert!(
            db.get_setting("gemini_common_config_credentials_scrubbed_v1")
                .expect("read flag")
                .is_none(),
            "the one-shot flag must not be set when the scrub aborted"
        );
    }

    #[tokio::test]
    #[serial]
    async fn scrub_gemini_leaves_no_residue_for_backfill_to_persist() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");
        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        seed_leaked_gemini_state(&db);

        ProviderService::scrub_leaked_gemini_common_config(&state)
            .await
            .expect("scrub must succeed");

        // Regression test for the ordering trap: if only the snippet were
        // cleaned, remove_common_config_from_settings would no longer
        // recognize this key when switching away from a provider, and the
        // leftover in live would get permanently written into the provider's
        // config by backfill.
        // Cleanup must be atomic -- once it's done, the value must not remain
        // anywhere.
        let snippet = db
            .get_config_snippet("gemini")
            .expect("read snippet")
            .unwrap_or_default();
        assert!(!snippet.contains("key-A-leaked"));

        for (id, provider) in db.get_all_providers("gemini").expect("providers") {
            assert!(
                !provider
                    .settings_config
                    .to_string()
                    .contains("key-A-leaked"),
                "provider '{id}' still carries the leaked value"
            );
        }
    }

    #[tokio::test]
    #[serial]
    async fn scrub_gemini_is_idempotent_and_skips_on_second_run() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");
        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        seed_leaked_gemini_state(&db);

        ProviderService::scrub_leaked_gemini_common_config(&state)
            .await
            .expect("first run");

        // The second run must be a no-op: a credential the user re-filled in
        // after cleanup must not get wiped out again
        db.set_config_snippet(
            "gemini",
            Some(json!({"GOOGLE_API_KEY": "restored"}).to_string()),
        )
        .expect("user re-adds a value");

        ProviderService::scrub_leaked_gemini_common_config(&state)
            .await
            .expect("second run");

        let snippet = db
            .get_config_snippet("gemini")
            .expect("read snippet")
            .expect("snippet exists");
        assert!(
            snippet.contains("restored"),
            "the one-shot flag must prevent a second scrub: {snippet}"
        );
    }

    #[test]
    fn extract_claude_common_config_strips_all_credentials_keeps_shareable() {
        // env mixes multiple credential types (Anthropic/OpenRouter/Google/OpenAI/Gemini
        // + AWS/Vertex) with shareable config; the top level mixes non-standard
        // apiKey/api_key credentials with ordinary settings.
        let settings = json!({
            "env": {
                "ANTHROPIC_API_KEY": "sk-ant",
                "ANTHROPIC_AUTH_TOKEN": "tok-ant",
                "OPENROUTER_API_KEY": "sk-or",
                "GOOGLE_API_KEY": "g-key",
                "OPENAI_API_KEY": "sk-oai",
                "GEMINI_API_KEY": "g-gem",
                "AWS_ACCESS_KEY_ID": "AKIA",
                "AWS_SECRET_ACCESS_KEY": "secret",
                "AWS_SESSION_TOKEN": "sess",
                "GOOGLE_APPLICATION_CREDENTIALS": "/path/creds.json",
                "AWS_BEARER_TOKEN_BEDROCK": "bedrock-tok",
                "ANTHROPIC_BASE_URL": "https://example.com",
                "ANTHROPIC_MODEL": "claude-x",
                "CLAUDE_CODE_SUBAGENT_MODEL": "gpt-5.4-mini",
                "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "400000",
                "CLAUDE_CODE_AUTO_COMPACT_WINDOW": "400000",
                // Shareable, non-secret config (plural _TOKENS must not be
                // mistakenly stripped)
                "ENABLE_TOOL_SEARCH": "true",
                "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "8192"
            },
            "apiKey": "sk-top",
            "api_key": "sk-top2",
            "theme": "dark",
            "includeCoAuthoredBy": false
        });

        let snippet = ProviderService::extract_claude_common_config(&settings)
            .expect("extract should succeed");
        let value: Value = serde_json::from_str(&snippet).expect("snippet is valid JSON");

        // No credential of any kind may appear in the shared snippet
        let env = value.get("env");
        for leaked in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "OPENROUTER_API_KEY",
            "GOOGLE_API_KEY",
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "AWS_BEARER_TOKEN_BEDROCK",
        ] {
            assert!(
                env.and_then(|e| e.get(leaked)).is_none(),
                "credential {leaked} must not leak into common config"
            );
        }
        assert!(
            value.get("apiKey").is_none() && value.get("api_key").is_none(),
            "top-level credentials must be stripped"
        );

        // Endpoints/models (provider-specific, non-secret) must also be stripped
        assert!(env.and_then(|e| e.get("ANTHROPIC_BASE_URL")).is_none());
        assert!(env.and_then(|e| e.get("ANTHROPIC_MODEL")).is_none());
        assert!(env
            .and_then(|e| e.get("CLAUDE_CODE_SUBAGENT_MODEL"))
            .is_none());
        assert!(env
            .and_then(|e| e.get("CLAUDE_CODE_MAX_CONTEXT_TOKENS"))
            .is_none());
        assert!(env
            .and_then(|e| e.get("CLAUDE_CODE_AUTO_COMPACT_WINDOW"))
            .is_none());

        // Shareable, non-secret config must be preserved (plural _TOKENS
        // must not be mistakenly stripped)
        assert_eq!(
            env.and_then(|e| e.get("ENABLE_TOOL_SEARCH"))
                .and_then(|v| v.as_str()),
            Some("true")
        );
        assert_eq!(
            env.and_then(|e| e.get("CLAUDE_CODE_MAX_OUTPUT_TOKENS"))
                .and_then(|v| v.as_str()),
            Some("8192")
        );
        assert_eq!(value.get("theme").and_then(|v| v.as_str()), Some("dark"));
        assert_eq!(value.get("includeCoAuthoredBy"), Some(&json!(false)));
    }

    /// Regression for issue #4272: Fable tier env keys must not enter the shared
    /// Claude common-config snippet (same class as haiku/sonnet/opus model pins).
    #[test]
    fn extract_claude_common_config_strips_fable_model_env_keys() {
        let settings = json!({
            "env": {
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "haiku-mapped",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME": "Haiku Mapped",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-mapped[1M]",
                "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "Sonnet Mapped",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-mapped[1M]",
                "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME": "Opus Mapped",
                "ANTHROPIC_DEFAULT_FABLE_MODEL": "deepseek-v4-flash[1M]",
                "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME": "deepseek-v4-flash",
                "ANTHROPIC_MODEL": "default-mapped",
                "ENABLE_TOOL_SEARCH": "true"
            },
            "theme": "dark"
        });

        let snippet = ProviderService::extract_claude_common_config(&settings)
            .expect("extract should succeed");
        let value: Value = serde_json::from_str(&snippet).expect("snippet is valid JSON");
        let env = value.get("env");

        for stripped in [
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
            "ANTHROPIC_MODEL",
        ] {
            assert!(
                env.and_then(|e| e.get(stripped)).is_none(),
                "provider-specific model key {stripped} must not enter common config"
            );
        }

        assert_eq!(
            env.and_then(|e| e.get("ENABLE_TOOL_SEARCH"))
                .and_then(|v| v.as_str()),
            Some("true")
        );
        assert_eq!(value.get("theme").and_then(|v| v.as_str()), Some("dark"));
    }

    #[test]
    fn validate_provider_settings_rejects_negative_cost_multiplier() {
        let mut provider = Provider::with_id(
            "claude".into(),
            "Claude".into(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token",
                    "ANTHROPIC_BASE_URL": "https://claude.example"
                }
            }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            cost_multiplier: Some("-1".to_string()),
            ..ProviderMeta::default()
        });

        let err = ProviderService::validate_provider_settings(&AppType::Claude, &provider)
            .expect_err("negative multiplier should be rejected");
        assert!(matches!(
            err,
            AppError::Localized {
                key: "error.invalidMultiplier",
                ..
            }
        ));
    }

    #[test]
    fn extract_codex_common_config_strips_provider_fields_and_injected_artifacts() {
        // A top-level experimental_bearer_token simulates the fallback injection when no route is active;
        // web_search = "disabled" is the sentinel cc-switch injects for blocklisted gateways;
        // a top-level wire_api simulates the fallback form used when there is no model_provider;
        // [mcp.servers] is the legacy wrong format that sync_all_enabled cannot clear.
        let config_toml = r#"model_provider = "azure"
model = "gpt-4"
wire_api = "chat"
disable_response_storage = true
experimental_bearer_token = "sk-live-secret"
model_catalog_json = "cc-switch-model-catalog.json"
web_search = "disabled"

[model_providers.azure]
name = "Azure OpenAI"
base_url = "https://azure.example/v1"
wire_api = "responses"

[mcp_servers.my_server]
base_url = "http://localhost:8080"

[mcp.servers.legacy_server]
command = "legacy-cmd"
"#;

        let settings = json!({ "config": config_toml });
        let extracted = ProviderService::extract_codex_common_config(&settings)
            .expect("extract_codex_common_config should succeed");

        assert!(
            !extracted
                .lines()
                .any(|line| line.trim_start().starts_with("model_provider")),
            "should remove top-level model_provider"
        );
        assert!(
            !extracted
                .lines()
                .any(|line| line.trim_start().starts_with("model =")),
            "should remove top-level model"
        );
        assert!(
            !extracted.contains("[model_providers"),
            "should remove entire model_providers table"
        );
        // MCP is owned by the DB mcp_servers table and must not enter the shared snippet (including the legacy [mcp.servers] form)
        assert!(
            !extracted.contains("mcp_servers") && !extracted.contains("http://localhost:8080"),
            "should strip mcp_servers from the shared snippet, got: {extracted}"
        );
        assert!(
            !extracted.contains("[mcp") && !extracted.contains("legacy-cmd"),
            "should strip the legacy [mcp.servers] form from the shared snippet, got: {extracted}"
        );
        // A top-level wire_api is provider routing semantics (the whole model_providers table is
        // already stripped, so any remaining wire_api means a leak)
        assert!(
            !extracted.contains("wire_api"),
            "should strip top-level wire_api from the shared snippet, got: {extracted}"
        );
        // Injected artifacts must not enter the shared snippet (a leaked bearer token is a secret-level issue)
        assert!(
            !extracted.contains("experimental_bearer_token")
                && !extracted.contains("sk-live-secret"),
            "should strip top-level fallback bearer token, got: {extracted}"
        );
        assert!(
            !extracted.contains("model_catalog_json"),
            "should strip catalog projection pointer, got: {extracted}"
        );
        assert!(
            !extracted.contains("web_search"),
            "should strip the cc-switch web_search disabled sentinel, got: {extracted}"
        );
        // Genuinely shareable keys are kept
        assert!(
            extracted.contains("disable_response_storage = true"),
            "shareable keys must survive extraction, got: {extracted}"
        );
    }

    #[test]
    fn extract_codex_common_config_keeps_user_set_web_search() {
        let config_toml = "web_search = \"enabled\"\ndisable_response_storage = true\n";
        let settings = json!({ "config": config_toml });
        let extracted = ProviderService::extract_codex_common_config(&settings)
            .expect("extract should succeed");
        assert!(
            extracted.contains("web_search = \"enabled\""),
            "a user-set web_search value is a shareable preference, got: {extracted}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn update_current_claude_provider_syncs_live_when_proxy_takeover_detected_without_backup()
    {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());

        let original = Provider::with_id(
            "p1".into(),
            "Claude A".into(),
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "token-a",
                    "ANTHROPIC_BASE_URL": "https://api.a.example",
                    "ANTHROPIC_MODEL": "model-a"
                },
                "permissions": { "allow": ["Bash"] }
            }),
            None,
        );
        db.save_provider("claude", &original)
            .expect("save provider");
        db.set_current_provider("claude", "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::Claude, Some("p1"))
            .expect("set local current provider");

        db.update_proxy_config(ProxyConfig {
            live_takeover_active: true,
            listen_port: 0,
            ..Default::default()
        })
        .await
        .expect("update proxy config");
        {
            let mut config = db
                .get_proxy_config_for_app("claude")
                .await
                .expect("get app proxy config");
            config.enabled = true;
            db.update_proxy_config_for_app(config)
                .await
                .expect("update app proxy config");
        }

        write_json_file(
            &get_claude_settings_path(),
            &json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721",
                    "ANTHROPIC_API_KEY": "PROXY_MANAGED",
                    "ANTHROPIC_MODEL": "stale-model"
                },
                "permissions": { "allow": ["Bash"] }
            }),
        )
        .expect("seed taken-over live file");

        let proxy_info = state
            .proxy_service
            .start()
            .await
            .expect("start proxy service");

        let updated = Provider::with_id(
            "p1".into(),
            "Claude A".into(),
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "token-updated",
                    "ANTHROPIC_BASE_URL": "https://api.updated.example",
                    "ANTHROPIC_MODEL": "model-updated"
                },
                "permissions": { "allow": ["Read"] }
            }),
            None,
        );

        ProviderService::update(&state, AppType::Claude, None, updated.clone())
            .expect("update current provider");

        let backup = db
            .get_live_backup("claude")
            .await
            .expect("get live backup")
            .expect("backup exists");
        let stored_provider = db
            .get_provider_by_id("p1", "claude")
            .expect("get stored provider")
            .expect("stored provider exists");
        let expected_backup =
            serde_json::to_string(&stored_provider.settings_config).expect("serialize");
        assert_eq!(backup.original_config, expected_backup);

        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        assert_eq!(
            live.get("permissions"),
            updated.settings_config.get("permissions"),
            "provider edits should propagate into Claude live config during takeover"
        );
        assert_eq!(
            live.get("env")
                .and_then(|env| env.get("ANTHROPIC_API_KEY"))
                .and_then(|v| v.as_str()),
            Some("PROXY_MANAGED"),
            "takeover placeholder should stay intact"
        );
        assert_eq!(
            live.get("env")
                .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
                .and_then(|v| v.as_str()),
            Some(format!("http://127.0.0.1:{}", proxy_info.port).as_str()),
            "proxy base URL should stay intact"
        );
        assert!(
            live.get("env")
                .and_then(|env| env.get("ANTHROPIC_MODEL"))
                .is_none(),
            "model override should be removed in takeover live config"
        );
    }

    #[tokio::test]
    #[serial]
    async fn update_current_codex_provider_refreshes_and_clears_catalog_during_takeover() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());

        let mut original = Provider::with_id(
            "p1".into(),
            "Codex A".into(),
            json!({
                "auth": { "OPENAI_API_KEY": "token-a" },
                "config": r#"model_provider = "custom"
model = "old-model"

[model_providers.custom]
name = "Codex A"
base_url = "https://api.a.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#,
                "modelCatalog": {
                    "models": [{ "model": "old-model" }]
                }
            }),
            None,
        );
        original.meta = Some(ProviderMeta {
            api_format: Some("openai_responses".into()),
            ..Default::default()
        });
        db.save_provider("codex", &original).expect("save provider");
        db.set_current_provider("codex", "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::Codex, Some("p1"))
            .expect("set local current provider");

        db.update_proxy_config(ProxyConfig {
            live_takeover_active: true,
            listen_port: 0,
            ..Default::default()
        })
        .await
        .expect("update proxy config");
        {
            let mut config = db
                .get_proxy_config_for_app("codex")
                .await
                .expect("get app proxy config");
            config.enabled = true;
            db.update_proxy_config_for_app(config)
                .await
                .expect("enable Codex proxy config");
        }
        db.save_live_backup(
            "codex",
            &serde_json::to_string(&original.settings_config).expect("serialize backup"),
        )
        .await
        .expect("seed live backup");

        state
            .proxy_service
            .start()
            .await
            .expect("start proxy service");
        state
            .proxy_service
            .sync_codex_live_from_provider_while_proxy_active(&original)
            .await
            .expect("seed taken-over Codex live config");
        assert!(
            state
                .proxy_service
                .detect_takeover_in_live_config_for_app(&AppType::Codex),
            "seeded Codex live config should be recognized as takeover-owned"
        );

        let mut updated = original.clone();
        updated.settings_config["config"] = json!(
            r#"model_provider = "custom"
model = "gpt-5.4"

[model_providers.custom]
name = "Codex A"
base_url = "https://api.updated.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#
        );
        updated.settings_config["modelCatalog"] = json!({
            "models": [{ "model": "gpt-5.4", "displayName": "GPT 5.4" }]
        });

        ProviderService::update(&state, AppType::Codex, None, updated.clone())
            .expect("update current Codex provider mapping");

        let catalog_path = crate::codex_config::get_codex_model_catalog_path();
        let catalog: Value = read_json_file(&catalog_path).expect("read generated catalog");
        assert_eq!(catalog["models"][0]["slug"], "gpt-5.4");
        assert_eq!(
            catalog["models"][0]["input_modalities"],
            json!(["text", "image"]),
            "unknown/GPT models must fail open to image input"
        );
        let live_config = fs::read_to_string(crate::codex_config::get_codex_config_path())
            .expect("read Codex config.toml");
        assert!(live_config.contains("model_catalog_json"));

        updated.settings_config["modelCatalog"] = json!({ "models": [] });
        ProviderService::update(&state, AppType::Codex, None, updated)
            .expect("remove current Codex provider mapping");

        let live_config = fs::read_to_string(crate::codex_config::get_codex_config_path())
            .expect("read Codex config.toml after mapping removal");
        assert!(
            !live_config.contains("model_catalog_json"),
            "removing mappings during takeover must clear the stale catalog pointer"
        );

        state
            .proxy_service
            .stop()
            .await
            .expect("stop proxy service");
    }

    #[cfg(any(target_os = "macos", windows))]
    #[tokio::test]
    #[serial]
    async fn update_current_claude_desktop_provider_syncs_profile_when_proxy_takeover_is_active() {
        let home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());

        let mut original = Provider::with_id(
            "p1".into(),
            "Desktop A".into(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token-a",
                    "ANTHROPIC_BASE_URL": "https://opencode.ai/zen/go"
                }
            }),
            None,
        );
        original.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".into()),
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-sonnet-4-6".into(),
                ClaudeDesktopModelRoute {
                    model: "deepseek-v4-flash".into(),
                    label_override: Some("DeepSeek V4 Flash".into()),
                    supports_1m: None,
                },
            )]),
            ..Default::default()
        });
        db.save_provider("claude-desktop", &original)
            .expect("save provider");
        db.set_current_provider("claude-desktop", "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::ClaudeDesktop, Some("p1"))
            .expect("set local current provider");

        // Claude Desktop keeps backup state from takeover startup; this sentinel only
        // marks takeover as active so provider updates rewrite the 3P profile.
        db.save_live_backup("claude-desktop", "{}")
            .await
            .expect("seed live backup");
        {
            let mut config = db
                .get_proxy_config_for_app("claude-desktop")
                .await
                .expect("get app proxy config");
            config.enabled = true;
            db.update_proxy_config_for_app(config)
                .await
                .expect("update app proxy config");
        }

        state
            .proxy_service
            .start()
            .await
            .expect("start proxy service");

        let mut updated = Provider::with_id(
            "p1".into(),
            "Desktop A".into(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token-updated",
                    "ANTHROPIC_BASE_URL": "https://opencode.ai/zen/go"
                }
            }),
            None,
        );
        updated.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".into()),
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-sonnet-4-6".into(),
                ClaudeDesktopModelRoute {
                    model: "deepseek-v4-flash".into(),
                    label_override: Some("DeepSeek V4 Flash Updated".into()),
                    supports_1m: Some(true),
                },
            )]),
            ..Default::default()
        });

        ProviderService::update(&state, AppType::ClaudeDesktop, None, updated.clone())
            .expect("update current provider");

        let backup = db
            .get_live_backup("claude-desktop")
            .await
            .expect("get live backup")
            .expect("backup exists");
        assert_eq!(
            backup.original_config, "{}",
            "Claude Desktop provider edits should not rewrite takeover backup"
        );

        let profile_path = claude_desktop_profile_path(home.dir.path());
        let profile: Value = read_json_file(&profile_path).expect("read desktop profile");
        assert_eq!(
            profile["inferenceGatewayBaseUrl"],
            json!("http://127.0.0.1:15721/claude-desktop"),
            "desktop profile should stay pointed at the local gateway during takeover"
        );
        assert_eq!(profile["inferenceGatewayAuthScheme"], json!("bearer"));
        assert_eq!(
            profile["inferenceModels"],
            json!([{ "name": "claude-sonnet-4-6", "labelOverride": "DeepSeek V4 Flash Updated", "supports1m": true }]),
            "provider edits should propagate into the Claude Desktop 3P profile during takeover"
        );
    }

    #[test]
    #[serial]
    fn rename_rejects_missing_original_provider() {
        with_test_home(|state, _| {
            let original = openclaw_provider("deepseek");
            ProviderService::add(state, AppType::OpenClaw, original.clone(), false)
                .expect("seed db-only provider");

            let mut renamed = original.clone();
            renamed.id = "deepseek-copy".to_string();

            let err = ProviderService::update(
                state,
                AppType::OpenClaw,
                Some("missing-provider"),
                renamed,
            )
            .expect_err("stale originalId should be rejected");

            assert!(
                err.to_string().contains("Original provider"),
                "expected missing original provider error, got {err:?}"
            );
            assert!(
                state
                    .db
                    .get_provider_by_id("deepseek-copy", AppType::OpenClaw.as_str())
                    .expect("query renamed provider")
                    .is_none(),
                "rename must not create a new row when originalId is stale"
            );
        });
    }

    #[test]
    #[serial]
    fn db_only_additive_update_survives_live_config_parse_errors() {
        with_test_home(|state, home| {
            let provider = openclaw_provider("deepseek");
            ProviderService::add(state, AppType::OpenClaw, provider.clone(), false)
                .expect("seed db-only provider");

            let stored = state
                .db
                .get_provider_by_id("deepseek", AppType::OpenClaw.as_str())
                .expect("query stored provider")
                .expect("provider should exist");
            assert_eq!(
                stored
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.live_config_managed),
                Some(false),
                "db-only provider should be marked as not live-managed"
            );

            let openclaw_dir = home.join(".openclaw");
            fs::create_dir_all(&openclaw_dir).expect("create openclaw dir");
            fs::write(openclaw_dir.join("openclaw.json"), "{ invalid json5")
                .expect("write malformed config");

            let mut updated = stored.clone();
            updated.name = "DeepSeek Edited".to_string();
            updated.meta.get_or_insert_with(ProviderMeta::default);

            ProviderService::update(state, AppType::OpenClaw, None, updated)
                .expect("db-only update should ignore live parse errors");

            let saved = state
                .db
                .get_provider_by_id("deepseek", AppType::OpenClaw.as_str())
                .expect("query updated provider")
                .expect("updated provider should exist");
            assert_eq!(saved.name, "DeepSeek Edited");
        });
    }

    #[test]
    #[serial]
    fn sync_current_provider_for_app_skips_db_only_opencode_provider() {
        with_test_home(|state, _| {
            let provider = opencode_provider("db-only-opencode");
            ProviderService::add(state, AppType::OpenCode, provider.clone(), false)
                .expect("seed db-only opencode provider");

            ProviderService::sync_current_provider_for_app(state, AppType::OpenCode)
                .expect("sync additive opencode providers");

            let live_providers = crate::opencode_config::get_providers()
                .expect("read opencode providers after sync");
            assert!(
                !live_providers.contains_key(&provider.id),
                "db-only opencode provider should not be written to live during sync"
            );
        });
    }

    #[test]
    #[serial]
    fn sync_current_provider_for_app_skips_db_only_openclaw_provider() {
        with_test_home(|state, _| {
            let provider = openclaw_provider("db-only-openclaw");
            ProviderService::add(state, AppType::OpenClaw, provider.clone(), false)
                .expect("seed db-only openclaw provider");

            ProviderService::sync_current_provider_for_app(state, AppType::OpenClaw)
                .expect("sync additive openclaw providers");

            let live_providers = crate::openclaw_config::get_providers()
                .expect("read openclaw providers after sync");
            assert!(
                !live_providers.contains_key(&provider.id),
                "db-only openclaw provider should not be written to live during sync"
            );
        });
    }

    #[test]
    #[serial]
    fn sync_current_provider_for_app_preserves_legacy_live_opencode_provider() {
        with_test_home(|state, _| {
            let provider = opencode_provider("legacy-opencode");
            crate::opencode_config::set_provider(&provider.id, provider.settings_config.clone())
                .expect("seed opencode live provider");
            state
                .db
                .save_provider(AppType::OpenCode.as_str(), &provider)
                .expect("seed legacy opencode provider in db");

            let mut updated = provider.clone();
            updated.settings_config["options"]["apiKey"] = Value::String("updated-key".to_string());
            state
                .db
                .save_provider(AppType::OpenCode.as_str(), &updated)
                .expect("update legacy opencode provider in db");

            ProviderService::sync_current_provider_for_app(state, AppType::OpenCode)
                .expect("sync legacy opencode provider");

            let live_providers =
                crate::opencode_config::get_providers().expect("read opencode providers");
            assert_eq!(
                live_providers
                    .get(&provider.id)
                    .and_then(|config| config.get("options"))
                    .and_then(|options| options.get("apiKey")),
                Some(&Value::String("updated-key".to_string())),
                "legacy provider that already exists in live should still be synced"
            );
        });
    }

    #[test]
    #[serial]
    fn sync_current_provider_for_app_restores_legacy_opencode_provider_after_live_reset() {
        with_test_home(|state, _| {
            let provider = opencode_provider("legacy-opencode-reset");
            state
                .db
                .save_provider(AppType::OpenCode.as_str(), &provider)
                .expect("seed legacy opencode provider in db");

            ProviderService::sync_current_provider_for_app(state, AppType::OpenCode)
                .expect("sync legacy opencode provider after reset");

            let live_providers =
                crate::opencode_config::get_providers().expect("read opencode providers");
            assert!(
                live_providers.contains_key(&provider.id),
                "legacy opencode provider should be restored when live config is reset"
            );
        });
    }

    #[test]
    #[serial]
    fn sync_current_provider_for_app_restores_legacy_openclaw_provider_after_live_reset() {
        with_test_home(|state, _| {
            let mut provider = openclaw_provider("legacy-openclaw-reset");
            provider.settings_config["models"] = json!([
                {
                    "id": "claude-sonnet-4",
                    "name": "Claude Sonnet 4"
                }
            ]);
            state
                .db
                .save_provider(AppType::OpenClaw.as_str(), &provider)
                .expect("seed legacy openclaw provider in db");

            ProviderService::sync_current_provider_for_app(state, AppType::OpenClaw)
                .expect("sync legacy openclaw provider after reset");

            let live_providers =
                crate::openclaw_config::get_providers().expect("read openclaw providers");
            assert!(
                live_providers.contains_key(&provider.id),
                "legacy openclaw provider should be restored when live config is reset"
            );
        });
    }

    #[test]
    #[serial]
    fn add_first_managed_codex_with_missing_account_leaves_no_provider_or_live_state() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            let provider = managed_codex_provider("managed-missing", "acct-missing");
            let live_before = crate::codex_config::CodexLiveStateSnapshot::capture()
                .expect("capture empty Codex live state");

            ProviderService::add(state, AppType::Codex, provider.clone(), false)
                .expect_err("missing managed account should fail before add commits");

            assert!(
                state
                    .db
                    .get_provider_by_id(&provider.id, AppType::Codex.as_str())
                    .expect("query failed managed add")
                    .is_none(),
                "failed preflight must not leave an orphan provider row"
            );
            assert_eq!(
                state
                    .db
                    .get_current_provider(AppType::Codex.as_str())
                    .expect("read current after failed add"),
                None
            );
            assert_eq!(
                crate::codex_config::CodexLiveStateSnapshot::capture()
                    .expect("capture Codex live after failed add"),
                live_before,
                "failed preflight must not mutate Codex live files"
            );
        });
    }

    #[test]
    #[serial]
    fn add_first_managed_codex_with_reauth_required_account_is_rejected() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token("acct-legacy", "managed-token", None)
                    .await
                    .expect("seed legacy account without id_token");
            });
            let provider = managed_codex_provider("managed-legacy", "acct-legacy");

            let error = ProviderService::add(state, AppType::Codex, provider.clone(), false)
                .expect_err("reauth-required account must not be written to live auth");
            assert!(
                error.to_string().contains("id_token"),
                "backend should require re-login even if the frontend gate is bypassed: {error}"
            );
            assert!(state
                .db
                .get_provider_by_id(&provider.id, AppType::Codex.as_str())
                .expect("query provider")
                .is_none());
            assert!(!crate::codex_config::get_codex_auth_path().exists());
        });
    }

    #[test]
    #[serial]
    fn add_first_managed_codex_current_failure_rolls_back_provider_and_live_state() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed",
                        "managed-token",
                        Some("managed-id-token"),
                    )
                    .await
                    .expect("seed managed Codex OAuth account");
            });

            let provider = managed_codex_provider("managed-first", "acct-managed");
            let live_before = crate::codex_config::CodexLiveStateSnapshot::capture()
                .expect("capture empty Codex live state");
            {
                let conn = state.db.conn.lock().expect("lock database");
                conn.execute_batch(
                    "CREATE TRIGGER reject_first_managed_current_update
                     BEFORE UPDATE OF is_current ON providers
                     WHEN NEW.app_type = 'codex'
                       AND NEW.id = 'managed-first'
                       AND NEW.is_current = 1
                     BEGIN
                       SELECT RAISE(ABORT, 'forced first managed Codex current failure');
                     END;",
                )
                .expect("install first-current failure trigger");
            }

            let error = ProviderService::add(state, AppType::Codex, provider.clone(), false)
                .expect_err("DB current failure should abort managed add");
            assert!(
                error
                    .to_string()
                    .contains("forced first managed Codex current failure"),
                "add should surface the DB current failure, got: {error}"
            );
            assert!(
                state
                    .db
                    .get_provider_by_id(&provider.id, AppType::Codex.as_str())
                    .expect("query rolled back provider")
                    .is_none(),
                "failed current commit must remove the newly inserted provider row"
            );
            assert_eq!(
                state
                    .db
                    .get_current_provider(AppType::Codex.as_str())
                    .expect("read current after rollback"),
                None
            );
            assert_eq!(
                crate::codex_config::CodexLiveStateSnapshot::capture()
                    .expect("capture Codex live after rollback"),
                live_before,
                "failed current commit must exactly restore Codex live files"
            );
        });
    }

    #[test]
    #[serial]
    fn switch_from_managed_codex_official_to_unbound_clears_live_without_backfilling_token() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed",
                        "managed-token",
                        Some("managed-id-token"),
                    )
                    .await
                    .expect("seed managed Codex OAuth account");
            });

            let mut managed = Provider::with_id(
                "managed-official".to_string(),
                "Managed Official".to_string(),
                json!({
                    "auth": {},
                    "config": ""
                }),
                None,
            );
            managed.category = Some("official".to_string());
            managed.meta = Some(ProviderMeta {
                auth_binding: Some(AuthBinding {
                    source: AuthBindingSource::ManagedAccount,
                    auth_provider: Some("codex_oauth".to_string()),
                    account_id: Some("acct-managed".to_string()),
                }),
                ..Default::default()
            });

            let mut unbound = Provider::with_id(
                "unbound-official".to_string(),
                "Unbound Official".to_string(),
                json!({
                    "auth": {},
                    "config": ""
                }),
                None,
            );
            unbound.category = Some("official".to_string());

            state
                .db
                .save_provider(AppType::Codex.as_str(), &managed)
                .expect("save managed provider");
            state
                .db
                .save_provider(AppType::Codex.as_str(), &unbound)
                .expect("save unbound provider");

            ProviderService::switch(state, AppType::Codex, "managed-official")
                .expect("switch to managed official");
            let live_auth: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                .expect("read managed live auth");
            assert_eq!(
                live_auth
                    .pointer("/tokens/access_token")
                    .and_then(Value::as_str),
                Some("managed-token"),
                "managed switch should write the selected ChatGPT token to live auth"
            );

            // Simulate a bare Codex CLI self-refresh. The app marker still
            // describes the pre-refresh write, while both access and refresh
            // token material on disk have rotated.
            let rotated_live_auth = crate::codex_config::codex_managed_oauth_auth_value(
                "acct-managed",
                "cli-rotated-access",
                Some("cli-rotated-id"),
                "cli-rotated-refresh",
                "2099-01-02T03:04:05Z",
            );
            write_json_file(
                &crate::codex_config::get_codex_auth_path(),
                &rotated_live_auth,
            )
            .expect("simulate Codex CLI token rotation");

            ProviderService::switch(state, AppType::Codex, "unbound-official")
                .expect("switch to unbound official");

            assert!(
                !crate::codex_config::get_codex_auth_path().exists(),
                "switching to an unbound official provider should clear the recorded managed live auth"
            );
            assert_eq!(
                tauri::async_runtime::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-managed")
                )
                .as_deref(),
                Some("cli-rotated-refresh"),
                "switch-away must adopt the CLI-rotated refresh token before deleting live auth"
            );

            let saved_managed = state
                .db
                .get_provider_by_id("managed-official", AppType::Codex.as_str())
                .expect("query managed provider")
                .expect("managed provider should exist");
            assert_eq!(
                saved_managed.settings_config.get("auth"),
                Some(&json!({})),
                "switch-away backfill must not persist the managed access token into provider storage"
            );
        });
    }

    #[test]
    #[serial]
    fn managed_codex_switch_adopts_outgoing_cli_rotation_before_account_or_key_overwrite() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-a",
                        "managed-access-a",
                        Some("managed-id-a"),
                    )
                    .await
                    .expect("seed account A");
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-b",
                        "managed-access-b",
                        Some("managed-id-b"),
                    )
                    .await
                    .expect("seed account B");
            });

            let provider_a = managed_codex_provider("managed-a", "acct-a");
            let provider_b = managed_codex_provider("managed-b", "acct-b");
            let mut third_party = Provider::with_id(
                "third-party".to_string(),
                "Third Party".to_string(),
                json!({
                    "auth": { "OPENAI_API_KEY": "sk-third-party" },
                    "config": r#"model_provider = "third"
[model_providers.third]
name = "Third"
base_url = "https://third.example/v1"
wire_api = "responses"
"#
                }),
                None,
            );
            third_party.category = Some("custom".to_string());
            for provider in [&provider_a, &provider_b, &third_party] {
                state
                    .db
                    .save_provider(AppType::Codex.as_str(), provider)
                    .expect("save provider");
            }

            ProviderService::switch(state, AppType::Codex, &provider_a.id)
                .expect("activate managed A");
            write_json_file(
                &crate::codex_config::get_codex_auth_path(),
                &crate::codex_config::codex_managed_oauth_auth_value(
                    "acct-a",
                    "cli-access-a1",
                    Some("cli-id-a1"),
                    "cli-refresh-a1",
                    "2099-01-02T00:00:00Z",
                ),
            )
            .expect("rotate account A live auth");

            ProviderService::switch(state, AppType::Codex, &provider_b.id)
                .expect("switch managed A to managed B");
            assert_eq!(
                tauri::async_runtime::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-a")
                )
                .as_deref(),
                Some("cli-refresh-a1"),
                "A's CLI generation must be adopted before B overwrites auth.json"
            );
            let live_b: Value =
                read_json_file(&crate::codex_config::get_codex_auth_path()).expect("read B auth");
            assert_eq!(
                live_b.pointer("/tokens/account_id").and_then(Value::as_str),
                Some("acct-b")
            );

            write_json_file(
                &crate::codex_config::get_codex_auth_path(),
                &crate::codex_config::codex_managed_oauth_auth_value(
                    "acct-b",
                    "cli-access-b1",
                    Some("cli-id-b1"),
                    "cli-refresh-b1",
                    "2099-01-03T00:00:00Z",
                ),
            )
            .expect("rotate account B live auth");

            ProviderService::switch(state, AppType::Codex, &third_party.id)
                .expect("switch managed B to API-key provider");
            assert_eq!(
                tauri::async_runtime::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-b")
                )
                .as_deref(),
                Some("cli-refresh-b1"),
                "B's CLI generation must be adopted before the third-party switch removes auth.json"
            );
            assert!(
                !crate::codex_config::get_codex_auth_path().exists(),
                "third-party switches are config-only: auth.json is removed"
            );
            let live_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
                .expect("read third-party config");
            assert!(
                live_config.contains("experimental_bearer_token = \"sk-third-party\""),
                "the third-party key rides in config.toml; got:\n{live_config}"
            );
        });
    }

    #[test]
    #[serial]
    fn managed_codex_direct_update_adopts_outgoing_cli_rotation_and_commits_target_binding() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-a",
                        "managed-access-a",
                        Some("managed-id-a"),
                    )
                    .await
                    .expect("seed account A");
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-b",
                        "managed-access-b",
                        Some("managed-id-b"),
                    )
                    .await
                    .expect("seed account B");
            });

            let provider = managed_codex_provider("managed-official-a", "acct-a");
            state
                .db
                .save_provider(AppType::Codex.as_str(), &provider)
                .expect("save managed official provider");
            ProviderService::switch(state, AppType::Codex, &provider.id)
                .expect("activate managed account A");
            assert!(
                tauri::async_runtime::block_on(state.db.get_live_backup(AppType::Codex.as_str()))
                    .expect("read initial live backup")
                    .is_none(),
                "direct update precondition requires no takeover backup"
            );

            write_json_file(
                &crate::codex_config::get_codex_auth_path(),
                &crate::codex_config::codex_managed_oauth_auth_value(
                    "acct-a",
                    "cli-access-a1",
                    Some("cli-id-a1"),
                    "cli-refresh-a1",
                    "2099-03-01T00:00:00Z",
                ),
            )
            .expect("simulate account A CLI rotation");

            let mut updated = provider.clone();
            updated.name = "OpenAI Official B".to_string();
            updated
                .meta
                .as_mut()
                .and_then(|meta| meta.auth_binding.as_mut())
                .expect("managed binding")
                .account_id = Some("acct-b".to_string());

            ProviderService::update(state, AppType::Codex, None, updated.clone())
                .expect("directly update managed binding from A to B");

            assert_eq!(
                tauri::async_runtime::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-a")
                )
                .as_deref(),
                Some("cli-refresh-a1"),
                "direct update must adopt A's CLI generation before overwriting live auth"
            );
            let saved = state
                .db
                .get_provider_by_id(&provider.id, AppType::Codex.as_str())
                .expect("read updated provider")
                .expect("updated provider exists");
            assert_eq!(saved.name, updated.name);
            assert_eq!(
                saved
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.managed_account_id_for("codex_oauth"))
                    .as_deref(),
                Some("acct-b")
            );

            let live_b: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                .expect("read account B live auth");
            assert_eq!(
                live_b.pointer("/tokens/account_id").and_then(Value::as_str),
                Some("acct-b")
            );
            assert!(
                crate::codex_config::codex_auth_matches_recorded_managed_oauth(&live_b, "acct-b")
                    .expect("check account B marker"),
                "clearing outgoing account A must not remove account B's marker"
            );
            assert!(
                tauri::async_runtime::block_on(state.db.get_live_backup(AppType::Codex.as_str()))
                    .expect("read live backup after direct update")
                    .is_none(),
                "direct update must not create a takeover backup"
            );
        });
    }

    #[test]
    #[serial]
    fn same_account_managed_codex_update_rejects_equal_timestamp_refresh_conflict() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed",
                        "managed-access",
                        Some("managed-id"),
                    )
                    .await
                    .expect("seed managed account");
            });

            let provider = managed_codex_provider("managed-same-account", "acct-managed");
            state
                .db
                .save_provider(AppType::Codex.as_str(), &provider)
                .expect("save managed provider");
            ProviderService::switch(state, AppType::Codex, &provider.id)
                .expect("activate managed provider");

            // Different refresh material at the exact manager generation
            // timestamp is ambiguous at millisecond precision. A same-account
            // update has no outgoing-account guard, so its managed bundle
            // preflight itself must refuse to overwrite this CLI generation.
            tauri::async_runtime::block_on(
                state
                    .codex_oauth_manager
                    .test_set_token_updated_at_ms("acct-managed", 1_700_000_000_000),
            );
            let cli_live_auth = crate::codex_config::codex_managed_oauth_auth_value(
                "acct-managed",
                "cli-access-r1",
                Some("cli-id-r1"),
                "cli-refresh-r1",
                "2023-11-14T22:13:20Z",
            );
            write_json_file(&crate::codex_config::get_codex_auth_path(), &cli_live_auth)
                .expect("seed equal-timestamp CLI generation");

            let mut updated = provider.clone();
            updated.name = "Managed updated".to_string();
            let error = ProviderService::update(state, AppType::Codex, None, updated)
                .expect_err("ambiguous same-account generation must block the live write");
            assert!(
                error
                    .to_string()
                    .contains("which refresh token is newer cannot be determined safely"),
                "update should explain the safe-write rejection: {error}"
            );

            let live_after: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                .expect("read preserved CLI auth");
            assert_eq!(
                live_after, cli_live_auth,
                "same-account managed update must not overwrite ambiguous CLI token material"
            );
            assert_eq!(
                tauri::async_runtime::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-managed")
                )
                .as_deref(),
                Some("test-refresh-token"),
                "ambiguous CLI material must not replace the manager generation either"
            );
            assert_eq!(
                state
                    .db
                    .get_provider_by_id(&provider.id, AppType::Codex.as_str())
                    .expect("read provider after rejected update")
                    .expect("provider remains present")
                    .name,
                provider.name,
                "rejected preflight must leave the provider row unchanged"
            );
        });
    }

    #[test]
    #[serial]
    fn switch_away_rejects_legacy_refresh_conflict_on_every_retry() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-legacy",
                        "managed-access",
                        Some("managed-id"),
                    )
                    .await
                    .expect("seed managed account");
            });

            let managed = managed_codex_provider("managed-legacy", "acct-legacy");
            let mut unbound = Provider::with_id(
                "unbound-official".to_string(),
                "Unbound Official".to_string(),
                json!({
                    "auth": {},
                    "config": ""
                }),
                None,
            );
            unbound.category = Some("official".to_string());
            for provider in [&managed, &unbound] {
                state
                    .db
                    .save_provider(AppType::Codex.as_str(), provider)
                    .expect("save provider");
            }
            ProviderService::switch(state, AppType::Codex, &managed.id)
                .expect("activate managed provider");

            tauri::async_runtime::block_on(
                state
                    .codex_oauth_manager
                    .test_set_token_updated_at_ms("acct-legacy", 0),
            );
            let cli_live_auth = crate::codex_config::codex_managed_oauth_auth_value(
                "acct-legacy",
                "cli-access-r1",
                Some("cli-id-r1"),
                "cli-refresh-r1",
                "2023-11-14T22:13:20Z",
            );
            write_json_file(&crate::codex_config::get_codex_auth_path(), &cli_live_auth)
                .expect("seed CLI generation against legacy manager state");

            for attempt in 1..=2 {
                let error = ProviderService::switch(state, AppType::Codex, &unbound.id)
                    .expect_err("legacy conflict must block every switch-away retry");
                assert!(
                    error
                        .to_string()
                        .contains("which refresh token is newer cannot be determined safely"),
                    "attempt {attempt} should remain ambiguous: {error}"
                );
                let live_after: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                    .expect("read preserved CLI auth");
                assert_eq!(
                    live_after, cli_live_auth,
                    "attempt {attempt} must not overwrite or delete the CLI generation"
                );
                assert_eq!(
                    state
                        .db
                        .get_current_provider(AppType::Codex.as_str())
                        .expect("read current provider")
                        .as_deref(),
                    Some(managed.id.as_str()),
                    "attempt {attempt} must not commit the target provider"
                );
            }

            assert_eq!(
                tauri::async_runtime::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-legacy")
                )
                .as_deref(),
                Some("test-refresh-token"),
                "ambiguous legacy retries must keep manager material unchanged"
            );
        });
    }

    #[test]
    #[serial]
    fn codex_auth_center_remove_and_logout_clear_live_credentials_and_marker() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed",
                        "managed-access",
                        Some("managed-id"),
                    )
                    .await
                    .expect("seed managed account");
            });
            let provider = managed_codex_provider("managed-auth-center", "acct-managed");
            state
                .db
                .save_provider(AppType::Codex.as_str(), &provider)
                .expect("save managed provider");
            ProviderService::switch(state, AppType::Codex, &provider.id)
                .expect("activate managed provider");
            assert!(crate::codex_config::get_codex_auth_path().exists());
            assert!(crate::codex_config::codex_managed_oauth_live_auth_marker_exists());

            tauri::async_runtime::block_on(
                state.codex_oauth_manager.remove_account("acct-managed"),
            )
            .expect("remove managed account");
            assert!(
                !crate::codex_config::get_codex_auth_path().exists(),
                "removing the active account must delete its refreshable live auth"
            );
            assert!(
                !crate::codex_config::codex_managed_oauth_live_auth_marker_exists(),
                "removing the active account must delete its marker"
            );
            assert_eq!(
                state
                    .db
                    .get_provider_by_id(&provider.id, AppType::Codex.as_str())
                    .expect("read provider")
                    .and_then(|provider| provider.meta)
                    .and_then(|meta| meta.managed_account_id_for("codex_oauth"))
                    .as_deref(),
                Some("acct-managed"),
                "the binding is retained so re-login with the same account can recover it"
            );

            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed",
                        "managed-access-2",
                        Some("managed-id-2"),
                    )
                    .await
                    .expect("re-login managed account");
            });
            ProviderService::switch(state, AppType::Codex, &provider.id)
                .expect("reactivate managed provider after re-login");
            assert!(crate::codex_config::get_codex_auth_path().exists());

            tauri::async_runtime::block_on(state.codex_oauth_manager.clear_auth())
                .expect("logout all managed accounts");
            assert!(!crate::codex_config::get_codex_auth_path().exists());
            assert!(!crate::codex_config::codex_managed_oauth_live_auth_marker_exists());
            assert!(
                tauri::async_runtime::block_on(state.codex_oauth_manager.list_accounts())
                    .is_empty()
            );
        });
    }

    #[test]
    #[serial]
    fn managed_codex_switch_db_current_failure_restores_live_bundle_and_current() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed-a",
                        "managed-token-a",
                        Some("managed-id-token-a"),
                    )
                    .await
                    .expect("seed first managed Codex OAuth account");
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed-b",
                        "managed-token-b",
                        Some("managed-id-token-b"),
                    )
                    .await
                    .expect("seed second managed Codex OAuth account");
            });

            let managed_provider = |id: &str, account_id: &str, model: &str| {
                let mut provider = Provider::with_id(
                    id.to_string(),
                    format!("Managed {id}"),
                    json!({
                        "auth": {},
                        "config": format!("model = \"{model}\"\n"),
                        "modelCatalog": {
                            "models": [{ "model": model }]
                        }
                    }),
                    None,
                );
                provider.category = Some("official".to_string());
                provider.meta = Some(ProviderMeta {
                    auth_binding: Some(AuthBinding {
                        source: AuthBindingSource::ManagedAccount,
                        auth_provider: Some("codex_oauth".to_string()),
                        account_id: Some(account_id.to_string()),
                    }),
                    ..Default::default()
                });
                provider
            };

            let provider_a = managed_provider("managed-a", "acct-managed-a", "gpt-5.4-managed-a");
            let provider_b = managed_provider("managed-b", "acct-managed-b", "gpt-5.4-managed-b");
            state
                .db
                .save_provider(AppType::Codex.as_str(), &provider_a)
                .expect("save first managed provider");
            state
                .db
                .save_provider(AppType::Codex.as_str(), &provider_b)
                .expect("save second managed provider");

            ProviderService::switch(state, AppType::Codex, &provider_a.id)
                .expect("activate first managed provider");
            let auth_before: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                .expect("read first managed auth");
            assert!(
                crate::codex_config::get_codex_config_path().exists(),
                "baseline must include config.toml"
            );
            assert!(
                crate::codex_config::get_codex_model_catalog_path().exists(),
                "baseline must include the generated model catalog"
            );
            assert!(
                crate::codex_config::codex_auth_matches_recorded_managed_oauth(
                    &auth_before,
                    "acct-managed-a",
                )
                .expect("check first managed auth marker"),
                "baseline must include a marker owned by the first managed account"
            );
            let live_before = crate::codex_config::CodexLiveStateSnapshot::capture()
                .expect("capture auth/config/catalog/marker before failed switch");

            {
                let conn = state.db.conn.lock().expect("lock database");
                conn.execute_batch(
                    "CREATE TRIGGER reject_managed_b_current_update
                     BEFORE UPDATE OF is_current ON providers
                     WHEN NEW.app_type = 'codex'
                       AND NEW.id = 'managed-b'
                       AND NEW.is_current = 1
                     BEGIN
                       SELECT RAISE(ABORT, 'forced managed Codex current failure');
                     END;",
                )
                .expect("install current-provider failure trigger");
            }

            let error = ProviderService::switch(state, AppType::Codex, &provider_b.id)
                .expect_err("DB current failure should abort managed switch");
            assert!(
                error
                    .to_string()
                    .contains("forced managed Codex current failure"),
                "switch should surface the DB commit failure, got: {error}"
            );

            let live_after = crate::codex_config::CodexLiveStateSnapshot::capture()
                .expect("capture auth/config/catalog/marker after rollback");
            assert_eq!(
                live_after, live_before,
                "failed switch must exactly restore auth, config, catalog, and managed marker"
            );
            assert_eq!(
                crate::settings::get_current_provider(&AppType::Codex).as_deref(),
                Some(provider_a.id.as_str()),
                "failed switch must restore the device-local current provider"
            );
            assert_eq!(
                state
                    .db
                    .get_current_provider(AppType::Codex.as_str())
                    .expect("read DB current after rollback")
                    .as_deref(),
                Some(provider_a.id.as_str()),
                "failed switch must keep the DB current provider unchanged"
            );
        });
    }

    #[test]
    #[serial]
    fn managed_codex_takeover_update_db_failure_restores_backup_live_and_binding() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed-a",
                        "managed-token-a",
                        Some("managed-id-a"),
                    )
                    .await
                    .expect("seed managed account A");
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed-b",
                        "managed-token-b",
                        Some("managed-id-b"),
                    )
                    .await
                    .expect("seed managed account B");
            });

            let mut provider = Provider::with_id(
                "managed-official-a".to_string(),
                "OpenAI Official A".to_string(),
                json!({
                    "auth": {},
                    "config": "model = \"gpt-5.4\"\n"
                }),
                None,
            );
            provider.category = Some("official".to_string());
            provider.meta = Some(ProviderMeta {
                auth_binding: Some(AuthBinding {
                    source: AuthBindingSource::ManagedAccount,
                    auth_provider: Some("codex_oauth".to_string()),
                    account_id: Some("acct-managed-a".to_string()),
                }),
                ..Default::default()
            });
            state
                .db
                .save_provider(AppType::Codex.as_str(), &provider)
                .expect("save official provider A");
            state
                .db
                .set_current_provider(AppType::Codex.as_str(), &provider.id)
                .expect("set DB current");
            crate::settings::set_current_provider(&AppType::Codex, Some(&provider.id))
                .expect("set local current");

            tauri::async_runtime::block_on(async {
                state
                    .db
                    .update_proxy_config(ProxyConfig {
                        listen_port: 15_721,
                        ..Default::default()
                    })
                    .await
                    .expect("set proxy port");
                state
                    .db
                    .save_live_backup(
                        AppType::Codex.as_str(),
                        &serde_json::to_string(&json!({
                            "config": "model = \"gpt-5.4\"\n"
                        }))
                        .expect("serialize baseline backup"),
                    )
                    .await
                    .expect("save baseline backup");
                state
                    .proxy_service
                    .sync_codex_live_from_provider_while_proxy_active(&provider)
                    .await
                    .expect("seed managed takeover live");
            });

            let backup_before =
                tauri::async_runtime::block_on(state.db.get_live_backup(AppType::Codex.as_str()))
                    .expect("read baseline backup")
                    .expect("baseline backup exists");
            let live_before = crate::codex_config::CodexLiveStateSnapshot::capture()
                .expect("capture managed takeover live");

            {
                let conn = state.db.conn.lock().expect("lock database");
                conn.execute_batch(
                    "CREATE TRIGGER reject_managed_takeover_provider_update
                     BEFORE UPDATE ON providers
                     WHEN NEW.app_type = 'codex'
                       AND NEW.id = 'managed-official-a'
                       AND NEW.name = 'OpenAI Official B'
                     BEGIN
                       SELECT RAISE(ABORT, 'forced managed takeover provider failure');
                     END;",
                )
                .expect("install provider failure trigger");
            }

            let mut updated = provider.clone();
            updated.name = "OpenAI Official B".to_string();
            updated
                .meta
                .as_mut()
                .and_then(|meta| meta.auth_binding.as_mut())
                .expect("managed binding")
                .account_id = Some("acct-managed-b".to_string());

            let error = ProviderService::update(state, AppType::Codex, None, updated)
                .expect_err("DB failure should abort takeover update");
            assert!(
                error
                    .to_string()
                    .contains("forced managed takeover provider failure"),
                "update should surface DB failure: {error}"
            );

            let saved = state
                .db
                .get_provider_by_id(&provider.id, AppType::Codex.as_str())
                .expect("read provider after rollback")
                .expect("provider still exists");
            assert_eq!(saved.name, "OpenAI Official A");
            assert_eq!(
                saved
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.managed_account_id_for("codex_oauth")),
                Some("acct-managed-a".to_string())
            );

            let backup_after =
                tauri::async_runtime::block_on(state.db.get_live_backup(AppType::Codex.as_str()))
                    .expect("read backup after rollback")
                    .expect("backup still exists");
            assert_eq!(backup_after.original_config, backup_before.original_config);
            assert_eq!(
                crate::codex_config::CodexLiveStateSnapshot::capture()
                    .expect("capture live after rollback"),
                live_before,
                "failed takeover update must restore auth/config/catalog/marker exactly"
            );
        });
    }

    #[test]
    #[serial]
    fn managed_codex_update_rechecks_current_after_waiting_for_switch_lock() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            tauri::async_runtime::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed-a",
                        "managed-token-a",
                        Some("managed-id-a"),
                    )
                    .await
                    .expect("seed managed account A");
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token(
                        "acct-managed-b",
                        "managed-token-b",
                        Some("managed-id-b"),
                    )
                    .await
                    .expect("seed managed account B");
            });

            let mut official = Provider::with_id(
                "managed-official-a".to_string(),
                "OpenAI Official".to_string(),
                json!({ "auth": {}, "config": "model = \"gpt-5.4\"\n" }),
                None,
            );
            official.category = Some("official".to_string());
            official.meta = Some(ProviderMeta {
                auth_binding: Some(AuthBinding {
                    source: AuthBindingSource::ManagedAccount,
                    auth_provider: Some("codex_oauth".to_string()),
                    account_id: Some("acct-managed-a".to_string()),
                }),
                ..Default::default()
            });
            state
                .db
                .save_provider(AppType::Codex.as_str(), &official)
                .expect("save official A");
            state
                .db
                .set_current_provider(AppType::Codex.as_str(), &official.id)
                .expect("set official current");
            crate::settings::set_current_provider(&AppType::Codex, Some(&official.id))
                .expect("set local official current");

            let mut third_party = Provider::with_id(
                "third-party-current".to_string(),
                "Third Party".to_string(),
                json!({
                    "auth": { "OPENAI_API_KEY": "sk-third" },
                    "config": r#"model_provider = "third"
[model_providers.third]
name = "Third"
base_url = "https://third.example/v1"
wire_api = "responses"
"#
                }),
                None,
            );
            third_party.category = Some("custom".to_string());
            state
                .db
                .save_provider(AppType::Codex.as_str(), &third_party)
                .expect("save third party");

            let mut updated = official.clone();
            updated
                .meta
                .as_mut()
                .and_then(|meta| meta.auth_binding.as_mut())
                .expect("managed binding")
                .account_id = Some("acct-managed-b".to_string());

            let switch_guard = tauri::async_runtime::block_on(
                state
                    .proxy_service
                    .lock_switch_for_app(AppType::Codex.as_str()),
            );
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let (update_result, live_after_switch) = std::thread::scope(|scope| {
                let updater = scope.spawn(move || {
                    started_tx.send(()).expect("signal updater start");
                    ProviderService::update(state, AppType::Codex, None, updated)
                });
                started_rx.recv().expect("wait for updater");

                // This emulates a switch that already owns the per-app lock and
                // commits a different current target before the queued update is
                // allowed to inspect current/existing state.
                state
                    .db
                    .set_current_provider(AppType::Codex.as_str(), &third_party.id)
                    .expect("switch DB current to third party");
                crate::settings::set_current_provider(
                    &AppType::Codex,
                    Some(third_party.id.as_str()),
                )
                .expect("switch local current to third party");
                write_live_with_common_config_for_state(state, &AppType::Codex, &third_party)
                    .expect("write third-party live");
                let live_after_switch = crate::codex_config::CodexLiveStateSnapshot::capture()
                    .expect("capture third-party live");

                drop(switch_guard);
                let result = updater.join().expect("join managed updater");
                (result, live_after_switch)
            });

            update_result.expect("save queued non-current managed row");
            assert_eq!(
                state
                    .db
                    .get_current_provider(AppType::Codex.as_str())
                    .expect("read DB current")
                    .as_deref(),
                Some(third_party.id.as_str())
            );
            assert_eq!(
                crate::codex_config::CodexLiveStateSnapshot::capture()
                    .expect("capture live after queued update"),
                live_after_switch,
                "queued provider edit must not rewrite the newly switched current live"
            );
            let saved_official = state
                .db
                .get_provider_by_id(&official.id, AppType::Codex.as_str())
                .expect("read saved official")
                .expect("official exists");
            assert_eq!(
                saved_official
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.managed_account_id_for("codex_oauth")),
                Some("acct-managed-b".to_string())
            );
        });
    }

    #[test]
    #[serial]
    fn switch_to_managed_codex_official_with_unresolvable_account_keeps_current_unchanged() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");

            // Baseline: an ordinary third-party provider that switches normally, used as the initial current.
            // config must carry a custom provider table: a config-only switch requires the key to have a
            // provider-level landing spot.
            let mut baseline = Provider::with_id(
                "baseline".to_string(),
                "Baseline".to_string(),
                json!({
                    "auth": { "OPENAI_API_KEY": "sk-baseline" },
                    "config": "model_provider = \"baseline\"\n[model_providers.baseline]\nbase_url = \"https://baseline.example/v1\"\n"
                }),
                None,
            );
            baseline.category = Some("custom".to_string());

            // A managed official provider bound to an account that does not exist in the manager: the
            // switch preflight token fetch is bound to fail.
            let mut managed = Provider::with_id(
                "managed-official".to_string(),
                "Managed Official".to_string(),
                json!({ "auth": {}, "config": "" }),
                None,
            );
            managed.category = Some("official".to_string());
            managed.meta = Some(ProviderMeta {
                auth_binding: Some(AuthBinding {
                    source: AuthBindingSource::ManagedAccount,
                    auth_provider: Some("codex_oauth".to_string()),
                    account_id: Some("acct-missing".to_string()),
                }),
                ..Default::default()
            });

            state
                .db
                .save_provider(AppType::Codex.as_str(), &baseline)
                .expect("save baseline");
            state
                .db
                .save_provider(AppType::Codex.as_str(), &managed)
                .expect("save managed");

            ProviderService::switch(state, AppType::Codex, "baseline").expect("switch to baseline");

            // Switch to the managed provider bound to a missing account: preflight fails -> returns Err.
            let result = ProviderService::switch(state, AppType::Codex, "managed-official");
            assert!(
                result.is_err(),
                "switch must fail when the managed OAuth token cannot be resolved"
            );

            // current must still be baseline: the preflight fails before current is committed, leaving no
            // inconsistent "DB/UI points at the new provider but live is still the old one" state.
            let current =
                crate::settings::get_effective_current_provider(&state.db, &AppType::Codex)
                    .expect("read current");
            assert_eq!(
                current.as_deref(),
                Some("baseline"),
                "a failed managed switch must not move current off the previous provider"
            );
        });
    }

    #[test]
    #[serial]
    fn import_opencode_providers_from_live_marks_provider_as_live_managed() {
        with_test_home(|state, _| {
            let provider = opencode_provider("imported-opencode");
            crate::opencode_config::set_provider(&provider.id, provider.settings_config.clone())
                .expect("seed opencode live provider");

            let imported = import_opencode_providers_from_live(state)
                .expect("import opencode providers from live");
            assert_eq!(imported, 1);

            let saved = state
                .db
                .get_provider_by_id(&provider.id, AppType::OpenCode.as_str())
                .expect("query imported opencode provider")
                .expect("imported opencode provider should exist");
            assert_eq!(
                saved
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.live_config_managed),
                Some(true),
                "providers imported from live should be treated as live-managed"
            );
        });
    }

    #[test]
    #[serial]
    fn import_opencode_providers_from_live_updates_existing_provider_from_live() {
        with_test_home(|state, _| {
            let provider = opencode_provider("existing-opencode");
            state
                .db
                .save_provider(AppType::OpenCode.as_str(), &provider)
                .expect("seed existing opencode provider");

            let mut live_settings = provider.settings_config.clone();
            live_settings.as_object_mut().unwrap().remove("name");
            live_settings["npm"] = Value::String("@ai-sdk/anthropic".to_string());
            live_settings["models"]["gpt-4o"]["name"] = Value::String("Claude Sonnet".to_string());
            crate::opencode_config::set_provider(&provider.id, live_settings)
                .expect("seed edited live opencode provider");

            let updated = import_opencode_providers_from_live(state)
                .expect("import opencode providers from live");
            assert_eq!(updated, 1);

            let saved = state
                .db
                .get_provider_by_id(&provider.id, AppType::OpenCode.as_str())
                .expect("query updated opencode provider")
                .expect("opencode provider should exist");
            assert_eq!(saved.name, provider.name);
            assert_eq!(saved.settings_config["npm"], json!("@ai-sdk/anthropic"));
            assert_eq!(
                saved.settings_config["models"]["gpt-4o"]["name"],
                json!("Claude Sonnet")
            );
        });
    }
    #[test]
    #[serial]
    fn import_openclaw_providers_from_live_marks_provider_as_live_managed() {
        with_test_home(|state, _| {
            let mut provider = openclaw_provider("imported-openclaw");
            provider.settings_config["models"] = json!([
                {
                    "id": "claude-sonnet-4",
                    "name": "Claude Sonnet 4"
                }
            ]);
            crate::openclaw_config::set_provider(&provider.id, provider.settings_config.clone())
                .expect("seed openclaw live provider");

            let imported = import_openclaw_providers_from_live(state)
                .expect("import openclaw providers from live");
            assert_eq!(imported, 1);

            let saved = state
                .db
                .get_provider_by_id(&provider.id, AppType::OpenClaw.as_str())
                .expect("query imported openclaw provider")
                .expect("imported openclaw provider should exist");
            assert_eq!(
                saved
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.live_config_managed),
                Some(true),
                "providers imported from live should be treated as live-managed"
            );
        });
    }

    #[test]
    #[serial]
    fn import_openclaw_providers_from_live_updates_existing_provider_from_live() {
        with_test_home(|state, _| {
            let mut provider = openclaw_provider("existing-openclaw");
            provider.settings_config["models"] = json!([
                {
                    "id": "claude-sonnet-4",
                    "name": "Claude Sonnet 4"
                }
            ]);
            state
                .db
                .save_provider(AppType::OpenClaw.as_str(), &provider)
                .expect("seed existing openclaw provider");

            let mut live_settings = provider.settings_config.clone();
            live_settings["baseUrl"] = Value::String("https://api.example.com/v1".to_string());
            live_settings["models"][0]["name"] = Value::String("Claude Sonnet 4.1".to_string());
            crate::openclaw_config::set_provider(&provider.id, live_settings)
                .expect("seed edited live openclaw provider");

            let updated = import_openclaw_providers_from_live(state)
                .expect("import openclaw providers from live");
            assert_eq!(updated, 1);

            let saved = state
                .db
                .get_provider_by_id(&provider.id, AppType::OpenClaw.as_str())
                .expect("query updated openclaw provider")
                .expect("openclaw provider should exist");
            assert_eq!(saved.name, provider.name);
            assert_eq!(
                saved.settings_config["baseUrl"],
                json!("https://api.example.com/v1")
            );
            assert_eq!(
                saved.settings_config["models"][0]["name"],
                json!("Claude Sonnet 4.1")
            );
        });
    }

    #[test]
    #[serial]
    fn import_hermes_providers_from_live_updates_existing_provider_from_live() {
        with_test_home(|state, _| {
            let provider = hermes_provider("existing-hermes");
            state
                .db
                .save_provider(AppType::Hermes.as_str(), &provider)
                .expect("seed existing hermes provider");

            let mut live_settings = provider.settings_config.clone();
            live_settings["base_url"] = Value::String("https://api.hermes.example/v1".to_string());
            live_settings["models"]["gpt-4o"]["name"] = Value::String("GPT-4o Updated".to_string());
            crate::hermes_config::set_provider(&provider.id, live_settings)
                .expect("seed edited live hermes provider");

            let updated = import_hermes_providers_from_live(state)
                .expect("import hermes providers from live");
            assert_eq!(updated, 1);

            let saved = state
                .db
                .get_provider_by_id(&provider.id, AppType::Hermes.as_str())
                .expect("query updated hermes provider")
                .expect("hermes provider should exist");
            assert_eq!(saved.name, provider.name);
            assert_eq!(
                saved.settings_config["base_url"],
                json!("https://api.hermes.example/v1")
            );
            // models are denormalized from YAML dict to UI-friendly array by
            // get_providers(), so access by index rather than dict key
            assert_eq!(
                saved.settings_config["models"][0]["name"],
                json!("GPT-4o Updated")
            );
            assert_eq!(saved.settings_config["models"][0]["id"], json!("gpt-4o"));
        });
    }

    #[test]
    #[serial]
    fn legacy_additive_provider_still_errors_on_live_config_parse_failure() {
        with_test_home(|state, home| {
            let provider = openclaw_provider("legacy-provider");
            state
                .db
                .save_provider(AppType::OpenClaw.as_str(), &provider)
                .expect("seed legacy provider without live_config_managed marker");

            let openclaw_dir = home.join(".openclaw");
            fs::create_dir_all(&openclaw_dir).expect("create openclaw dir");
            fs::write(openclaw_dir.join("openclaw.json"), "{ invalid json5")
                .expect("write malformed config");

            let mut updated = provider.clone();
            updated.name = "Legacy Edited".to_string();

            let err = ProviderService::update(state, AppType::OpenClaw, None, updated)
                .expect_err("legacy providers should still surface live parse errors");
            assert!(
                err.to_string().contains("Failed to parse OpenClaw config"),
                "expected parse error, got {err:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn update_persists_non_current_omo_variants_in_database() {
        with_test_home(|state, _| {
            for category in ["omo", "omo-slim"] {
                let provider = opencode_omo_provider(&format!("{category}-provider"), category);
                state
                    .db
                    .save_provider(AppType::OpenCode.as_str(), &provider)
                    .unwrap_or_else(|err| panic!("seed {category} provider: {err}"));

                let mut updated = provider.clone();
                updated.name = format!("Updated {category}");
                updated.settings_config["agents"]["writer"]["model"] =
                    Value::String(format!("{category}-next-model"));

                ProviderService::update(state, AppType::OpenCode, None, updated)
                    .unwrap_or_else(|err| panic!("update {category} provider: {err}"));

                let saved = state
                    .db
                    .get_provider_by_id(&provider.id, AppType::OpenCode.as_str())
                    .unwrap_or_else(|err| panic!("query updated {category} provider: {err}"))
                    .unwrap_or_else(|| panic!("{category} provider should exist"));

                assert_eq!(saved.name, format!("Updated {category}"));
                assert_eq!(
                    saved.settings_config["agents"]["writer"]["model"],
                    Value::String(format!("{category}-next-model")),
                    "{category} updates should persist in the database"
                );
            }
        });
    }

    #[test]
    #[serial]
    fn update_current_omo_variant_rewrites_config_from_saved_provider() {
        with_test_home(|state, home| {
            for category in ["omo", "omo-slim"] {
                let provider = opencode_omo_provider(&format!("{category}-current"), category);
                state
                    .db
                    .save_provider(AppType::OpenCode.as_str(), &provider)
                    .unwrap_or_else(|err| panic!("seed current {category} provider: {err}"));
                state
                    .db
                    .set_omo_provider_current(AppType::OpenCode.as_str(), &provider.id, category)
                    .unwrap_or_else(|err| panic!("set current {category} provider: {err}"));

                let mut updated = provider.clone();
                updated.name = format!("Current {category} updated");
                updated.settings_config["agents"]["writer"]["model"] =
                    Value::String(format!("{category}-saved-model"));
                updated.settings_config["otherFields"]["theme"] =
                    Value::String(format!("{category}-light"));

                ProviderService::update(state, AppType::OpenCode, None, updated)
                    .unwrap_or_else(|err| panic!("update current {category} provider: {err}"));

                let saved = state
                    .db
                    .get_provider_by_id(&provider.id, AppType::OpenCode.as_str())
                    .unwrap_or_else(|err| panic!("query current {category} provider: {err}"))
                    .unwrap_or_else(|| panic!("current {category} provider should exist"));
                assert_eq!(saved.name, format!("Current {category} updated"));

                let written = fs::read_to_string(omo_config_path(home, category))
                    .unwrap_or_else(|err| panic!("read written {category} config: {err}"));
                let written_json: Value = serde_json::from_str(&written)
                    .unwrap_or_else(|err| panic!("parse written {category} config: {err}"));

                assert_eq!(
                    written_json["agents"]["writer"]["model"],
                    Value::String(format!("{category}-saved-model")),
                    "{category} config should be written from the saved provider state"
                );
                assert_eq!(
                    written_json["theme"],
                    Value::String(format!("{category}-light")),
                    "{category} top-level config should reflect updated otherFields"
                );
            }
        });
    }

    #[test]
    #[serial]
    fn update_current_omo_variant_does_not_persist_database_when_file_write_fails() {
        with_test_home(|state, home| {
            let provider = opencode_omo_provider("omo-current", "omo");
            state
                .db
                .save_provider(AppType::OpenCode.as_str(), &provider)
                .unwrap_or_else(|err| panic!("seed current omo provider: {err}"));
            state
                .db
                .set_omo_provider_current(AppType::OpenCode.as_str(), &provider.id, "omo")
                .unwrap_or_else(|err| panic!("set current omo provider: {err}"));

            let config_dir = home.join(".config").join("opencode");
            fs::create_dir_all(config_dir.parent().expect("config dir parent"))
                .expect("create .config dir");
            fs::write(&config_dir, "not a directory").expect("block opencode config dir");

            let mut updated = provider.clone();
            updated.name = "Current omo updated".to_string();
            updated.settings_config["agents"]["writer"]["model"] =
                Value::String("omo-saved-model".to_string());

            ProviderService::update(state, AppType::OpenCode, None, updated)
                .expect_err("update should fail when current omo file write fails");

            let saved = state
                .db
                .get_provider_by_id(&provider.id, AppType::OpenCode.as_str())
                .unwrap_or_else(|err| panic!("query current omo provider: {err}"))
                .unwrap_or_else(|| panic!("current omo provider should exist"));

            assert_eq!(saved.name, provider.name);
            assert_eq!(
                saved.settings_config["agents"]["writer"]["model"],
                provider.settings_config["agents"]["writer"]["model"],
                "database should remain unchanged when file write fails"
            );
        });
    }

    #[test]
    #[serial]
    fn update_current_omo_variant_rolls_back_file_when_plugin_sync_fails() {
        with_test_home(|state, home| {
            let provider = opencode_omo_provider("omo-current", "omo");
            state
                .db
                .save_provider(AppType::OpenCode.as_str(), &provider)
                .unwrap_or_else(|err| panic!("seed current omo provider: {err}"));
            state
                .db
                .set_omo_provider_current(AppType::OpenCode.as_str(), &provider.id, "omo")
                .unwrap_or_else(|err| panic!("set current omo provider: {err}"));

            let config_path = omo_config_path(home, "omo");
            fs::create_dir_all(config_path.parent().expect("omo config parent"))
                .expect("create omo config dir");
            let previous_content = serde_json::to_string_pretty(&json!({
                "theme": "legacy-live-theme",
                "agents": {
                    "writer": {
                        "model": "legacy-live-model"
                    }
                },
                "categories": {
                    "default": ["writer"]
                }
            }))
            .expect("serialize previous config");
            fs::write(&config_path, &previous_content).expect("seed previous omo config");

            let opencode_config_path = home.join(".config").join("opencode").join("opencode.json");
            fs::write(&opencode_config_path, "{ invalid json").expect("seed malformed opencode");

            let mut updated = provider.clone();
            updated.name = "Current omo updated".to_string();
            updated.settings_config["agents"]["writer"]["model"] =
                Value::String("omo-saved-model".to_string());
            updated.settings_config["otherFields"]["theme"] =
                Value::String("omo-light".to_string());

            ProviderService::update(state, AppType::OpenCode, None, updated)
                .expect_err("update should fail when plugin sync fails");

            let saved = state
                .db
                .get_provider_by_id(&provider.id, AppType::OpenCode.as_str())
                .unwrap_or_else(|err| panic!("query current omo provider: {err}"))
                .unwrap_or_else(|| panic!("current omo provider should exist"));

            assert_eq!(saved.name, provider.name);
            assert_eq!(
                saved.settings_config["agents"]["writer"]["model"],
                provider.settings_config["agents"]["writer"]["model"],
                "database should remain unchanged when plugin sync fails"
            );

            let written =
                fs::read_to_string(&config_path).expect("read rolled back omo config content");
            assert_eq!(
                written, previous_content,
                "OMO config should roll back to its previous on-disk contents"
            );
        });
    }

    #[test]
    #[serial]
    fn sync_universal_to_apps_preserves_child_metadata() {
        with_test_home(|state, _home| {
            let mut universal = UniversalProvider::new(
                "metadata".into(),
                "Original".into(),
                "custom".into(),
                "https://old.example".into(),
                "old-key".into(),
            );
            universal.apps.claude = true;
            universal.apps.codex = true;
            universal.apps.gemini = true;
            universal.meta = Some(
                serde_json::from_value(json!({
                    "usage_script": {"enabled": false, "language": "javascript", "code": "parent"}
                }))
                .unwrap(),
            );
            state.db.save_universal_provider(&universal).unwrap();
            ProviderService::sync_universal_to_apps(state, &universal.id).unwrap();

            let mut expected = Vec::new();
            for (index, app) in ["claude", "codex", "gemini"].iter().enumerate() {
                let id = format!("universal-{app}-metadata");
                let mut child = state.db.get_provider_by_id(&id, app).unwrap().unwrap();
                assert_eq!(
                    serde_json::to_value(&child.meta).unwrap(),
                    serde_json::to_value(&universal.meta).unwrap()
                );
                child.meta = Some(
                    serde_json::from_value(json!({
                        "usage_script": {"enabled": true, "language": "javascript", "code": app,
                            "apiKey": "usage-only-key", "autoQueryInterval": 15},
                        "commonConfigEnabled": false,
                        "endpointAutoSelect": true
                    }))
                    .unwrap(),
                );
                child.created_at = Some(123 + index as i64);
                child.sort_index = Some(10 + index);
                child.settings_config["local_setting"] = json!(app);
                state.db.save_provider(app, &child).unwrap();
                state
                    .db
                    .add_custom_endpoint(app, &id, "https://extra.example")
                    .unwrap();
                expected.push(child);
            }

            universal.name = "Updated".into();
            universal.base_url = "https://new.example".into();
            universal.api_key = "new-key".into();
            universal.notes = Some("shared note".into());
            universal.models = serde_json::from_value(json!({
                "claude": {"model": "claude-new"},
                "codex": {"model": "codex-new", "reasoningEffort": "low"},
                "gemini": {"model": "gemini-new"}
            }))
            .unwrap();
            // Both absent and present parent metadata must not overwrite child settings.
            for parent_meta in [None, universal.meta.clone()] {
                universal.meta = parent_meta;
                state.db.save_universal_provider(&universal).unwrap();
                ProviderService::sync_universal_to_apps(state, &universal.id).unwrap();
                for (app, before) in ["claude", "codex", "gemini"].iter().zip(&expected) {
                    let after = state
                        .db
                        .get_provider_by_id(&before.id, app)
                        .unwrap()
                        .unwrap();
                    assert_eq!(
                        serde_json::to_value(&after.meta).unwrap(),
                        serde_json::to_value(&before.meta).unwrap(),
                        "{app}"
                    );
                    assert_eq!(after.created_at, before.created_at, "{app}");
                    assert_eq!(after.sort_index, before.sort_index, "{app}");
                    assert_eq!(after.name, "Updated");
                    assert_eq!(after.notes.as_deref(), Some("shared note"));
                    assert_eq!(after.settings_config["local_setting"], json!(app));
                    let generated = match *app {
                        "claude" => universal.to_claude_provider(),
                        "codex" => universal.to_codex_provider(),
                        _ => universal.to_gemini_provider(),
                    }
                    .unwrap();
                    let mut expected_settings = before.settings_config.clone();
                    ProviderService::merge_json(&mut expected_settings, &generated.settings_config);
                    assert_eq!(after.settings_config, expected_settings);
                    assert_eq!(
                        state.db.get_all_providers(app).unwrap()[&before.id]
                            .meta
                            .as_ref()
                            .unwrap()
                            .custom_endpoints
                            .len(),
                        1
                    );
                }
            }
        });
    }
}

impl ProviderService {
    fn managed_codex_oauth_account_id(provider: &Provider) -> Option<String> {
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.managed_account_id_for("codex_oauth"))
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
    }

    /// Preflight before committing current (settings/DB): if the target is a managed Codex official
    /// provider, resolve a valid live config once (which fetches and caches a token over the network).
    /// The resolved config is returned so the later write reuses the same token bundle instead of refreshing twice.
    fn preflight_managed_codex_live(
        state: &AppState,
        app_type: &AppType,
        provider: &Provider,
    ) -> Result<Option<Provider>, AppError> {
        if matches!(app_type, AppType::Codex)
            && Self::managed_codex_oauth_account_id(provider).is_some()
        {
            return build_effective_provider_for_live_with_codex_oauth_manager(
                state.db.as_ref(),
                app_type,
                provider,
                &state.codex_oauth_manager,
            )
            .map(Some);
        }
        Ok(None)
    }

    fn write_preflighted_or_current_live(
        state: &AppState,
        app_type: &AppType,
        provider: &Provider,
        preflighted_provider: Option<&Provider>,
    ) -> Result<(), AppError> {
        if let Some(effective_provider) = preflighted_provider {
            live::write_live_snapshot(app_type, effective_provider)
        } else {
            write_live_with_common_config_for_state(state, app_type, provider)
        }
    }

    fn managed_codex_transaction_error(
        operation: &str,
        error: AppError,
        snapshot: &crate::codex_config::CodexLiveStateSnapshot,
        restore_local_current: Option<(&AppType, Option<&str>)>,
    ) -> AppError {
        let mut rollback_failures = Vec::new();
        if let Some((app_type, previous_local_current)) = restore_local_current {
            if let Err(rollback_error) =
                crate::settings::set_current_provider(app_type, previous_local_current)
            {
                rollback_failures
                    .push(format!("failed to restore local current: {rollback_error}"));
            }
        }
        if let Err(rollback_error) = snapshot.restore_preserving_newer_same_account_auth() {
            rollback_failures.push(rollback_error.to_string());
        }

        if rollback_failures.is_empty() {
            error
        } else {
            AppError::Message(format!(
                "{operation} failed: {error}; rollback also failed: {}",
                rollback_failures.join("; ")
            ))
        }
    }

    fn managed_codex_add_transaction_error(
        state: &AppState,
        operation: &str,
        error: AppError,
        provider: &Provider,
        previous_provider: Option<&Provider>,
        provider_saved: bool,
        snapshot: &crate::codex_config::CodexLiveStateSnapshot,
    ) -> AppError {
        let mut rollback_failures = Vec::new();

        if provider_saved {
            let provider_rollback = match previous_provider {
                Some(previous) => state.db.save_provider(AppType::Codex.as_str(), previous),
                None => state
                    .db
                    .delete_provider(AppType::Codex.as_str(), &provider.id),
            };
            if let Err(rollback_error) = provider_rollback {
                rollback_failures
                    .push(format!("failed to restore provider data: {rollback_error}"));
            }
        }

        if let Err(rollback_error) = snapshot.restore_preserving_newer_same_account_auth() {
            rollback_failures.push(rollback_error.to_string());
        }

        if rollback_failures.is_empty() {
            error
        } else {
            AppError::Message(format!(
                "{operation} failed: {error}; rollback also failed: {}",
                rollback_failures.join("; ")
            ))
        }
    }

    fn managed_codex_takeover_transaction_error(
        state: &AppState,
        operation: &str,
        error: AppError,
        snapshot: &crate::codex_config::CodexLiveStateSnapshot,
        previous_backup: Option<&crate::proxy::types::LiveBackup>,
        restore_local_current: Option<(&AppType, Option<&str>)>,
    ) -> AppError {
        let mut rollback_failures = Vec::new();
        if let Some((app_type, previous_local_current)) = restore_local_current {
            if let Err(rollback_error) =
                crate::settings::set_current_provider(app_type, previous_local_current)
            {
                rollback_failures
                    .push(format!("failed to restore local current: {rollback_error}"));
            }
        }
        let backup_restore = match previous_backup {
            Some(backup) => futures::executor::block_on(
                state
                    .db
                    .save_live_backup(AppType::Codex.as_str(), &backup.original_config),
            ),
            None => {
                futures::executor::block_on(state.db.delete_live_backup(AppType::Codex.as_str()))
            }
        };
        if let Err(rollback_error) = backup_restore {
            rollback_failures.push(format!(
                "failed to restore the Codex live backup: {rollback_error}"
            ));
        }
        if let Err(rollback_error) = snapshot.restore_preserving_newer_same_account_auth() {
            rollback_failures.push(rollback_error.to_string());
        }

        if rollback_failures.is_empty() {
            error
        } else {
            AppError::Message(format!(
                "{operation} failed: {error}; rollback also failed: {}",
                rollback_failures.join("; ")
            ))
        }
    }

    fn outgoing_managed_codex_oauth_account_id(
        app_type: &AppType,
        existing_provider: Option<&Provider>,
        provider: &Provider,
    ) -> Option<String> {
        if !matches!(app_type, AppType::Codex) {
            return None;
        }

        let old_account_id = existing_provider.and_then(Self::managed_codex_oauth_account_id)?;
        if Self::managed_codex_oauth_account_id(provider).as_deref()
            == Some(old_account_id.as_str())
        {
            return None;
        }

        Some(old_account_id)
    }

    fn prepare_outgoing_managed_codex_live_auth(
        state: &AppState,
        account_id: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let Some(account_id) = account_id else {
            return Ok(None);
        };
        live::prepare_codex_managed_oauth_live_auth_switch_away(
            state.codex_oauth_manager.clone(),
            account_id.to_string(),
        )
    }

    fn ensure_outgoing_managed_codex_live_auth_unchanged(
        account_id: Option<&str>,
        expected_refresh_token: Option<&str>,
    ) -> Result<(), AppError> {
        if let (Some(account_id), Some(expected_refresh_token)) =
            (account_id, expected_refresh_token)
        {
            crate::codex_config::ensure_codex_live_auth_unchanged_for_managed_account(
                account_id,
                expected_refresh_token,
            )?;
        }
        Ok(())
    }

    fn clear_outgoing_managed_codex_live_auth(
        account_id: Option<&str>,
        expected_refresh_token: Option<&str>,
    ) -> Result<(), AppError> {
        let Some(account_id) = account_id else {
            return Ok(());
        };
        if let Some(expected_refresh_token) = expected_refresh_token {
            crate::codex_config::clear_codex_live_auth_for_managed_account_if_unchanged(
                account_id,
                Some(expected_refresh_token),
            )
        } else {
            crate::codex_config::clear_codex_live_auth_for_managed_account(account_id)
        }
    }

    fn normalize_provider_if_claude(app_type: &AppType, provider: &mut Provider) {
        if matches!(app_type, AppType::Claude) {
            let mut v = provider.settings_config.clone();
            if normalize_claude_models_in_value(&mut v) {
                provider.settings_config = v;
            }
        }
    }

    /// Check whether a provider exists in live config, tolerating parse errors
    /// only for providers that are explicitly marked as DB-only.
    fn check_live_config_exists(
        app_type: &AppType,
        provider_id: &str,
        live_config_managed: Option<bool>,
    ) -> Result<bool, AppError> {
        if live_config_managed == Some(false) {
            Ok(provider_exists_in_live_config(app_type, provider_id).unwrap_or(false))
        } else {
            provider_exists_in_live_config(app_type, provider_id)
        }
    }

    fn provider_live_config_managed(provider: &Provider) -> Option<bool> {
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.live_config_managed)
    }

    fn set_provider_live_config_managed(provider: &mut Provider, managed: bool) {
        provider
            .meta
            .get_or_insert_with(Default::default)
            .live_config_managed = Some(managed);
    }

    fn normalize_usage_script_credential_overrides(app_type: &AppType, provider: &mut Provider) {
        let current_credentials = provider.resolve_usage_credentials(app_type);

        let Some(usage_script) = provider
            .meta
            .as_mut()
            .and_then(|meta| meta.usage_script.as_mut())
        else {
            return;
        };

        if usage_script.template_type.as_deref() == Some("token_plan") {
            return;
        }

        if usage_script.api_key.as_deref().is_some_and(|api_key| {
            Self::should_clear_usage_api_key_override(api_key, &current_credentials)
        }) {
            usage_script.api_key = None;
        }

        if usage_script.base_url.as_deref().is_some_and(|base_url| {
            Self::should_clear_usage_base_url_override(base_url, &current_credentials)
        }) {
            usage_script.base_url = None;
        }
    }

    fn should_clear_usage_api_key_override(
        script_api_key: &str,
        current_credentials: &(String, String),
    ) -> bool {
        let candidate = script_api_key.trim();
        if candidate.is_empty() {
            return true;
        }

        let matches_provider_key = |api_key: &str| {
            let api_key = api_key.trim();
            !api_key.is_empty() && api_key == candidate
        };

        matches_provider_key(&current_credentials.1)
    }

    fn should_clear_usage_base_url_override(
        script_base_url: &str,
        current_credentials: &(String, String),
    ) -> bool {
        let candidate = Self::normalize_usage_base_url_for_compare(script_base_url);
        if candidate.is_empty() {
            return true;
        }

        let matches_provider_base_url = |base_url: &str| {
            let base_url = Self::normalize_usage_base_url_for_compare(base_url);
            !base_url.is_empty() && base_url == candidate
        };

        matches_provider_base_url(&current_credentials.0)
    }

    fn normalize_usage_base_url_for_compare(base_url: &str) -> String {
        base_url.trim().trim_end_matches('/').to_string()
    }

    /// List all providers for an app type
    pub fn list(
        state: &AppState,
        app_type: AppType,
    ) -> Result<IndexMap<String, Provider>, AppError> {
        if app_type == AppType::Pi {
            return pi::list(state);
        }
        state.db.get_all_providers(app_type.as_str())
    }

    /// Get current provider ID
    ///
    /// Uses the effective current provider ID (its existence is validated).
    /// Prefers the local settings value and, after validation, falls back to the database is_current field.
    /// This lets multiple devices pick providers independently when cloud sync is on, and guarantees a valid ID.
    ///
    /// Additive mode apps (OpenCode, OpenClaw) have no "current provider" concept, so an empty string is returned.
    pub fn current(state: &AppState, app_type: AppType) -> Result<String, AppError> {
        // Additive mode apps have no "current" provider concept
        if app_type.is_additive_mode() {
            return Ok(String::new());
        }
        crate::settings::get_effective_current_provider(&state.db, &app_type)
            .map(|opt| opt.unwrap_or_default())
    }

    /// Add a new provider
    pub fn add(
        state: &AppState,
        app_type: AppType,
        provider: Provider,
        add_to_live: bool,
    ) -> Result<bool, AppError> {
        if app_type == AppType::Pi {
            return pi::add(state, provider, add_to_live);
        }

        let mut provider = provider;
        // Normalize Claude model keys
        Self::normalize_provider_if_claude(&app_type, &mut provider);
        Self::validate_provider_settings(&app_type, &provider)?;
        normalize_provider_common_config_for_storage(state.db.as_ref(), &app_type, &mut provider)?;
        Self::normalize_usage_script_credential_overrides(&app_type, &mut provider);
        if app_type.is_additive_mode() {
            Self::set_provider_live_config_managed(&mut provider, add_to_live);
        }

        let is_managed_codex_add = matches!(app_type, AppType::Codex)
            && Self::managed_codex_oauth_account_id(&provider).is_some();
        let _managed_codex_add_guard = if is_managed_codex_add {
            Some(futures::executor::block_on(
                state.proxy_service.lock_switch_for_app(app_type.as_str()),
            ))
        } else {
            None
        };

        if is_managed_codex_add {
            let effective_current =
                crate::settings::get_effective_current_provider(&state.db, &app_type)?;

            // Adding a non-current managed provider only mutates its DB row. Keep
            // the same switch lock until that row is committed so a waiting switch
            // cannot observe a partially saved binding.
            if effective_current.is_some() {
                state.db.save_provider(app_type.as_str(), &provider)?;
                return Ok(true);
            }

            // For the first managed Codex provider, resolve the complete live
            // bundle before mutating DB state. Then commit Live -> provider row ->
            // current under one switch lock. A failure restores both files and the
            // provider row, avoiding a visible but unusable orphan provider.
            let previous_provider = state
                .db
                .get_provider_by_id(&provider.id, app_type.as_str())?;
            let preflighted_provider =
                Self::preflight_managed_codex_live(state, &app_type, &provider)?;
            let snapshot = crate::codex_config::CodexLiveStateSnapshot::capture()?;
            let mut provider_saved = false;
            let commit_result = (|| {
                Self::write_preflighted_or_current_live(
                    state,
                    &app_type,
                    &provider,
                    preflighted_provider.as_ref(),
                )?;
                state.db.save_provider(app_type.as_str(), &provider)?;
                provider_saved = true;
                state
                    .db
                    .set_current_provider(app_type.as_str(), &provider.id)?;
                Ok::<(), AppError>(())
            })();

            if let Err(error) = commit_result {
                return Err(Self::managed_codex_add_transaction_error(
                    state,
                    "adding the first managed Codex provider",
                    error,
                    &provider,
                    previous_provider.as_ref(),
                    provider_saved,
                    &snapshot,
                ));
            }

            return Ok(true);
        }

        // Save to database
        state.db.save_provider(app_type.as_str(), &provider)?;

        // Additive mode apps (OpenCode, OpenClaw): optionally write to live config.
        if app_type.is_additive_mode() {
            // OMO / OMO Slim providers use exclusive mode and write to dedicated config file.
            if matches!(app_type, AppType::OpenCode)
                && matches!(provider.category.as_deref(), Some("omo") | Some("omo-slim"))
            {
                // Do not auto-enable newly added OMO / OMO Slim providers.
                // Users must explicitly switch/apply an OMO provider to activate it.
                return Ok(true);
            }
            if !add_to_live {
                return Ok(true);
            }
            write_live_with_common_config_for_state(state, &app_type, &provider)?;
            return Ok(true);
        }

        // For other apps: Check if sync is needed (if this is current provider, or no current provider)
        let current = state.db.get_current_provider(app_type.as_str())?;
        if current.is_none() {
            // No current provider, set as current and sync. Managed Codex adds
            // use the transactional path above because token resolution can fail.
            state
                .db
                .set_current_provider(app_type.as_str(), &provider.id)?;
            write_live_with_common_config_for_state(state, &app_type, &provider)?;
        }

        Ok(true)
    }

    /// Update a provider
    pub fn update(
        state: &AppState,
        app_type: AppType,
        original_id: Option<&str>,
        provider: Provider,
    ) -> Result<bool, AppError> {
        if app_type == AppType::Pi {
            return pi::update(state, original_id, provider);
        }

        let mut provider = provider;
        let original_id = original_id.unwrap_or(provider.id.as_str()).to_string();
        let provider_id_changed = original_id != provider.id;
        // Serialize the read/decide/commit window for every Codex update. We do
        // not yet know whether the stored row is managed (the request may be an
        // unbind), so the existing row and effective current must both be read
        // only after this lock is held. Non-managed Codex updates release it
        // before entering the legacy path, whose proxy helpers take the lock
        // themselves.
        let codex_update_switch_guard = if matches!(app_type, AppType::Codex) {
            Some(futures::executor::block_on(
                state.proxy_service.lock_switch_for_app(app_type.as_str()),
            ))
        } else {
            None
        };
        let existing_provider = state
            .db
            .get_provider_by_id(&original_id, app_type.as_str())?;
        // Normalize Claude model keys
        Self::normalize_provider_if_claude(&app_type, &mut provider);
        Self::validate_provider_settings(&app_type, &provider)?;
        normalize_provider_common_config_for_storage(state.db.as_ref(), &app_type, &mut provider)?;
        if matches!(app_type, AppType::Codex) && provider.category.as_deref() == Some("official") {
            crate::codex_config::strip_codex_unified_session_bucket_from_settings(
                &mut provider.settings_config,
            )?;
        }
        Self::normalize_usage_script_credential_overrides(&app_type, &mut provider);

        if provider_id_changed {
            if !app_type.is_additive_mode() {
                return Err(AppError::Message(
                    "Only additive-mode providers support changing provider key".to_string(),
                ));
            }

            let Some(existing_provider) = existing_provider else {
                return Err(AppError::Message(format!(
                    "Original provider '{}' does not exist in app '{}'",
                    original_id,
                    app_type.as_str()
                )));
            };

            // OMO / OMO Slim providers are activated via a dedicated current-state mechanism
            // (set_omo_provider_current) that is NOT captured by provider_exists_in_live_config,
            // which only checks opencode.json. A rename would orphan that current-state marker
            // and silently break subsequent OMO file syncs. Block it unconditionally.
            if matches!(app_type, AppType::OpenCode)
                && matches!(
                    existing_provider.category.as_deref(),
                    Some("omo") | Some("omo-slim")
                )
            {
                return Err(AppError::Message(
                    "Provider key cannot be changed for OMO/OMO Slim providers".to_string(),
                ));
            }

            let original_in_live = Self::check_live_config_exists(
                &app_type,
                &original_id,
                Self::provider_live_config_managed(&existing_provider),
            )?;
            if original_in_live {
                return Err(AppError::Message(
                    "Provider key cannot be changed after the provider has been added to the app config"
                        .to_string(),
                ));
            }

            let next_id_in_live = Self::check_live_config_exists(
                &app_type,
                &provider.id,
                Self::provider_live_config_managed(&existing_provider),
            )?;
            if state
                .db
                .get_provider_by_id(&provider.id, app_type.as_str())?
                .is_some()
                || next_id_in_live
            {
                return Err(AppError::Message(format!(
                    "Provider '{}' already exists in app '{}'",
                    provider.id,
                    app_type.as_str()
                )));
            }

            Self::set_provider_live_config_managed(&mut provider, false);
            state.db.save_provider(app_type.as_str(), &provider)?;
            state.db.delete_provider(app_type.as_str(), &original_id)?;

            if crate::settings::get_current_provider(&app_type).as_deref() == Some(&original_id) {
                crate::settings::set_current_provider(&app_type, Some(provider.id.as_str()))?;
            }

            return Ok(true);
        }

        // Additive mode apps (OpenCode, OpenClaw): only sync to live when the provider
        // already exists in live config. Editing a DB-only provider must not auto-add it.
        if app_type.is_additive_mode() {
            let omo_variant = if matches!(app_type, AppType::OpenCode) {
                match provider.category.as_deref() {
                    Some("omo") => Some(&crate::services::omo::STANDARD),
                    Some("omo-slim") => Some(&crate::services::omo::SLIM),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(variant) = omo_variant {
                let is_current = state.db.is_omo_provider_current(
                    app_type.as_str(),
                    &provider.id,
                    variant.category,
                )?;
                if is_current {
                    crate::services::OmoService::write_provider_config_to_file(&provider, variant)?;
                }
                if let Err(err) = state.db.save_provider(app_type.as_str(), &provider) {
                    if is_current {
                        if let Err(rollback_err) =
                            crate::services::OmoService::write_config_to_file(state, variant)
                        {
                            log::warn!(
                                "Failed to roll back {} config after DB save error: {}",
                                variant.label,
                                rollback_err
                            );
                        }
                    }
                    return Err(err);
                }
                return Ok(true);
            }
            let live_config_managed = Self::check_live_config_exists(
                &app_type,
                &provider.id,
                Self::provider_live_config_managed(&provider).or_else(|| {
                    existing_provider
                        .as_ref()
                        .and_then(Self::provider_live_config_managed)
                }),
            )?;
            Self::set_provider_live_config_managed(&mut provider, live_config_managed);

            // Save to database after live-config presence is resolved so parse errors
            // do not report failure after already mutating DB state.
            state.db.save_provider(app_type.as_str(), &provider)?;

            if !live_config_managed {
                return Ok(true);
            }
            write_live_with_common_config_for_state(state, &app_type, &provider)?;
            return Ok(true);
        }

        // For other apps: Check if this is current provider (use effective current, not just DB)
        let effective_current =
            crate::settings::get_effective_current_provider(&state.db, &app_type)?;
        let is_current = effective_current.as_deref() == Some(provider.id.as_str());

        let existing_managed_codex_account_id = existing_provider
            .as_ref()
            .and_then(Self::managed_codex_oauth_account_id);
        let target_managed_codex_account_id = Self::managed_codex_oauth_account_id(&provider);
        let outgoing_managed_codex_account_id = Self::outgoing_managed_codex_oauth_account_id(
            &app_type,
            existing_provider.as_ref(),
            &provider,
        );
        let managed_codex_update = matches!(app_type, AppType::Codex)
            && (existing_managed_codex_account_id.is_some()
                || target_managed_codex_account_id.is_some());

        if managed_codex_update {
            // A non-current managed row still commits under the same lock: once
            // the row is saved, a waiting switch observes the new binding. If we
            // released first, a switch could activate the old binding and leave
            // DB current/live inconsistent with the subsequent save.
            if !is_current {
                state.db.save_provider(app_type.as_str(), &provider)?;
                return Ok(true);
            }

            let outgoing_live_refresh_token = Self::prepare_outgoing_managed_codex_live_auth(
                state,
                outgoing_managed_codex_account_id.as_deref(),
            )?;

            // The lock acquired before reading existing/current spans the
            // complete direct/takeover transaction. Backup update and takeover
            // Live sync therefore cannot expose a gap to concurrent hot-switch.
            let previous_backup =
                futures::executor::block_on(state.db.get_live_backup(app_type.as_str()))?;
            let has_live_backup = previous_backup.is_some();
            let live_taken_over = state
                .proxy_service
                .detect_takeover_in_live_config_for_app(&app_type);
            let preflighted_provider =
                Self::preflight_managed_codex_live(state, &app_type, &provider)?;
            // Capture after preflight: a legitimate refresh may have advanced
            // auth.json, and rollback must never restore the older generation.
            let snapshot = crate::codex_config::CodexLiveStateSnapshot::capture()?;

            if !has_live_backup && !live_taken_over {
                let commit_result = (|| {
                    Self::ensure_outgoing_managed_codex_live_auth_unchanged(
                        outgoing_managed_codex_account_id.as_deref(),
                        outgoing_live_refresh_token.as_deref(),
                    )?;
                    Self::write_preflighted_or_current_live(
                        state,
                        &app_type,
                        &provider,
                        preflighted_provider.as_ref(),
                    )?;
                    Self::clear_outgoing_managed_codex_live_auth(
                        outgoing_managed_codex_account_id.as_deref(),
                        outgoing_live_refresh_token.as_deref(),
                    )?;
                    state.db.save_provider(app_type.as_str(), &provider)?;
                    Ok::<(), AppError>(())
                })();
                if let Err(error) = commit_result {
                    return Err(Self::managed_codex_transaction_error(
                        "updating the managed Codex provider",
                        error,
                        &snapshot,
                        None,
                    ));
                }

                if let Err(err) = McpService::sync_enabled_for_app(state, &app_type) {
                    log::warn!(
                        "Failed to re-project {app_type:?} MCP after saving the provider (self-heals on the next sync): {err}"
                    );
                }
                return Ok(true);
            }

            let commit_result = (|| {
                Self::ensure_outgoing_managed_codex_live_auth_unchanged(
                    outgoing_managed_codex_account_id.as_deref(),
                    outgoing_live_refresh_token.as_deref(),
                )?;
                futures::executor::block_on(
                    state.proxy_service.update_live_backup_from_provider_inner(
                        app_type.as_str(),
                        &provider,
                        outgoing_managed_codex_account_id.as_deref(),
                    ),
                )
                .map_err(|error| {
                    AppError::Message(format!("Failed to update the live backup: {error}"))
                })?;

                if live_taken_over {
                    futures::executor::block_on(
                        state
                            .proxy_service
                            .sync_codex_live_from_provider_while_proxy_active_guarded(
                                &provider,
                                outgoing_managed_codex_account_id.as_deref(),
                                outgoing_live_refresh_token.as_deref(),
                            ),
                    )
                    .map_err(|error| {
                        AppError::Message(format!("Failed to sync the Codex live config: {error}"))
                    })?;
                } else {
                    // A backup without a takeover marker is a recoverable
                    // half-takeover state. Keep the actual Live bundle aligned
                    // with the edited current provider as well as the backup.
                    Self::ensure_outgoing_managed_codex_live_auth_unchanged(
                        outgoing_managed_codex_account_id.as_deref(),
                        outgoing_live_refresh_token.as_deref(),
                    )?;
                    Self::write_preflighted_or_current_live(
                        state,
                        &app_type,
                        &provider,
                        preflighted_provider.as_ref(),
                    )?;
                }

                Self::clear_outgoing_managed_codex_live_auth(
                    outgoing_managed_codex_account_id.as_deref(),
                    outgoing_live_refresh_token.as_deref(),
                )?;

                // DB is the final commit. Every fallible side effect above can be
                // restored exactly while the previous provider row is untouched.
                state.db.save_provider(app_type.as_str(), &provider)?;
                Ok::<(), AppError>(())
            })();
            if let Err(error) = commit_result {
                return Err(Self::managed_codex_takeover_transaction_error(
                    state,
                    "updating the Codex provider under takeover",
                    error,
                    &snapshot,
                    previous_backup.as_ref(),
                    None,
                ));
            }

            return Ok(true);
        }

        drop(codex_update_switch_guard);

        // Save to database
        state.db.save_provider(app_type.as_str(), &provider)?;

        if is_current {
            let outcome =
                live::sync_live_for_provider_respecting_takeover(state, &app_type, &provider)?;
            if outcome == LiveSyncOutcome::WroteLive {
                // MCP lives in the database and is re-projected after a real
                // live write. Failure remains best-effort and can self-heal.
                if let Err(err) = McpService::sync_enabled_for_app(state, &app_type) {
                    log::warn!(
                        "Failed to re-project {app_type:?} MCP after saving the provider (self-heals on the next sync): {err}"
                    );
                }
            }
        }

        Ok(true)
    }

    /// Delete a provider
    ///
    /// Checks both the local settings and the database current provider, preventing deletion of a provider in use on either side.
    /// For additive mode apps (OpenCode, OpenClaw) any provider can be deleted at any time and is removed from the live config too.
    pub fn delete(state: &AppState, app_type: AppType, id: &str) -> Result<(), AppError> {
        if app_type == AppType::Pi {
            return pi::delete(state, id);
        }

        // Additive mode apps - no current provider concept
        if app_type.is_additive_mode() {
            // Single DB read shared across all additive-mode sub-paths below.
            let existing = state.db.get_provider_by_id(id, app_type.as_str())?;

            if matches!(app_type, AppType::OpenCode) {
                let provider_category = existing.as_ref().and_then(|p| p.category.clone());
                let omo_variant = match provider_category.as_deref() {
                    Some("omo") => Some(&crate::services::omo::STANDARD),
                    Some("omo-slim") => Some(&crate::services::omo::SLIM),
                    _ => None,
                };
                if let Some(variant) = omo_variant {
                    let was_current = state.db.is_omo_provider_current(
                        app_type.as_str(),
                        id,
                        variant.category,
                    )?;
                    state.db.delete_provider(app_type.as_str(), id)?;
                    if was_current {
                        crate::services::OmoService::delete_config_file(variant)?;
                    }
                    return Ok(());
                }
            }

            // Non-OMO path for both OpenCode and OpenClaw:
            // remove from live first (atomicity), then DB.
            //
            // Use check_live_config_exists rather than trusting the flag alone: the flag
            // can be stale (Some(false) for a provider that was written to live before the
            // live_config_managed flip was introduced). check_live_config_exists reads the
            // actual file when the flag is Some(false), so it handles historical data correctly.
            let live_managed = existing
                .as_ref()
                .and_then(Self::provider_live_config_managed);
            if Self::check_live_config_exists(&app_type, id, live_managed)? {
                match app_type {
                    AppType::OpenCode => remove_opencode_provider_from_live(id)?,
                    AppType::OpenClaw => remove_openclaw_provider_from_live(id)?,
                    AppType::Hermes => remove_hermes_provider_from_live(id)?,
                    _ => {}
                }
            }
            state.db.delete_provider(app_type.as_str(), id)?;
            return Ok(());
        }

        // For other apps: Check both local settings and database
        let local_current = crate::settings::get_current_provider(&app_type);
        let db_current = state.db.get_current_provider(app_type.as_str())?;

        if local_current.as_deref() == Some(id) || db_current.as_deref() == Some(id) {
            return Err(AppError::Message(
                "Cannot delete the provider currently in use".to_string(),
            ));
        }

        state.db.delete_provider(app_type.as_str(), id)
    }

    /// Remove provider from live config only (for additive mode apps like OpenCode, OpenClaw)
    ///
    /// Does NOT delete from database - provider remains in the list.
    /// This is used when user wants to "remove" a provider from active config
    /// but keep it available for future use.
    pub fn remove_from_live_config(
        state: &AppState,
        app_type: AppType,
        id: &str,
    ) -> Result<(), AppError> {
        if app_type == AppType::Pi {
            return pi::remove(state, id);
        }

        match app_type {
            AppType::OpenCode => {
                let provider_category = state
                    .db
                    .get_provider_by_id(id, app_type.as_str())?
                    .and_then(|p| p.category);

                let omo_variant = match provider_category.as_deref() {
                    Some("omo") => Some(&crate::services::omo::STANDARD),
                    Some("omo-slim") => Some(&crate::services::omo::SLIM),
                    _ => None,
                };
                if let Some(variant) = omo_variant {
                    state
                        .db
                        .clear_omo_provider_current(app_type.as_str(), id, variant.category)?;
                    let still_has_current = state
                        .db
                        .get_current_omo_provider("opencode", variant.category)?
                        .is_some();
                    if still_has_current {
                        crate::services::OmoService::write_config_to_file(state, variant)?;
                    } else {
                        crate::services::OmoService::delete_config_file(variant)?;
                    }
                } else {
                    remove_opencode_provider_from_live(id)?;
                }
            }
            AppType::OpenClaw => {
                remove_openclaw_provider_from_live(id)?;
            }
            AppType::Hermes => {
                remove_hermes_provider_from_live(id)?;
            }
            _ => {
                return Err(AppError::Message(format!(
                    "App {} does not support remove from live config",
                    app_type.as_str()
                )));
            }
        }

        if let Some(mut provider) = state.db.get_provider_by_id(id, app_type.as_str())? {
            Self::set_provider_live_config_managed(&mut provider, false);
            state.db.save_provider(app_type.as_str(), &provider)?;
        }

        Ok(())
    }

    /// Switch to a provider
    ///
    /// Switch flow:
    /// 1. Validate target provider exists
    /// 2. Check if proxy takeover mode is active AND proxy server is running
    /// 3. If takeover mode active: hot-switch proxy target and refresh proxy-safe Live labels
    /// 4. If normal mode:
    ///    a. **Backfill mechanism**: Backfill current live config to current provider
    ///    b. Update local settings current_provider_xxx (device-level)
    ///    c. Update database is_current (as default for new devices)
    ///    d. Write target provider config to live files
    ///    e. Sync MCP configuration
    pub fn switch(state: &AppState, app_type: AppType, id: &str) -> Result<SwitchResult, AppError> {
        if app_type == AppType::Pi {
            return pi::enable(state, id);
        }

        // Check if provider exists
        let providers = state.db.get_all_providers(app_type.as_str())?;
        let _provider = providers
            .get(id)
            .ok_or_else(|| AppError::Message(format!("Provider {id} not found")))?;

        // OMO providers are switched through their own exclusive path.
        if matches!(app_type, AppType::OpenCode) && _provider.category.as_deref() == Some("omo") {
            return Self::switch_normal(state, app_type, id, &providers);
        }

        // OMO Slim providers are switched through their own exclusive path.
        if matches!(app_type, AppType::OpenCode)
            && _provider.category.as_deref() == Some("omo-slim")
        {
            return Self::switch_normal(state, app_type, id, &providers);
        }

        if matches!(app_type, AppType::ClaudeDesktop) {
            return Self::switch_normal(state, app_type, id, &providers);
        }

        // Provider switches and takeover toggles both mutate live config and the
        // restore backup. Serialize them per app, then decide from the locked
        // current state so a just-started takeover cannot be overwritten by a
        // normal live write.
        let _switch_guard = if app_type.supports_local_proxy() {
            Some(futures::executor::block_on(
                state.proxy_service.lock_switch_for_app(app_type.as_str()),
            ))
        } else {
            None
        };

        // Backup or live placeholders mean the live file is owned by proxy
        // takeover, even if the proxy server is temporarily stopped or is in the
        // activation window before enabled=true is committed.
        let is_app_taken_over =
            futures::executor::block_on(state.db.get_live_backup(app_type.as_str()))
                .ok()
                .flatten()
                .is_some();
        let live_taken_over = state
            .proxy_service
            .detect_takeover_in_live_config_for_app(&app_type);

        let should_hot_switch = is_app_taken_over || live_taken_over;

        // Block switching to unsupported official providers when proxy takeover
        // is active. Codex official account cards use native auth passthrough.
        if should_hot_switch
            && _provider.category.as_deref() == Some("official")
            && !official_provider_supports_proxy_takeover(&app_type, _provider)
        {
            return Err(AppError::localized(
                "switch.official_blocked_by_proxy",
                "Cannot switch to official provider while proxy takeover is active. Using proxy with official APIs may cause account bans.",
            ));
        }

        if should_hot_switch {
            // Proxy takeover mode: hot-switch without restoring upstream Live config.
            // The proxy layer may still refresh proxy-safe Live fields so client labels
            // follow the selected provider while endpoints remain local.
            log::info!(
                "Proxy takeover mode: hot-switching the target provider of {} to {}",
                app_type.as_str(),
                id
            );

            futures::executor::block_on(
                state
                    .proxy_service
                    .hot_switch_provider_inner(app_type.as_str(), id),
            )
            .map_err(|e| AppError::Message(format!("Hot switch failed: {e}")))?;

            // The proxy server will route requests to the new provider via is_current.
            // MCP sync is intentionally skipped while Live config is owned by takeover.
            return Ok(SwitchResult::default());
        }

        // Normal mode: full switch with Live config write
        Self::switch_normal(state, app_type, id, &providers)
    }

    /// Normal switch flow (non-proxy mode)
    fn switch_normal(
        state: &AppState,
        app_type: AppType,
        id: &str,
        providers: &indexmap::IndexMap<String, Provider>,
    ) -> Result<SwitchResult, AppError> {
        let provider = providers
            .get(id)
            .ok_or_else(|| AppError::Message(format!("Provider {id} not found")))?;

        // OMO ↔ OMO Slim are mutually exclusive; activating one removes the other's config file.
        if matches!(app_type, AppType::OpenCode) {
            let omo_pair = match provider.category.as_deref() {
                Some("omo") => Some((&crate::services::omo::STANDARD, &crate::services::omo::SLIM)),
                Some("omo-slim") => {
                    Some((&crate::services::omo::SLIM, &crate::services::omo::STANDARD))
                }
                _ => None,
            };
            if let Some((enable, disable)) = omo_pair {
                state
                    .db
                    .set_omo_provider_current(app_type.as_str(), id, enable.category)?;
                crate::services::OmoService::write_config_to_file(state, enable)?;
                let _ = crate::services::OmoService::delete_config_file(disable);
                return Ok(SwitchResult::default());
            }
        }

        let mut result = SwitchResult::default();

        // Backfill: Backfill current live config to current provider
        // Use effective current provider (validated existence) to ensure backfill targets valid provider
        let current_id = crate::settings::get_effective_current_provider(&state.db, &app_type)?;
        let current_managed_codex_account_id = current_id
            .as_deref()
            .and_then(|current_id| providers.get(current_id))
            .and_then(Self::managed_codex_oauth_account_id);
        let mut backfill_completed = false;
        if let Some(current_id) = current_id {
            if current_id != id {
                // Additive mode apps - all providers coexist in the same file,
                // no backfill needed (backfill is for exclusive mode apps like Claude/Codex/Gemini)
                if !app_type.is_additive_mode() {
                    // Only backfill when switching to a different provider
                    if let Ok(live_config) = read_live_settings(app_type.clone()) {
                        if let Some(mut current_provider) = providers.get(&current_id).cloned() {
                            // Before switching away, sync the shareable live changes (including plugins,
                            // hooks and preferences the user edited directly in the app) into the common
                            // config snippet, then strip and backfill. See sync_common_config_snippet_from_live.
                            Self::sync_common_config_snippet_from_live(
                                state,
                                &app_type,
                                &current_provider,
                                &live_config,
                                &mut result,
                            );

                            current_provider.settings_config =
                                strip_common_config_from_live_settings(
                                    state.db.as_ref(),
                                    &app_type,
                                    &current_provider,
                                    live_config,
                                );
                            if let Err(e) =
                                state.db.save_provider(app_type.as_str(), &current_provider)
                            {
                                log::warn!("Backfill failed: {e}");
                                result
                                    .warnings
                                    .push(format!("backfill_failed:{current_id}"));
                            } else {
                                backfill_completed = true;
                            }
                        }
                    }
                }
            }
        }

        let target_managed_codex_account_id = Self::managed_codex_oauth_account_id(provider);
        let outgoing_managed_codex_account_id = current_managed_codex_account_id
            .as_ref()
            .filter(|account_id| target_managed_codex_account_id.as_ref() != Some(*account_id))
            .cloned();
        let outgoing_live_refresh_token = Self::prepare_outgoing_managed_codex_live_auth(
            state,
            outgoing_managed_codex_account_id.as_deref(),
        )?;

        // Preflight the managed Codex token before committing current (see preflight_managed_codex_live).
        let preflighted_provider = Self::preflight_managed_codex_live(state, &app_type, provider)?;
        let use_managed_codex_transaction = matches!(app_type, AppType::Codex)
            && (current_managed_codex_account_id.is_some()
                || target_managed_codex_account_id.is_some());

        if use_managed_codex_transaction {
            // auth/config/catalog/marker form one logical live commit. Write them
            // before current, then restore the exact four-file snapshot on any
            // failure so native logins and CLI-rotated tokens are not reconstructed
            // from a stale provider row.
            let snapshot = crate::codex_config::CodexLiveStateSnapshot::capture()?;
            let live_result = (|| {
                Self::ensure_outgoing_managed_codex_live_auth_unchanged(
                    outgoing_managed_codex_account_id.as_deref(),
                    outgoing_live_refresh_token.as_deref(),
                )?;
                Self::write_preflighted_or_current_live(
                    state,
                    &app_type,
                    provider,
                    preflighted_provider.as_ref(),
                )?;
                Self::clear_outgoing_managed_codex_live_auth(
                    outgoing_managed_codex_account_id.as_deref(),
                    outgoing_live_refresh_token.as_deref(),
                )?;
                Ok::<(), AppError>(())
            })();
            if let Err(error) = live_result {
                return Err(Self::managed_codex_transaction_error(
                    "writing Codex live",
                    error,
                    &snapshot,
                    None,
                ));
            }

            let previous_local_current = crate::settings::get_current_provider(&app_type);
            if let Err(error) = crate::settings::set_current_provider(&app_type, Some(id)) {
                return Err(Self::managed_codex_transaction_error(
                    "updating local current",
                    error,
                    &snapshot,
                    Some((&app_type, previous_local_current.as_deref())),
                ));
            }
            if let Err(error) = state.db.set_current_provider(app_type.as_str(), id) {
                return Err(Self::managed_codex_transaction_error(
                    "updating database current",
                    error,
                    &snapshot,
                    Some((&app_type, previous_local_current.as_deref())),
                ));
            }
        } else {
            // Codex: validate the live projection before committing current —
            // the write-layer safety gates can refuse the switch, and a
            // refusal after current moved would let the next switch backfill
            // the old live config into the new provider's DB row. (The
            // managed branch above has its own snapshot rollback instead.)
            if matches!(app_type, AppType::Codex) && preflighted_provider.is_none() {
                live::preflight_codex_live_write_for_state(state, provider)?;
            }

            // Additive mode apps skip setting is_current (no such concept).
            if !app_type.is_additive_mode() {
                crate::settings::set_current_provider(&app_type, Some(id))?;
                state.db.set_current_provider(app_type.as_str(), id)?;
            }

            // Sync to live (write_gemini_live handles security flag internally for Gemini).
            Self::write_preflighted_or_current_live(
                state,
                &app_type,
                provider,
                preflighted_provider.as_ref(),
            )?;
        }

        // A material-less official Codex provider gets a config-only live
        // write, which can leave the previous third-party key in
        // ~/.codex/auth.json and strand the user on a 401 with no login
        // screen. Only clean up after a successful backfill — the DB copy
        // made above is what keeps that key recoverable. Failures degrade to
        // a log entry: config.toml and is_current are already committed, so
        // failing the switch here would report a switch that in fact happened.
        if matches!(app_type, AppType::Codex)
            && backfill_completed
            && (provider.category.as_deref() == Some("official")
                || crate::proxy::providers::is_codex_official_provider(provider))
            && target_managed_codex_account_id.is_none()
        {
            let db_auth = provider.settings_config.get("auth");
            match crate::codex_config::clear_stale_codex_live_auth_after_official_switch(
                db_auth.unwrap_or(&serde_json::Value::Null),
            ) {
                Ok(true) => log::info!(
                    "Removed stale third-party auth.json after switching to official Codex provider '{}'",
                    provider.id
                ),
                Ok(false) => {}
                Err(e) => log::warn!("Failed to clean stale Codex auth.json: {e}"),
            }
        }
        // Third-party dual of the block above: with preservation off, the
        // config-only write is expected to delete auth.json. A deletion
        // failure (read-only dir, ACL, file lock) must not fail the switch —
        // config and current are already committed — but the user has to see
        // that the official login is still on disk, so surface it as a
        // switch warning instead of only a log line.
        if matches!(app_type, AppType::Codex)
            && provider.category.as_deref() != Some("official")
            && !crate::proxy::providers::is_codex_official_provider(provider)
            && !crate::settings::preserve_codex_official_auth_on_switch()
            && crate::codex_config::get_codex_auth_path().exists()
        {
            log::warn!("Codex auth.json still present after a preservation-off third-party switch");
            result
                .warnings
                .push("codex_auth_cleanup_failed".to_string());
        }
        // Hermes is additive, so "switching" doesn't overwrite a live config file
        // — we instead update the top-level `model:` section to point at this
        // provider's first declared model. Without this, clicking "switch" would
        // only shuffle entries in custom_providers[] while Hermes keeps using
        // whatever `model.provider` was set before.
        if matches!(app_type, AppType::Hermes) {
            if let Err(e) =
                crate::hermes_config::apply_switch_defaults(&provider.id, &provider.settings_config)
            {
                log::warn!(
                    "Failed to update Hermes model defaults after switching to '{}': {e}",
                    provider.id
                );
                result
                    .warnings
                    .push(format!("hermes_model_defaults_failed:{}", provider.id));
            }
        }

        // For additive-mode providers that were DB-only (live_config_managed == Some(false)),
        // flip the flag to true now that the provider has been successfully written to the live
        // file. This ensures sync_all_providers_to_live() will include it on future syncs.
        //
        // If persisting the marker fails, roll back the just-written live config so we don't leave
        // the provider in a silent inconsistent state (present in live, but still marked DB-only).
        if app_type.is_additive_mode() && Self::provider_live_config_managed(provider) != Some(true)
        {
            let mut updated = provider.clone();
            Self::set_provider_live_config_managed(&mut updated, true);
            if let Err(e) = state.db.save_provider(app_type.as_str(), &updated) {
                let rollback_result = match app_type {
                    AppType::OpenCode => remove_opencode_provider_from_live(&provider.id),
                    AppType::OpenClaw => remove_openclaw_provider_from_live(&provider.id),
                    AppType::Hermes => remove_hermes_provider_from_live(&provider.id),
                    _ => Ok(()),
                };

                match rollback_result {
                    Ok(()) => {
                        return Err(AppError::Message(format!(
                            "Failed to persist live_config_managed for '{}' after writing live config; live changes were rolled back: {e}",
                            provider.id
                        )));
                    }
                    Err(rollback_err) => {
                        return Err(AppError::Message(format!(
                            "Failed to persist live_config_managed for '{}' after writing live config: {e}; additionally failed to roll back live config: {rollback_err}",
                            provider.id
                        )));
                    }
                }
            }
        }

        // The switch rewrote the target app's live, so only that app's MCP is re-projected (Codex keeps
        // [mcp_servers] in the same file as live and must get them back after a wholesale replace; the
        // other apps keep MCP in files independent of live, where projection is idempotent maintenance).
        // Not a full sync_all_enabled: live corruption in an unrelated app (e.g. a broken ~/.claude.json)
        // must not block the switch. By this point DB is_current and live are both persisted, so the
        // switch already succeeded; propagating a projection failure would make the frontend report a
        // bogus "switch failed", hence the downgrade to a warning (MCP projection self-heals).
        if let Err(err) = McpService::sync_enabled_for_app(state, &app_type) {
            log::warn!("Failed to re-project {app_type:?} MCP after the provider switch (self-heals on the next sync): {err}");
        }

        Ok(result)
    }

    /// Sync current provider to live configuration (re-export)
    pub fn sync_current_to_live(state: &AppState) -> Result<(), AppError> {
        sync_current_to_live(state)
    }

    pub fn sync_current_provider_for_app(
        state: &AppState,
        app_type: AppType,
    ) -> Result<(), AppError> {
        if app_type.is_additive_mode() {
            return sync_current_provider_for_app_to_live(state, &app_type);
        }

        let current_id =
            match crate::settings::get_effective_current_provider(&state.db, &app_type)? {
                Some(id) => id,
                None => return Ok(()),
            };

        let providers = state.db.get_all_providers(app_type.as_str())?;
        let Some(provider) = providers.get(&current_id) else {
            return Ok(());
        };

        let outcome = live::sync_live_for_provider_respecting_takeover(state, &app_type, provider)?;
        if outcome == LiveSyncOutcome::BackupOnly {
            return Ok(());
        }

        McpService::sync_enabled_for_app(state, &app_type)
    }

    pub fn migrate_legacy_common_config_usage(
        state: &AppState,
        app_type: AppType,
        legacy_snippet: &str,
    ) -> Result<(), AppError> {
        if app_type.is_additive_mode() || legacy_snippet.trim().is_empty() {
            return Ok(());
        }

        let providers = state.db.get_all_providers(app_type.as_str())?;

        for provider in providers.values() {
            if provider
                .meta
                .as_ref()
                .and_then(|meta| meta.common_config_enabled)
                .is_some()
            {
                continue;
            }

            if !live::provider_uses_common_config(&app_type, provider, Some(legacy_snippet)) {
                continue;
            }

            let mut updated_provider = provider.clone();
            updated_provider
                .meta
                .get_or_insert_with(Default::default)
                .common_config_enabled = Some(true);

            match live::remove_common_config_from_settings(
                &app_type,
                &updated_provider.settings_config,
                legacy_snippet,
            ) {
                Ok(settings) => updated_provider.settings_config = settings,
                Err(err) => {
                    log::warn!(
                        "Failed to normalize legacy common config for {} provider '{}': {err}",
                        app_type.as_str(),
                        updated_provider.id
                    );
                }
            }

            state
                .db
                .save_provider(app_type.as_str(), &updated_provider)?;
        }

        Ok(())
    }

    pub fn migrate_legacy_common_config_usage_if_needed(
        state: &AppState,
        app_type: AppType,
    ) -> Result<(), AppError> {
        if app_type.is_additive_mode() {
            return Ok(());
        }

        let Some(snippet) = state.db.get_config_snippet(app_type.as_str())? else {
            return Ok(());
        };

        if snippet.trim().is_empty() {
            return Ok(());
        }

        Self::migrate_legacy_common_config_usage(state, app_type, &snippet)
    }

    /// Before switching away from a provider, re-extract the shareable part of its live config and
    /// **replace** the common config snippet wholesale, so edits made directly in the app survive the switch.
    ///
    /// "Full re-extract + replace" is used instead of "merge additions only" to cover three cases at once:
    /// - **Additions**: the user installed a plugin, added a hook, or changed env/theme/permission
    ///   preferences directly in the app; they are captured and carry over to other providers;
    /// - **Deletions**: removed keys are absent from the fresh extraction, so they disappear from the
    ///   snippet and are not re-injected on the next switch - otherwise plugins could never be deleted;
    /// - **Secret safety**: the extractor already strips auth / model / endpoint, so secrets never reach the snippet.
    ///
    /// Wholesale replacement is safe because every live write merges the current snippet in, so the live
    /// read at switch-away time is always a superset of "snippet + local edits"; re-extracting only drops
    /// keys the user genuinely deleted and never removes content shared by other providers.
    ///
    /// **Scope**: Claude + Codex. The Codex extractor (`extract_codex_common_config`) already strips
    /// every provider-specific and cc-switch-injected item: `model` / `model_provider` / top-level
    /// `base_url` / the entire `model_providers` table (endpoints and the unified session bucket),
    /// `mcp_servers` (SSOT is the DB table), the top-level `experimental_bearer_token` fallback,
    /// `model_catalog_json`, and the `web_search = "disabled"` sentinel - so neither secrets nor
    /// injected artifacts reach the snippet. Gemini is not covered yet; verify it separately before adding.
    ///
    /// Only applies to providers that **explicitly opted into "write to common config"**
    /// (`meta.common_config_enabled == Some(true)`); skipped when the user **explicitly cleared** the
    /// snippet (`_cleared`), so cleared config is not pushed back. All failures are non-fatal warnings.
    fn sync_common_config_snippet_from_live(
        state: &AppState,
        app_type: &AppType,
        provider: &Provider,
        live_config: &Value,
        result: &mut SwitchResult,
    ) {
        // Scope limited to Claude + Codex (see the function docs).
        if !matches!(app_type, AppType::Claude | AppType::Codex) {
            return;
        }

        let opted_in = provider
            .meta
            .as_ref()
            .and_then(|meta| meta.common_config_enabled)
            == Some(true);
        if !opted_in {
            return;
        }

        match state.db.is_config_snippet_cleared(app_type.as_str()) {
            Ok(true) => return, // user explicitly cleared the common config; respect that and do not push it back
            Ok(false) => {}
            Err(err) => {
                log::warn!(
                    "Failed to read common config cleared flag for {}: {err}",
                    app_type.as_str()
                );
                return;
            }
        }

        let new_snippet = match Self::extract_common_config_snippet_from_settings(
            app_type.clone(),
            live_config,
        ) {
            Ok(snippet) => snippet,
            Err(err) => {
                log::warn!(
                    "Failed to extract common config from live for {} provider '{}': {err}",
                    app_type.as_str(),
                    provider.id
                );
                return;
            }
        };

        // Skip when unchanged to avoid a pointless DB write (the normal path when the live config is not switched).
        let current = state
            .db
            .get_config_snippet(app_type.as_str())
            .ok()
            .flatten();
        if current.as_deref() == Some(new_snippet.as_str()) {
            return;
        }

        if let Err(err) = state
            .db
            .set_config_snippet(app_type.as_str(), Some(new_snippet))
        {
            log::warn!(
                "Failed to persist synced common config for {} provider '{}': {err}",
                app_type.as_str(),
                provider.id
            );
            result
                .warnings
                .push(format!("common_config_sync_failed:{}", provider.id));
        }
    }

    /// Extract common config snippet from current provider
    ///
    /// Extracts the current provider's configuration and removes provider-specific fields
    /// (API keys, model settings, endpoints) to create a reusable common config snippet.
    pub fn extract_common_config_snippet(
        state: &AppState,
        app_type: AppType,
    ) -> Result<String, AppError> {
        // Get current provider
        let current_id = Self::current(state, app_type.clone())?;
        if current_id.is_empty() {
            return Err(AppError::Message("No current provider".to_string()));
        }

        let providers = state.db.get_all_providers(app_type.as_str())?;
        let provider = providers
            .get(&current_id)
            .ok_or_else(|| AppError::Message(format!("Provider {current_id} not found")))?;

        match app_type {
            AppType::Claude => Self::extract_claude_common_config(&provider.settings_config),
            AppType::ClaudeDesktop => Ok(String::new()),
            AppType::Codex => Self::extract_codex_common_config(&provider.settings_config),
            AppType::Gemini => Self::extract_gemini_common_config(&provider.settings_config),
            AppType::GrokBuild => Ok(String::new()),
            AppType::OpenCode => Self::extract_opencode_common_config(&provider.settings_config),
            AppType::OpenClaw => Self::extract_openclaw_common_config(&provider.settings_config),
            AppType::Hermes => Ok(String::new()), // Hermes doesn't use common config snippets
            AppType::Pi => Ok(String::new()),
        }
    }

    /// Extract common config snippet from a config value (e.g. editor content).
    pub fn extract_common_config_snippet_from_settings(
        app_type: AppType,
        settings_config: &Value,
    ) -> Result<String, AppError> {
        match app_type {
            AppType::Claude => Self::extract_claude_common_config(settings_config),
            AppType::ClaudeDesktop => Ok(String::new()),
            AppType::Codex => Self::extract_codex_common_config(settings_config),
            AppType::Gemini => Self::extract_gemini_common_config(settings_config),
            AppType::GrokBuild => Ok(String::new()),
            AppType::OpenCode => Self::extract_opencode_common_config(settings_config),
            AppType::OpenClaw => Self::extract_openclaw_common_config(settings_config),
            AppType::Hermes => Ok(String::new()), // Hermes doesn't use common config snippets
            AppType::Pi => Ok(String::new()),
        }
    }

    /// Decide whether an env / top-level config key name is a credential or secret: any hit must never
    /// be written into the shared common config snippet. **Deliberately strict** - stripping one extra
    /// non-secret key only means it is not shared (a recoverable nuisance), while missing one credential
    /// injects a secret into every provider (an unrecoverable leak).
    ///
    /// Pattern matching covers whole classes instead of enumerating names (an enumeration always misses
    /// the next `*_API_KEY`): `*_API_KEY` for Anthropic / OpenRouter / Google / OpenAI / Gemini and
    /// friends (Claude provider credentials in `Provider::resolve_usage_credentials` really do fall back
    /// to `OPENROUTER_API_KEY` / `GOOGLE_API_KEY`), all kinds of `*_AUTH_TOKEN` / singular `*_TOKEN`,
    /// AWS Bedrock / Vertex credentials, and generic secret / password / private-key naming.
    pub(crate) fn is_sensitive_config_key(name: &str) -> bool {
        let upper = name.to_ascii_uppercase();

        // Singular `_TOKEN` catches AWS_SESSION_TOKEN and friends but must **not** hit the plural `_TOKENS`
        // (CLAUDE_CODE_MAX_OUTPUT_TOKENS / MAX_THINKING_TOKENS are normal shareable config).
        const SENSITIVE_SUFFIXES: &[&str] = &[
            // Bare `_KEY` is the most common credential form (OPENAI_KEY / GROQ_KEY / XAI_KEY...) and must
            // be listed on its own: enumerating only subtypes like `_API_KEY` / `_ACCESS_KEY` would leave
            // the plainest one out. The `_*_KEY` entries below are implied by it, kept to document coverage.
            "_KEY",
            "_API_KEY",
            "_ACCESS_KEY",
            "_ACCESS_KEY_ID",
            "_KEY_ID",
            "_PRIVATE_KEY",
            // Compound forms without a separator need their own suffixes: `_KEY` does not reach `..._APIKEY`
            // (the fourth character from the end is I, not an underscore). VOLC_ACCESSKEY is the official
            // variable name in the Volcengine docs, and this repo implements Volcengine AK/SK usage queries.
            "_APIKEY",
            "_ACCESSKEY",
            "_SECRETKEY",
            "_APITOKEN",
            "_AUTH_TOKEN",
            "_TOKEN",
            // The idiomatic form of personal access tokens such as GITHUB_PAT / GITLAB_PAT, which contain
            // neither TOKEN nor KEY, so none of the rules above reach them.
            "_PAT",
            // Common password abbreviations. `_PASS` does not hit `*_BYPASS` (that one ends in `_BYPASS`),
            // and `_PWD` does not hit the shell's PWD / OLDPWD.
            "_PWD",
            "_PASS",
            "_PASSPHRASE",
            "_CREDS",
        ];
        const SENSITIVE_EXACT: &[&str] = &[
            "APIKEY",
            "API_KEY",
            "TOKEN",
            "SECRET",
            "PASSWORD",
            "CREDENTIALS",
        ];
        // contains: covers variants such as AWS_SECRET_ACCESS_KEY / *_CLIENT_SECRET /
        // GOOGLE_APPLICATION_CREDENTIALS / AWS_BEARER_TOKEN_BEDROCK.
        const SENSITIVE_CONTAINS: &[&str] = &[
            "SECRET",
            "PASSWORD",
            "PASSWD",
            "CREDENTIAL",
            "PRIVATE_KEY",
            "BEARER_TOKEN",
        ];

        SENSITIVE_EXACT.contains(&upper.as_str())
            || SENSITIVE_SUFFIXES.iter().any(|s| upper.ends_with(s))
            || SENSITIVE_CONTAINS.iter().any(|c| upper.contains(c))
    }

    /// Extract common config for Claude (JSON format)
    fn extract_claude_common_config(settings: &Value) -> Result<String, AppError> {
        let mut config = settings.clone();

        // Provider-specific **non-secret** fields (models + endpoint) that must not be shared. Credentials
        // are not listed here; `is_sensitive_config_key` (pattern matching) strips them, so a new provider's
        // `*_API_KEY` is covered without maintaining this list by hand.
        const ENV_PROVIDER_SPECIFIC_EXCLUDES: &[&str] = &[
            "ANTHROPIC_MODEL",
            "ANTHROPIC_REASONING_MODEL", // legacy: deprecated, but old configs may still carry it
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
            // Fable is the fourth model mapping tier added in v3.16.3; like haiku/sonnet/opus it is
            // provider-specific and must not enter the common config snippet (issue #4272).
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
            "CLAUDE_CODE_SUBAGENT_MODEL",
            // Context limits follow the actual upstream model. Sharing these
            // across providers can cap GPT/Kimi to the wrong window and make
            // Claude Code compact too early or miss the upstream limit.
            "CLAUDE_CODE_MAX_CONTEXT_TOKENS",
            "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
            "ANTHROPIC_BASE_URL",
        ];

        const TOP_LEVEL_EXCLUDES: &[&str] = &[
            "apiBaseUrl",
            // Legacy model fields
            "primaryModel",
            "smallFastModel",
        ];

        // Remove env fields: provider-specific (models/endpoint) + any credential key.
        if let Some(env) = config.get_mut("env").and_then(|v| v.as_object_mut()) {
            let sensitive: Vec<String> = env
                .keys()
                .filter(|k| Self::is_sensitive_config_key(k))
                .cloned()
                .collect();
            for key in ENV_PROVIDER_SPECIFIC_EXCLUDES {
                env.remove(*key);
            }
            for key in &sensitive {
                env.remove(key);
            }
            // If env is empty after removal, remove the env object itself
            if env.is_empty() {
                config.as_object_mut().map(|obj| obj.remove("env"));
            }
        }

        // Remove top-level fields: legacy model fields + any credential key
        // (for example a non-standard top-level apiKey / api_key / *_TOKEN).
        if let Some(obj) = config.as_object_mut() {
            let sensitive: Vec<String> = obj
                .keys()
                .filter(|k| Self::is_sensitive_config_key(k))
                .cloned()
                .collect();
            for key in TOP_LEVEL_EXCLUDES {
                obj.remove(*key);
            }
            for key in &sensitive {
                obj.remove(key);
            }
        }

        // Check if result is empty
        if config.as_object().is_none_or(|obj| obj.is_empty()) {
            return Ok("{}".to_string());
        }

        serde_json::to_string_pretty(&config)
            .map_err(|e| AppError::Message(format!("Serialization failed: {e}")))
    }

    /// Extract common config for Codex (TOML format)
    fn extract_codex_common_config(settings: &Value) -> Result<String, AppError> {
        // Codex config is stored as { "auth": {...}, "config": "toml string" }
        let config_toml = settings
            .get("config")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if config_toml.is_empty() {
            return Ok(String::new());
        }

        let mut doc = config_toml
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| AppError::Message(format!("TOML parse error: {e}")))?;

        // Remove provider-specific fields.
        let root = doc.as_table_mut();
        root.remove("model");
        root.remove("model_provider");
        // Legacy/alt formats might use a top-level base_url.
        root.remove("base_url");
        // wire_api carries the same provider routing semantics as base_url: without a model_provider,
        // update_codex_toml_field and the frontend setCodexWireApi both land it at the top level, and in
        // the snippet it would rewrite other providers' protocol choice (chat vs responses).
        root.remove("wire_api");

        // Remove entire model_providers table (provider-specific configuration)
        root.remove("model_providers");

        // MCP servers are owned by the DB mcp_servers table: in the shared snippet they would bypass the
        // per-app enabled state, get merged into every provider that opted into the common config, and
        // show up as a "duplicate" MCP config in the common config editor.
        root.remove("mcp_servers");
        // The legacy wrong format [mcp.servers] is stripped as well (consistent with
        // strip_codex_mcp_servers_from_settings): sync_all_enabled only manages [mcp_servers.*], so once
        // the legacy form enters the snippet it is merged into every provider with no path that clears it.
        if let Some(mcp_tbl) = root
            .get_mut("mcp")
            .and_then(|item| item.as_table_like_mut())
        {
            mcp_tbl.remove("servers");
            if mcp_tbl.is_empty() {
                root.remove("mcp");
            }
        }

        // Artifacts cc-switch injects when writing live never enter the shared snippet:
        // - experimental_bearer_token normally lives inside [model_providers.<id>] (the whole table is
        //   stripped above), but the three fallbacks (no active route / built-in reserved id / missing
        //   route table) land it at the top level - not stripping it writes an API key into the snippet.
        root.remove("experimental_bearer_token");
        // - model_catalog_json points at the per-provider generated catalog projection file (DB is SSOT).
        root.remove("model_catalog_json");
        // - For web_search only the "disabled" sentinel injected by cc-switch is stripped; other values
        //   set by the user are shareable preferences and are kept.
        if root
            .get(crate::codex_config::CODEX_WEB_SEARCH_FIELD)
            .and_then(|item| item.as_str())
            == Some(crate::codex_config::CODEX_WEB_SEARCH_DISABLED)
        {
            root.remove(crate::codex_config::CODEX_WEB_SEARCH_FIELD);
        }

        // Clean up multiple empty lines (keep at most one blank line).
        let mut cleaned = String::new();
        let mut blank_run = 0usize;
        for line in doc.to_string().lines() {
            if line.trim().is_empty() {
                blank_run += 1;
                if blank_run <= 1 {
                    cleaned.push('\n');
                }
                continue;
            }
            blank_run = 0;
            cleaned.push_str(line);
            cleaned.push('\n');
        }

        Ok(cleaned.trim().to_string())
    }

    /// Extract common config for Gemini (JSON format)
    ///
    /// Extracts `.env` values while excluding provider-specific credentials:
    /// - GOOGLE_GEMINI_BASE_URL
    /// - GEMINI_API_KEY
    fn extract_gemini_common_config(settings: &Value) -> Result<String, AppError> {
        let env = settings.get("env").and_then(|v| v.as_object());

        let mut snippet = serde_json::Map::new();
        if let Some(env) = env {
            for (key, value) in env {
                // The endpoint is stripped by name (it is not a credential, so pattern matching misses it);
                // credentials all go through `is_sensitive_config_key` (same as the Claude extractor).
                // A fixed list would miss the next `*_API_KEY` - for example `GOOGLE_API_KEY` (a first-class
                // Gemini credential recognized by provider.rs) - and the shared snippet is deep-merged back
                // into other Gemini providers, so a missed strip writes account A's key into provider B and
                // sends it to B's base_url. `GEMINI_API_KEY` needs no entry: the `_KEY` suffix covers it.
                if key == "GOOGLE_GEMINI_BASE_URL" || Self::is_sensitive_config_key(key) {
                    continue;
                }
                let Value::String(v) = value else {
                    continue;
                };
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    snippet.insert(key.to_string(), Value::String(trimmed.to_string()));
                }
            }
        }

        if snippet.is_empty() {
            return Ok("{}".to_string());
        }

        serde_json::to_string_pretty(&Value::Object(snippet))
            .map_err(|e| AppError::Message(format!("Serialization failed: {e}")))
    }

    /// One-off cleanup: erase credentials historically leaked into the Gemini shared snippet everywhere.
    ///
    /// Background: `extract_gemini_common_config` used to strip only two fixed key names, so first-class
    /// credentials such as `GOOGLE_API_KEY` entered the shared snippet and were deep-merged by
    /// `apply_common_config_to_settings` into the env of **other** Gemini providers and sent to their base_url.
    ///
    /// Fixing the extractor is not enough: once a Gemini snippet exists it is **never re-extracted**
    /// automatically (startup auto-extract and post-import extraction both require `snippet.is_none()`,
    /// and the switch-time write-back only applies to Claude / Codex), so existing snippets keep injecting.
    ///
    /// Two key constraints:
    ///
    /// 1. **Clearing the snippet alone is not enough**. Merge and strip are a pair that cancels out by
    ///    strict value equality: on switch-away `remove_common_config_from_settings` deletes the injected
    ///    keys based on the snippet content. Once the key is gone from the snippet, backfill writes the
    ///    residual secret from live straight into the victim's `settings_config` - turning transient
    ///    pollution into permanent pollution. Snippet, provider configs and live files must all be cleaned.
    /// 2. **Delete by value equality, not by key name across the board**. Reusing
    ///    `remove_common_config_from_settings` clears only the spread copy and keeps a provider's own key of the same name with a different value.
    ///
    /// The step order is itself a safety property: **clearing the snippet must come last**. The snippet is
    /// the only source of "which keys to strip" for `remove_common_config_from_settings`, so once it is
    /// empty, any residue (in live files, or for the next retry) can never be recognized and stripped
    /// again. Every step that can fail comes first and returns the error, so the next startup can redo it.
    ///
    /// After the cleanup some providers show a missing API key and the user must refill it - that is
    /// correct: the key never belonged to them. (A victim's own key of the same name was overwritten at
    /// merge time and cannot be recovered.) Before touching anything, an audit record is written to the
    /// `gemini_common_config_scrub_audit_v1` setting containing **key names and affected provider ids,
    /// never values**: `settings` is uploaded by WebDAV/S3 sync, and what we handle here is exactly the
    /// credential that must be destroyed - keeping the value would turn one erasure into a cross-device, UI-less, never-expiring plaintext copy.
    pub async fn scrub_leaked_gemini_common_config(state: &AppState) -> Result<(), AppError> {
        const FLAG: &str = "gemini_common_config_credentials_scrubbed_v1";
        const AUDIT_KEY: &str = "gemini_common_config_scrub_audit_v1";
        let app = AppType::Gemini;

        if state.db.get_bool_flag(FLAG).unwrap_or(false) {
            return Ok(());
        }

        let Some(snippet_text) = state.db.get_config_snippet(app.as_str())? else {
            state.db.set_setting(FLAG, "true")?;
            return Ok(());
        };

        // If the snippet cannot be parsed, leave it alone and only mark completion - mangling user data is worse
        let Ok(Value::Object(entries)) = serde_json::from_str::<Value>(&snippet_text) else {
            state.db.set_setting(FLAG, "true")?;
            return Ok(());
        };

        let mut poison = serde_json::Map::new();
        let mut clean = serde_json::Map::new();
        for (key, value) in entries {
            if Self::is_sensitive_config_key(&key) {
                poison.insert(key, value);
            } else {
                clean.insert(key, value);
            }
        }

        if poison.is_empty() {
            state.db.set_setting(FLAG, "true")?;
            return Ok(());
        }

        log::warn!(
            "Detected {} credential keys left in the Gemini common config snippet, starting the one-off cleanup",
            poison.len()
        );

        let poison_keys: Vec<String> = poison.keys().cloned().collect();
        let poison_value = Value::Object(poison);
        let poison_text = serde_json::to_string(&poison_value)
            .map_err(|e| AppError::Message(format!("Serialization failed: {e}")))?;

        // 1) Compute the cleaned config of every provider first, but **do not persist yet**
        let providers = state.db.get_all_providers(app.as_str())?;
        let mut pending: Vec<(String, Provider, Value)> = Vec::new();
        for (id, provider) in providers {
            let cleaned = match live::remove_common_config_from_settings(
                &app,
                &provider.settings_config,
                &poison_text,
            ) {
                Ok(cleaned) => cleaned,
                Err(err) => {
                    log::warn!("Failed to clean leaked credentials of provider '{id}': {err}");
                    continue;
                }
            };
            if cleaned != provider.settings_config {
                pending.push((id, provider, cleaned));
            }
        }

        // 2) Leave an audit record before persisting: **key names and affected providers only, never values**.
        //
        //    "Targeted deletion by value equality" also fires in one legitimate case: the user intentionally
        //    reuses the same key across providers. So we must record "what was deleted and from where",
        //    otherwise the user can only dig through logs. But not the value - the `settings` table is not
        //    in `SYNC_SKIP_TABLES` and is uploaded by WebDAV/S3 sync, while what we handle here is exactly
        //    the leaked credential that must be destroyed: keeping the value turns one erasure into a
        //    UI-less, never-expiring, cross-device plaintext copy. Keys should be rotated anyway.
        let removed_env_keys = |before: &Value, after: &Value| -> Vec<String> {
            let before_env = before.get("env").and_then(Value::as_object);
            let after_env = after.get("env").and_then(Value::as_object);
            match (before_env, after_env) {
                (Some(before_env), Some(after_env)) => before_env
                    .keys()
                    .filter(|key| !after_env.contains_key(*key))
                    .cloned()
                    .collect(),
                (Some(before_env), None) => before_env.keys().cloned().collect(),
                _ => Vec::new(),
            }
        };
        let audit = serde_json::json!({
            "removedFromSnippet": poison_keys,
            "providers": pending
                .iter()
                .map(|(id, provider, cleaned)| serde_json::json!({
                    "id": id,
                    "removedKeys": removed_env_keys(&provider.settings_config, cleaned),
                }))
                .collect::<Vec<_>>(),
        });
        let audit_text = serde_json::to_string(&audit)
            .map_err(|e| AppError::Message(format!("Serialization failed: {e}")))?;
        // Write only when no record exists. Provider writes are not one transaction (each save_provider
        // commits separately), so a previous round may have aborted halfway; the completion flag was not
        // set then, the next startup reruns, and the "original state" it sees is already partial. An
        // unconditional INSERT OR REPLACE would let that partial record overwrite the complete first one.
        if state.db.get_setting(AUDIT_KEY)?.is_none() {
            state.db.set_setting(AUDIT_KEY, &audit_text)?;
        }

        // 3) Per-provider settings_config: delete the spread copies by value equality
        for (id, provider, cleaned) in pending {
            let mut updated = provider;
            updated.settings_config = cleaned;
            state.db.save_provider(app.as_str(), &updated)?;
            log::info!("Cleared the leaked shared credential from Gemini provider '{id}'");
        }

        // 4) The live snapshot of an active proxy takeover may hold a copy too. A failure here **must propagate**:
        //
        //    when the proxy stops, `restore_live_config_for_app_with_fallback_inner` (proxy.rs:869) writes
        //    that snapshot back to `~/.gemini/.env` verbatim. If it is still poisoned while we clear the
        //    snippet and set the completion flag, the credential resurrects the moment the proxy stops, and
        //    the one-off flag guarantees no second cleanup; the key is then gone from the snippet, so the
        //    next switch backfills it permanently into the victim's config - the same order trap as above.
        //
        //    Returning the error is the safe failure mode: the caller (lib.rs:1189) only warns and does not
        //    abort startup, snippet and flag stay as they are, and the next startup redoes it.
        if let Some(backup) = state.db.get_live_backup(app.as_str()).await? {
            let original: Value = serde_json::from_str(&backup.original_config).map_err(|e| {
                AppError::Message(format!(
                    "Failed to parse the Gemini proxy takeover backup: {e}"
                ))
            })?;
            let cleaned = live::remove_common_config_from_settings(&app, &original, &poison_text)?;
            if cleaned != original {
                let text = serde_json::to_string(&cleaned)
                    .map_err(|e| AppError::Message(format!("Serialization failed: {e}")))?;
                state.db.save_live_backup(app.as_str(), &text).await?;
                log::info!(
                    "Cleared the leaked shared credential from the Gemini proxy takeover backup"
                );
            }
        }

        // 5) `~/.gemini/.env`: a **targeted** delete that must happen before clearing the snippet, aborting on failure.
        //
        //    Why not re-project with `sync_current_provider_for_app`: with no current provider it just
        //    returns Ok without writing the file, so the leaked value stays in live; once the snippet is
        //    cleared, the next switch makes `remove_common_config_from_settings` blind to that key and
        //    backfill writes it permanently into the victim's config - exactly the order trap described at
        //    the top, turned from "unfixed" into "half-fixed and worse". A targeted delete also preserves
        //    manual env entries that only exist in live and belong to no provider (re-projection wipes them).
        //
        //    The delete uses the **order-preserving** implementation in `remove_gemini_env_entries` rather
        //    than a read -> HashMap -> write round trip: the latter would also wipe comments, blank lines
        //    and unrecognized lines and re-sort the whole file by key. That is fine for a full projection,
        //    but this is a startup cleanup the user never triggered and must not rewrite unrelated content.
        //
        //    On failure it returns the error: the snippet still holds the poisoned key, the completion flag
        //    is unset, and the next startup can redo it. Clearing the snippet is irreversible and comes last.
        let poison_env: HashMap<String, String> = poison_value
            .as_object()
            .map(|map| {
                map.iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|text| (key.clone(), text.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if crate::gemini_config::remove_gemini_env_entries(&poison_env)? {
            log::info!("Cleared the leaked shared credential from ~/.gemini/.env");
        }

        // 6) The snippet itself: keep the shareable part. When everything is gone, delete the row instead
        //    of writing "{}" - an empty row would make should_auto_extract_config_snippet false forever and
        //    the user's legitimate shared config could never be rebuilt. For the same reason never set cleared.
        if clean.is_empty() {
            state.db.set_config_snippet(app.as_str(), None)?;
        } else {
            let cleaned_snippet = serde_json::to_string_pretty(&Value::Object(clean))
                .map_err(|e| AppError::Message(format!("Serialization failed: {e}")))?;
            state
                .db
                .set_config_snippet(app.as_str(), Some(cleaned_snippet))?;
        }

        state.db.set_setting(FLAG, "true")?;
        log::info!("Gemini common config credential cleanup finished");
        Ok(())
    }

    /// Extract common config for OpenCode (JSON format)
    fn extract_opencode_common_config(settings: &Value) -> Result<String, AppError> {
        // OpenCode uses a different config structure with npm, options, models
        // For common config, we exclude provider-specific fields like apiKey
        let mut config = settings.clone();

        // Remove provider-specific fields
        if let Some(obj) = config.as_object_mut() {
            if let Some(options) = obj.get_mut("options").and_then(|v| v.as_object_mut()) {
                options.remove("apiKey");
                options.remove("baseURL");
            }
            // Keep npm and models as they might be common
        }

        if config.is_null() || (config.is_object() && config.as_object().unwrap().is_empty()) {
            return Ok("{}".to_string());
        }

        serde_json::to_string_pretty(&config)
            .map_err(|e| AppError::Message(format!("Serialization failed: {e}")))
    }

    /// Extract common config for OpenClaw (JSON format)
    fn extract_openclaw_common_config(settings: &Value) -> Result<String, AppError> {
        // OpenClaw uses a different config structure with baseUrl, apiKey, api, models
        // For common config, we exclude provider-specific fields like apiKey
        let mut config = settings.clone();

        // Remove provider-specific fields
        if let Some(obj) = config.as_object_mut() {
            obj.remove("apiKey");
            obj.remove("baseUrl");
            // Keep api and models as they might be common
        }

        if config.is_null() || (config.is_object() && config.as_object().unwrap().is_empty()) {
            return Ok("{}".to_string());
        }

        serde_json::to_string_pretty(&config)
            .map_err(|e| AppError::Message(format!("Serialization failed: {e}")))
    }

    /// Import default configuration from live files (re-export)
    ///
    /// Returns `Ok(true)` if imported, `Ok(false)` if skipped.
    pub fn import_default_config(state: &AppState, app_type: AppType) -> Result<bool, AppError> {
        import_default_config(state, app_type)
    }

    pub fn should_import_default_config_on_startup(
        state: &AppState,
        app_type: &AppType,
    ) -> Result<bool, AppError> {
        should_import_default_config_on_startup(state, app_type)
    }

    /// Read current live settings (re-export)
    pub fn read_live_settings(app_type: AppType) -> Result<Value, AppError> {
        read_live_settings(app_type)
    }

    /// Get custom endpoints list (re-export)
    pub fn get_custom_endpoints(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
    ) -> Result<Vec<CustomEndpoint>, AppError> {
        endpoints::get_custom_endpoints(state, app_type, provider_id)
    }

    /// Add custom endpoint (re-export)
    pub fn add_custom_endpoint(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
        url: String,
    ) -> Result<(), AppError> {
        endpoints::add_custom_endpoint(state, app_type, provider_id, url)
    }

    /// Remove custom endpoint (re-export)
    pub fn remove_custom_endpoint(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
        url: String,
    ) -> Result<(), AppError> {
        endpoints::remove_custom_endpoint(state, app_type, provider_id, url)
    }

    /// Update endpoint last used timestamp (re-export)
    pub fn update_endpoint_last_used(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
        url: String,
    ) -> Result<(), AppError> {
        endpoints::update_endpoint_last_used(state, app_type, provider_id, url)
    }

    /// Update provider sort order
    pub fn update_sort_order(
        state: &AppState,
        app_type: AppType,
        updates: Vec<ProviderSortUpdate>,
    ) -> Result<bool, AppError> {
        let mut providers = state.db.get_all_providers(app_type.as_str())?;

        for update in updates {
            if let Some(provider) = providers.get_mut(&update.id) {
                provider.sort_index = Some(update.sort_index);
                state.db.save_provider(app_type.as_str(), provider)?;
            }
        }

        Ok(true)
    }

    /// Query provider usage (re-export)
    pub async fn query_usage(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
    ) -> Result<UsageResult, AppError> {
        usage::query_usage(state, app_type, provider_id).await
    }

    /// Test usage script (re-export)
    #[allow(clippy::too_many_arguments)]
    pub async fn test_usage_script(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
        script_code: &str,
        timeout: u64,
        api_key: Option<&str>,
        base_url: Option<&str>,
        access_token: Option<&str>,
        user_id: Option<&str>,
        template_type: Option<&str>,
    ) -> Result<UsageResult, AppError> {
        usage::test_usage_script(
            state,
            app_type,
            provider_id,
            script_code,
            timeout,
            api_key,
            base_url,
            access_token,
            user_id,
            template_type,
        )
        .await
    }

    pub(crate) fn write_gemini_live(provider: &Provider) -> Result<(), AppError> {
        write_gemini_live(provider)
    }

    fn validate_provider_settings(app_type: &AppType, provider: &Provider) -> Result<(), AppError> {
        match app_type {
            AppType::Claude => {
                if !provider.settings_config.is_object() {
                    return Err(AppError::localized(
                        "provider.claude.settings.not_object",
                        "Claude configuration must be a JSON object",
                    ));
                }
            }
            AppType::ClaudeDesktop => {
                crate::claude_desktop_config::validate_provider(provider)?;
            }
            AppType::Codex => {
                let settings = provider.settings_config.as_object().ok_or_else(|| {
                    AppError::localized(
                        "provider.codex.settings.not_object",
                        "Codex configuration must be a JSON object",
                    )
                })?;

                let auth = settings.get("auth").ok_or_else(|| {
                    AppError::localized(
                        "provider.codex.auth.missing",
                        format!("Provider {} is missing auth configuration", provider.id),
                    )
                })?;
                if !auth.is_object() {
                    return Err(AppError::localized(
                        "provider.codex.auth.not_object",
                        format!(
                            "Provider {} auth configuration must be a JSON object",
                            provider.id
                        ),
                    ));
                }

                if let Some(config_value) = settings.get("config") {
                    if !(config_value.is_string() || config_value.is_null()) {
                        return Err(AppError::localized(
                            "provider.codex.config.invalid_type",
                            "Codex config field must be a string",
                        ));
                    }
                    if let Some(cfg_text) = config_value.as_str() {
                        crate::codex_config::validate_config_toml(cfg_text)?;
                    }
                }
            }
            AppType::Gemini => {
                use crate::gemini_config::validate_gemini_settings;
                validate_gemini_settings(&provider.settings_config)?
            }
            AppType::GrokBuild => {
                let settings = provider.settings_config.as_object().ok_or_else(|| {
                    AppError::localized(
                        "provider.grokbuild.settings.not_object",
                        "Grok Build configuration must be a JSON object",
                    )
                })?;
                let config = settings
                    .get("config")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        AppError::localized(
                            "provider.grokbuild.config.missing",
                            "Grok Build configuration is missing the config field",
                        )
                    })?;
                if provider.category.as_deref() == Some("official") {
                    // Official entries use the Grok CLI's own OAuth: an empty config is valid,
                    // the backfill snapshot only requires valid TOML syntax.
                    crate::grok_config::validate_config_toml_syntax(config)?;
                } else {
                    crate::grok_config::validate_config_toml(config)?;
                }
            }
            AppType::OpenCode => {
                // OpenCode uses a different config structure: { npm, options, models }
                // Basic validation - must be an object
                if !provider.settings_config.is_object() {
                    return Err(AppError::localized(
                        "provider.opencode.settings.not_object",
                        "OpenCode configuration must be a JSON object",
                    ));
                }
            }
            AppType::OpenClaw => {
                // OpenClaw uses config structure: { baseUrl, apiKey, api, models }
                // Basic validation - must be an object
                if !provider.settings_config.is_object() {
                    return Err(AppError::localized(
                        "provider.openclaw.settings.not_object",
                        "OpenClaw configuration must be a JSON object",
                    ));
                }
            }
            AppType::Hermes => {
                // Hermes: accept any JSON object for now
                if !provider.settings_config.is_object() {
                    return Err(AppError::localized(
                        "provider.hermes.settings.not_object",
                        "Hermes configuration must be a JSON object",
                    ));
                }
            }
            AppType::Pi => {
                crate::pi_config::validate_provider_node(&provider.id, &provider.settings_config)?;
            }
        }

        // Validate and clean UsageScript configuration (common for all app types)
        if let Some(meta) = &provider.meta {
            if let Some(multiplier) = meta.cost_multiplier.as_deref() {
                validate_cost_multiplier(multiplier)?;
            }
            if let Some(source) = meta.pricing_model_source.as_deref() {
                validate_pricing_source(source)?;
            }
            if let Some(usage_script) = &meta.usage_script {
                validate_usage_script(usage_script)?;
            }
        }

        Ok(())
    }
}

/// Normalize Claude model keys in a JSON value
///
/// Reads old key (ANTHROPIC_SMALL_FAST_MODEL), writes new keys (DEFAULT_*), and deletes old key.
pub(crate) fn normalize_claude_models_in_value(settings: &mut Value) -> bool {
    let mut changed = false;
    let env = match settings.get_mut("env").and_then(|v| v.as_object_mut()) {
        Some(obj) => obj,
        None => return changed,
    };

    let model = env
        .get("ANTHROPIC_MODEL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let small_fast = env
        .get("ANTHROPIC_SMALL_FAST_MODEL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let current_haiku = env
        .get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let current_sonnet = env
        .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let current_opus = env
        .get("ANTHROPIC_DEFAULT_OPUS_MODEL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let target_haiku = current_haiku
        .or_else(|| small_fast.clone())
        .or_else(|| model.clone());
    let target_sonnet = current_sonnet
        .or_else(|| model.clone())
        .or_else(|| small_fast.clone());
    let target_opus = current_opus
        .or_else(|| model.clone())
        .or_else(|| small_fast.clone());

    if env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").is_none() {
        if let Some(v) = target_haiku {
            env.insert(
                "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                Value::String(v),
            );
            changed = true;
        }
    }
    if env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").is_none() {
        if let Some(v) = target_sonnet {
            env.insert(
                "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
                Value::String(v),
            );
            changed = true;
        }
    }
    if env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none() {
        if let Some(v) = target_opus {
            env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), Value::String(v));
            changed = true;
        }
    }

    if env.remove("ANTHROPIC_SMALL_FAST_MODEL").is_some() {
        changed = true;
    }

    changed
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderSortUpdate {
    pub id: String,
    #[serde(rename = "sortIndex")]
    pub sort_index: usize,
}

// ============================================================================
// Universal Provider service methods
// ============================================================================

use crate::provider::UniversalProvider;
use std::collections::HashMap;

impl ProviderService {
    /// Get all universal providers
    pub fn list_universal(
        state: &AppState,
    ) -> Result<HashMap<String, UniversalProvider>, AppError> {
        state.db.get_all_universal_providers()
    }

    /// Get a single universal provider
    pub fn get_universal(
        state: &AppState,
        id: &str,
    ) -> Result<Option<UniversalProvider>, AppError> {
        state.db.get_universal_provider(id)
    }

    /// Add or update a universal provider (no automatic sync; call sync_universal_to_apps manually)
    pub fn upsert_universal(
        state: &AppState,
        provider: UniversalProvider,
    ) -> Result<bool, AppError> {
        // Save the universal provider
        state.db.save_universal_provider(&provider)?;

        Ok(true)
    }

    /// Delete a universal provider
    pub fn delete_universal(state: &AppState, id: &str) -> Result<bool, AppError> {
        // Read the universal provider (used to delete the generated child providers)
        let provider = state.db.get_universal_provider(id)?;

        // Delete the universal provider
        state.db.delete_universal_provider(id)?;

        // Delete the generated child providers
        if let Some(p) = provider {
            if p.apps.claude {
                let claude_id = format!("universal-claude-{id}");
                let _ = state.db.delete_provider("claude", &claude_id);
            }
            if p.apps.codex {
                let codex_id = format!("universal-codex-{id}");
                let _ = state.db.delete_provider("codex", &codex_id);
            }
            if p.apps.gemini {
                let gemini_id = format!("universal-gemini-{id}");
                let _ = state.db.delete_provider("gemini", &gemini_id);
            }
        }

        Ok(true)
    }

    /// Sync a universal provider to every app
    pub fn sync_universal_to_apps(state: &AppState, id: &str) -> Result<bool, AppError> {
        let provider = state
            .db
            .get_universal_provider(id)?
            .ok_or_else(|| AppError::Message(format!("Universal provider {id} not found")))?;

        // Sync to Claude
        if let Some(mut claude_provider) = provider.to_claude_provider() {
            // Merge the existing config
            if let Some(existing) = state.db.get_provider_by_id(&claude_provider.id, "claude")? {
                let mut merged = existing.settings_config.clone();
                Self::merge_json(&mut merged, &claude_provider.settings_config);
                claude_provider.settings_config = merged;
                // App-specific config and ordering of an existing child provider are not managed by the universal provider.
                claude_provider.meta = existing.meta;
                claude_provider.created_at = existing.created_at;
                claude_provider.sort_index = existing.sort_index;
            }
            state.db.save_provider("claude", &claude_provider)?;
        } else {
            // If Claude is disabled, delete the matching child provider
            let claude_id = format!("universal-claude-{id}");
            let _ = state.db.delete_provider("claude", &claude_id);
        }

        // Sync to Codex
        if let Some(mut codex_provider) = provider.to_codex_provider() {
            // Merge the existing config
            if let Some(existing) = state.db.get_provider_by_id(&codex_provider.id, "codex")? {
                let mut merged = existing.settings_config.clone();
                Self::merge_json(&mut merged, &codex_provider.settings_config);
                codex_provider.settings_config = merged;
                // App-specific config and ordering of an existing child provider are not managed by the universal provider.
                codex_provider.meta = existing.meta;
                codex_provider.created_at = existing.created_at;
                codex_provider.sort_index = existing.sort_index;
            }
            state.db.save_provider("codex", &codex_provider)?;
        } else {
            let codex_id = format!("universal-codex-{id}");
            let _ = state.db.delete_provider("codex", &codex_id);
        }

        // Sync to Gemini
        if let Some(mut gemini_provider) = provider.to_gemini_provider() {
            // Merge the existing config
            if let Some(existing) = state.db.get_provider_by_id(&gemini_provider.id, "gemini")? {
                let mut merged = existing.settings_config.clone();
                Self::merge_json(&mut merged, &gemini_provider.settings_config);
                gemini_provider.settings_config = merged;
                // App-specific config and ordering of an existing child provider are not managed by the universal provider.
                gemini_provider.meta = existing.meta;
                gemini_provider.created_at = existing.created_at;
                gemini_provider.sort_index = existing.sort_index;
            }
            state.db.save_provider("gemini", &gemini_provider)?;
        } else {
            let gemini_id = format!("universal-gemini-{id}");
            let _ = state.db.delete_provider("gemini", &gemini_id);
        }

        Ok(true)
    }

    /// Recursively merge JSON: base is the base, patch overrides same-named fields
    fn merge_json(base: &mut serde_json::Value, patch: &serde_json::Value) {
        use serde_json::Value;

        match (base, patch) {
            (Value::Object(base_map), Value::Object(patch_map)) => {
                for (k, v_patch) in patch_map {
                    match base_map.get_mut(k) {
                        Some(v_base) => Self::merge_json(v_base, v_patch),
                        None => {
                            base_map.insert(k.clone(), v_patch.clone());
                        }
                    }
                }
            }
            // Other types: overwrite directly
            (base_val, patch_val) => {
                *base_val = patch_val.clone();
            }
        }
    }
}
