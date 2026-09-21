use super::{
    app_type_for, probe_target_for, provider_from_upstream, reachability_from, ProviderStore,
};
use crate::app_config::AppType;
use crate::compat::ccswitch::tools::tool_id_to_app_type;
use crate::domain::{ProviderKind, ProviderReachability, ToolId};
use crate::provider::Provider as UpstreamProvider;
use crate::services::stream_check::HealthStatus;
use serde_json::json;

#[derive(Default)]
struct ProbeGate {
    started: std::sync::Mutex<usize>,
    ready: std::sync::Condvar,
}

fn gated_endpoint(
    gate: std::sync::Arc<ProbeGate>,
) -> (String, std::thread::JoinHandle<(bool, String)>) {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind failover endpoint");
    let address = listener.local_addr().expect("read failover endpoint");
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept failover probe");
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set probe read timeout");
        let mut bytes = [0_u8; 4_096];
        let size = socket.read(&mut bytes).expect("read failover probe");
        let request = String::from_utf8_lossy(&bytes[..size]).into_owned();
        let mut started = gate.started.lock().expect("lock failover probe gate");
        *started += 1;
        gate.ready.notify_all();
        let (started, _) = gate
            .ready
            .wait_timeout_while(started, Duration::from_secs(3), |started| *started < 2)
            .expect("wait for concurrent failover probe");
        let overlapped = *started == 2;
        socket
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .expect("write failover response");
        (overlapped, request)
    });
    (format!("http://{address}"), server)
}

fn upstream(id: &str, name: &str, settings: serde_json::Value) -> UpstreamProvider {
    UpstreamProvider::with_id(id.to_string(), name.to_string(), settings, None)
}

#[test]
fn the_two_app_type_mappings_can_never_drift_apart() {
    // tools.rs holds one ToolId -> &str mapping and this file holds a ToolId -> AppType one.
    // The two must agree verbatim, otherwise the table being read and the table being written
    // point at different rows.
    for tool in ToolId::ALL {
        let Some(mapped) = tool_id_to_app_type(tool) else {
            continue;
        };
        assert_eq!(
            app_type_for(tool).as_str(),
            mapped,
            "{tool:?} maps to two different upstream app types"
        );
    }
}

#[test]
fn additive_capability_is_projected_from_upstream_not_inferred_from_active_flag() {
    for tool in ToolId::ALL {
        let Some(_) = tool_id_to_app_type(tool) else {
            continue;
        };
        let raw = upstream("test", "Test", json!({}));
        let provider = provider_from_upstream(tool, &raw, "");
        assert_eq!(provider.additive, app_type_for(tool).is_additive_mode());
    }
}

#[test]
fn a_custom_claude_service_exposes_its_address_and_its_key() {
    let raw = upstream(
        "custom",
        "My Relay",
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F"
            }
        }),
    );

    let provider = provider_from_upstream(ToolId::ClaudeCode, &raw, "custom");

    assert_eq!(provider.id, "custom");
    assert_eq!(provider.tool, ToolId::ClaudeCode);
    assert_eq!(provider.name, "My Relay");
    assert_eq!(provider.kind, ProviderKind::Custom);
    assert!(provider.active, "the current id was passed in");
    assert_eq!(
        provider.base_url.as_deref(),
        Some("https://relay.example.com")
    );
    assert_eq!(
        provider.api_key.as_deref(),
        Some("sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F")
    );
    assert!(provider.testable);
    assert!(!provider.can_remove, "the active service must fail closed");
}

