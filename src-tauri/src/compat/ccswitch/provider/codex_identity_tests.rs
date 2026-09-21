use super::align_template;
use crate::app_config::AppType;
use crate::compat::ccswitch::provider::{tests_create::TestHome, ProviderStore};
use crate::domain::{ProviderCreateDraft, ProviderCustomCreateDraft, ToolId};
use serde_json::json;
use std::sync::Arc;

const TEMPLATE: &str = r#"model_provider = "custom"
[model_providers.custom]
name = "New service"
base_url = "https://new.example.test/v1"
wire_api = "responses"
requires_openai_auth = true
"#;
const REQUEST_ID: &str = "6436323d-eeb7-400e-81fa-66c02cb0ecdb";

fn source(id: &str) -> String {
    format!(
        "model_provider = {id:?}\n[model_providers.{id}]\nname = \"Old\"\nbase_url = \"https://old.example.test/v1\"\nexperimental_bearer_token = \"sk-old-fixture\"\n"
    )
}

#[test]
fn retains_case_sensitive_custom_id_but_not_the_old_endpoint_or_token() {
    let merged = align_template(TEMPLATE, &source("OpenAI"), false).unwrap();
    let doc: toml::Value = merged.parse().unwrap();
    assert_eq!(doc["model_provider"].as_str(), Some("OpenAI"));
    assert_eq!(
        doc["model_providers"]["OpenAI"]["base_url"].as_str(),
        Some("https://new.example.test/v1")
    );
    assert!(doc["model_providers"].get("custom").is_none());
    assert!(!merged.contains("sk-old-fixture"));
}

#[test]
fn new_templates_never_override_built_ins_or_propagate_a_branded_id() {
    for source in [
        "".to_string(),
        "model_provider = \"openai\"\n".into(),
        source("ai_manager_openai"),
    ] {
        assert_eq!(align_template(TEMPLATE, &source, false).unwrap(), TEMPLATE);
    }
    assert_eq!(
        align_template("model_provider = \"openai\"\n", &source("OpenAI"), false).unwrap(),
        "model_provider = \"openai\"\n"
    );
}

#[test]
fn retry_keeps_the_persisted_identity_even_for_a_legacy_product_record() {
    let result = align_template(TEMPLATE, &source("ai_manager_openai"), true).unwrap();
    let doc: toml::Value = result.parse().unwrap();
    assert_eq!(doc["model_provider"].as_str(), Some("ai_manager_openai"));
}

#[test]
fn malformed_or_colliding_configs_fail_without_silently_replacing_tables() {
    for invalid in ["model_provider = ", "model_provider = \"missing\"\n"] {
        assert!(align_template(TEMPLATE, invalid, false).is_err());
    }
    let collision = format!("{TEMPLATE}\n[model_providers.OpenAI]\nname = \"Do not overwrite\"\n");
    assert!(align_template(&collision, &source("OpenAI"), false).is_err());
}

#[test]
fn inline_tables_and_quoted_ids_remain_valid() {
    let inline = "model_provider = \"custom\"\nmodel_providers = { custom = { name = \"New\", base_url = \"https://new.example.test/v1\" } }\n";
    let source =
        "model_provider = \"my.provider\"\n[model_providers.\"my.provider\"]\nname = \"Old\"\n";
    let result = align_template(inline, source, false).unwrap();
    let doc: toml::Value = result.parse().unwrap();
    assert_eq!(doc["model_provider"].as_str(), Some("my.provider"));
    assert!(doc["model_providers"].get("my.provider").is_some());
}

fn store() -> ProviderStore {
    ProviderStore {
        state: crate::store::AppState::new(Arc::new(crate::database::Database::memory().unwrap())),
    }
}

fn seed(text: &str) {
    crate::codex_config::write_codex_live_config_atomic(Some(text)).unwrap();
}

fn create(store: &ProviderStore, custom: bool) -> String {
    if custom {
        store
            .create_custom(
                ToolId::Codex,
                REQUEST_ID,
                &ProviderCustomCreateDraft {
                    name: "Private relay".into(),
                    api_key: "sk-new-fixture".into(),
                    model: "model-fixture".into(),
                    base_url: "https://new.example.test/v1".into(),
                },
            )
            .unwrap()
            .created_provider_id
    } else {
        store
            .create(
                ToolId::Codex,
                REQUEST_ID,
                &ProviderCreateDraft {
                    preset_id: "official".into(),
                    name: "API".into(),
                    api_key: "sk-new-fixture".into(),
                    model: "model-fixture".into(),
                },
            )
            .unwrap()
            .created_provider_id
    }
}

