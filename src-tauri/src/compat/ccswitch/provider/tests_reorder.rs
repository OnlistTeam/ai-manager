use std::sync::Arc;

use serde_json::json;

use super::tests_create::TestHome;
use super::ProviderStore;
use crate::app_config::AppType;
use crate::database::Database;
use crate::domain::ToolId;
use crate::provider::Provider as UpstreamProvider;
use crate::services::ProviderService;
use crate::store::AppState;

fn state() -> AppState {
    AppState::new(Arc::new(
        Database::memory().expect("create reorder test database"),
    ))
}

fn claude(id: &str, sort_index: usize) -> UpstreamProvider {
    let mut provider = UpstreamProvider::with_id(
        id.to_string(),
        id.to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": format!("https://{id}.example.test"),
                "ANTHROPIC_AUTH_TOKEN": "sk-reorder-fixture-0123456789"
            }
        }),
        None,
    );
    provider.sort_index = Some(sort_index);
    provider
}

fn codex(id: &str, sort_index: usize) -> UpstreamProvider {
    let mut provider = UpstreamProvider::with_id(
        id.to_string(),
        id.to_string(),
        json!({ "auth": { "OPENAI_API_KEY": "sk-reorder-codex-0123456789" }, "config": "" }),
        None,
    );
    provider.sort_index = Some(sort_index);
    provider
}

fn store_with(state: &AppState, app: AppType, providers: &[UpstreamProvider]) -> ProviderStore {
    for provider in providers {
        state
            .db
            .save_provider(app.as_str(), provider)
            .expect("save provider");
    }
    ProviderStore {
        state: state.clone(),
    }
}

fn ids(store: &ProviderStore, tool: ToolId) -> Vec<String> {
    store
        .list(tool)
        .expect("list services")
        .into_iter()
        .map(|provider| provider.id)
        .collect()
}

fn order(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
#[serial_test::serial]
fn saves_the_new_order_without_touching_the_current_service() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let store = store_with(
        &state,
        AppType::Claude,
        &[claude("alpha", 0), claude("beta", 1), claude("gamma", 2)],
    );
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), "beta")
        .expect("select current provider");

    let listed = store
        .reorder(ToolId::ClaudeCode, &order(&["gamma", "alpha", "beta"]))
        .expect("reorder services");

    let returned: Vec<&str> = listed.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(returned, ["gamma", "alpha", "beta"]);
    assert!(listed.iter().any(|p| p.id == "beta" && p.active));
    assert_eq!(ids(&store, ToolId::ClaudeCode), ["gamma", "alpha", "beta"]);
    assert_eq!(
        ProviderService::current(&state, AppType::Claude).expect("read current"),
        "beta"
    );
}

#[test]
#[serial_test::serial]
fn switching_after_a_reorder_keeps_every_position() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let store = store_with(
        &state,
        AppType::Claude,
        &[claude("alpha", 0), claude("beta", 1)],
    );
    store
        .reorder(ToolId::ClaudeCode, &order(&["beta", "alpha"]))
        .expect("reorder services");

    let listed = store
        .switch(ToolId::ClaudeCode, "alpha")
        .expect("switch service");

    let returned: Vec<&str> = listed.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(returned, ["beta", "alpha"]);
}

#[test]
#[serial_test::serial]
fn keeps_the_hidden_import_placeholder_behind_the_visible_services() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let mut placeholder = UpstreamProvider::with_id(
        "default".into(),
        "default".into(),
        json!({ "env": {} }),
        None,
    );
    placeholder.sort_index = Some(0);
    let store = store_with(
        &state,
        AppType::Claude,
        &[placeholder, claude("alpha", 1), claude("beta", 2)],
    );

    store
        .reorder(ToolId::ClaudeCode, &order(&["beta", "alpha"]))
        .expect("reorder services");

    let rows = state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("read rows");
    let saved: Vec<(&str, Option<usize>)> = rows
        .values()
        .map(|row| (row.id.as_str(), row.sort_index))
        .collect();
    assert_eq!(
        saved,
        [("beta", Some(0)), ("alpha", Some(1)), ("default", Some(2))]
    );
}

#[test]
#[serial_test::serial]
fn refuses_an_order_that_does_not_match_the_saved_services() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let store = store_with(
        &state,
        AppType::Claude,
        &[claude("alpha", 0), claude("beta", 1)],
    );

    for stale in [
        order(&["beta"]),
        order(&["beta", "alpha", "ghost"]),
        order(&["beta", "beta"]),
        order(&["beta", "ghost"]),
    ] {
        let error = store
            .reorder(ToolId::ClaudeCode, &stale)
            .expect_err("a stale order must be refused");
        assert_eq!(error.message_key, "error.provider.reorderFailed");
    }
    assert_eq!(ids(&store, ToolId::ClaudeCode), ["alpha", "beta"]);
}

#[test]
#[serial_test::serial]
fn orders_each_tool_on_its_own() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let store = store_with(
        &state,
        AppType::Claude,
        &[claude("alpha", 0), claude("beta", 1)],
    );
    store_with(&state, AppType::Codex, &[codex("one", 0), codex("two", 1)]);

    store
        .reorder(ToolId::ClaudeCode, &order(&["beta", "alpha"]))
        .expect("reorder Claude services");

    assert_eq!(ids(&store, ToolId::ClaudeCode), ["beta", "alpha"]);
    assert_eq!(ids(&store, ToolId::Codex), ["one", "two"]);
}