#[test]
fn the_key_reaches_the_renderer_but_never_a_debug_line() {
    let secret = "sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F";
    let raw = upstream(
        "custom",
        "My Relay",
        json!({ "env": { "ANTHROPIC_BASE_URL": "https://relay.example.com",
                         "ANTHROPIC_AUTH_TOKEN": secret } }),
    );
    let provider = provider_from_upstream(ToolId::ClaudeCode, &raw, "other");
    let json = serde_json::to_string(&provider).expect("serialize");
    assert!(
        json.contains(secret),
        "the card has to be able to show and copy the key: {json}"
    );
    // Going out to the renderer through the wire format is deliberate; going into the logs through `{:?}` is not (AI_RULES rule 6).
    let debug = format!("{provider:?}");
    assert!(
        !debug.contains(secret),
        "Debug leaked the plaintext key: {debug}"
    );
    assert!(!provider.active, "a different id is current");
    assert!(
        provider.can_remove,
        "an inactive standard service is removable"
    );
}

#[test]
fn an_official_service_uses_the_reviewed_catalog_as_its_probe_capability() {
    let mut raw = upstream("official", "Anthropic", json!({ "env": {} }));
    raw.category = Some("official".to_string());
    let provider = provider_from_upstream(ToolId::ClaudeCode, &raw, "official");
    assert_eq!(provider.kind, ProviderKind::Official);
    assert!(provider.testable);
    assert_eq!(
        provider.base_url, None,
        "official target must stay native-only"
    );
    assert_eq!(
        probe_target_for(ToolId::ClaudeCode, &raw)
            .expect("reviewed catalog")
            .as_deref(),
        Some("https://api.anthropic.com")
    );
}

#[test]
fn an_official_service_without_a_reviewed_target_fails_closed() {
    let mut raw = upstream("official", "Pi Official", json!({}));
    raw.category = Some("official".to_string());
    let provider = provider_from_upstream(ToolId::Pi, &raw, "official");
    assert_eq!(provider.kind, ProviderKind::Official);
    assert!(!provider.testable);
    assert_eq!(
        probe_target_for(ToolId::Pi, &raw).expect("reviewed catalog"),
        None
    );
}

#[test]
fn a_service_without_an_address_is_not_testable_either() {
    let raw = upstream("bare", "Bare", json!({ "env": {} }));
    let provider = provider_from_upstream(ToolId::ClaudeCode, &raw, "bare");
    assert_eq!(provider.kind, ProviderKind::Custom);
    assert_eq!(provider.base_url, None);
    assert_eq!(provider.api_key, None);
    assert!(!provider.testable);
}

#[test]
fn a_credentialed_or_query_token_url_never_crosses_the_product_boundary() {
    for base_url in [
        "https://user:password@example.test/v1",
        "https://example.test/v1?token=secret-query-value",
    ] {
        let raw = upstream(
            "unsafe-url",
            "Unsafe URL",
            json!({ "env": { "ANTHROPIC_BASE_URL": base_url } }),
        );
        let provider = provider_from_upstream(ToolId::ClaudeCode, &raw, "other");
        assert_eq!(provider.base_url, None);
        assert!(!provider.testable);
        let wire = serde_json::to_string(&provider).expect("serialize safe provider");
        assert!(!wire.contains("password"));
        assert!(!wire.contains("secret-query-value"));
    }
}

