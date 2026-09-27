//! Live routing trace hooks for the inherited proxy forwarder (ADR-0050).
//!
//! The forwarder calls these at the points where it already decides: a
//! service passed over, a try started, the answer began, the request failed.
//! A started try is pushed at once as a `pending` attempt and replaced by its
//! outcome when it ends, so the renderer can show a request on its way.
//! Everything here reduces inherited types to the content-free trace entry;
//! request bodies are read only for the requested model name.

use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use crate::app_config::AppType;
use crate::domain::routing_trace::bounded_model_name;
use crate::domain::{
    RoutingAttemptOutcome, RoutingErrorCategory, RoutingTraceAttempt, RoutingTraceStatus, ToolId,
};
use crate::infrastructure::RoutingTraceLog;
use crate::provider::Provider;
use crate::proxy::ProxyError;

use super::ROUTING_APPS;

struct OpenAttempt {
    provider_id: String,
    provider_name: String,
    started: Instant,
}

struct Recording {
    log: Arc<RoutingTraceLog>,
    seq: u64,
    started: Instant,
    open: Option<OpenAttempt>,
    answered: bool,
    finished: bool,
}

/// One request's trace. A disabled trace (no log registered, or a tool
/// without a routing target) accepts every call and records nothing, so the
/// forwarder hooks stay one line each.
///
/// Dropping the trace closes the request: after `answered` it counts as
/// succeeded once the response (including a streamed body) is released;
/// before that it means the tool went away mid-request.
pub struct RequestTrace(Option<Recording>);

impl RequestTrace {
    pub fn disabled() -> Self {
        Self(None)
    }

    /// Starts a trace when the app has a trace log and the tool is a routing target.
    pub fn begin(
        app_handle: Option<&tauri::AppHandle>,
        app_type: &AppType,
        endpoint: &str,
        body: &Value,
    ) -> Self {
        let Some(log) = app_handle
            .and_then(tauri::Manager::try_state::<Arc<RoutingTraceLog>>)
            .map(|state| state.inner().clone())
        else {
            return Self::disabled();
        };
        let Some(tool) = tool_for_app(app_type.as_str()) else {
            return Self::disabled();
        };
        Self::start(log, tool, requested_model(endpoint, body))
    }

    pub(crate) fn start(log: Arc<RoutingTraceLog>, tool: ToolId, model: Option<String>) -> Self {
        let started_at = chrono::Utc::now().timestamp_millis();
        let seq = log.begin(tool, model, started_at);
        Self(Some(Recording {
            log,
            seq,
            started: Instant::now(),
            open: None,
            answered: false,
            finished: false,
        }))
    }

    /// The forwarder passed over `provider` without sending anything.
    /// `previous_error` is the error that ended the try still open, if any.
    pub fn skipped(&mut self, provider: &Provider, previous_error: Option<&ProxyError>) {
        let Some(recording) = self.0.as_mut() else {
            return;
        };
        recording.close_open(previous_error);
        recording.log.push_attempt(
            recording.seq,
            RoutingTraceAttempt {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                outcome: RoutingAttemptOutcome::Skipped,
                http_status: None,
                error: None,
                ms: 0,
            },
        );
    }

    /// The forwarder is about to send to `provider`.
    /// `previous_error` is the error that ended the try still open, if any.
    pub fn attempt(&mut self, provider: &Provider, previous_error: Option<&ProxyError>) {
        let Some(recording) = self.0.as_mut() else {
            return;
        };
        recording.close_open(previous_error);
        recording.log.push_attempt(
            recording.seq,
            RoutingTraceAttempt {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                outcome: RoutingAttemptOutcome::Pending,
                http_status: None,
                error: None,
                ms: 0,
            },
        );
        recording.open = Some(OpenAttempt {
            provider_id: provider.id.clone(),
            provider_name: provider.name.clone(),
            started: Instant::now(),
        });
    }

    /// `provider` started answering. The request stays pending until the
    /// response is released, so a streamed answer is timed to its end.
    pub fn answered(&mut self, provider: &Provider) {
        let Some(recording) = self.0.as_mut() else {
            return;
        };
        let open = recording.open.take();
        let ms = open.as_ref().map_or(0, |open| elapsed_ms(open.started));
        recording.log.settle_attempt(
            recording.seq,
            RoutingTraceAttempt {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                outcome: RoutingAttemptOutcome::Ok,
                http_status: None,
                error: None,
                ms,
            },
        );
        recording.answered = true;
    }

