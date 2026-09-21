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

const SECRET: &str = "sk-switch-secret-0123456789ABCD";

fn state() -> AppState {
    AppState::new(Arc::new(
        Database::memory().expect("create switch test database"),
    ))
}

fn claude(id: &str, name: &str) -> UpstreamProvider {
    UpstreamProvider::with_id(
        id.to_string(),
        name.to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.test",
                "ANTHROPIC_AUTH_TOKEN": SECRET
            }
        }),
        None,
    )
}

#[test]
#[serial_test::serial]
fn a_failed_live_write_leaves_current_on_the_service_that_is_actually_applied() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    // Upstream commits `current` before writing Claude's live file. A regular
    // file where the config directory belongs makes that write fail after the
    // commit, which is exactly the half-switched state the product must undo.
    std::fs::write(temp.path().join(".claude"), b"not a directory")
        .expect("block the Claude config directory");
    let state = state();
    let current = claude("switch-current-1a2b", "Current");
    let target = claude("switch-target-3c4d", "Target");
    for provider in [&current, &target] {
        state
            .db
            .save_provider(AppType::Claude.as_str(), provider)
            .expect("save provider");
    }
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), &current.id)
        .expect("select current provider");
    let store = ProviderStore {
        state: state.clone(),
    };

    let error = store
        .switch(ToolId::ClaudeCode, &target.id)
        .expect_err("the blocked live write must fail the switch");

    assert_eq!(error.message_key, "error.provider.switchFailed");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains(SECRET));
    assert_eq!(
        ProviderService::current(&state, AppType::Claude).expect("read current"),
        current.id,
        "current moved to a service whose live config was never written"
    );
    let listed = store.list(ToolId::ClaudeCode).expect("list services");
    let active = |id: &str| listed.iter().find(|p| p.id == id).expect("listed").active;
    assert!(active(&current.id));
    assert!(!active(&target.id));
}