#[test]
fn every_tool_reads_its_key_out_of_its_own_settings_shape() {
    // The settings_config shape of all eight tools differs; the conversion must hold for every one
    // of them, which it does thanks to the exhaustive match in the upstream
    // resolve_usage_credentials rather than any guessing of our own.
    let cases = [
        (
            ToolId::ClaudeCode,
            json!({ "env": { "ANTHROPIC_BASE_URL": "https://a.example.com",
                             "ANTHROPIC_AUTH_TOKEN": "sk-claude-0123456789ABCD" } }),
        ),
        (
            ToolId::Codex,
            json!({ "auth": { "OPENAI_API_KEY": "sk-codex-0123456789ABCD" } }),
        ),
        (
            ToolId::OpenCode,
            json!({ "options": { "baseURL": "https://o.example.com",
                                 "apiKey": "sk-opencode-0123456789ABCD" } }),
        ),
        (
            ToolId::GeminiCli,
            json!({ "env": { "GOOGLE_GEMINI_BASE_URL": "https://g.example.com",
                             "GEMINI_API_KEY": "sk-gemini-0123456789ABCD" } }),
        ),
        (
            ToolId::GrokBuild,
            json!({ "config": "[models]\ndefault = \"profile\"\n\n[model.profile]\nmodel = \"grok-4.5\"\nbase_url = \"https://api.x.ai/v1\"\nname = \"xAI\"\napi_key = \"sk-grok-0123456789ABCD\"\napi_backend = \"responses\"\ncontext_window = 500000\n" }),
        ),
        (
            ToolId::OpenClaw,
            json!({ "baseUrl": "https://openclaw.example.com", "apiKey": "sk-openclaw-0123456789ABCD" }),
        ),
        (
            ToolId::Hermes,
            json!({ "base_url": "https://hermes.example.com", "api_key": "sk-hermes-0123456789ABCD" }),
        ),
        (
            ToolId::Pi,
            json!({ "baseUrl": "https://pi.example.com", "apiKey": "sk-pi-0123456789ABCD" }),
        ),
    ];
    for (tool, settings) in cases {
        let raw = upstream("p", "P", settings);
        let provider = provider_from_upstream(tool, &raw, "p");
        assert!(
            provider
                .api_key
                .as_deref()
                .is_some_and(|key| key.ends_with("0123456789ABCD")),
            "{tool:?} lost its key during conversion"
        );
    }
}

#[test]
fn app_type_for_covers_exactly_the_eight_upstream_provider_tools() {
    assert_eq!(app_type_for(ToolId::ClaudeCode), AppType::Claude);
    assert_eq!(app_type_for(ToolId::Codex), AppType::Codex);
    assert_eq!(app_type_for(ToolId::OpenCode), AppType::OpenCode);
    assert_eq!(app_type_for(ToolId::GeminiCli), AppType::Gemini);
    assert_eq!(app_type_for(ToolId::GrokBuild), AppType::GrokBuild);
    assert_eq!(app_type_for(ToolId::OpenClaw), AppType::OpenClaw);
    assert_eq!(app_type_for(ToolId::Hermes), AppType::Hermes);
    assert_eq!(app_type_for(ToolId::Pi), AppType::Pi);
    assert_eq!(tool_id_to_app_type(ToolId::KimiCode), None);
    assert_eq!(tool_id_to_app_type(ToolId::DeepSeekDsh), None);
}

#[test]
fn every_upstream_health_status_maps_to_exactly_one_product_state() {
    assert_eq!(
        reachability_from(&HealthStatus::Operational),
        ProviderReachability::Operational
    );
    assert_eq!(
        reachability_from(&HealthStatus::Degraded),
        ProviderReachability::Degraded
    );
    assert_eq!(
        reachability_from(&HealthStatus::Failed),
        ProviderReachability::Failed
    );
}

