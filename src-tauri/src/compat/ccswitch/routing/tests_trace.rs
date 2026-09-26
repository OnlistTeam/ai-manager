use std::sync::{Arc, Mutex};

use serde_json::json;

use super::{error_category, requested_model, RequestTrace};
use crate::domain::{
    RoutingAttemptOutcome, RoutingErrorCategory, RoutingTraceStatus, RoutingTraceUpdate, ToolId,
};
use crate::infrastructure::{RoutingTraceEvents, RoutingTraceLog};
use crate::provider::Provider;
use crate::proxy::ProxyError;

#[derive(Default)]
struct Recorded(Mutex<Vec<RoutingTraceUpdate>>);

impl RoutingTraceEvents for Arc<Recorded> {
    fn emit(&self, update: &RoutingTraceUpdate) {
        self.0.lock().expect("recorded events").push(update.clone());
    }
}

fn trace_log() -> (Arc<RoutingTraceLog>, Arc<Recorded>) {
    let recorded = Arc::new(Recorded::default());
    (
        Arc::new(RoutingTraceLog::new(Box::new(recorded.clone()))),
        recorded,
    )
}

fn provider(id: &str, name: &str) -> Provider {
    let mut provider = Provider::with_id(
        id.to_string(),
        name.to_string(),
        json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "sk-never-on-the-trace",
                "ANTHROPIC_BASE_URL": "https://user:pass@relay.example.com"
            }
        }),
        None,
    );
    provider.notes = Some("private operator note".to_string());
    provider
}

fn upstream(status: u16) -> ProxyError {
    ProxyError::UpstreamError {
        status,
        body: Some("{\"error\":\"Bearer sk-upstream-secret leaked in body\"}".to_string()),
    }
}

#[test]
fn a_direct_success_is_pending_until_the_response_is_released() {
    let (log, _) = trace_log();
    let a = provider("a", "Service A");
    let mut trace = RequestTrace::start(log.clone(), ToolId::ClaudeCode, Some("opus".into()));
    trace.attempt(&a, None);
    trace.answered(&a);

    let entry = log.snapshot().entries[0].clone();
    assert_eq!(entry.status, RoutingTraceStatus::Pending);
    assert_eq!(entry.attempts.len(), 1);
    assert_eq!(entry.attempts[0].outcome, RoutingAttemptOutcome::Ok);
    assert_eq!(entry.model.as_deref(), Some("opus"));

    drop(trace);
    let snapshot = log.snapshot();
    assert_eq!(snapshot.entries[0].status, RoutingTraceStatus::Ok);
    assert!(snapshot.entries[0].total_ms.is_some());
    assert!(!snapshot.entries[0].failed_over);
    assert_eq!(snapshot.counts.failed, 0);
}

#[test]
fn failover_records_each_try_in_order_with_its_category() {
    let (log, _) = trace_log();
    let (a, b, c) = (
        provider("a", "Service A"),
        provider("b", "Service B"),
        provider("c", "Service C"),
    );
    let mut trace = RequestTrace::start(log.clone(), ToolId::Codex, None);
    trace.attempt(&a, None);
    let first_error = upstream(429);
    trace.skipped(&b, Some(&first_error));
    trace.attempt(&c, Some(&first_error));
    trace.answered(&c);
    drop(trace);

    let snapshot = log.snapshot();
    let entry = &snapshot.entries[0];
    assert_eq!(
        entry
            .attempts
            .iter()
            .map(|attempt| (attempt.provider_id.as_str(), attempt.outcome))
            .collect::<Vec<_>>(),
        vec![
            ("a", RoutingAttemptOutcome::Failed),
            ("b", RoutingAttemptOutcome::Skipped),
            ("c", RoutingAttemptOutcome::Ok),
        ]
    );
    assert_eq!(entry.attempts[0].http_status, Some(429));
    assert_eq!(
        entry.attempts[0].error,
        Some(RoutingErrorCategory::RateLimited)
    );
    assert_eq!(entry.status, RoutingTraceStatus::Ok);
    assert!(entry.failed_over);
    assert_eq!(snapshot.counts.rerouted, 1);
}

