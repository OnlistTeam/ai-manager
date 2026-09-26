//! Live routing trace wire format (ADR-0050, amending ADR-0007 decision 4).
//!
//! One entry per request the local route handled: which tool sent it, the
//! model it asked for, which services were tried in order and how each try
//! ended, and how long the request took. Nothing else crosses the IPC: no
//! request or response content, headers, keys, URLs or upstream error bodies.
//! An upstream failure is reduced to a short category.

use serde::{Deserialize, Serialize};

use super::ToolId;

/// How many recent requests the in-memory trace keeps.
pub const MAX_ROUTING_TRACE_ENTRIES: usize = 60;
/// Upper bound on recorded tries per request; the proxy's own retry cap is far lower.
pub const MAX_ROUTING_TRACE_ATTEMPTS: usize = 16;
/// Model names are identifiers, not content; anything longer is cut.
pub const MAX_ROUTING_TRACE_MODEL_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoutingAttemptOutcome {
    /// The try has started and no answer or error has arrived yet. It is
    /// replaced by the try's final outcome when it ends.
    Pending,
    Ok,
    Failed,
    /// The service was passed over without a request, e.g. while it is paused
    /// after repeated failures.
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoutingTraceStatus {
    Pending,
    Ok,
    Failed,
}

/// A short, content-free reason for a failed try or request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoutingErrorCategory {
    RateLimited,
    AuthFailed,
    ServerError,
    Timeout,
    Network,
    /// The service refused the request itself (a 4xx other than auth or rate limit).
    Rejected,
    /// No service could take the request.
    Unavailable,
    /// The tool stopped waiting before an answer arrived.
    Cancelled,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingTraceAttempt {
    pub provider_id: String,
    pub provider_name: String,
    pub outcome: RoutingAttemptOutcome,
    pub http_status: Option<u16>,
    pub error: Option<RoutingErrorCategory>,
    pub ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingTraceEntry {
    /// Monotonic per app run; the newest request has the highest number.
    pub seq: u64,
    /// The trace revision of this entry's latest change, so a reader can keep
    /// whichever copy of an entry is newer.
    pub revision: u64,
    /// Unix epoch milliseconds.
    pub started_at: i64,
    pub tool: ToolId,
    /// The model as the tool asked for it, before any mapping.
    pub model: Option<String>,
    pub attempts: Vec<RoutingTraceAttempt>,
    pub status: RoutingTraceStatus,
    /// Why the request failed; set only when `status` is `failed`.
    pub error: Option<RoutingErrorCategory>,
    /// Set once the request has finished, including a streamed answer.
    pub total_ms: Option<u64>,
    /// More than one service was tried.
    pub failed_over: bool,
}

/// Running totals since the app started, independent of the bounded list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingTraceCounts {
    pub requests: u64,
    /// Requests that were tried on more than one service.
    pub rerouted: u64,
    /// Requests whose failure reached the tool.
    pub failed: u64,
}

/// What the read command returns: newest entry first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingTraceSnapshot {
    pub revision: u64,
    pub counts: RoutingTraceCounts,
    pub entries: Vec<RoutingTraceEntry>,
}

/// What the `routing://trace` event carries: one changed entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingTraceUpdate {
    pub revision: u64,
    pub counts: RoutingTraceCounts,
    pub entry: RoutingTraceEntry,
}

/// Trims and bounds a requested model name; empty names are dropped.
pub fn bounded_model_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .chars()
            .take(MAX_ROUTING_TRACE_MODEL_CHARS)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_names_are_trimmed_and_bounded() {
        assert_eq!(bounded_model_name("  "), None);
        assert_eq!(bounded_model_name(" gpt-5 "), Some("gpt-5".to_string()));
        let long = "m".repeat(MAX_ROUTING_TRACE_MODEL_CHARS + 10);
        assert_eq!(
            bounded_model_name(&long).map(|name| name.chars().count()),
            Some(MAX_ROUTING_TRACE_MODEL_CHARS)
        );
    }

    #[test]
    fn wire_names_are_camel_case() {
        let json = serde_json::to_value(RoutingTraceAttempt {
            provider_id: "a".to_string(),
            provider_name: "A".to_string(),
            outcome: RoutingAttemptOutcome::Failed,
            http_status: Some(429),
            error: Some(RoutingErrorCategory::RateLimited),
            ms: 12,
        })
        .expect("serialize attempt");
        assert_eq!(
            json,
            serde_json::json!({
                "providerId": "a",
                "providerName": "A",
                "outcome": "failed",
                "httpStatus": 429,
                "error": "rateLimited",
                "ms": 12
            })
        );
        assert_eq!(
            serde_json::to_value(RoutingAttemptOutcome::Pending).expect("serialize outcome"),
            serde_json::json!("pending")
        );
    }
}
