use std::sync::Arc;

use serde_json::json;

use crate::app_config::AppType;
use crate::compat::ccswitch::routing::RoutingStore;
use crate::database::Database;
use crate::domain::{AppError, ErrorCode, RoutingPickup, ToolId};
use crate::provider::Provider;
use crate::services::ProviderService;
use crate::store::AppState;

struct TestHome(Option<std::ffi::OsString>);

impl TestHome {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("AI_MANAGER_TEST_HOME");
        std::env::set_var("AI_MANAGER_TEST_HOME", path);
        crate::settings::reload_settings().expect("reload settings for the test home");
        Self(previous)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
            None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
        }
        let _ = crate::settings::reload_settings();
    }
}

struct Fixture {
    state: AppState,
    store: RoutingStore,
    live: std::path::PathBuf,
    _home: TestHome,
    _temp: tempfile::TempDir,
}

fn relay(id: &str, url: &str, token: &str) -> Provider {
    let mut provider = Provider::with_id(
        id.into(),
        format!("Relay {id}"),
        json!({ "env": { "ANTHROPIC_BASE_URL": url, "ANTHROPIC_AUTH_TOKEN": token } }),
        None,
    );
    provider.category = Some("third_party".into());
    provider
}

fn signed_in() -> Provider {
    let mut provider = Provider::with_id(
        "official".into(),
        "Claude sign-in".into(),
        json!({ "env": {} }),
        None,
    );
    provider.category = Some("official".into());
    provider
}

/// Claude Code routed through the gateway on relay `a`, with `others` saved.
async fn routed_claude(others: &[Provider]) -> Fixture {
    let temp = tempfile::tempdir().expect("temp home");
    let home = TestHome::set(temp.path());
    let db = Arc::new(Database::memory().expect("memory database"));
    let mut config = db.get_proxy_config().await.expect("read proxy config");
    config.listen_port = 0;
    db.update_proxy_config(config)
        .await
        .expect("write proxy config");

    let current = relay("a", "https://a.example.com", "sk-a");
    let live = crate::config::get_claude_settings_path();
    std::fs::create_dir_all(live.parent().expect("claude dir")).expect("claude dir");
    crate::config::write_json_file(&live, &current.settings_config).expect("seed claude live");
    db.save_provider("claude", &current).expect("save a");
    for other in others {
        db.save_provider("claude", other).expect("save other");
    }
    db.set_current_provider("claude", "a").expect("current a");
    crate::settings::set_current_provider(&AppType::Claude, Some("a")).expect("local current");

    let state = AppState::new(db.clone());
    let store = RoutingStore::with_parts(
        db,
        state.proxy_service.clone(),
        state.routing_mutation_lock.clone(),
    );
    store
        .set_takeover(ToolId::ClaudeCode, true)
        .await
        .expect("route claude");
    Fixture {
        state,
        store,
        live,
        _home: home,
        _temp: temp,
    }
}

fn read(path: &std::path::Path) -> serde_json::Value {
    crate::config::read_json_file(path).expect("read claude live")
}

fn upstream(error: crate::error::AppError) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.switchFailed")
        .with_technical(error.to_string())
}

fn switch_to(state: &AppState, id: &'static str) -> impl FnOnce() -> Result<(), AppError> {
    let state = state.clone();
    move || {
        ProviderService::switch(&state, AppType::Claude, id)
            .map(|_| ())
            .map_err(upstream)
    }
}

async fn routed(fixture: &Fixture) -> bool {
    let overview = fixture.store.overview().await.expect("overview");
    overview
        .targets
        .iter()
        .find(|target| target.tool == ToolId::ClaudeCode)
        .expect("claude target")
        .takeover_enabled
}

#[tokio::test]
#[serial_test::serial]
async fn switching_a_routed_tool_to_its_own_login_ends_the_route_first() {
    let fixture = routed_claude(&[signed_in()]).await;
    let through = read(&fixture.live);
    assert!(through["env"]["ANTHROPIC_BASE_URL"]
        .as_str()
        .is_some_and(|url| url.starts_with("http://127.0.0.1:")));

    let ((), ended) = fixture
        .store
        .switch_endpoint(
            ToolId::ClaudeCode,
            "official",
            switch_to(&fixture.state, "official"),
        )
        .await
        .expect("switch to the sign-in");

    assert_eq!(ended, Some(RoutingPickup::Live));
    assert!(!routed(&fixture).await);
    let live = read(&fixture.live);
    assert!(
        live["env"].get("ANTHROPIC_BASE_URL").is_none(),
        "the sign-in has no address and nothing points at the gateway: {live}"
    );
    assert!(live["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
    fixture.store.release_before_exit().await;
}

#[tokio::test]
#[serial_test::serial]
async fn switching_a_routed_tool_to_another_forwardable_endpoint_keeps_the_route() {
    let fixture = routed_claude(&[relay("b", "https://b.example.com", "sk-b")]).await;

    let ((), ended) = fixture
        .store
        .switch_endpoint(ToolId::ClaudeCode, "b", switch_to(&fixture.state, "b"))
        .await
        .expect("switch to b");

    assert_eq!(ended, None);
    assert!(routed(&fixture).await);
    let live = read(&fixture.live);
    assert!(live["env"]["ANTHROPIC_BASE_URL"]
        .as_str()
        .is_some_and(|url| url.starts_with("http://127.0.0.1:")));

    // The route now forwards to b; ending it puts b back, not a.
    fixture
        .store
        .set_takeover(ToolId::ClaudeCode, false)
        .await
        .expect("turn routing off");
    assert_eq!(
        read(&fixture.live)["env"]["ANTHROPIC_BASE_URL"],
        "https://b.example.com"
    );
    fixture.store.release_before_exit().await;
}

#[tokio::test]
#[serial_test::serial]
async fn saving_the_routed_endpoint_without_its_key_ends_the_route() {
    let fixture = routed_claude(&[]).await;
    let state = fixture.state.clone();
    let save = move || {
        let keyless = Provider {
            settings_config: json!({ "env": { "ANTHROPIC_BASE_URL": "https://a.example.com" } }),
            ..relay("a", "https://a.example.com", "sk-a")
        };
        ProviderService::update(&state, AppType::Claude, Some("a"), keyless)
            .map(|_| ())
            .map_err(upstream)
    };

    let ((), ended) = fixture
        .store
        .save_endpoint(ToolId::ClaudeCode, "a", save)
        .await
        .expect("save a without its key");

    assert_eq!(ended, Some(RoutingPickup::Live));
    assert!(!routed(&fixture).await);
    let live = read(&fixture.live);
    assert_eq!(live["env"]["ANTHROPIC_BASE_URL"], "https://a.example.com");
    assert!(live["env"].get("ANTHROPIC_AUTH_TOKEN").is_none(), "{live}");
    fixture.store.release_before_exit().await;
}

#[tokio::test]
#[serial_test::serial]
async fn saving_or_switching_a_tool_that_is_not_routed_leaves_routing_alone() {
    let fixture = routed_claude(&[signed_in()]).await;
    fixture
        .store
        .set_takeover(ToolId::ClaudeCode, false)
        .await
        .expect("turn routing off");

    let ((), ended) = fixture
        .store
        .switch_endpoint(
            ToolId::ClaudeCode,
            "official",
            switch_to(&fixture.state, "official"),
        )
        .await
        .expect("switch directly");
    assert_eq!(ended, None);

    let ((), saved) = fixture
        .store
        .save_endpoint(ToolId::ClaudeCode, "official", || Ok(()))
        .await
        .expect("save");
    assert_eq!(saved, None);
    fixture.store.release_before_exit().await;
}