#[test]
fn when_every_service_fails_the_request_fails_with_the_last_category() {
    let (log, _) = trace_log();
    let (a, b) = (provider("a", "Service A"), provider("b", "Service B"));
    let mut trace = RequestTrace::start(log.clone(), ToolId::GeminiCli, None);
    trace.attempt(&a, None);
    let first = ProxyError::Timeout("30s".into());
    trace.attempt(&b, Some(&first));
    trace.failed(&upstream(503));
    drop(trace);

    let snapshot = log.snapshot();
    let entry = &snapshot.entries[0];
    assert_eq!(entry.status, RoutingTraceStatus::Failed);
    assert_eq!(entry.error, Some(RoutingErrorCategory::ServerError));
    assert_eq!(entry.attempts[0].error, Some(RoutingErrorCategory::Timeout));
    assert_eq!(entry.attempts[1].http_status, Some(503));
    assert_eq!(snapshot.counts.failed, 1);
    assert_eq!(snapshot.counts.rerouted, 1);
}

#[test]
fn a_request_dropped_mid_try_is_recorded_as_cancelled() {
    let (log, _) = trace_log();
    let a = provider("a", "Service A");
    let mut trace = RequestTrace::start(log.clone(), ToolId::GrokBuild, None);
    trace.attempt(&a, None);
    drop(trace);

    let entry = log.snapshot().entries[0].clone();
    assert_eq!(entry.status, RoutingTraceStatus::Failed);
    assert_eq!(entry.error, Some(RoutingErrorCategory::Cancelled));
    assert_eq!(
        entry.attempts[0].error,
        Some(RoutingErrorCategory::Cancelled)
    );
}

#[test]
fn a_disabled_trace_records_nothing() {
    let a = provider("a", "Service A");
    let mut trace =
        RequestTrace::begin(None, &crate::app_config::AppType::Claude, "/v1", &json!({}));
    trace.attempt(&a, None);
    trace.answered(&a);
    drop(trace);
    // Nothing to observe beyond not panicking: there is no log to write to.
}

#[test]
fn the_pushed_payload_carries_no_content_keys_urls_or_error_bodies() {
    let (log, recorded) = trace_log();
    let (a, b) = (provider("a", "Service A"), provider("b", "Service B"));
    let mut trace = RequestTrace::start(log.clone(), ToolId::ClaudeCode, Some("sonnet".into()));
    trace.attempt(&a, None);
    let error = upstream(401);
    trace.attempt(&b, Some(&error));
    trace.failed(&ProxyError::ForwardFailed(
        "connect https://user:pass@relay.example.com failed".into(),
    ));
    drop(trace);

    let mut wire = serde_json::to_string(&log.snapshot()).expect("snapshot json");
    for update in recorded.0.lock().expect("events").iter() {
        wire.push_str(&serde_json::to_string(update).expect("update json"));
    }
    for private in [
        "sk-never-on-the-trace",
        "sk-upstream-secret",
        "Bearer",
        "user:pass",
        "relay.example.com",
        "private operator note",
        "settingsConfig",
        "body",
        "headers",
        "url",
    ] {
        assert!(!wire.contains(private), "trace leaked {private}");
    }
    assert!(wire.contains("\"authFailed\""));
    assert!(wire.contains("\"network\""));
}

#[test]
fn the_requested_model_comes_from_the_body_or_the_models_path_segment() {
    assert_eq!(
        requested_model("/v1/messages", &json!({ "model": " claude-opus " })),
        Some("claude-opus".to_string())
    );
    assert_eq!(
        requested_model(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
            &json!({ "contents": [] })
        ),
        Some("gemini-2.5-pro".to_string())
    );
    assert_eq!(requested_model("/v1/responses", &json!({})), None);
}

#[test]
fn errors_reduce_to_short_categories() {
    for (error, expected) in [
        (upstream(429), RoutingErrorCategory::RateLimited),
        (upstream(403), RoutingErrorCategory::AuthFailed),
        (upstream(404), RoutingErrorCategory::Rejected),
        (upstream(502), RoutingErrorCategory::ServerError),
        (
            ProxyError::AuthError("expired".into()),
            RoutingErrorCategory::AuthFailed,
        ),
        (
            ProxyError::ForwardFailed("reset".into()),
            RoutingErrorCategory::Network,
        ),
        (
            ProxyError::NoAvailableProvider,
            RoutingErrorCategory::Unavailable,
        ),
        (
            ProxyError::Internal("boom".into()),
            RoutingErrorCategory::Other,
        ),
    ] {
        assert_eq!(error_category(&error), expected, "{error}");
    }
}
