use std::sync::Arc;

use serde_json::json;

use super::{app_for_tool, supports_local_routing, RoutingStore, ROUTING_APPS};
use crate::database::Database;
use crate::domain::ToolId;
use crate::provider::{AuthBinding, AuthBindingSource, Provider, ProviderMeta};
use crate::services::ProxyService;

fn store(db: Arc<Database>) -> RoutingStore {
    RoutingStore::with_parts(
        db.clone(),
        ProxyService::new(db),
        Arc::new(tokio::sync::Mutex::new(())),
    )
}

fn provider(id: &str, name: &str, sort_index: usize) -> Provider {
    let mut provider = Provider::with_id(
        id.to_string(),
        name.to_string(),
        json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "must-never-cross-product-wire"
            }
        }),
        None,
    );
    provider.sort_index = Some(sort_index);
    provider.notes = Some("private operator note".to_string());
    provider
}

#[test]
fn mapping_is_exactly_the_four_inherited_proxy_data_planes() {
    assert_eq!(ROUTING_APPS.len(), 4);
    assert_eq!(app_for_tool(ToolId::ClaudeCode).expect("claude"), "claude");
    assert_eq!(app_for_tool(ToolId::Codex).expect("codex"), "codex");
    assert_eq!(app_for_tool(ToolId::GeminiCli).expect("gemini"), "gemini");
    assert_eq!(app_for_tool(ToolId::GrokBuild).expect("grok"), "grokbuild");
    for unsupported in [
        ToolId::OpenCode,
        ToolId::OpenClaw,
        ToolId::Hermes,
        ToolId::Pi,
    ] {
        assert!(app_for_tool(unsupported).is_err());
    }
    // Lightweight failover answers from the same table, never a second list.
    for tool in ToolId::ALL {
        assert_eq!(
            supports_local_routing(tool),
            app_for_tool(tool).is_ok(),
            "{tool:?}"
        );
    }
}

#[tokio::test]
async fn overview_projects_identity_health_and_queue_without_private_fields() {
    let db = Arc::new(Database::memory().expect("memory database"));
    let mut first = provider("provider-a", "Provider A", 1);
    first.in_failover_queue = true;
    db.save_provider("claude", &first).expect("save first");
    db.set_current_provider("claude", &first.id)
        .expect("set current");

    let second = provider("provider-b", "Provider B", 2);
    db.save_provider("claude", &second).expect("save second");
    db.update_provider_health_with_threshold(
        &first.id,
        "claude",
        false,
        Some("Bearer should-not-leak".to_string()),
        1,
    )
    .await
    .expect("health");

    let overview = store(db).overview().await.expect("overview");
    assert!(!overview.running);
    assert_eq!(overview.address, None);
    assert_eq!(overview.port, None);
    assert_eq!(overview.targets.len(), 4);
    let claude = overview
        .targets
        .iter()
        .find(|target| target.tool == ToolId::ClaudeCode)
        .expect("claude target");
    assert_eq!(claude.queue.len(), 1);
    assert_eq!(claude.queue[0].priority, Some(1));
    assert!(!claude.queue[0].healthy);
    assert_eq!(claude.queue[0].consecutive_failures, 1);
    assert_eq!(claude.available.len(), 1);

    let wire = serde_json::to_string(&overview).expect("serialize overview");
    for private in [
        "must-never-cross-product-wire",
        "private operator note",
        "Bearer should-not-leak",
        "settingsConfig",
        "providerNotes",
        "lastError",
    ] {
        assert!(!wire.contains(private), "wire leaked {private}");
    }
}

#[tokio::test]
async fn codex_official_account_cards_never_enter_the_failover_lists() {
    let db = Arc::new(Database::memory().expect("memory database"));
    let mut official = Provider::with_id(
        "official-a".to_string(),
        "OpenAI Official".to_string(),
        json!({ "auth": {}, "config": "" }),
        None,
    );
    official.sort_index = Some(1);
    official.category = Some("official".to_string());
    official.meta = Some(ProviderMeta {
        auth_binding: Some(AuthBinding {
            source: AuthBindingSource::ManagedAccount,
            auth_provider: Some("codex_oauth".to_string()),
            account_id: Some("account-a".to_string()),
        }),
        ..Default::default()
    });
    official.in_failover_queue = true;
    db.save_provider("codex", &official).expect("save official");
    db.set_current_provider("codex", &official.id)
        .expect("set current");

    let codex = store(db)
        .overview()
        .await
        .expect("overview")
        .targets
        .into_iter()
        .find(|target| target.tool == ToolId::Codex)
        .expect("codex target");
    assert_eq!(codex.current_provider.expect("current").id, official.id);
    assert!(codex.queue.is_empty());
    assert!(codex.available.is_empty());
}

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

async fn set_auto_failover(db: &Database, app: &str, enabled: bool) {
    let mut config = db
        .get_proxy_config_for_app(app)
        .await
        .expect("read proxy config");
    config.auto_failover_enabled = enabled;
    db.update_proxy_config_for_app(config)
        .await
        .expect("write proxy config");
}

async fn auto_failover(db: &Database, app: &str) -> bool {
    db.get_proxy_config_for_app(app)
        .await
        .expect("read proxy config")
        .auto_failover_enabled
}

#[tokio::test]
#[serial_test::serial]
async fn turning_takeover_off_also_turns_automatic_failover_off() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let db = Arc::new(Database::memory().expect("memory database"));
    // Upstream leaves the failover switch on when takeover goes off. Turning
    // takeover back on would then revive failover without the P1 switch that
    // `set_failover(true)` requires (ADR-0007).
    set_auto_failover(&db, "claude", true).await;

    let overview = store(db.clone())
        .set_takeover(ToolId::ClaudeCode, false)
        .await
        .expect("takeover off");

    let claude = overview
        .targets
        .iter()
        .find(|target| target.tool == ToolId::ClaudeCode)
        .expect("claude target");
    assert!(!claude.takeover_enabled);
    assert!(!claude.auto_failover_enabled);
    assert!(!auto_failover(&db, "claude").await);
}

#[tokio::test]
#[serial_test::serial]
async fn stopping_all_routing_turns_every_automatic_failover_off() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let db = Arc::new(Database::memory().expect("memory database"));
    for app in ["codex", "gemini"] {
        set_auto_failover(&db, app, true).await;
    }

    let overview = store(db.clone()).stop_all().await.expect("stop all");

    for target in &overview.targets {
        assert!(!target.takeover_enabled, "{:?}", target.tool);
        assert!(!target.auto_failover_enabled, "{:?}", target.tool);
    }
    assert!(!auto_failover(&db, "codex").await);
    assert!(!auto_failover(&db, "gemini").await);
}
