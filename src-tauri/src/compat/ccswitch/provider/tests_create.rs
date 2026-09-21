use std::sync::Arc;

use serde_json::json;

use super::{
    create::{provider_for_create, provider_id_for_attempt, ConnectionCreateMode},
    ProviderStore,
};
use crate::app_config::AppType;
use crate::database::Database;
use crate::domain::{ErrorCode, ProviderCreateDraft, ProviderCustomCreateDraft, ToolId};
use crate::provider::{Provider as UpstreamProvider, ProviderMeta};
use crate::store::AppState;

const REQUEST_ID: &str = "2ef45bc4-2918-4af7-a87f-8efc9d852116";
const FIRST_KEY: &str = "sk-first-connect-secret-0123456789ABCD";
const RETRY_KEY: &str = "sk-retry-connect-secret-9876543210WXYZ";

fn state() -> AppState {
    AppState::new(Arc::new(
        Database::memory().expect("create provider-connect test database"),
    ))
}

fn draft(name: &str, key: &str) -> ProviderCreateDraft {
    ProviderCreateDraft {
        preset_id: "official".to_string(),
        name: name.to_string(),
        api_key: key.to_string(),
        model: "test-model".to_string(),
    }
}

fn default_draft(tool: ToolId, name: &str, key: &str) -> ProviderCreateDraft {
    let profile = super::connection_profile_for(tool).expect("connection profile");
    let preset = profile
        .presets
        .iter()
        .find(|preset| preset.id == profile.default_preset_id)
        .expect("default preset");
    ProviderCreateDraft {
        preset_id: preset.id.clone(),
        name: name.to_string(),
        api_key: key.to_string(),
        model: preset.default_model.clone(),
    }
}

fn custom_draft(name: &str, key: &str) -> ProviderCustomCreateDraft {
    ProviderCustomCreateDraft {
        name: name.to_string(),
        api_key: key.to_string(),
        model: "custom-model".to_string(),
        base_url: "https://relay.example.test/v1".to_string(),
    }
}

fn current_provider() -> UpstreamProvider {
    UpstreamProvider::with_id(
        "existing-current".to_string(),
        "Existing".to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://existing.example.test",
                "ANTHROPIC_API_KEY": "sk-existing-0123456789ABCD"
            }
        }),
        None,
    )
}

pub(super) struct TestHome {
    previous_home: Option<std::ffi::OsString>,
    previous_hermes_home: Option<std::ffi::OsString>,
    previous_local_appdata: Option<std::ffi::OsString>,
}

impl TestHome {
    pub(super) fn set(path: &std::path::Path) -> Self {
        let previous_home = std::env::var_os("AI_MANAGER_TEST_HOME");
        std::env::set_var("AI_MANAGER_TEST_HOME", path);
        // `get_hermes_dir()` reads HERMES_HOME first and falls back to %LOCALAPPDATA% on
        // Windows, so neither is covered by the test home. Left in place, the Hermes cases
        // would read and write the machine's real Hermes config.
        let previous_hermes_home = std::env::var_os("HERMES_HOME");
        let previous_local_appdata = std::env::var_os("LOCALAPPDATA");
        std::env::remove_var("HERMES_HOME");
        std::env::remove_var("LOCALAPPDATA");
        Self {
            previous_home,
            previous_hermes_home,
            previous_local_appdata,
        }
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        match self.previous_local_appdata.take() {
            Some(value) => std::env::set_var("LOCALAPPDATA", value),
            None => std::env::remove_var("LOCALAPPDATA"),
        }
        match self.previous_hermes_home.take() {
            Some(value) => std::env::set_var("HERMES_HOME", value),
            None => std::env::remove_var("HERMES_HOME"),
        }
        match self.previous_home.take() {
            Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
            None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
        }
    }
}

#[test]
#[serial_test::serial]
fn a_new_first_connection_records_its_activate_intent_in_the_id() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let store = ProviderStore { state: state() };

    let result = store
        .create(ToolId::ClaudeCode, REQUEST_ID, &draft("First", FIRST_KEY))
        .expect("first connection should become active");

    assert_eq!(
        result.created_provider_id,
        provider_id_for_attempt(REQUEST_ID, ConnectionCreateMode::Activate)
            .expect("valid request id")
    );
    assert!(result.providers[0].active);
}

