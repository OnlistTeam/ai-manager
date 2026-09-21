//! Proxy session - request session management
//!
//! Creates a session context for every proxied request and tracks state and metadata for its whole lifetime.
//!
//! ## Session ID extraction
//!
//! A session ID can be extracted from the client request to correlate requests of one conversation:
//! - Claude: from `metadata.user_id` (format: `user_xxx_session_yyy`) or `metadata.session_id`
//! - Codex: from the `session_id` / `x-session-id` headers or `metadata.session_id`
//! - Grok Build: from the `x-grok-conv-id` / `x-grok-session-id` headers
//! - Everything else: generate a new UUID

use axum::http::HeaderMap;
use uuid::Uuid;

// ============================================================================
// Session ID extractor
// ============================================================================

/// Where the session ID came from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIdSource {
    /// Extracted from metadata.user_id (Claude)
    MetadataUserId,
    /// Extracted from metadata.session_id
    MetadataSessionId,
    /// Extracted from headers
    Header,
    /// Newly generated
    Generated,
}

/// Result of session ID extraction
#[derive(Debug, Clone)]
pub struct SessionIdResult {
    /// The extracted or generated session ID
    pub session_id: String,
    /// Where the session ID came from
    pub source: SessionIdSource,
    /// Whether the client supplied the ID (i.e. not newly generated)
    pub client_provided: bool,
}

/// Extracts a session ID from the request, or generates one
///
/// Lightweight: it only extracts session_id for logging and does no real session management.
///
/// ## Extraction priority
///
/// ### Claude requests
/// 1. `metadata.user_id` (format: `user_xxx_session_yyy`) -> take the `yyy` part
/// 2. `metadata.session_id` -> use as is
/// 3. Generate a new UUID
///
/// ### Codex requests
/// 1. Headers: `session_id` or `x-session-id`
/// 2. `metadata.session_id`
/// 3. Generate a new UUID
///
/// ### Grok Build requests
/// 1. Headers: `x-grok-conv-id` or `x-grok-session-id`
/// 2. `metadata.session_id`
/// 3. Generate a new UUID
///
/// ## Example
///
/// ```ignore
/// let result = extract_session_id(&headers, &body, "claude");
/// println!("Session ID: {} (from {:?})", result.session_id, result.source);
/// ```
pub fn extract_session_id(
    headers: &HeaderMap,
    body: &serde_json::Value,
    client_format: &str,
) -> SessionIdResult {
    if client_format == "claude" {
        if let Some(result) = extract_claude_session(headers, body) {
            return result;
        }
    }

    // Special handling for Responses requests. Grok Build speaks the same client protocol as Codex,
    // but keeps its own prefix so stats and cache keys cannot collide across apps.
    if matches!(client_format, "codex" | "openai" | "grokbuild") {
        let prefix = if client_format == "grokbuild" {
            "grokbuild"
        } else {
            "codex"
        };
        if let Some(result) = extract_responses_session(headers, body, prefix) {
            return result;
        }
    }

    // Claude requests: extract from metadata
    if let Some(result) = extract_from_metadata(body) {
        return result;
    }

    // Fallback: generate a new session ID
    generate_new_session_id()
}

/// Extracts the Claude session ID
fn extract_claude_session(
    headers: &HeaderMap,
    body: &serde_json::Value,
) -> Option<SessionIdResult> {
    for header_name in &["x-claude-code-session-id", "claude-code-session-id"] {
        if let Some(value) = headers.get(*header_name) {
            if let Ok(session_id) = value.to_str() {
                if !session_id.is_empty() {
                    return Some(SessionIdResult {
                        session_id: session_id.to_string(),
                        source: SessionIdSource::Header,
                        client_provided: true,
                    });
                }
            }
        }
    }

    extract_from_metadata(body)
}