fn stored_id(store: &ProviderStore, id: &str) -> String {
    let provider = store.find_raw(ToolId::Codex, id).unwrap();
    let doc: toml::Value = provider.settings_config["config"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    doc["model_provider"].as_str().unwrap().to_string()
}

#[test]
#[serial_test::serial]
fn first_preset_and_custom_connections_keep_native_identity_and_leave_history_untouched() {
    for custom in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let _home = TestHome::set(temp.path());
        seed(&source("OpenAI"));
        let codex_dir = crate::codex_config::get_codex_config_path()
            .parent()
            .unwrap()
            .to_path_buf();
        let history_path = codex_dir.join("sessions/fixture.jsonl");
        std::fs::create_dir_all(history_path.parent().unwrap()).unwrap();
        let history = b"{\"type\":\"session_meta\",\"payload\":{\"model_provider\":\"OpenAI\"}}\n";
        std::fs::write(&history_path, history).unwrap();
        let auth_path = codex_dir.join("auth.json");
        let auth =
            json!({"tokens":{"access_token":"oauth-fixture", "refresh_token":"refresh-fixture"}})
                .to_string();
        std::fs::write(&auth_path, &auth).unwrap();
        let store = store();
        let id = create(&store, custom);
        assert_eq!(stored_id(&store, &id), "OpenAI");
        let live: toml::Value = crate::codex_config::read_codex_config_text()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(live["model_provider"].as_str(), Some("OpenAI"));
        assert_eq!(
            live["model_providers"]["OpenAI"]["experimental_bearer_token"].as_str(),
            Some("sk-new-fixture")
        );
        assert_eq!(std::fs::read(&history_path).unwrap(), history);
        // Switching to an API-key connection drops the ChatGPT login cache on
        // purpose: this adapter must not paste the old credential back, and it
        // must not suppress the upstream cleanup either.
        assert!(!auth_path.exists());
    }
}

#[test]
#[serial_test::serial]
fn store_only_create_and_retry_preserve_record_identity_without_touching_live() {
    let temp = tempfile::tempdir().unwrap();
    let _home = TestHome::set(temp.path());
    let store = store();
    let current = crate::provider::Provider::with_id(
        "existing".into(),
        "Existing".into(),
        json!({"auth":{}, "config":source("OpenAI")}),
        None,
    );
    store
        .state
        .db
        .save_provider(AppType::Codex.as_str(), &current)
        .unwrap();
    store
        .state
        .db
        .set_current_provider(AppType::Codex.as_str(), &current.id)
        .unwrap();
    seed(&source("OpenAI"));
    let id = create(&store, true);
    assert_eq!(stored_id(&store, &id), "OpenAI");
    assert_eq!(
        crate::codex_config::read_codex_config_text().unwrap(),
        source("OpenAI")
    );
    seed(&source("another"));
    assert_eq!(create(&store, true), id);
    assert_eq!(stored_id(&store, &id), "OpenAI");
    assert_eq!(
        crate::codex_config::read_codex_config_text().unwrap(),
        source("another")
    );
}

#[test]
#[serial_test::serial]
fn invalid_live_toml_aborts_before_creating_a_record() {
    let temp = tempfile::tempdir().unwrap();
    let _home = TestHome::set(temp.path());
    seed("");
    std::fs::write(
        crate::codex_config::get_codex_config_path(),
        "model_provider = ",
    )
    .unwrap();
    let store = store();
    let result = store.create(
        ToolId::Codex,
        REQUEST_ID,
        &ProviderCreateDraft {
            preset_id: "official".into(),
            name: "API".into(),
            api_key: "sk-fixture".into(),
            model: "model-fixture".into(),
        },
    );
    assert!(result.is_err());
    assert!(store.list(ToolId::Codex).unwrap().is_empty());
    assert_eq!(
        crate::codex_config::read_codex_config_text().unwrap(),
        "model_provider = "
    );
}