#[test]
#[serial_test::serial]
fn a_custom_connection_uses_the_same_transaction_without_exposing_its_key() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let store = ProviderStore {
        state: state.clone(),
    };

    let result = store
        .create_custom(
            ToolId::ClaudeCode,
            REQUEST_ID,
            &custom_draft("Private relay", FIRST_KEY),
        )
        .expect("custom connection");
    let created = result
        .providers
        .iter()
        .find(|provider| provider.id == result.created_provider_id)
        .expect("created provider");
    assert_eq!(
        created.base_url.as_deref(),
        Some("https://relay.example.test/v1")
    );
    assert_eq!(created.website_url, None);
    assert!(created.active);
    let stored = state
        .db
        .get_provider_by_id(&result.created_provider_id, AppType::Claude.as_str())
        .expect("read custom row")
        .expect("custom row");
    assert_eq!(
        stored.resolve_usage_credentials(&AppType::Claude),
        (
            "https://relay.example.test/v1".to_string(),
            FIRST_KEY.to_string()
        )
    );
    // The key travelling back to the UI with the product list is deliberate; that free-form JSON blob from upstream does not follow.
    let wire = serde_json::to_string(&result).expect("safe result");
    assert!(wire.contains(FIRST_KEY));
    assert!(!wire.contains("settingsConfig"));
}

#[test]
#[serial_test::serial]
fn a_new_connection_beside_a_current_one_records_store_only_intent() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let store = ProviderStore {
        state: state.clone(),
    };
    let current = current_provider();
    state
        .db
        .save_provider(AppType::Claude.as_str(), &current)
        .expect("save existing current");
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), &current.id)
        .expect("select existing current");

    let result = store
        .create(ToolId::ClaudeCode, REQUEST_ID, &draft("Second", FIRST_KEY))
        .expect("second connection should only be stored");

    let id = provider_id_for_attempt(REQUEST_ID, ConnectionCreateMode::StoreOnly)
        .expect("valid request id");
    assert_eq!(result.created_provider_id, id);
    assert!(
        !result
            .providers
            .iter()
            .find(|provider| provider.id == id)
            .expect("new provider returned")
            .active
    );
    assert_eq!(
        state
            .db
            .get_current_provider(AppType::Claude.as_str())
            .expect("read current")
            .expect("current remains"),
        current.id
    );
}

#[test]
#[serial_test::serial]
fn retry_repairs_one_partially_active_service_instead_of_adding_another() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let store = ProviderStore {
        state: state.clone(),
    };
    let id = provider_id_for_attempt(REQUEST_ID, ConnectionCreateMode::Activate)
        .expect("valid request id");
    let partial = provider_for_create(ToolId::ClaudeCode, &id, &draft("First", FIRST_KEY))
        .expect("valid partial provider");
    state
        .db
        .save_provider(AppType::Claude.as_str(), &partial)
        .expect("save partial row");
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), &id)
        .expect("simulate current committed before live write failed");

    let result = store
        .create(ToolId::ClaudeCode, REQUEST_ID, &draft("Retried", RETRY_KEY))
        .expect("retry should converge the same row and live config");

    assert_eq!(result.created_provider_id, id);
    assert_eq!(result.providers.len(), 1, "retry created a duplicate row");
    assert_eq!(result.providers[0].name, "Retried");
    assert!(result.providers[0].active);
    let live: serde_json::Value = serde_json::from_slice(
        &std::fs::read(crate::config::get_claude_settings_path())
            .expect("read repaired Claude live config"),
    )
    .expect("valid Claude live config");
    let live_provider =
        UpstreamProvider::with_id("live".to_string(), "Live".to_string(), live, None);
    let (_, live_key) = live_provider.resolve_usage_credentials(&AppType::Claude);
    assert_eq!(live_key, RETRY_KEY);
    let wire = serde_json::to_string(&result).expect("serialize safe result");
    assert!(wire.contains(RETRY_KEY));
    assert!(!wire.contains("settingsConfig"));
}