/// Extracts the session ID of a Responses client
fn extract_responses_session(
    headers: &HeaderMap,
    body: &serde_json::Value,
    prefix: &str,
) -> Option<SessionIdResult> {
    // 1. Extract from headers
    let header_names: &[&str] = if prefix == "grokbuild" {
        // The conversation ID is stable across turns; the session ID is the fallback when the client has
        // no conversation ID. x-grok-req-id is per-request and must not be used for aggregation.
        &["x-grok-conv-id", "x-grok-session-id"]
    } else {
        &["session_id", "x-session-id"]
    };
    for header_name in header_names {
        if let Some(value) = headers.get(*header_name) {
            if let Ok(session_id) = value.to_str() {
                let session_id = session_id.trim();
                // Responses clients usually have long session IDs (UUID form)
                if session_id.len() > 20 {
                    return Some(SessionIdResult {
                        session_id: format!("{prefix}_{session_id}"),
                        source: SessionIdSource::Header,
                        client_provided: true,
                    });
                }
            }
        }
    }

    // 2. Extract from body.metadata.session_id
    if let Some(session_id) = body
        .get("metadata")
        .and_then(|m| m.get("session_id"))
        .and_then(|v| v.as_str())
    {
        let session_id = session_id.trim();
        if session_id.len() > 10 {
            return Some(SessionIdResult {
                session_id: format!("{prefix}_{session_id}"),
                source: SessionIdSource::MetadataSessionId,
                client_provided: true,
            });
        }
    }

    // previous_response_id is a response cursor in the Responses protocol, not a stable session identity.
    // When bridging Chat/Responses it usually comes from the random response id upstream returns each
    // turn, so using it as prompt_cache_key or the Codex session header changes the cache key every turn.

    None
}

/// Extracts the session ID from metadata (Claude)
fn extract_from_metadata(body: &serde_json::Value) -> Option<SessionIdResult> {
    let metadata = body.get("metadata")?;

    // 1. Extract from metadata.user_id (format: user_xxx_session_yyy)
    if let Some(user_id) = metadata.get("user_id").and_then(|v| v.as_str()) {
        if let Some(session_id) = parse_session_from_user_id(user_id) {
            return Some(SessionIdResult {
                session_id,
                source: SessionIdSource::MetadataUserId,
                client_provided: true,
            });
        }
    }

    // 2. Extract directly from metadata.session_id
    if let Some(session_id) = metadata.get("session_id").and_then(|v| v.as_str()) {
        if !session_id.is_empty() {
            return Some(SessionIdResult {
                session_id: session_id.to_string(),
                source: SessionIdSource::MetadataSessionId,
                client_provided: true,
            });
        }
    }

    None
}

/// Parses session_id out of user_id
///
/// Format: `user_identifier_session_actual_session_id`
pub(super) fn parse_session_from_user_id(user_id: &str) -> Option<String> {
    // Find the "_session_" separator
    if let Some(pos) = user_id.find("_session_") {
        let session_id = &user_id[pos + 9..]; // "_session_" is 9 chars long
        if !session_id.is_empty() {
            return Some(session_id.to_string());
        }
    }
    None
}

