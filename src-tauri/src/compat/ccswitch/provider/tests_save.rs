use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use super::create::provider_for_create;
use super::tests_create::TestHome;
use super::{app_type_for, saving, ProviderStore};
use crate::app_config::AppType;
use crate::database::Database;
use crate::domain::{ProviderAdvancedDraft, ProviderCreateDraft, ProviderDraft, ToolId};
use crate::provider::{Provider as UpstreamProvider, ProviderMeta};
use crate::services::ProviderService;
use crate::settings::CustomEndpoint;
use crate::store::AppState;

const KEY: &str = "sk-save-secret-0123456789ABCD";

fn state() -> AppState {
    AppState::new(Arc::new(
        Database::memory().expect("create save test database"),
    ))
}

fn codex(id: &str, name: &str, endpoint: &str, model: &str) -> UpstreamProvider {
    UpstreamProvider::with_id(
        id.to_string(),
        name.to_string(),
        json!({
            "auth": { "OPENAI_API_KEY": KEY },
            "config": format!(
                "model_provider = \"custom\"\nmodel = \"{model}\"\n\n[model_providers.custom]\nname = \"Custom\"\nbase_url = \"{endpoint}\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n"
            )
        }),
        None,
    )
}

fn draft() -> ProviderDraft {
    ProviderDraft {
        name: "Edited".to_string(),
        api_key: None,
        models: Some(vec!["new-model".to_string()]),
        advanced: Some(ProviderAdvancedDraft {
            base_url_changed: true,
            base_url: Some("https://new.example/v1".to_string()),
            endpoint_candidates: None,
            endpoint_auto_select: None,
            headers: None,
        }),
    }
}

fn seeded_store() -> (ProviderStore, UpstreamProvider) {
    let state = state();
    let current = codex(
        "current",
        "Current",
        "https://current.example/v1",
        "current-model",
    );
    let original = codex("target", "Original", "https://old.example/v1", "old-model");
    state
        .db
        .save_provider(AppType::Codex.as_str(), &current)
        .expect("save current");
    state
        .db
        .save_provider(AppType::Codex.as_str(), &original)
        .expect("save target");
    state
        .db
        .set_current_provider(AppType::Codex.as_str(), &current.id)
        .expect("select current");
    let original = state
        .db
        .get_provider_by_id(&original.id, AppType::Codex.as_str())
        .expect("read seeded target")
        .expect("seeded target exists");
    (ProviderStore { state }, original)
}

fn route_draft() -> ProviderDraft {
    ProviderDraft {
        name: "Original".to_string(),
        api_key: None,
        models: None,
        advanced: Some(ProviderAdvancedDraft {
            base_url_changed: false,
            base_url: None,
            endpoint_candidates: Some(vec![
                "https://old.example/v1".to_string(),
                "https://backup.example/v1".to_string(),
            ]),
            endpoint_auto_select: Some(false),
            headers: None,
        }),
    }
}

fn seeded_route_store() -> (ProviderStore, UpstreamProvider) {
    let state = state();
    let current = codex(
        "current",
        "Current",
        "https://current.example/v1",
        "current-model",
    );
    let mut original = codex("target", "Original", "https://old.example/v1", "old-model");
    original.meta = Some(ProviderMeta {
        custom_endpoints: HashMap::from([(
            "https://old.example/v1".to_string(),
            CustomEndpoint {
                url: "https://old.example/v1".to_string(),
                added_at: 10,
                last_used: None,
            },
        )]),
        endpoint_auto_select: Some(true),
        ..Default::default()
    });
    state
        .db
        .save_provider(AppType::Codex.as_str(), &current)
        .expect("save current");
    state
        .db
        .save_provider(AppType::Codex.as_str(), &original)
        .expect("save route provider");
    state
        .db
        .set_current_provider(AppType::Codex.as_str(), &current.id)
        .expect("select current");
    let store = ProviderStore { state };
    let original = store
        .find_raw(ToolId::Codex, &original.id)
        .expect("read route provider with endpoint rows");
    (store, original)
}

