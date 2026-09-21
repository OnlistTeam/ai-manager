//! Response parser - extracts token usage from API responses
//!
//! Supported API formats:
//! - Claude API (non-streaming and streaming)
//! - OpenRouter (OpenAI format)
//! - Codex API (non-streaming and streaming)
//! - Gemini API (non-streaming and streaming)

use serde::{Deserialize, Serialize};
use serde_json::Value;

fn openai_cache_read_tokens(usage: &Value) -> u32 {
    usage
        .get("cache_read_input_tokens")
        .or_else(|| usage.pointer("/input_tokens_details/cached_tokens"))
        .or_else(|| usage.pointer("/prompt_tokens_details/cached_tokens"))
        // DeepSeek Chat's documented cache-hit field, used as the last fallback: the official endpoint
        // currently mirrors the same value into the undocumented prompt_tokens_details.cached_tokens
        // (matched by the standard field above), so this fallback only applies when upstream sends the
        // documented field without the mirror (some relays), and it also guards against the undocumented
        // mirror disappearing. prompt_tokens already covers hits plus misses (see prompt_cache_miss_tokens,
        // informational only, no subtraction needed here), so the hit count is used directly as cache_read.
        .or_else(|| usage.get("prompt_cache_hit_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32
}

fn openai_cache_write_tokens(usage: &Value) -> u32 {
    usage
        .get("cache_creation_input_tokens")
        .or_else(|| usage.pointer("/input_tokens_details/cache_write_tokens"))
        .or_else(|| usage.pointer("/prompt_tokens_details/cache_write_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32
}

/// request_id prefix for session logs, matching the format in `session_usage.rs`
pub const SESSION_REQUEST_ID_PREFIX: &str = "session:";

/// Claude Code and Claude Desktop share Claude message ids with the session
/// importer, so both use the bare `session:{message_id}` namespace. Other
/// apps retain app/provider scoping to avoid collisions between upstreams.
pub fn dedup_scope_for_app<'a>(
    app_type: &'a str,
    provider_id: &'a str,
) -> Option<(&'a str, &'a str)> {
    (!matches!(app_type, "claude" | "claude-desktop")).then_some((app_type, provider_id))
}

fn response_id(body: &Value, field: &str) -> Option<String> {
    body.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Token usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
    /// The actual model name extracted from the response, when available
    pub model: Option<String>,
    /// The message ID extracted from the response (used for cross-source dedupe)
    ///
    /// Claude API: `msg_xxx`, matching `message.id` in the session JSONL
    #[serde(skip)]
    pub message_id: Option<String>,
}

impl TokenUsage {
    /// Builds a stable request_id. Claude gets no scope so it keeps converging on the
    /// `session:{message_id}` primary key of the session JSONL; other protocols add an app/provider
    /// scope so upstreams reusing an envelope id cannot overwrite each other.
    pub fn dedup_request_id(&self, scope: Option<(&str, &str)>) -> String {
        self.message_id
            .as_ref()
            .map(|message_id| match scope {
                Some((app_type, provider_id)) => {
                    format!("{SESSION_REQUEST_ID_PREFIX}{app_type}:{provider_id}:{message_id}")
                }
                None => format!("{SESSION_REQUEST_ID_PREFIX}{message_id}"),
            })
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
    }

    /// Whether any billable token dimension was produced.
    ///
    /// Filters all-zero usage before writing: when an OpenAI-compatible upstream omits usage while
    /// streaming, the converter synthesizes an all-zero terminal event, and without a message_id
    /// `dedup_request_id` degrades to a random UUID, inserting a meaningless empty row per request and inflating the request count.
    pub fn has_billable_tokens(&self) -> bool {
        self.input_tokens > 0
            || self.output_tokens > 0
            || self.cache_read_tokens > 0
            || self.cache_creation_tokens > 0
    }
}

impl TokenUsage {
    /// Parses a non-streaming Claude API response
    pub fn from_claude_response(body: &Value) -> Option<Self> {
        let usage = body.get("usage")?;
        // Extract the model name from the response
        let model = body
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let message_id = response_id(body, "id");

        Some(Self {
            input_tokens: usage.get("input_tokens")?.as_u64()? as u32,
            output_tokens: usage.get("output_tokens")?.as_u64()? as u32,
            cache_read_tokens: usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            cache_creation_tokens: usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            model,
            message_id,
        })
    }

    /// Parses a streaming Claude API response
    pub fn from_claude_stream_events(events: &[Value]) -> Option<Self> {
        let mut usage = Self::default();
        let mut model: Option<String> = None;
        let mut message_id: Option<String> = None;
        let mut input_from_delta = false;

        for event in events {
            if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
                match event_type {
                    "message_start" => {
                        if let Some(message) = event.get("message") {
                            if model.is_none() {
                                if let Some(m) = message.get("model").and_then(|v| v.as_str()) {
                                    model = Some(m.to_string());
                                }
                            }
                            if message_id.is_none() {
                                if let Some(id) = response_id(message, "id") {
                                    message_id = Some(id);
                                }
                            }
                        }
                        if let Some(msg_usage) = event.get("message").and_then(|m| m.get("usage")) {
                            // Read input_tokens from message_start (native Claude API)
                            if let Some(input) =
                                msg_usage.get("input_tokens").and_then(|v| v.as_u64())
                            {
                                usage.input_tokens = input as u32;
                            }
                            usage.cache_read_tokens = msg_usage
                                .get("cache_read_input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as u32;
                            usage.cache_creation_tokens = msg_usage
                                .get("cache_creation_input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as u32;
                        }
                    }
                    "message_delta" => {
                        if let Some(delta_usage) = event.get("usage") {
                            // Read output_tokens from message_delta
                            if let Some(output) =
                                delta_usage.get("output_tokens").and_then(|v| v.as_u64())
                            {
                                usage.output_tokens = output as u32;
                            }

                            let delta_input = delta_usage
                                .get("input_tokens")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u32);
                            let delta_cache_read = delta_usage
                                .get("cache_read_input_tokens")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u32);
                            let delta_cache_creation = delta_usage
                                .get("cache_creation_input_tokens")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u32);

                            // Some Anthropic-compatible SSE providers report the full context in message_start but a
                            // corrected fresh input in message_delta. When a smaller positive delta input arrives, use it;
                            // if the same usage block carries cache counts, adopt those too to avoid double counting.
                            // If the delta has no cache fields, keep the ones from start as a best-effort fallback.
                            if let Some(input) = delta_input {
                                let should_use_delta_input = input > 0
                                    && (usage.input_tokens == 0
                                        || input < usage.input_tokens
                                        || (input_from_delta && input <= usage.input_tokens));

                                if should_use_delta_input {
                                    usage.input_tokens = input;
                                    input_from_delta = true;
                                    if let Some(cache_read) = delta_cache_read {
                                        usage.cache_read_tokens = cache_read;
                                    }
                                    if let Some(cache_creation) = delta_cache_creation {
                                        usage.cache_creation_tokens = cache_creation;
                                    }
                                }
                            }
                            // Handle cache hits from message_delta (cache_read_input_tokens)
                            if usage.cache_read_tokens == 0 {
                                if let Some(cache_read) = delta_cache_read {
                                    usage.cache_read_tokens = cache_read;
                                }
                            }
                            // Handle cache creation from message_delta (cache_creation_input_tokens)
                            // Note: zhipu currently does not return the cache_creation_input_tokens field
                            if usage.cache_creation_tokens == 0 {
                                if let Some(cache_creation) = delta_cache_creation {
                                    usage.cache_creation_tokens = cache_creation;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Use has_billable_tokens rather than input/output alone: a fully cached streaming request with
        // no output (input==0 && output==0 but cache_read>0) is a real cache-read charge and must be kept.
        // The Gemini -> Anthropic path hits this fully-cached case especially often after input became
        // fresh (promptTokenCount - cachedContentTokenCount); the old gate discarded it as "no usage".
        if usage.has_billable_tokens() {
            usage.model = model;
            usage.message_id = message_id;
            Some(usage)
        } else {
            None
        }
    }

    /// Parses a non-streaming Codex API response
    pub fn from_codex_response(body: &Value) -> Option<Self> {
        let usage = body.get("usage");
        if usage.is_none() {
            log::debug!(
                "[Codex] no usage field in the response, body keys: {:?}",
                body.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
            return None;
        }
        let usage = usage?;

        let input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64());
        let output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64());

        if input_tokens.is_none() || output_tokens.is_none() {
            log::debug!("[Codex] usage lacks input_tokens or output_tokens, usage: {usage:?}");
            return None;
        }

        // Extract the model name from the response
        let model = body
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let cached_tokens = openai_cache_read_tokens(usage);
        let cache_write_tokens = openai_cache_write_tokens(usage);

        Some(Self {
            input_tokens: input_tokens? as u32,
            output_tokens: output_tokens? as u32,
            cache_read_tokens: cached_tokens,
            cache_creation_tokens: cache_write_tokens,
            model,
            message_id: response_id(body, "id"),
        })
    }

    /// Smart Codex response parsing - auto-detects the OpenAI or Codex format
    ///
    /// Codex supports two API formats:
    /// - `/v1/responses`: uses input_tokens/output_tokens
    /// - `/v1/chat/completions`: uses prompt_tokens/completion_tokens (OpenAI format)
    ///
    /// Note: the raw input_tokens is recorded; cached_tokens is subtracted during cost calculation
    pub fn from_codex_response_auto(body: &Value) -> Option<Self> {
        let usage = body.get("usage")?;

        // Detect the format: OpenAI uses prompt_tokens, Codex uses input_tokens
        if usage.get("prompt_tokens").is_some() {
            log::debug!("[Codex] detected the OpenAI format (prompt_tokens)");
            Self::from_openai_response(body)
        } else if usage.get("input_tokens").is_some() {
            log::debug!("[Codex] detected the Codex format (input_tokens)");
            // Use the unadjusted variant and record the raw input_tokens
            Self::from_codex_response(body)
        } else {
            log::debug!("[Codex] unrecognized response format, usage: {usage:?}");
            None
        }
    }

    /// Smart Codex streaming parsing - auto-detects the Codex Responses / Images / OpenAI formats
    pub fn from_codex_stream_events_auto(events: &[Value]) -> Option<Self> {
        log::debug!("[Codex] smart-parsing {} streaming events", events.len());

        // Try the Codex Responses API format first (the response.completed event)
        for event in events {
            if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
                if event_type == "response.completed" {
                    if let Some(response) = event.get("response") {
                        log::debug!("[Codex] found a response.completed event");
                        return Self::from_codex_response_auto(response);
                    }
                }
            }
        }

        // Images API streaming format (the image_generation.completed event): usage sits at the top
        // level of the event with the same field shape as a non-streaming Codex response; scan backwards
        // for the last event parseable in that shape, skipping earlier partial_image events without usage.
        // If that fails, fall through to the OpenAI fallback below, leaving existing paths unchanged
        if let Some(usage) = events
            .iter()
            .rev()
            .filter(|event| event.pointer("/usage/input_tokens").is_some())
            .find_map(Self::from_codex_response)
        {
            log::debug!("[Codex] found an event with top-level usage.input_tokens");
            return Some(usage);
        }

        // Fall back to the OpenAI Chat Completions format (the last chunk carries usage)
        log::debug!("[Codex] trying the OpenAI streaming format");
        Self::from_openai_stream_events(events)
    }

    /// Parses an OpenAI Chat Completions API response (prompt_tokens, completion_tokens)
    pub fn from_openai_response(body: &Value) -> Option<Self> {
        let usage = body.get("usage")?;

        // OpenAI uses prompt_tokens and completion_tokens
        let prompt_tokens = usage.get("prompt_tokens").and_then(|v| v.as_u64())?;
        let completion_tokens = usage.get("completion_tokens").and_then(|v| v.as_u64())?;

        // Read cached_tokens (may live inside prompt_tokens_details)
        let cached_tokens = openai_cache_read_tokens(usage);
        let cache_write_tokens = openai_cache_write_tokens(usage);

        // Extract the model name from the response
        let model = body
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Some(Self {
            input_tokens: prompt_tokens as u32,
            output_tokens: completion_tokens as u32,
            cache_read_tokens: cached_tokens,
            cache_creation_tokens: cache_write_tokens,
            model,
            message_id: response_id(body, "id"),
        })
    }

    /// Parses a streaming OpenAI Chat Completions API response
    pub fn from_openai_stream_events(events: &[Value]) -> Option<Self> {
        log::debug!("[Codex] parsing {} OpenAI streaming events", events.len());
        // An OpenAI streaming response carries usage in the last chunk
        for event in events.iter().rev() {
            if let Some(usage) = event.get("usage") {
                if !usage.is_null() {
                    log::debug!("[Codex] found usage: {usage:?}");
                    let mut parsed = Self::from_openai_response(event)?;
                    if parsed.message_id.is_none() {
                        parsed.message_id =
                            events.iter().find_map(|chunk| response_id(chunk, "id"));
                    }
                    return Some(parsed);
                }
            }
        }
        log::debug!("[Codex] no usage information found");
        None
    }

    /// Parses a non-streaming Gemini API response
    pub fn from_gemini_response(body: &Value) -> Option<Self> {
        let usage = body.get("usageMetadata")?;
        // Extract the model actually used (the modelVersion field)
        let model = body
            .get("modelVersion")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let prompt_tokens = usage.get("promptTokenCount")?.as_u64()? as u32;
        let total_tokens = usage.get("totalTokenCount")?.as_u64()? as u32;

        // output tokens = total tokens - input tokens
        // This covers candidatesTokenCount + thoughtsTokenCount
        let output_tokens = total_tokens.saturating_sub(prompt_tokens);

        Some(Self {
            input_tokens: prompt_tokens,
            output_tokens,
            cache_read_tokens: usage
                .get("cachedContentTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            cache_creation_tokens: 0,
            model,
            message_id: response_id(body, "responseId"),
        })
    }

    /// Parses a streaming Gemini API response
    pub fn from_gemini_stream_chunks(chunks: &[Value]) -> Option<Self> {
        let mut total_input = 0u32;
        let mut total_tokens = 0u32;
        let mut total_cache_read = 0u32;
        let mut model: Option<String> = None;
        let mut message_id: Option<String> = None;

        for chunk in chunks {
            if let Some(usage) = chunk.get("usageMetadata") {
                // input tokens (usually identical across all chunks)
                total_input = usage
                    .get("promptTokenCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;

                // total tokens (input + output + thinking)
                total_tokens = usage
                    .get("totalTokenCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;

                // cache read tokens
                total_cache_read = usage
                    .get("cachedContentTokenCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
            }

            // Extract the model actually used (the modelVersion field)
            if model.is_none() {
                if let Some(model_version) = chunk.get("modelVersion").and_then(|v| v.as_str()) {
                    model = Some(model_version.to_string());
                }
            }
            if message_id.is_none() {
                message_id = response_id(chunk, "responseId");
            }
        }

        // output tokens = total tokens - input tokens
        let total_output = total_tokens.saturating_sub(total_input);

        if total_input > 0 || total_output > 0 {
            Some(Self {
                input_tokens: total_input,
                output_tokens: total_output,
                cache_read_tokens: total_cache_read,
                cache_creation_tokens: 0,
                model,
                message_id,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn response_ids_produce_scoped_dedup_keys_and_empty_ids_fall_back() {
        let response = json!({
            "id": "resp_123",
            "model": "gpt-5.6",
            "usage": { "input_tokens": 10, "output_tokens": 2 }
        });
        let usage = TokenUsage::from_codex_response(&response).unwrap();
        assert_eq!(usage.message_id.as_deref(), Some("resp_123"));
        assert_eq!(
            usage.dedup_request_id(Some(("codex", "provider-a"))),
            "session:codex:provider-a:resp_123"
        );

        let empty = json!({
            "id": "",
            "usage": { "input_tokens": 10, "output_tokens": 2 }
        });
        let empty_usage = TokenUsage::from_codex_response(&empty).unwrap();
        assert!(empty_usage.message_id.is_none());
        assert!(!empty_usage
            .dedup_request_id(Some(("codex", "provider-a")))
            .starts_with("session:"));
    }

    #[test]
    fn claude_apps_share_the_session_request_id_namespace() {
        let usage = TokenUsage {
            message_id: Some("msg_123".to_string()),
            ..Default::default()
        };

        for app_type in ["claude", "claude-desktop"] {
            assert_eq!(
                usage.dedup_request_id(dedup_scope_for_app(app_type, "provider-a")),
                "session:msg_123"
            );
        }
        assert_eq!(
            usage.dedup_request_id(dedup_scope_for_app("codex", "provider-a")),
            "session:codex:provider-a:msg_123"
        );
    }

    #[test]
    fn stream_parsers_recover_ids_from_envelope_chunks() {
        let openai = vec![
            json!({"id": "chatcmpl_123", "choices": []}),
            json!({
                "usage": { "prompt_tokens": 10, "completion_tokens": 2 },
                "choices": []
            }),
        ];
        assert_eq!(
            TokenUsage::from_openai_stream_events(&openai)
                .unwrap()
                .message_id
                .as_deref(),
            Some("chatcmpl_123")
        );

        let gemini = vec![json!({
            "responseId": "gemini_123",
            "usageMetadata": { "promptTokenCount": 10, "totalTokenCount": 12 }
        })];
        assert_eq!(
            TokenUsage::from_gemini_stream_chunks(&gemini)
                .unwrap()
                .message_id
                .as_deref(),
            Some("gemini_123")
        );
    }

    #[test]
    fn test_claude_response_parsing() {
        let response = json!({
            "model": "claude-sonnet-4-20250514",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_read_input_tokens": 20,
                "cache_creation_input_tokens": 10
            }
        });

        let usage = TokenUsage::from_claude_response(&response).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 20);
        assert_eq!(usage.cache_creation_tokens, 10);
        assert_eq!(usage.model, Some("claude-sonnet-4-20250514".to_string()));
    }

    #[test]
    fn test_has_billable_tokens_gates_empty_usage() {
        // All-zero usage (such as the synthesized terminal event when upstream omits usage) must not be
        // billed; this is the gate behind fix (D) for extra empty Codex streaming rows.
        assert!(!TokenUsage::default().has_billable_tokens());
        // cache_read alone is still a real billable token and must count.
        let only_cache = TokenUsage {
            cache_read_tokens: 100,
            ..Default::default()
        };
        assert!(only_cache.has_billable_tokens());
        let normal = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        };
        assert!(normal.has_billable_tokens());
    }

    #[test]
    fn test_claude_stream_cache_only_request_is_recorded() {
        // P2 regression: a fully cached streaming request with no output (input==0 && output==0 but cache_read>0)
        // is a real charge and must be kept; the old gate `input>0 || output>0` discarded it.
        let events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "id": "msg_cacheonly",
                    "model": "claude-opus-4-8",
                    "usage": {
                        "input_tokens": 0,
                        "cache_read_input_tokens": 50000,
                        "cache_creation_input_tokens": 0
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": { "output_tokens": 0 }
            }),
        ];
        let usage = TokenUsage::from_claude_stream_events(&events)
            .expect("a cache-only streaming request must not be dropped by the gate");
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.cache_read_tokens, 50000);
        assert_eq!(usage.message_id, Some("msg_cacheonly".to_string()));
    }

    #[test]
    fn test_codex_response_auto_returns_some_for_synthetic_all_zero() {
        // P3 regression: for the all-zero usage synthesized when a non-streaming Chat upstream omits it,
        // from_codex_response_auto still returns Some (the fields exist and there is no positivity check),
        // proving handlers need the has_billable_tokens gate to block empty rows; `if let Some` is not enough.
        let synthetic = json!({
            "usage": { "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 }
        });
        let usage = TokenUsage::from_codex_response_auto(&synthetic)
            .expect("from_codex_response_auto returns Some when all-zero usage fields exist");
        assert!(
            !usage.has_billable_tokens(),
            "all-zero usage must be non-billable per has_billable_tokens and skipped by the handler gate"
        );
    }

    #[test]
    fn test_claude_response_parsing_no_model() {
        let response = json!({
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_read_input_tokens": 20,
                "cache_creation_input_tokens": 10
            }
        });

        let usage = TokenUsage::from_claude_response(&response).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 20);
        assert_eq!(usage.cache_creation_tokens, 10);
        assert_eq!(usage.model, None);
    }

    #[test]
    fn test_claude_stream_parsing() {
        let events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "model": "claude-sonnet-4-20250514",
                    "usage": {
                        "input_tokens": 100,
                        "cache_read_input_tokens": 20,
                        "cache_creation_input_tokens": 10
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "output_tokens": 50
                }
            }),
        ];

        let usage = TokenUsage::from_claude_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 20);
        assert_eq!(usage.cache_creation_tokens, 10);
        assert_eq!(usage.model, Some("claude-sonnet-4-20250514".to_string()));
    }

    #[test]
    fn test_claude_stream_parsing_no_model() {
        let events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "usage": {
                        "input_tokens": 100,
                        "cache_read_input_tokens": 20,
                        "cache_creation_input_tokens": 10
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "output_tokens": 50
                }
            }),
        ];

        let usage = TokenUsage::from_claude_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 20);
        assert_eq!(usage.cache_creation_tokens, 10);
        assert_eq!(usage.model, None);
    }

    #[test]
    fn test_gemini_response_parsing() {
        let response = json!({
            "modelVersion": "gemini-3-pro-high",
            "usageMetadata": {
                "promptTokenCount": 8383,
                "candidatesTokenCount": 50,
                "thoughtsTokenCount": 114,
                "totalTokenCount": 8547,
                "cachedContentTokenCount": 20
            }
        });

        let usage = TokenUsage::from_gemini_response(&response).unwrap();
        assert_eq!(usage.input_tokens, 8383);
        // output_tokens = totalTokenCount - promptTokenCount = 8547 - 8383 = 164
        assert_eq!(usage.output_tokens, 164);
        assert_eq!(usage.cache_read_tokens, 20);
        assert_eq!(usage.cache_creation_tokens, 0);
        assert_eq!(usage.model, Some("gemini-3-pro-high".to_string()));
    }

    #[test]
    fn test_gemini_response_parsing_no_model() {
        // Test the case with no modelVersion field
        let response = json!({
            "usageMetadata": {
                "promptTokenCount": 100,
                "totalTokenCount": 150,
                "cachedContentTokenCount": 20
            }
        });

        let usage = TokenUsage::from_gemini_response(&response).unwrap();
        assert_eq!(usage.input_tokens, 100);
        // output_tokens = totalTokenCount - promptTokenCount = 150 - 100 = 50
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 20);
        assert_eq!(usage.cache_creation_tokens, 0);
        assert_eq!(usage.model, None);
    }

    #[test]
    fn test_gemini_response_with_thoughts() {
        // Test a real response containing thoughtsTokenCount
        // This is a real scenario reported by a user
        let response = json!({
            "candidates": [
                {
                    "content": {
                        "parts": [
                            {
                                "text": "",
                                "thoughtSignature": "EvcECvQE..."
                            }
                        ],
                        "role": "model"
                    },
                    "finishReason": "STOP"
                }
            ],
            "modelVersion": "gemini-3-pro-high",
            "responseId": "yupTafqLDu-PjMcPhrOx4QQ",
            "usageMetadata": {
                "candidatesTokenCount": 50,
                "promptTokenCount": 8383,
                "thoughtsTokenCount": 114,
                "totalTokenCount": 8547
            }
        });

        let usage = TokenUsage::from_gemini_response(&response).unwrap();
        assert_eq!(usage.input_tokens, 8383);
        // output_tokens = totalTokenCount - promptTokenCount
        // = 8547 - 8383 = 164 (candidatesTokenCount 50 + thoughtsTokenCount 114)
        assert_eq!(usage.output_tokens, 164);
        assert_eq!(usage.cache_read_tokens, 0);
        assert_eq!(usage.cache_creation_tokens, 0);
        assert_eq!(usage.model, Some("gemini-3-pro-high".to_string()));
    }

    #[test]
    fn test_codex_response_parsing_cached_tokens_in_details() {
        let response = json!({
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 500,
                "input_tokens_details": {
                    "cached_tokens": 300
                }
            }
        });

        let usage = TokenUsage::from_codex_response(&response).unwrap();
        // Unadjusted mode: input_tokens keeps its original value but cache hits must still be recorded
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 500);
        assert_eq!(usage.cache_read_tokens, 300);
    }

    #[test]
    fn test_codex_response_parsing_cache_write_tokens_in_details() {
        let response = json!({
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 500,
                "input_tokens_details": {
                    "cached_tokens": 300,
                    "cache_write_tokens": 200
                }
            }
        });

        let usage = TokenUsage::from_codex_response(&response).unwrap();
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.cache_read_tokens, 300);
        assert_eq!(usage.cache_creation_tokens, 200);
    }

    #[test]
    fn test_openrouter_stream_parsing() {
        // Test parsing a converted OpenRouter streaming response
        // After conversion, an OpenRouter streaming response carries input_tokens in message_delta
        let events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "model": "claude-sonnet-4-20250514",
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 0
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": "end_turn"
                },
                "usage": {
                    "input_tokens": 150,
                    "output_tokens": 75
                }
            }),
        ];

        let usage = TokenUsage::from_claude_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 150);
        assert_eq!(usage.output_tokens, 75);
        assert_eq!(usage.model, Some("claude-sonnet-4-20250514".to_string()));
    }

    #[test]
    fn test_claude_stream_prefers_smaller_delta_input_and_cache_pair() {
        // Some Anthropic-compatible providers report the cache-inclusive total context in message_start
        // and the corrected fresh input in message_delta, so the delta usage wins.
        let events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "model": "qwen-max",
                    "usage": {
                        "input_tokens": 200_000,
                        "cache_read_input_tokens": 180_000,
                        "cache_creation_input_tokens": 2_000
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "input_tokens": 80_000,
                    "output_tokens": 1_000,
                    "cache_read_input_tokens": 120_000,
                    "cache_creation_input_tokens": 500
                }
            }),
        ];

        let usage = TokenUsage::from_claude_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 80_000);
        assert_eq!(usage.output_tokens, 1_000);
        assert_eq!(usage.cache_read_tokens, 120_000);
        assert_eq!(usage.cache_creation_tokens, 500);
        assert_eq!(usage.model, Some("qwen-max".to_string()));
    }

    #[test]
    fn test_claude_stream_updates_cache_pair_from_later_delta_input() {
        // Some providers send several message_delta events with input; once a delta input is adopted,
        // later deltas with the same or smaller input keep updating the cache counts in that block.
        let events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "model": "qwen-max",
                    "usage": {
                        "input_tokens": 200_000,
                        "cache_read_input_tokens": 180_000,
                        "cache_creation_input_tokens": 2_000
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "input_tokens": 80_000,
                    "output_tokens": 100,
                    "cache_read_input_tokens": 110_000,
                    "cache_creation_input_tokens": 300
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "input_tokens": 80_000,
                    "output_tokens": 1_000,
                    "cache_read_input_tokens": 120_000,
                    "cache_creation_input_tokens": 500
                }
            }),
        ];

        let usage = TokenUsage::from_claude_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 80_000);
        assert_eq!(usage.output_tokens, 1_000);
        assert_eq!(usage.cache_read_tokens, 120_000);
        assert_eq!(usage.cache_creation_tokens, 500);
        assert_eq!(usage.model, Some("qwen-max".to_string()));
    }

    #[test]
    fn test_claude_stream_keeps_start_when_delta_input_is_larger() {
        // Under normal Anthropic semantics the input_tokens in message_start is already trustworthy,
        // so a larger delta input must not overwrite the start input/cache.
        let events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "usage": {
                        "input_tokens": 100,
                        "cache_read_input_tokens": 20
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "input_tokens": 150,
                    "output_tokens": 75,
                    "cache_read_input_tokens": 30
                }
            }),
        ];

        let usage = TokenUsage::from_claude_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 75);
        assert_eq!(usage.cache_read_tokens, 20);
    }

    #[test]
    fn test_native_claude_stream_parsing() {
        // Test parsing a native Claude API streaming response
        // The native Claude API puts input_tokens in message_start
        let events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "model": "claude-sonnet-4-20250514",
                    "usage": {
                        "input_tokens": 200,
                        "cache_read_input_tokens": 50
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "output_tokens": 100
                }
            }),
        ];

        let usage = TokenUsage::from_claude_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 200);
        assert_eq!(usage.output_tokens, 100);
        assert_eq!(usage.cache_read_tokens, 50);
        assert_eq!(usage.model, Some("claude-sonnet-4-20250514".to_string()));
    }

    // ============================================================================
    // Smart Codex parsing tests
    // ============================================================================

    #[test]
    fn test_codex_response_auto_openai_format() {
        // OpenAI format (prompt_tokens/completion_tokens)
        let response = json!({
            "model": "gpt-4o",
            "usage": {
                "prompt_tokens": 1000,
                "completion_tokens": 500,
                "prompt_tokens_details": {
                    "cached_tokens": 200
                }
            }
        });

        let usage = TokenUsage::from_codex_response_auto(&response).unwrap();
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 500);
        assert_eq!(usage.cache_read_tokens, 200);
        assert_eq!(usage.model, Some("gpt-4o".to_string()));
    }

    #[test]
    fn test_openai_response_deepseek_cache_hit_fields() {
        // DeepSeek Chat format (related to issue #6073): cache hits and misses are listed separately in
        // the documented prompt_cache_hit_tokens / prompt_cache_miss_tokens, and prompt_tokens covers both.
        // When upstream sends only these documented fields without mirroring
        // prompt_tokens_details.cached_tokens (as some relays do), without this fallback cache hits record as 0 and the cost is inflated to full price.
        let response = json!({
            "model": "deepseek-v4-flash",
            "usage": {
                "prompt_tokens": 1000,
                "completion_tokens": 100,
                "prompt_cache_hit_tokens": 600,
                "prompt_cache_miss_tokens": 400,
                "total_tokens": 1100
            }
        });

        let usage = TokenUsage::from_openai_response(&response).unwrap();
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 100);
        assert_eq!(usage.cache_read_tokens, 600);
        assert_eq!(usage.cache_creation_tokens, 0);
        assert_eq!(usage.model, Some("deepseek-v4-flash".to_string()));
    }

    #[test]
    fn openai_cache_read_prefers_standard_field_over_deepseek_specific() {
        // When both field sets appear the standard field wins (including an explicit 0, since Some(0)
        // short-circuits the or_else chain). The order is deliberate: a relay hardcoding cached_tokens: 0
        // while passing prompt_cache_hit_tokens through still reads 0, matching behaviour before this fallback landed.
        let response = json!({
            "model": "deepseek-v4-flash",
            "usage": {
                "prompt_tokens": 1000,
                "completion_tokens": 10,
                "prompt_tokens_details": { "cached_tokens": 0 },
                "prompt_cache_hit_tokens": 600
            }
        });
        let usage = TokenUsage::from_openai_response(&response).unwrap();
        assert_eq!(usage.cache_read_tokens, 0);
    }

    #[test]
    fn test_openai_stream_deepseek_cache_hit_fields() {
        // Streaming path: usage sits on the final chunk, and DeepSeek cache hits must be extracted too.
        let events = vec![
            json!({
                "id": "chatcmpl-ds",
                "model": "deepseek-v4-flash",
                "choices": [{"delta": {"content": "Hi"}}]
            }),
            json!({
                "id": "chatcmpl-ds",
                "model": "deepseek-v4-flash",
                "choices": [],
                "usage": {
                    "prompt_tokens": 800,
                    "completion_tokens": 50,
                    "prompt_cache_hit_tokens": 512,
                    "prompt_cache_miss_tokens": 288,
                    "total_tokens": 850
                }
            }),
        ];

        let usage = TokenUsage::from_openai_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 800);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 512);
        assert_eq!(usage.message_id.as_deref(), Some("chatcmpl-ds"));
    }

    #[test]
    fn test_codex_response_auto_codex_format() {
        // Codex format (input_tokens/output_tokens)
        let response = json!({
            "model": "o3",
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 500,
                "input_tokens_details": {
                    "cached_tokens": 300
                }
            }
        });

        let usage = TokenUsage::from_codex_response_auto(&response).unwrap();
        // Record the raw input_tokens without adjustment
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 500);
        assert_eq!(usage.cache_read_tokens, 300);
        assert_eq!(usage.model, Some("o3".to_string()));
    }

    #[test]
    fn test_codex_stream_events_auto_codex_format() {
        // Codex Responses API streaming format (the response.completed event)
        let events = vec![
            json!({
                "type": "response.created",
                "response": {
                    "id": "resp_123"
                }
            }),
            json!({
                "type": "response.completed",
                "response": {
                    "model": "o3",
                    "usage": {
                        "input_tokens": 1000,
                        "output_tokens": 500,
                        "input_tokens_details": {
                            "cached_tokens": 200
                        }
                    }
                }
            }),
        ];

        let usage = TokenUsage::from_codex_stream_events_auto(&events).unwrap();
        // Record the raw input_tokens without adjustment
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 500);
        assert_eq!(usage.cache_read_tokens, 200);
        assert_eq!(usage.model, Some("o3".to_string()));
    }

    #[test]
    fn test_codex_stream_events_auto_openai_format() {
        // OpenAI Chat Completions streaming format (the last chunk carries usage)
        let events = vec![
            json!({
                "id": "chatcmpl-123",
                "model": "gpt-4o",
                "choices": [{"delta": {"content": "Hello"}}]
            }),
            json!({
                "id": "chatcmpl-123",
                "model": "gpt-4o",
                "choices": [{"delta": {}}],
                "usage": {
                    "prompt_tokens": 100,
                    "completion_tokens": 50
                }
            }),
        ];

        let usage = TokenUsage::from_codex_stream_events_auto(&events).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.model, Some("gpt-4o".to_string()));
    }

    #[test]
    fn test_codex_stream_events_auto_image_generation_completed() {
        // Images API streaming format: usage sits at the top level of the image_generation.completed event,
        // with the same field shape as a non-streaming Codex response (input_tokens / output_tokens)
        let events = vec![
            json!({
                "type": "image_generation.partial_image",
                "b64_json": "cGFydGlhbA==",
                "partial_image_index": 0
            }),
            json!({
                "type": "image_generation.completed",
                "b64_json": "aW1hZ2U=",
                "created_at": 1778832973,
                "usage": {
                    "input_tokens": 1474,
                    "input_tokens_details": {
                        "image_tokens": 1457,
                        "text_tokens": 17
                    },
                    "output_tokens": 1372,
                    "output_tokens_details": {
                        "image_tokens": 1372,
                        "text_tokens": 0
                    },
                    "total_tokens": 2846
                }
            }),
        ];

        let usage = TokenUsage::from_codex_stream_events_auto(&events)
            .expect("image_generation.completed usage should be parsed");
        assert_eq!(usage.input_tokens, 1474);
        assert_eq!(usage.output_tokens, 1372);
        assert_eq!(usage.cache_read_tokens, 0);
        assert_eq!(usage.cache_creation_tokens, 0);
        assert_eq!(usage.model, None);
    }
}