#[test]
#[serial_test::serial]
fn retry_preserves_the_original_store_only_intent() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let store = ProviderStore {
        state: state.clone(),
    };
    let current = current_provider();
    state
        .db
        .save_provider(AppType::Claude.as_str(), &current)
        .expect("save existing current");
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), &current.id)
        .expect("select existing current");
    let id = provider_id_for_attempt(REQUEST_ID, ConnectionCreateMode::StoreOnly)
        .expect("valid request id");
    let partial = provider_for_create(ToolId::ClaudeCode, &id, &draft("First", FIRST_KEY))
        .expect("valid partial provider");
    state
        .db
        .save_provider(AppType::Claude.as_str(), &partial)
        .expect("save partial DB-only row");

    let result = store
        .create(ToolId::ClaudeCode, REQUEST_ID, &draft("Retried", RETRY_KEY))
        .expect("retry should finish the DB-only create");

    assert_eq!(result.created_provider_id, id);
    assert_eq!(result.providers.len(), 2, "retry created a duplicate row");
    assert_eq!(
        state
            .db
            .get_current_provider(AppType::Claude.as_str())
            .expect("read current")
            .expect("current remains"),
        current.id
    );
    assert!(
        !result
            .providers
            .iter()
            .find(|provider| provider.id == id)
            .expect("created provider returned")
            .active
    );
    let stored = state
        .db
        .get_provider_by_id(&id, AppType::Claude.as_str())
        .expect("read retried row")
        .expect("retried row exists");
    let (_, key) = stored.resolve_usage_credentials(&AppType::Claude);
    assert_eq!(key, RETRY_KEY);
    assert!(
        !crate::config::get_claude_settings_path().exists(),
        "a DB-only connection unexpectedly replaced the active live config"
    );
}

#[test]
#[serial_test::serial]
fn retry_repairs_one_partially_live_additive_service() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let store = ProviderStore {
        state: state.clone(),
    };
    let id = provider_id_for_attempt(REQUEST_ID, ConnectionCreateMode::Activate)
        .expect("valid request id");
    let mut partial = provider_for_create(ToolId::OpenCode, &id, &draft("First", FIRST_KEY))
        .expect("valid partial provider");
    partial.meta = Some(ProviderMeta {
        live_config_managed: Some(true),
        ..ProviderMeta::default()
    });
    state
        .db
        .save_provider(AppType::OpenCode.as_str(), &partial)
        .expect("save partial additive row");

    let result = store
        .create(ToolId::OpenCode, REQUEST_ID, &draft("Retried", RETRY_KEY))
        .expect("retry should converge the additive row and live config");

    assert_eq!(result.created_provider_id, id);
    assert_eq!(result.providers.len(), 1, "retry created a duplicate row");
    let live: serde_json::Value = serde_json::from_slice(
        &std::fs::read(crate::opencode_config::get_opencode_config_path())
            .expect("read repaired OpenCode live config"),
    )
    .expect("valid OpenCode live config");
    assert_eq!(live["provider"][&id]["options"]["apiKey"], RETRY_KEY);
}

#[test]
#[serial_test::serial]
fn long_tail_defaults_complete_the_upstream_create_and_live_write_transaction() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let store = ProviderStore { state: state() };

    for (index, tool) in [
        ToolId::GrokBuild,
        ToolId::OpenClaw,
        ToolId::Hermes,
        ToolId::Pi,
    ]
    .into_iter()
    .enumerate()
    {
        let request_id = format!("00000000-0000-4000-8000-{index:012}");
        let result = store
            .create(
                tool,
                &request_id,
                &default_draft(tool, tool.as_str(), FIRST_KEY),
            )
            .expect("long-tail connection");
        let created = result
            .providers
            .iter()
            .find(|provider| provider.id == result.created_provider_id)
            .expect("created provider returned");
        assert_eq!(created.name, tool.as_str());
        assert!(created
            .base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://")));
        assert!(created.api_key.is_some(), "{tool:?} key was not readable");
    }

    assert!(crate::grok_config::get_grok_config_path().is_file());
    assert!(crate::openclaw_config::get_openclaw_config_path().is_file());
    assert!(crate::hermes_config::get_hermes_config_path().is_file());
    assert!(crate::pi_config::get_pi_models_path()
        .expect("Pi models path")
        .is_file());
}

#[test]
fn an_invalid_request_id_is_rejected_before_writing() {
    let state = state();
    let store = ProviderStore {
        state: state.clone(),
    };

    let error = store
        .create(
            ToolId::ClaudeCode,
            "not-a-uuid",
            &draft("Rejected", FIRST_KEY),
        )
        .expect_err("invalid id must be rejected");

    assert_eq!(error.code, ErrorCode::ConfigWriteFailed);
    assert_eq!(error.message_key, "error.provider.createFailed");
    assert!(state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("read unchanged database")
        .is_empty());
}