#[test]
fn a_partial_upstream_commit_is_rolled_back_to_the_exact_snapshot() {
    let (store, original) = seeded_store();
    let error = saving::save_with(
        &store,
        ToolId::Codex,
        &original.id,
        &draft(),
        |state, app_type, _, target| {
            state.db.save_provider(app_type.as_str(), &target)?;
            Err(crate::error::AppError::Message(
                "write failed token=super-secret-value".to_string(),
            ))
        },
    )
    .expect_err("the simulated live failure must surface");

    assert_eq!(error.message_key, "error.provider.saveFailed");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("super-secret-value"));
    let restored = store
        .state
        .db
        .get_provider_by_id(&original.id, AppType::Codex.as_str())
        .expect("read restored row")
        .expect("restored row exists");
    assert_eq!(
        serde_json::to_value(restored).expect("serialize restored"),
        serde_json::to_value(original).expect("serialize original")
    );
}

#[test]
fn a_panic_after_commit_is_caught_and_rolled_back() {
    let (store, original) = seeded_store();
    let error = saving::save_with(
        &store,
        ToolId::Codex,
        &original.id,
        &draft(),
        |state, app_type, _, target| {
            state.db.save_provider(app_type.as_str(), &target)?;
            panic!("panic token=panic-secret-value")
        },
    )
    .expect_err("panic must become a product error");

    assert_eq!(error.message_key, "error.provider.saveFailed");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("panic-secret-value"));
    let restored = store
        .state
        .db
        .get_provider_by_id(&original.id, AppType::Codex.as_str())
        .expect("read restored row")
        .expect("restored row exists");
    assert_eq!(restored.name, "Original");
}

#[test]
fn a_success_is_verified_and_returns_only_the_safe_product_list() {
    let (store, original) = seeded_store();
    let providers = saving::save(&store, ToolId::Codex, &original.id, &draft())
        .expect("save inactive provider");
    let saved = providers
        .iter()
        .find(|provider| provider.id == original.id)
        .expect("safe saved provider");
    assert_eq!(saved.name, "Edited");
    assert_eq!(saved.base_url.as_deref(), Some("https://new.example/v1"));
    let wire = serde_json::to_string(&providers).expect("serialize providers");
    assert!(wire.contains(KEY), "the saved key belongs on the card");
    assert!(!wire.contains("settingsConfig"));
}

#[test]
fn route_membership_and_preference_commit_through_the_verified_save() {
    let (store, original) = seeded_route_store();
    saving::save(&store, ToolId::Codex, &original.id, &route_draft())
        .expect("save alternate routes");

    let saved = store
        .find_raw(ToolId::Codex, &original.id)
        .expect("read committed routes");
    let meta = saved.meta.expect("route metadata");
    assert_eq!(meta.endpoint_auto_select, Some(false));
    assert_eq!(meta.custom_endpoints.len(), 2);
    assert!(meta.custom_endpoints.contains_key("https://old.example/v1"));
    assert!(meta
        .custom_endpoints
        .contains_key("https://backup.example/v1"));
}

#[test]
fn failed_route_verification_restores_provider_and_endpoint_snapshots() {
    let (store, original) = seeded_route_store();
    let error = saving::save_with(
        &store,
        ToolId::Codex,
        &original.id,
        &route_draft(),
        |state, app_type, _, mut target| {
            target.name = "Unexpected committed name".to_string();
            state.db.save_provider(app_type.as_str(), &target)?;
            Ok(true)
        },
    )
    .expect_err("verification mismatch must restore the whole snapshot");
    assert_eq!(error.message_key, "error.provider.saveFailed");

    let restored = store
        .find_raw(ToolId::Codex, &original.id)
        .expect("read restored provider");
    assert_eq!(
        serde_json::to_value(restored).expect("serialize restored"),
        serde_json::to_value(original).expect("serialize original")
    );
}

const LIVE_EDIT_URL: &str = "https://new.example/v1";