/// Generates a new session ID
fn generate_new_session_id() -> SessionIdResult {
    SessionIdResult {
        session_id: Uuid::new_v4().to_string(),
        source: SessionIdSource::Generated,
        client_provided: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ========== Session ID extraction tests ==========

    #[test]
    fn test_extract_session_from_claude_metadata_user_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}],
            "metadata": {
                "user_id": "user_john_doe_session_abc123def456"
            }
        });

        let result = extract_session_id(&headers, &body, "claude");

        assert_eq!(result.session_id, "abc123def456");
        assert_eq!(result.source, SessionIdSource::MetadataUserId);
        assert!(result.client_provided);
    }

    #[test]
    fn test_extract_session_from_claude_metadata_session_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}],
            "metadata": {
                "session_id": "my-session-123"
            }
        });

        let result = extract_session_id(&headers, &body, "claude");

        assert_eq!(result.session_id, "my-session-123");
        assert_eq!(result.source, SessionIdSource::MetadataSessionId);
        assert!(result.client_provided);
    }

    #[test]
    fn test_extract_session_from_claude_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-claude-code-session-id",
            "d937243f-2702-4f20-97b6-c9682235ab81".parse().unwrap(),
        );
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = extract_session_id(&headers, &body, "claude");

        assert_eq!(result.session_id, "d937243f-2702-4f20-97b6-c9682235ab81");
        assert_eq!(result.source, SessionIdSource::Header);
        assert!(result.client_provided);
    }

    #[test]
    fn test_extract_session_from_claude_header_precedes_metadata() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-claude-code-session-id",
            "header-session-123".parse().unwrap(),
        );
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}],
            "metadata": {
                "session_id": "my-session-123"
            }
        });

        let result = extract_session_id(&headers, &body, "claude");

        assert_eq!(result.session_id, "header-session-123");
        assert_eq!(result.source, SessionIdSource::Header);
        assert!(result.client_provided);
    }

    #[test]
    fn test_codex_previous_response_id_is_not_stable_session_identity() {
        let headers = HeaderMap::new();
        let body = json!({
            "input": "Write a function",
            "previous_response_id": "resp_abc123def456789"
        });

        let result = extract_session_id(&headers, &body, "codex");

        assert!(!result.session_id.is_empty());
        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }

    #[test]
    fn test_codex_keeps_existing_response_session_headers() {
        let body = json!({ "input": "Write a function" });

        for header_name in ["session_id", "x-session-id"] {
            let mut headers = HeaderMap::new();
            headers.insert(
                header_name,
                "d937243f-2702-4f20-97b6-c9682235ab81".parse().unwrap(),
            );

            let result = extract_session_id(&headers, &body, "codex");

            assert_eq!(
                result.session_id,
                "codex_d937243f-2702-4f20-97b6-c9682235ab81"
            );
            assert_eq!(result.source, SessionIdSource::Header);
            assert!(result.client_provided);
        }
    }

    #[test]
    fn test_grokbuild_prefers_conversation_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-grok-conv-id",
            "conv-724f4275-584e-43af-ad46-b5e7509a3ca2".parse().unwrap(),
        );
        headers.insert(
            "x-grok-session-id",
            "session-d937243f-2702-4f20-97b6-c9682235ab81"
                .parse()
                .unwrap(),
        );
        let body = json!({ "input": "Write a function" });

        let result = extract_session_id(&headers, &body, "grokbuild");

        assert_eq!(
            result.session_id,
            "grokbuild_conv-724f4275-584e-43af-ad46-b5e7509a3ca2"
        );
        assert_eq!(result.source, SessionIdSource::Header);
        assert!(result.client_provided);
    }

    #[test]
    fn test_grokbuild_falls_back_to_session_header() {
        let body = json!({ "input": "Write a function" });

        for conversation_id in ["", "                         "] {
            let mut headers = HeaderMap::new();
            headers.insert("x-grok-conv-id", conversation_id.parse().unwrap());
            headers.insert(
                "x-grok-session-id",
                "session-d937243f-2702-4f20-97b6-c9682235ab81"
                    .parse()
                    .unwrap(),
            );

            let result = extract_session_id(&headers, &body, "grokbuild");

            assert_eq!(
                result.session_id,
                "grokbuild_session-d937243f-2702-4f20-97b6-c9682235ab81"
            );
            assert_eq!(result.source, SessionIdSource::Header);
            assert!(result.client_provided);
        }
    }

    #[test]
    fn test_grokbuild_ignores_request_and_codex_session_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-grok-req-id",
            "request-724f4275-584e-43af-ad46-b5e7509a3ca2"
                .parse()
                .unwrap(),
        );
        headers.insert(
            "x-session-id",
            "codex-d937243f-2702-4f20-97b6-c9682235ab81"
                .parse()
                .unwrap(),
        );
        let body = json!({ "input": "Write a function" });

        let result = extract_session_id(&headers, &body, "grokbuild");

        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }

    #[test]
    fn test_extract_session_generates_new_when_not_found() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = extract_session_id(&headers, &body, "claude");

        assert!(!result.session_id.is_empty());
        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }

    #[test]
    fn test_parse_session_from_user_id() {
        assert_eq!(
            parse_session_from_user_id("user_john_session_abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            parse_session_from_user_id("my_app_session_xyz789"),
            Some("xyz789".to_string())
        );
        // Note: "_session_" is the separator, so the string below matches
        assert_eq!(
            parse_session_from_user_id("no_session_marker"),
            Some("marker".to_string())
        );
        // Case with no "_session_" separator
        assert_eq!(parse_session_from_user_id("user_john_abc123"), None);
        assert_eq!(parse_session_from_user_id("_session_"), None);
    }
}
