use std::sync::Arc;

use serde_json::json;

use crate::app_config::AppType;
use crate::compat::ccswitch::routing::RoutingStore;
use crate::database::Database;
use crate::domain::{ErrorCode, RoutingOverview, ToolId};
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

fn gemini_service() -> Provider {
    Provider::with_id(
        "g".into(),
        "Service G".into(),
        json!({ "env": { "GEMINI_API_KEY": "sk-gemini" } }),
        None,
    )
}

fn takeover(overview: &RoutingOverview, tool: ToolId) -> bool {
    overview
        .targets
        .iter()
        .find(|target| target.tool == tool)
        .expect("routing target")
        .takeover_enabled
}

#[tokio::test]
#[serial_test::serial]
async fn live_mode_routes_every_ready_tool_reports_the_rest_and_restores_on_off() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let db = Arc::new(Database::memory().expect("memory database"));
    use_ephemeral_port(&db).await;

    // Claude has a live config to back up, so its takeover can succeed.
    let original = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-live-claude",
            "ANTHROPIC_BASE_URL": "https://api.example.com"
        }
    });
    let settings_path = crate::config::get_claude_settings_path();
    std::fs::create_dir_all(settings_path.parent().expect("claude dir")).expect("claude dir");
    crate::config::write_json_file(&settings_path, &original).expect("seed claude live");
    make_current(
        &db,
        AppType::Claude,
        &Provider::with_id("a".into(), "Service A".into(), original.clone(), None),
    );
    // Gemini has a current service but no live .env, so its backup fails.
    make_current(&db, AppType::Gemini, &gemini_service());

    let store = store(db.clone());
    let on = store.set_live_mode(true).await.expect("live mode on");
    assert!(on.overview.running);
    assert!(takeover(&on.overview, ToolId::ClaudeCode));
    assert!(!takeover(&on.overview, ToolId::GeminiCli));
    // Tools without a current service are not attempted, so not reported.
    assert_eq!(on.failures.len(), 1);
    assert_eq!(on.failures[0].tool, ToolId::GeminiCli);
    assert_eq!(on.failures[0].error.code, ErrorCode::ConfigWriteFailed);
    assert_eq!(
        on.failures[0].error.message_key,
        "error.routing.liveTakeoverFailed"
    );

    let off = store.set_live_mode(false).await.expect("live mode off");
    assert!(!off.overview.running);
    assert!(off.failures.is_empty());
    assert!(off
        .overview
        .targets
        .iter()
        .all(|target| !target.takeover_enabled));
    let restored: serde_json::Value =
        crate::config::read_json_file(&settings_path).expect("read restored claude live");
    assert_eq!(restored, original);
}

#[tokio::test]
#[serial_test::serial]
async fn live_mode_leaves_everything_direct_when_no_tool_can_be_routed() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let db = Arc::new(Database::memory().expect("memory database"));
    use_ephemeral_port(&db).await;

    let error = store(db.clone())
        .set_live_mode(true)
        .await
        .expect_err("no tool has a current service");
    assert_eq!(error.message_key, "error.routing.noLiveTargets");

    make_current(&db, AppType::Gemini, &gemini_service());
    let outcome = store(db).set_live_mode(true).await.expect("attempted");
    assert_eq!(outcome.failures.len(), 1);
    assert!(
        !outcome.overview.running,
        "a route that routes nothing is stopped"
    );
}
