use std::sync::Arc;

use serde_json::json;

use crate::app_config::AppType;
use crate::compat::ccswitch::routing::RoutingStore;
use crate::database::Database;
use crate::domain::{
    RoutedTool, RoutingOverview, RoutingPickup, RoutingTarget, RoutingUnavailable, ToolId,
};
use crate::provider::Provider;
use crate::services::ProxyService;

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

fn store(db: Arc<Database>) -> RoutingStore {
    RoutingStore::with_parts(
        db.clone(),
        ProxyService::new(db),
        Arc::new(tokio::sync::Mutex::new(())),
    )
}

async fn use_ephemeral_port(db: &Database) {
    let mut config = db.get_proxy_config().await.expect("read proxy config");
    config.listen_port = 0;
    db.update_proxy_config(config)
        .await
        .expect("write proxy config");
}

fn make_current(db: &Database, app: AppType, provider: &Provider) {
    db.save_provider(app.as_str(), provider)
        .expect("save provider");
    db.set_current_provider(app.as_str(), &provider.id)
        .expect("set current provider");
    crate::settings::set_current_provider(&app, Some(provider.id.as_str()))
        .expect("set local current provider");
}

fn target(overview: &RoutingOverview, tool: ToolId) -> &RoutingTarget {
    overview
        .targets
        .iter()
        .find(|target| target.tool == tool)
        .expect("routing target")
}

fn relay_settings() -> serde_json::Value {
    json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-live-claude",
            "ANTHROPIC_BASE_URL": "https://api.example.com"
        }
    })
}

fn seed_claude(db: &Database, settings: &serde_json::Value, category: &str) -> std::path::PathBuf {
    let path = crate::config::get_claude_settings_path();
    std::fs::create_dir_all(path.parent().expect("claude dir")).expect("claude dir");
    crate::config::write_json_file(&path, settings).expect("seed claude live");
    let mut provider = Provider::with_id("a".into(), "Service A".into(), settings.clone(), None);
    provider.category = Some(category.to_string());
    make_current(db, AppType::Claude, &provider);
    path
}

fn read(path: &std::path::Path) -> serde_json::Value {
    crate::config::read_json_file(path).expect("read claude live")
}

#[tokio::test]
#[serial_test::serial]
async fn one_tool_is_routed_on_its_own_and_turning_it_off_keeps_later_edits() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let db = Arc::new(Database::memory().expect("memory database"));
    use_ephemeral_port(&db).await;
    let original = relay_settings();
    let path = seed_claude(&db, &original, "third_party");
    let store = store(db.clone());

    let on = store
        .set_takeover(ToolId::ClaudeCode, true)
        .await
        .expect("route claude");
    assert!(on.running);
    assert!(target(&on, ToolId::ClaudeCode).takeover_enabled);
    assert!(!target(&on, ToolId::Codex).takeover_enabled);
    let routed = read(&path);
    let routed_url = routed["env"]["ANTHROPIC_BASE_URL"].as_str().expect("url");
    assert!(routed_url.starts_with("http://127.0.0.1:"), "{routed_url}");

    // While routed, the user (or the tool) changes something unrelated.
    let mut edited = routed.clone();
    edited["permissions"] = json!({ "allow": ["Bash"] });
    crate::config::write_json_file(&path, &edited).expect("user edit");

    let off = store
        .set_takeover(ToolId::ClaudeCode, false)
        .await
        .expect("stop routing claude");
    assert!(!target(&off, ToolId::ClaudeCode).takeover_enabled);
    // The gateway keeps answering sessions that still hold its address.
    assert!(off.running);
    let mut expected = original.clone();
    expected["permissions"] = json!({ "allow": ["Bash"] });
    assert_eq!(read(&path), expected);

    store.release_before_exit().await;
    assert!(!store.overview().await.expect("overview").running);
}

#[tokio::test]
#[serial_test::serial]
async fn an_own_account_login_is_shown_as_unavailable_and_never_taken_over() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let db = Arc::new(Database::memory().expect("memory database"));
    use_ephemeral_port(&db).await;
    let signed_in = json!({ "env": {}, "model": "opus" });
    let path = seed_claude(&db, &signed_in, "official");
    let store = store(db.clone());

    let overview = store.overview().await.expect("overview");
    assert_eq!(
        target(&overview, ToolId::ClaudeCode).unavailable,
        Some(RoutingUnavailable::OwnLogin)
    );
    assert_eq!(
        target(&overview, ToolId::Codex).unavailable,
        Some(RoutingUnavailable::NoService)
    );

    let error = store
        .set_takeover(ToolId::ClaudeCode, true)
        .await
        .expect_err("an own login cannot be forwarded");
    assert_eq!(error.message_key, "error.routing.cannotForward");
    assert_eq!(read(&path), signed_in);
    assert!(!store.overview().await.expect("overview").running);
}

#[tokio::test]
#[serial_test::serial]
async fn quitting_puts_routed_tools_back_and_the_next_launch_starts_direct() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let db = Arc::new(Database::memory().expect("memory database"));
    use_ephemeral_port(&db).await;
    let original = relay_settings();
    let path = seed_claude(&db, &original, "third_party");
    let store = store(db.clone());

    store
        .set_takeover(ToolId::ClaudeCode, true)
        .await
        .expect("route claude");
    assert_eq!(
        store.routed_tools().await.expect("routed"),
        vec![RoutedTool {
            tool: ToolId::ClaudeCode,
            pickup: RoutingPickup::Live,
        }]
    );
    store.release_before_exit().await;
    assert_eq!(read(&path), original);
    assert!(store.routed_tools().await.expect("routed").is_empty());

    // A run that ended without that cleanup leaves the switch and the file.
    store
        .set_takeover(ToolId::ClaudeCode, true)
        .await
        .expect("route claude again");
    let mut edited = read(&path);
    edited["permissions"] = json!({ "allow": ["Read"] });
    crate::config::write_json_file(&path, &edited).expect("user edit");
    store
        .proxy
        .stop()
        .await
        .expect("gateway dies with the process");

    store.recover_at_launch().await;
    let mut expected = original.clone();
    expected["permissions"] = json!({ "allow": ["Read"] });
    assert_eq!(read(&path), expected);
    assert!(store.routed_tools().await.expect("routed").is_empty());
}