fn default_create_draft(tool: ToolId, name: &str) -> ProviderCreateDraft {
    let profile = super::connection_profile_for(tool).expect("connection profile");
    let preset = profile
        .presets
        .iter()
        .find(|preset| preset.id == profile.default_preset_id)
        .expect("default preset");
    ProviderCreateDraft {
        preset_id: preset.id.clone(),
        name: name.to_string(),
        api_key: KEY.to_string(),
        model: preset.default_model.clone(),
    }
}

/// Creates the service exactly like the product does: through upstream `add`
/// with `add_to_live = true`, so the row is marked live-managed and the
/// tool's own live file already contains it before the edit under test.
fn connected_live_store(tool: ToolId) -> (ProviderStore, String) {
    let store = ProviderStore { state: state() };
    let id = format!("save-live-{}", tool.as_str());
    let raw = provider_for_create(tool, &id, &default_create_draft(tool, "Original"))
        .expect("valid connect draft");
    ProviderService::add(&store.state, app_type_for(tool), raw, true).expect("add live service");
    (store, id)
}

fn live_edit_draft(name: &str) -> ProviderDraft {
    ProviderDraft {
        name: name.to_string(),
        api_key: None,
        models: Some(vec!["new-model".to_string()]),
        advanced: Some(ProviderAdvancedDraft {
            base_url_changed: true,
            base_url: Some(LIVE_EDIT_URL.to_string()),
            endpoint_candidates: None,
            endpoint_auto_select: None,
            headers: None,
        }),
    }
}

/// Reads the single live node back through the same upstream reader each tool
/// uses for its own live file, then resolves the endpoint like upstream does.
fn live_base_url(tool: ToolId, id: &str) -> String {
    let node = match tool {
        ToolId::OpenCode => crate::opencode_config::get_providers()
            .expect("read OpenCode live config")
            .get(id)
            .cloned(),
        ToolId::OpenClaw => crate::openclaw_config::get_provider(id).expect("read OpenClaw live"),
        ToolId::Hermes => crate::hermes_config::get_provider(id).expect("read Hermes live"),
        ToolId::Pi => crate::pi_config::read_pi_native_provider(id).expect("read Pi live"),
        other => panic!("{other:?} is not an additive tool"),
    }
    .expect("service present in live config");
    let (base_url, _) = UpstreamProvider::with_id(id.to_string(), id.to_string(), node, None)
        .resolve_usage_credentials(&app_type_for(tool));
    base_url
}

fn assert_live_edit_round_trips(tool: ToolId, name: &str) {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let (store, id) = connected_live_store(tool);

    let providers = saving::save(&store, tool, &id, &live_edit_draft(name))
        .unwrap_or_else(|error| panic!("{tool:?} live edit failed: {error:?}"));

    let saved = providers
        .iter()
        .find(|provider| provider.id == id)
        .expect("edited service listed");
    assert_eq!(saved.name, name);
    assert_eq!(saved.base_url.as_deref(), Some(LIVE_EDIT_URL));
    assert_eq!(live_base_url(tool, &id), LIVE_EDIT_URL);
    assert_eq!(
        store.edit_profile(tool, &id).expect("edit profile").models,
        vec!["new-model".to_string()]
    );
}

#[test]
#[serial_test::serial]
fn an_opencode_live_service_can_be_edited_again() {
    assert_live_edit_round_trips(ToolId::OpenCode, "Edited");
}

#[test]
#[serial_test::serial]
fn an_openclaw_live_service_can_be_edited_again() {
    assert_live_edit_round_trips(ToolId::OpenClaw, "Edited");
}

#[test]
#[serial_test::serial]
fn a_hermes_live_service_can_be_edited_again() {
    assert_live_edit_round_trips(ToolId::Hermes, "Edited");
}

#[test]
#[serial_test::serial]
fn a_pi_live_service_can_be_edited_again() {
    // Pi's display name is owned by the native `name` node and upstream
    // `pi::update` does not realign it, so a rename is a separate concern;
    // this covers the live model/endpoint edit path only.
    assert_live_edit_round_trips(ToolId::Pi, "Original");
}
