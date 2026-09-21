//! Handler configuration module
//!
//! Defines the config structs and usage parsers for each API handler

use crate::proxy::usage::parser::TokenUsage;
use serde_json::Value;

/// Usage parser type aliases
pub type StreamUsageParser = fn(&[Value]) -> Option<TokenUsage>;
pub type ResponseUsageParser = fn(&Value) -> Option<TokenUsage>;

/// Model extractor type alias
/// Params: (stream event list, model name from the request) -> the model name actually used
pub type StreamModelExtractor = fn(&[Value], &str) -> String;

/// Type alias for the streaming usage event pre-filter.
///
/// Takes the raw SSE `data:` string. Returning false skips JSON parsing, avoiding
/// parsing usage-irrelevant events on the high-frequency token/chunk path.
pub type StreamUsageEventFilter = fn(&str) -> bool;

/// Usage-parsing config for each API
#[derive(Clone, Copy)]
pub struct UsageParserConfig {
    /// Streaming response parser
    pub stream_parser: StreamUsageParser,
    /// Non-streaming response parser
    pub response_parser: ResponseUsageParser,
    /// Model extractor for streaming responses
    pub model_extractor: StreamModelExtractor,
    /// Streaming usage event pre-filter
    pub stream_event_filter: Option<StreamUsageEventFilter>,
    /// App type string (for logging)
    pub app_type_str: &'static str,
}

// ============================================================================
// Streaming usage event pre-filter
// ============================================================================

pub fn claude_stream_usage_event_filter(data: &str) -> bool {
    data.contains("\"message_start\"") || data.contains("\"message_delta\"")
}

fn openai_stream_usage_event_filter(data: &str) -> bool {
    data.contains("\"usage\"")
}

pub fn codex_stream_usage_event_filter(data: &str) -> bool {
    data.contains("\"response.completed\"") || data.contains("\"usage\"")
}

fn gemini_stream_usage_event_filter(data: &str) -> bool {
    data.contains("\"usageMetadata\"")
}

// ============================================================================
// Model extractor implementations
// ============================================================================

/// Claude streaming response model extraction (prefers usage.model)
///
/// An empty model name is treated as missing (the transform layer synthesizes
/// model:"" for upstreams without echo), falling back to fallback_model (the
/// mapped outbound model or the client-requested model).
fn claude_model_extractor(events: &[Value], fallback_model: &str) -> String {
    // First try to get the model from the parsed usage
    if let Some(usage) = TokenUsage::from_claude_stream_events(events) {
        if let Some(model) = usage.model.filter(|m| !m.is_empty()) {
            return model;
        }
    }
    fallback_model.to_string()
}

/// OpenAI Chat Completions streaming response model extraction (prefers usage.model)
fn openai_model_extractor(events: &[Value], fallback_model: &str) -> String {
    // First try to get the model from the parsed usage
    if let Some(usage) = TokenUsage::from_openai_stream_events(events) {
        if let Some(model) = usage.model.filter(|m| !m.is_empty()) {
            return model;
        }
    }
    // Fallback: extract directly from the events
    events
        .iter()
        .find_map(|e| e.get("model")?.as_str().filter(|m| !m.is_empty()))
        .unwrap_or(fallback_model)
        .to_string()
}

/// Codex smart streaming response model extraction (auto-detects format)
fn codex_auto_model_extractor(events: &[Value], fallback_model: &str) -> String {
    // First try to get the model from the parsed usage
    if let Some(usage) = TokenUsage::from_codex_stream_events_auto(events) {
        if let Some(model) = usage.model.filter(|m| !m.is_empty()) {
            return model;
        }
    }
    // Fallback: extract from the response.completed event
    events
        .iter()
        .find_map(|e| {
            if e.get("type")?.as_str()? == "response.completed" {
                e.get("response")?
                    .get("model")?
                    .as_str()
                    .filter(|m| !m.is_empty())
            } else {
                None
            }
        })
        .or_else(|| {
            // Further fallback: extract from OpenAI-format events
            events
                .iter()
                .find_map(|e| e.get("model")?.as_str().filter(|m| !m.is_empty()))
        })
        .unwrap_or(fallback_model)
        .to_string()
}

/// Gemini streaming response model extraction (prefers usage.model)
fn gemini_model_extractor(events: &[Value], fallback_model: &str) -> String {
    // First try to get the model from the parsed usage
    if let Some(usage) = TokenUsage::from_gemini_stream_chunks(events) {
        if let Some(model) = usage.model.filter(|m| !m.is_empty()) {
            return model;
        }
    }
    fallback_model.to_string()
}

// ============================================================================
// Predefined configs
// ============================================================================

/// Claude API parsing config
pub const CLAUDE_PARSER_CONFIG: UsageParserConfig = UsageParserConfig {
    stream_parser: TokenUsage::from_claude_stream_events,
    response_parser: TokenUsage::from_claude_response,
    model_extractor: claude_model_extractor,
    stream_event_filter: Some(claude_stream_usage_event_filter),
    app_type_str: "claude",
};

/// OpenAI Chat Completions API parsing config (used by Codex /v1/chat/completions)
pub const OPENAI_PARSER_CONFIG: UsageParserConfig = UsageParserConfig {
    stream_parser: TokenUsage::from_openai_stream_events,
    response_parser: TokenUsage::from_openai_response,
    model_extractor: openai_model_extractor,
    stream_event_filter: Some(openai_stream_usage_event_filter),
    app_type_str: "codex",
};

/// Codex smart parsing config (auto-detects OpenAI or Codex format)
pub const CODEX_PARSER_CONFIG: UsageParserConfig = UsageParserConfig {
    stream_parser: TokenUsage::from_codex_stream_events_auto,
    response_parser: TokenUsage::from_codex_response_auto,
    model_extractor: codex_auto_model_extractor,
    stream_event_filter: Some(codex_stream_usage_event_filter),
    app_type_str: "codex",
};

/// Gemini API parsing config
pub const GEMINI_PARSER_CONFIG: UsageParserConfig = UsageParserConfig {
    stream_parser: TokenUsage::from_gemini_stream_chunks,
    response_parser: TokenUsage::from_gemini_response,
    model_extractor: gemini_model_extractor,
    stream_event_filter: Some(gemini_stream_usage_event_filter),
    app_type_str: "gemini",
};

// ============================================================================
// Handler configuration (reserved for further simplification)
// ============================================================================
