//! Safe product wire types for testing several provider endpoints at once.
//!
//! Endpoint URLs are input-only. Results are correlated by an opaque candidate
//! ID so a URL cannot leak back through IPC, logs, or an error presentation.

use serde::{Deserialize, Serialize};

pub const MAX_PROVIDER_ENDPOINT_CANDIDATES: usize = 32;
pub const MAX_PROVIDER_ENDPOINT_ID_BYTES: usize = 128;

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProviderEndpointCandidate {
    pub id: String,
    pub url: String,
}

impl std::fmt::Debug for ProviderEndpointCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderEndpointCandidate")
            .field("id", &self.id)
            .field("url", &"<redacted>")
            .finish()
    }
}

/// Stable machine-readable reasons. Renderer code owns all display copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderEndpointFailure {
    InvalidUrl,
    Timeout,
    Dns,
    Tls,
    Connection,
    Request,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEndpointTestResult {
    pub candidate_id: String,
    pub latency_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub failure: Option<ProviderEndpointFailure>,
}

#[cfg(test)]
mod tests {
    use super::{ProviderEndpointCandidate, ProviderEndpointFailure, ProviderEndpointTestResult};

    #[test]
    fn candidate_urls_are_input_only_and_redacted_from_debug() {
        let candidate: ProviderEndpointCandidate = serde_json::from_str(
            r#"{"id":"official-cn","url":"https://user:secret@example.test/v1?token=hidden"}"#,
        )
        .expect("deserialize candidate");

        let debug = format!("{candidate:?}");
        assert!(debug.contains("official-cn"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("example.test"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("hidden"));
    }

    #[test]
    fn result_wire_format_has_no_url_or_display_message() {
        let result = ProviderEndpointTestResult {
            candidate_id: "official-cn".to_string(),
            latency_ms: None,
            http_status: None,
            failure: Some(ProviderEndpointFailure::Dns),
        };

        let json = serde_json::to_string(&result).expect("serialize endpoint result");
        assert_eq!(
            json,
            r#"{"candidateId":"official-cn","latencyMs":null,"httpStatus":null,"failure":"dns"}"#
        );
        assert!(!json.contains("url"));
        assert!(!json.contains("message"));
    }

    #[test]
    fn every_failure_reason_has_a_stable_camel_case_value() {
        for (failure, expected) in [
            (ProviderEndpointFailure::InvalidUrl, "\"invalidUrl\""),
            (ProviderEndpointFailure::Timeout, "\"timeout\""),
            (ProviderEndpointFailure::Dns, "\"dns\""),
            (ProviderEndpointFailure::Tls, "\"tls\""),
            (ProviderEndpointFailure::Connection, "\"connection\""),
            (ProviderEndpointFailure::Request, "\"request\""),
        ] {
            assert_eq!(serde_json::to_string(&failure).unwrap(), expected);
        }
    }
}