    /// The request failed with `error`, which also ends the open try.
    pub fn failed(&mut self, error: &ProxyError) {
        let Some(recording) = self.0.as_mut() else {
            return;
        };
        recording.close_open(Some(error));
        recording.finish(RoutingTraceStatus::Failed, Some(error_category(error)));
    }
}

impl Recording {
    fn close_open(&mut self, error: Option<&ProxyError>) {
        let Some(open) = self.open.take() else {
            return;
        };
        self.log.settle_attempt(
            self.seq,
            RoutingTraceAttempt {
                provider_id: open.provider_id,
                provider_name: open.provider_name,
                outcome: RoutingAttemptOutcome::Failed,
                http_status: error.and_then(http_status),
                error: Some(error.map_or(RoutingErrorCategory::Other, error_category)),
                ms: elapsed_ms(open.started),
            },
        );
    }

    fn finish(&mut self, status: RoutingTraceStatus, error: Option<RoutingErrorCategory>) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.log
            .finish(self.seq, status, error, elapsed_ms(self.started));
    }
}

impl Drop for RequestTrace {
    fn drop(&mut self) {
        let Some(recording) = self.0.as_mut() else {
            return;
        };
        if recording.answered {
            recording.finish(RoutingTraceStatus::Ok, None);
            return;
        }
        if let Some(open) = recording.open.take() {
            recording.log.settle_attempt(
                recording.seq,
                RoutingTraceAttempt {
                    provider_id: open.provider_id,
                    provider_name: open.provider_name,
                    outcome: RoutingAttemptOutcome::Failed,
                    http_status: None,
                    error: Some(RoutingErrorCategory::Cancelled),
                    ms: elapsed_ms(open.started),
                },
            );
        }
        recording.finish(
            RoutingTraceStatus::Failed,
            Some(RoutingErrorCategory::Cancelled),
        );
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn tool_for_app(app: &str) -> Option<ToolId> {
    ROUTING_APPS
        .iter()
        .find_map(|entry| (entry.app == app).then_some(entry.tool))
}

/// The model the tool asked for: the body's `model`, or for path-addressed
/// APIs the `models/<name>` segment of the endpoint.
fn requested_model(endpoint: &str, body: &Value) -> Option<String> {
    if let Some(model) = body.get("model").and_then(Value::as_str) {
        return bounded_model_name(model);
    }
    let (_, rest) = endpoint.split_once("models/")?;
    let name = rest.split([':', '?', '/']).next().unwrap_or_default();
    bounded_model_name(name)
}

fn http_status(error: &ProxyError) -> Option<u16> {
    match error {
        ProxyError::UpstreamError { status, .. } => Some(*status),
        _ => None,
    }
}

pub(crate) fn error_category(error: &ProxyError) -> RoutingErrorCategory {
    match error {
        ProxyError::UpstreamError { status, .. } => match *status {
            429 => RoutingErrorCategory::RateLimited,
            401 | 403 => RoutingErrorCategory::AuthFailed,
            408 => RoutingErrorCategory::Timeout,
            500..=599 => RoutingErrorCategory::ServerError,
            400..=499 => RoutingErrorCategory::Rejected,
            _ => RoutingErrorCategory::Other,
        },
        ProxyError::AuthError(_) => RoutingErrorCategory::AuthFailed,
        ProxyError::Timeout(_) => RoutingErrorCategory::Timeout,
        ProxyError::ForwardFailed(_) => RoutingErrorCategory::Network,
        ProxyError::InvalidRequest(_) => RoutingErrorCategory::Rejected,
        ProxyError::NoAvailableProvider
        | ProxyError::AllProvidersCircuitOpen
        | ProxyError::NoProvidersConfigured
        | ProxyError::MaxRetriesExceeded => RoutingErrorCategory::Unavailable,
        _ => RoutingErrorCategory::Other,
    }
}

#[cfg(test)]
#[path = "tests_trace.rs"]
mod tests;