#[test]
fn the_probe_budget_stays_inside_an_inline_spinner() {
    // The arithmetic premise of decision 2: a single probe takes at worst timeout * (1 + max_retries).
    // If upstream ever raises the defaults into the minutes, this goes red first and forces us to
    // reconsider the OperationManager.
    let config = crate::services::stream_check::StreamCheckConfig::default();
    let worst_case_secs = config.timeout_secs * u64::from(config.max_retries + 1);
    assert!(
        worst_case_secs <= 20,
        "a connection check now takes up to {worst_case_secs}s; that no longer belongs on a button"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn saved_failover_candidates_are_probed_concurrently_without_auth_headers() {
    use crate::database::Database;
    use crate::store::AppState;
    use std::sync::Arc;

    let gate = Arc::new(ProbeGate::default());
    let (first_url, first_server) = gated_endpoint(gate.clone());
    let (second_url, second_server) = gated_endpoint(gate);
    let state = AppState::new(Arc::new(
        Database::memory().expect("create failover test database"),
    ));
    for (id, url) in [("first", first_url), ("second", second_url)] {
        let provider = upstream(
            id,
            id,
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": url,
                    "ANTHROPIC_API_KEY": "sk-must-never-be-sent-0123456789"
                }
            }),
        );
        state
            .db
            .save_provider(AppType::Claude.as_str(), &provider)
            .expect("save failover candidate");
    }
    let store = ProviderStore { state };

    let results = store
        .test_for_failover(
            ToolId::ClaudeCode,
            &["first".to_string(), "second".to_string()],
        )
        .await
        .expect("probe failover candidates");

    assert_eq!(
        results
            .iter()
            .map(|result| result.provider_id.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    for server in [first_server, second_server] {
        let (overlapped, request) = server.join().expect("join failover endpoint");
        assert!(overlapped, "candidate probes ran sequentially");
        let request = request.to_ascii_lowercase();
        assert!(!request.contains("authorization:"));
        assert!(!request.contains("x-api-key:"));
        assert!(!request.contains("sk-must-never-be-sent"));
    }
}

#[tokio::test]
async fn failover_probe_rejects_an_oversized_renderer_id_batch_before_lookup() {
    use crate::database::Database;
    use crate::store::AppState;
    use std::sync::Arc;

    let store = ProviderStore {
        state: AppState::new(Arc::new(
            Database::memory().expect("create failover limit database"),
        )),
    };
    let ids = (0..=crate::domain::MAX_PROVIDER_ENDPOINT_CANDIDATES)
        .map(|index| format!("candidate-{index}"))
        .collect::<Vec<_>>();

    let error = store
        .test_for_failover(ToolId::ClaudeCode, &ids)
        .await
        .expect_err("oversized candidate list must fail closed");

    assert_eq!(error.code, crate::domain::ErrorCode::ProviderUnreachable);
}

#[test]
fn a_panic_while_holding_the_mutation_lock_does_not_disable_later_writes() {
    use crate::database::Database;
    use crate::store::AppState;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Arc;

    let store = ProviderStore {
        state: AppState::new(Arc::new(
            Database::memory().expect("create lock test database"),
        )),
    };
    let poisoned = catch_unwind(AssertUnwindSafe(|| {
        let _guard = store.lock_mutation();
        panic!("simulated panic while a provider write held the lock");
    }));
    assert!(
        poisoned.is_err(),
        "the simulated panic must poison the lock"
    );

    // The lock only orders writers; it guards no data. The next write must be
    // able to take it again instead of failing until the app restarts.
    let _guard = store.lock_mutation();
}

/// One loopback endpoint that answers the first probe with 204 immediately.
fn quick_endpoint() -> String {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind quick endpoint");
    let address = listener.local_addr().expect("read quick endpoint");
    std::thread::spawn(move || {
        if let Ok((mut socket, _)) = listener.accept() {
            let mut bytes = [0_u8; 4_096];
            let _ = socket.read(&mut bytes);
            let _ = socket.write_all(
                b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    format!("http://{address}")
}

fn failover_store(candidates: &[(&str, serde_json::Value)]) -> ProviderStore {
    use crate::database::Database;
    use crate::store::AppState;
    use std::sync::Arc;

    let state = AppState::new(Arc::new(
        Database::memory().expect("create failover round database"),
    ));
    for (id, settings) in candidates {
        state
            .db
            .save_provider(
                AppType::Claude.as_str(),
                &upstream(id, id, settings.clone()),
            )
            .expect("save failover candidate");
    }
    ProviderStore { state }
}

#[tokio::test]
#[serial_test::serial]
async fn a_candidate_deleted_mid_round_does_not_abort_the_other_probes() {
    let store = failover_store(&[(
        "still-here",
        json!({ "env": { "ANTHROPIC_BASE_URL": quick_endpoint() } }),
    )]);

    // The renderer snapshot named a service that was removed before the
    // round ran; the remaining candidates must still be measured.
    let results = store
        .test_for_failover(
            ToolId::ClaudeCode,
            &["gone-1f3a".to_string(), "still-here".to_string()],
        )
        .await
        .expect("the round continues past a missing candidate");

    assert_eq!(
        results
            .iter()
            .map(|result| result.provider_id.as_str())
            .collect::<Vec<_>>(),
        vec!["still-here"]
    );
    assert_eq!(results[0].reachability, ProviderReachability::Operational);
}

#[tokio::test]
#[serial_test::serial]
async fn a_candidate_without_an_address_is_reported_failed_instead_of_aborting_the_round() {
    let store = failover_store(&[
        (
            "no-address",
            json!({ "env": { "ANTHROPIC_API_KEY": "sk-no-address-0123456789" } }),
        ),
        (
            "still-here",
            json!({ "env": { "ANTHROPIC_BASE_URL": quick_endpoint() } }),
        ),
    ]);

    let results = store
        .test_for_failover(
            ToolId::ClaudeCode,
            &["no-address".to_string(), "still-here".to_string()],
        )
        .await
        .expect("an untestable candidate is one failed result, not a failed round");

    assert_eq!(
        results
            .iter()
            .map(|result| (result.provider_id.as_str(), result.reachability))
            .collect::<Vec<_>>(),
        vec![
            ("no-address", ProviderReachability::Failed),
            ("still-here", ProviderReachability::Operational),
        ]
    );
    assert_eq!(results[0].http_status, None);
}

#[test]
#[serial_test::serial]
fn the_empty_import_placeholder_stays_out_of_the_product_list() {
    use crate::database::Database;
    use crate::store::AppState;
    use std::sync::Arc;

    let state = AppState::new(Arc::new(Database::memory().expect("db")));
    let placeholder = UpstreamProvider::with_id(
        "default".into(),
        "default".into(),
        serde_json::json!({}),
        None,
    );
    let mut official = UpstreamProvider::with_id(
        "claude-official".into(),
        "Claude Official".into(),
        serde_json::json!({"env": {}}),
        None,
    );
    official.category = Some("official".into());
    for provider in [&placeholder, &official] {
        state
            .db
            .save_provider(AppType::Claude.as_str(), provider)
            .expect("save");
    }
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), "default")
        .expect("current");
    let store = ProviderStore { state };

    let ids: Vec<String> = store
        .list(ToolId::ClaudeCode)
        .expect("list")
        .into_iter()
        .map(|provider| provider.id)
        .collect();
    assert_eq!(ids, vec!["claude-official".to_string()]);
}

#[test]
#[serial_test::serial]
fn a_custom_default_with_a_key_is_not_a_placeholder() {
    use crate::database::Database;
    use crate::store::AppState;
    use std::sync::Arc;

    let state = AppState::new(Arc::new(Database::memory().expect("db")));
    let keyed_default = UpstreamProvider::with_id(
        "default".into(),
        "default".into(),
        serde_json::json!({"env": {
            "ANTHROPIC_BASE_URL": "https://relay.example.test",
            "ANTHROPIC_AUTH_TOKEN": "sk-default-0123456789ABCDEF"
        }}),
        None,
    );
    let mut official = UpstreamProvider::with_id(
        "claude-official".into(),
        "Claude Official".into(),
        serde_json::json!({"env": {}}),
        None,
    );
    official.category = Some("official".into());
    for provider in [&keyed_default, &official] {
        state
            .db
            .save_provider(AppType::Claude.as_str(), provider)
            .expect("save");
    }
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), "default")
        .expect("current");
    let store = ProviderStore { state };

    let mut ids: Vec<String> = store
        .list(ToolId::ClaudeCode)
        .expect("list")
        .into_iter()
        .map(|provider| provider.id)
        .collect();
    ids.sort();
    let mut expected = vec!["default".to_string(), "claude-official".to_string()];
    expected.sort();
    assert_eq!(ids, expected);
}
