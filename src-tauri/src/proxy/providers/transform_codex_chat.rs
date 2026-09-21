//! Codex Responses ↔ OpenAI Chat Completions conversion.
//!
//! This module is used when the Codex client talks to CC Switch through the
//! Responses API, while the selected upstream provider only exposes an
//! OpenAI-compatible Chat Completions endpoint.

use super::codex_chat_common::{
    append_reasoning_content, extract_reasoning_field_text, extract_reasoning_summary_text,
    response_function_call_item, response_function_call_item_with_namespace,
    split_leading_think_block,
};
use crate::provider::CodexChatReasoningConfig;
use crate::proxy::{
    error::ProxyError,
    json_canonical::{
        canonical_json_string, canonicalize_json_string_if_parseable, canonicalize_tool_arguments,
        short_sha256_hex,
    },
    tool_media::{
        chat_file_from_input_file, flush_pending_chat_tool_media, plan_chat_tool_output_media,
        queue_chat_tool_output_media, strip_and_clamp_media_from_tool_value, ToolMediaScope,
        TOOL_RESULT_MEDIA_MOVED_MARKER,
    },
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

const EXTRA_CHAT_PASSTHROUGH_FIELDS: &[&str] = &[
    "frequency_penalty",
    "logit_bias",
    "logprobs",
    "metadata",
    "n",
    "parallel_tool_calls",
    "presence_penalty",
    "response_format",
    "seed",
    "service_tier",
    "stop",
    "stream_options",
    "top_logprobs",
    "user",
];

const TOOL_SEARCH_PROXY_NAME: &str = "tool_search";
const CUSTOM_TOOL_INPUT_FIELD: &str = "input";
const CHAT_TOOL_NAME_MAX_LEN: usize = 64;
const CUSTOM_TOOL_INPUT_DESCRIPTION: &str = "Raw string input for the original custom tool. Preserve formatting exactly and follow the original tool definition embedded in the description.";
const CUSTOM_TOOL_PRESERVED_METADATA_HEADING: &str = "Original tool definition:";
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodexToolKind {
    Function,
    Namespace,
    Custom,
    ToolSearch,
}

#[derive(Debug, Clone)]
pub(crate) struct CodexToolSpec {
    pub(crate) kind: CodexToolKind,
    pub(crate) name: String,
    pub(crate) namespace: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodexToolContext {
    chat_tools: Vec<Value>,
    seen_chat_names: HashSet<String>,
    chat_name_to_spec: HashMap<String, CodexToolSpec>,
    namespace_name_to_chat_name: HashMap<(String, String), String>,
}

impl CodexToolContext {
    pub(crate) fn chat_tools(&self) -> &[Value] {
        &self.chat_tools
    }

    pub(crate) fn lookup_chat_name(&self, chat_name: &str) -> Option<&CodexToolSpec> {
        self.chat_name_to_spec.get(chat_name)
    }

    pub(crate) fn is_custom_tool_chat_name(&self, chat_name: &str) -> bool {
        self.lookup_chat_name(chat_name)
            .is_some_and(|spec| matches!(&spec.kind, CodexToolKind::Custom))
    }

    pub(crate) fn chat_name_for_response_function(
        &self,
        name: &str,
        namespace: Option<&str>,
    ) -> String {
        if let Some(namespace) = namespace.filter(|value| !value.is_empty()) {
            if let Some(chat_name) = self
                .namespace_name_to_chat_name
                .get(&(namespace.to_string(), name.to_string()))
            {
                return chat_name.clone();
            }
            return flatten_namespace_tool_name(namespace, name);
        }

        name.to_string()
    }

    fn add_chat_tool(&mut self, chat_name: String, spec: CodexToolSpec, chat_tool: Value) {
        if chat_name.trim().is_empty() || self.seen_chat_names.contains(&chat_name) {
            return;
        }
        self.seen_chat_names.insert(chat_name.clone());
        if let Some(namespace) = spec.namespace.as_ref() {
            self.namespace_name_to_chat_name
                .insert((namespace.clone(), spec.name.clone()), chat_name.clone());
        }
        self.chat_name_to_spec.insert(chat_name, spec);
        self.chat_tools.push(chat_tool);
    }

    fn add_function_tool(&mut self, tool: &Value, namespace: Option<&str>) {
        let Some(original_name) = responses_tool_name(tool) else {
            return;
        };
        let chat_name = namespace
            .map(|namespace| flatten_namespace_tool_name(namespace, &original_name))
            .unwrap_or_else(|| original_name.clone());

        let Some(chat_tool) = responses_function_tool_to_chat_tool(tool, &chat_name) else {
            return;
        };
        let spec = CodexToolSpec {
            kind: if namespace.is_some() {
                CodexToolKind::Namespace
            } else {
                CodexToolKind::Function
            },
            name: original_name,
            namespace: namespace.map(ToString::to_string),
        };
        self.add_chat_tool(chat_name, spec, chat_tool);
    }

    fn add_custom_tool(&mut self, tool: &Value) {
        let Some(name) = responses_tool_name(tool) else {
            return;
        };
        let description = json!(responses_custom_tool_description(tool));
        let chat_tool = json!({
            "type": "function",
            "function": {
                "name": name,
                "description": description,
                "parameters": {
                    "type": "object",
                    "properties": {
                        CUSTOM_TOOL_INPUT_FIELD: {
                            "type": "string",
                            "description": CUSTOM_TOOL_INPUT_DESCRIPTION
                        }
                    },
                    "required": [CUSTOM_TOOL_INPUT_FIELD]
                }
            }
        });
        let spec = CodexToolSpec {
            kind: CodexToolKind::Custom,
            name: name.clone(),
            namespace: None,
        };
        self.add_chat_tool(name, spec, chat_tool);
    }

    fn add_tool_search_tool(&mut self) {
        let chat_tool = json!({
            "type": "function",
            "function": {
                "name": TOOL_SEARCH_PROXY_NAME,
                "description": "Search and load Codex tools, plugins, connectors, and MCP namespaces for the current task.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query for tools or connectors to load."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of tool groups to return."
                        }
                    },
                    "required": ["query"]
                }
            }
        });
        let spec = CodexToolSpec {
            kind: CodexToolKind::ToolSearch,
            name: TOOL_SEARCH_PROXY_NAME.to_string(),
            namespace: None,
        };
        self.add_chat_tool(TOOL_SEARCH_PROXY_NAME.to_string(), spec, chat_tool);
    }

    fn add_namespace_tool(&mut self, namespace_tool: &Value) {
        let Some(namespace) = namespace_tool.get("name").and_then(|v| v.as_str()) else {
            return;
        };
        let Some(children) = namespace_tool
            .get("tools")
            .or_else(|| namespace_tool.get("children"))
            .and_then(|v| v.as_array())
        else {
            return;
        };

        for child in children {
            if child.get("type").and_then(|v| v.as_str()) == Some("function") {
                self.add_function_tool(child, Some(namespace));
            }
        }
    }

    fn add_response_tool(&mut self, tool: &Value) {
        match tool {
            Value::String(name) => {
                self.add_custom_tool(&json!({
                    "type": "custom",
                    "name": name
                }));
            }
            Value::Object(_) => match tool.get("type").and_then(|v| v.as_str()) {
                Some("function") => self.add_function_tool(tool, None),
                Some("custom") => self.add_custom_tool(tool),
                Some("tool_search") => self.add_tool_search_tool(),
                Some("namespace") => self.add_namespace_tool(tool),
                _ => {}
            },
            _ => {}
        }
    }
}

pub(crate) fn build_codex_tool_context_from_request(body: &Value) -> CodexToolContext {
    let mut context = CodexToolContext::default();

    if let Some(tools) = body.get("tools").and_then(|v| v.as_array()) {
        for tool in tools {
            context.add_response_tool(tool);
        }
    }

    if let Some(input) = body.get("input") {
        collect_tool_search_output_tools(input, &mut context);
    }

    context
}

/// Convert an OpenAI Responses request into an OpenAI Chat Completions request,
/// using provider-declared Codex Chat reasoning capabilities when available.
pub fn responses_to_chat_completions_with_reasoning(
    body: Value,
    reasoning_config: Option<&CodexChatReasoningConfig>,
) -> Result<Value, ProxyError> {
    let mut result = json!({});
    let tool_context = build_codex_tool_context_from_request(&body);

    if let Some(model) = body.get("model") {
        result["model"] = model.clone();
    }

    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        let instructions = instruction_text(instructions);
        if !instructions.is_empty() {
            messages.push(json!({
                "role": "system",
                "content": instructions
            }));
        }
    }

    if let Some(input) = body.get("input") {
        append_responses_input_as_chat_messages(input, &mut messages, &tool_context)?;
    }
    let messages = collapse_system_messages_to_head(messages);
    result["messages"] = json!(messages);

    let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("");
    if let Some(max_tokens) = body.get("max_output_tokens") {
        if super::transform::is_openai_o_series(model) {
            result["max_completion_tokens"] = max_tokens.clone();
        } else {
            result["max_tokens"] = max_tokens.clone();
        }
    }
    if let Some(max_tokens) = body.get("max_tokens") {
        result["max_tokens"] = max_tokens.clone();
    }
    if let Some(max_tokens) = body.get("max_completion_tokens") {
        result["max_completion_tokens"] = max_tokens.clone();
    }

    for key in ["temperature", "top_p", "stream"] {
        if let Some(value) = body.get(key) {
            result[key] = value.clone();
        }
    }

    apply_reasoning_options(&mut result, &body, model, reasoning_config);

    let tools = tool_context.chat_tools();
    if !tools.is_empty() {
        result["tools"] = json!(tools);
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] = responses_tool_choice_to_chat(tool_choice, &tool_context);
    }

    for key in EXTRA_CHAT_PASSTHROUGH_FIELDS {
        if let Some(value) = body.get(*key) {
            result[*key] = value.clone();
        }
    }

    // Strict OpenAI-compatible upstreams (vLLM, enterprise gateways) reject
    // requests that carry tool_choice or parallel_tool_calls without a non-empty
    // tools array. Drop both fields when tools ended up absent or empty after
    // conversion to avoid 503/400 from such providers.
    let has_tools = result
        .get("tools")
        .is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty()));
    if !has_tools {
        if let Some(obj) = result.as_object_mut() {
            obj.remove("tool_choice");
            obj.remove("parallel_tool_calls");
        }
    }
    // OpenAI-compatible upstreams do not return usage in SSE by default; include_usage must be set
    // explicitly for the trailing usage chunk. The Codex CLI speaks the Responses protocol and sends
    // no stream_options of its own, so without this injection tokens, cost, and cache hit rate are all
    // lost for third-party streaming requests such as kimi/MiniMax (input/output/cache all 0).
    // Shares one helper with the Claude -> openai_chat path so both client directions match.
    super::transform::inject_openai_stream_include_usage(&mut result);

    Ok(result)
}

fn apply_reasoning_options(
    result: &mut Value,
    body: &Value,
    model: &str,
    config: Option<&CodexChatReasoningConfig>,
) {
    let Some(config) = config else {
        if super::transform::supports_reasoning_effort(model) {
            if let Some(effort) = body.pointer("/reasoning/effort") {
                result["reasoning_effort"] = effort.clone();
            }
        }
        return;
    };

    let supports_effort = config.supports_effort.unwrap_or(false);
    let supports_thinking = config.supports_thinking.unwrap_or(false) || supports_effort;
    let Some(reasoning_enabled) = reasoning_requested(body) else {
        return;
    };

    if supports_thinking {
        match config
            .thinking_param
            .as_deref()
            .unwrap_or("thinking")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "thinking" => {
                result["thinking"] = json!({
                    "type": if reasoning_enabled { "enabled" } else { "disabled" }
                });
            }
            "enable_thinking" => {
                result["enable_thinking"] = json!(reasoning_enabled);
            }
            "reasoning_split" => {
                result["reasoning_split"] = json!(reasoning_enabled);
            }
            _ => {}
        }
    }

    // effort_param is computed before the early return: the explicit-off branch of the reasoning.effort shape needs it.
    let effort_param = config
        .effort_param
        .as_deref()
        .unwrap_or("reasoning_effort")
        .trim()
        .to_ascii_lowercase();

    if !reasoning_enabled {
        // OpenRouter's native reasoning.effort accepts an explicit "none" meaning reasoning is fully off.
        // When the client explicitly sends effort=none/off/disabled (or reasoning=null), reasoning_enabled
        // is false, and returning early would drop that intent: some OpenRouter models think by default and
        // cannot be turned off without the field, skewing behaviour and cost, so this shape forwards {"reasoning":{"effort":"none"}} faithfully.
        // Platforms using top-level reasoning_effort have no none in their enum, so they still take the thinking-off path above and send no effort.
        // Note: with no reasoning field at all, reasoning_requested returns None and we already returned,
        // so only an explicit off from the client forwards none.
        if effort_param == "reasoning.effort" {
            result["reasoning"] = json!({ "effort": "none" });
        }
        return;
    }

    if !supports_effort {
        return;
    }

    let Some(effort) = body.pointer("/reasoning/effort").and_then(|v| v.as_str()) else {
        return;
    };
    let Some(mapped) = map_reasoning_effort(
        effort,
        config.effort_value_mode.as_deref(),
        config.effort_levels.as_deref(),
    ) else {
        return;
    };

    match effort_param.as_str() {
        // OpenAI-style top-level field (DeepSeek's own API, OpenAI o-series, and so on).
        "reasoning_effort" => {
            result["reasoning_effort"] = json!(mapped);
        }
        // OpenRouter's native normalized object: OpenRouter translates reasoning.effort into the right
        // reasoning parameter for each underlying model (OpenAI/Grok/Gemini/Anthropic), covering more than the top-level OpenAI alias.
        // This conversion builds from an empty object and keeps nothing of the original reasoning object,
        // so reasoning and reasoning_effort never coexist and trigger a 400 (see openclaw#24119).
        "reasoning.effort" => {
            result["reasoning"] = json!({ "effort": mapped });
        }
        _ => {}
    }
}

fn reasoning_requested(body: &Value) -> Option<bool> {
    if let Some(effort) = body.pointer("/reasoning/effort").and_then(|v| v.as_str()) {
        return Some(!matches!(
            effort.trim().to_ascii_lowercase().as_str(),
            "none" | "off" | "disabled"
        ));
    }

    body.get("reasoning").map(|value| !value.is_null())
}

fn map_reasoning_effort<'a>(
    effort: &str,
    mode: Option<&str>,
    effort_levels: Option<&'a [String]>,
) -> Option<&'a str> {
    let effort = effort.trim().to_ascii_lowercase();
    if matches!(effort.as_str(), "none" | "off" | "disabled") {
        return None;
    }

    // ultra is a Codex extension tier: dedicated modes with known enums (deepseek/openrouter/low_high)
    // clamp it to their own highest valid tier rather than dropping it, since dropping silently turns
    // "deepest thinking" into "no effort at all". passthrough targets generic upstreams with unknown
    // enums, where the tier is backed by the user's or preset's reasoningLevels, so it passes through like max/xhigh.
    match mode.unwrap_or("passthrough") {
        "deepseek" => match effort.as_str() {
            "max" | "xhigh" | "ultra" => Some("max"),
            _ => Some("high"),
        },
        "low_high" => match effort.as_str() {
            "minimal" | "low" => Some("low"),
            _ => Some("high"),
        },
        // The OpenRouter effort enum is xhigh|high|medium|low|minimal with no max. max is a Codex or
        // per-model extension tier that is invalid for OpenRouter and triggers
        // `400 reasoning_effort: Invalid option` (see openclaw#77350), so it clamps to the highest valid
        // tier xhigh, other valid values pass through, and unknown values are dropped to avoid rejection.
        "openrouter" => match effort.as_str() {
            "max" | "xhigh" | "ultra" => Some("xhigh"),
            "high" => Some("high"),
            "medium" => Some("medium"),
            "low" => Some("low"),
            "minimal" => Some("minimal"),
            _ => None,
        },
        // OpenCode Zen: valid tiers are per model (the table is reasoningLevels on each provider
        // modelCatalog entry, mirroring models.dev: glm-5.2 only high|max, deepseek-v4-flash
        // low|high|max, kimi-k3 only max), and the opencode client also sends values strictly as declared,
        // so a single union mapping will not do. With no table (model absent from the catalog, or a
        // toggle/budget model declaring no effort) the result is None and reasoning_effort is omitted; with
        // a table it clamps to the nearest valid tier not below the request, taking the highest valid tier
        // when the request exceeds it; an unrecognizable request value yields None (same drop policy as other modes).
        "zen" => {
            let levels = effort_levels?;
            let requested = zen_effort_rank(&effort)?;
            levels
                .iter()
                .filter_map(|level| zen_effort_rank(level).map(|rank| (rank, level.as_str())))
                .filter(|(rank, _)| *rank >= requested)
                .min_by_key(|(rank, _)| *rank)
                .or_else(|| {
                    levels
                        .iter()
                        .filter_map(|level| {
                            zen_effort_rank(level).map(|rank| (rank, level.as_str()))
                        })
                        .max_by_key(|(rank, _)| *rank)
                })
                .map(|(_, level)| level)
        }
        _ => match effort.as_str() {
            "minimal" => Some("minimal"),
            "low" => Some("low"),
            "medium" => Some("medium"),
            "high" => Some("high"),
            "xhigh" => Some("xhigh"),
            "max" => Some("max"),
            "ultra" => Some("ultra"),
            _ => None,
        },
    }
}

/// The canonical Codex tier order (minimal < low < medium < high < xhigh < max < ultra), used by
/// the zen per-model clamp for comparisons; invalid or extension values in the catalog (such as "none") return None and are filtered out.
fn zen_effort_rank(effort: &str) -> Option<u8> {
    match effort.trim().to_ascii_lowercase().as_str() {
        "minimal" => Some(0),
        "low" => Some(1),
        "medium" => Some(2),
        "high" => Some(3),
        "xhigh" => Some(4),
        "max" => Some(5),
        "ultra" => Some(6),
        _ => None,
    }
}

/// MiniMax strictly requires `role=system` to appear only as the first message, otherwise it returns
/// `invalid params, chat content has invalid message role: system (2013)`.
/// Merging every system message to the front avoids tripping that constraint on a mid-list system
/// message (such as the Codex `developer` instruction), and the reorder is lossless for lenient compatibility layers like OpenAI / DeepSeek.
fn collapse_system_messages_to_head(messages: Vec<Value>) -> Vec<Value> {
    let mut system_chunks: Vec<String> = Vec::new();
    let mut rest: Vec<Value> = Vec::with_capacity(messages.len());

    for msg in messages {
        if msg.get("role").and_then(|v| v.as_str()) == Some("system") {
            if let Some(text) = msg.get("content").and_then(|v| v.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    system_chunks.push(text.to_string());
                }
                continue;
            }
        }
        rest.push(msg);
    }

    let mut out: Vec<Value> = Vec::with_capacity(rest.len() + 1);
    if !system_chunks.is_empty() {
        out.push(json!({
            "role": "system",
            "content": system_chunks.join("\n\n")
        }));
    }
    out.extend(rest);
    out
}

fn instruction_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(|v| v.as_str())
                    .or_else(|| part.as_str())
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        other => other.as_str().unwrap_or_default().to_string(),
    }
}

fn append_responses_input_as_chat_messages(
    input: &Value,
    messages: &mut Vec<Value>,
    tool_context: &CodexToolContext,
) -> Result<(), ProxyError> {
    let mut pending_tool_calls = Vec::new();
    let mut pending_media = Vec::new();
    let mut pending_reasoning: Option<String> = None;
    let mut last_assistant_index: Option<usize> = None;

    match input {
        Value::String(text) => {
            messages.push(json!({
                "role": "user",
                "content": text
            }));
        }
        Value::Array(items) => {
            for item in items {
                append_responses_item_as_chat_message(
                    item,
                    messages,
                    &mut pending_tool_calls,
                    &mut pending_media,
                    &mut pending_reasoning,
                    &mut last_assistant_index,
                    tool_context,
                )?;
            }
        }
        Value::Object(_) => {
            append_responses_item_as_chat_message(
                input,
                messages,
                &mut pending_tool_calls,
                &mut pending_media,
                &mut pending_reasoning,
                &mut last_assistant_index,
                tool_context,
            )?;
        }
        _ => {}
    }

    // If a later assistant tool-call batch was accumulated after an earlier
    // media-bearing result, the synthetic user media belongs before that next
    // assistant turn.
    flush_pending_chat_tool_media(messages, &mut pending_media);
    flush_pending_tool_calls(
        messages,
        &mut pending_tool_calls,
        &mut pending_media,
        &mut pending_reasoning,
        &mut last_assistant_index,
    );
    // Pending reasoning still left after the whole input is processed is genuinely trailing thinking
    // (no message / function_call remains to attach it forward to), so it is attached backwards to the
    // last assistant; if the target already has reasoning_content it is appended, preserving both the
    // embedded and the trailing reasoning of the same turn.
    attach_pending_reasoning_to_previous_assistant(
        messages,
        last_assistant_index,
        &mut pending_reasoning,
    );
    backfill_tool_call_reasoning_placeholders(messages);
    Ok(())
}

fn append_responses_item_as_chat_message(
    item: &Value,
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    pending_media: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
    last_assistant_index: &mut Option<usize>,
    tool_context: &CodexToolContext,
) -> Result<(), ProxyError> {
    let item_type = item.get("type").and_then(|v| v.as_str());
    match item_type {
        Some("function_call") => {
            append_unique_pending_reasoning(pending_reasoning, responses_item_reasoning_text(item));
            pending_tool_calls.push(responses_function_call_to_chat_tool_call(
                item,
                tool_context,
            ));
        }
        Some("custom_tool_call") => {
            append_unique_pending_reasoning(pending_reasoning, responses_item_reasoning_text(item));
            pending_tool_calls.push(responses_custom_tool_call_to_chat_tool_call(item));
        }
        Some("tool_search_call") => {
            append_unique_pending_reasoning(pending_reasoning, responses_item_reasoning_text(item));
            pending_tool_calls.push(responses_tool_search_call_to_chat_tool_call(item));
        }
        Some("function_call_output") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_media,
                pending_reasoning,
                last_assistant_index,
            );
            let call_id = item.get("call_id").and_then(|v| v.as_str()).unwrap_or("");
            let media_plan = item
                .get("output")
                .cloned()
                .and_then(plan_chat_tool_output_media);
            let output = if let Some(media_plan) = media_plan {
                queue_chat_tool_output_media(pending_media, call_id, media_plan.media_parts);
                media_plan.tool_content
            } else {
                // Cache-sensitive no-media fallback: keep these expressions
                // byte-for-byte equivalent to the pre-fix conversion.
                match item.get("output") {
                    Some(Value::String(s)) => canonicalize_json_string_if_parseable(s),
                    Some(v) => canonical_json_string(v),
                    None => String::new(),
                }
            };
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output
            }));
        }
        Some("custom_tool_call_output") | Some("tool_search_output") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_media,
                pending_reasoning,
                last_assistant_index,
            );
            let call_id = item.get("call_id").and_then(|v| v.as_str()).unwrap_or("");
            let mut transformed_item = item.clone();
            let replacement_block = json!({
                "type": "text",
                "text": TOOL_RESULT_MEDIA_MOVED_MARKER
            });
            let mut media_parts = Vec::new();
            let replaced = transformed_item
                .get_mut("output")
                .map(|output| {
                    strip_and_clamp_media_from_tool_value(
                        output,
                        &mut media_parts,
                        ToolMediaScope::AllSupported,
                        &replacement_block,
                        TOOL_RESULT_MEDIA_MOVED_MARKER,
                    )
                })
                .unwrap_or(0);
            let output = if replaced > 0 {
                queue_chat_tool_output_media(pending_media, call_id, media_parts);
                canonical_json_string(&transformed_item)
            } else {
                // Preserve the legacy whole-item representation exactly.
                canonical_json_string(item)
            };
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output
            }));
        }
        Some("reasoning") => {
            // Reasoning always enters pending_reasoning first and is attached forward to the following
            // message / function_call (the latter consumed by flush_pending_tool_calls).
            // Previously, with pending_tool_calls empty this attached backwards to the previous assistant,
            // splicing a new turn's thinking into an old message so the following plain-text assistant lost its
            // reasoning_content, which made multi-turn conversations with thinking models (kimi and friends) break.
            // Genuinely trailing leftovers are attached backwards by the end-of-input wrap-up, or when a turn
            // boundary message (user and friends) arrives; see attach_pending_reasoning_to_previous_assistant.
            append_pending_reasoning(pending_reasoning, responses_reasoning_item_text(item));
        }
        Some("input_text" | "input_image" | "input_file" | "input_audio") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_media,
                pending_reasoning,
                last_assistant_index,
            );
            // `flush_pending_tool_calls` intentionally returns early when
            // there is no new assistant batch. A previous tool result may
            // still have media waiting, so flush it before this new message.
            flush_pending_chat_tool_media(messages, pending_media);
            let role = item
                .get("role")
                .and_then(|v| v.as_str())
                .map(responses_role_to_chat_role)
                .unwrap_or("user");
            let message = json!({
                "role": role,
                "content": responses_content_to_chat_content(role, &Value::Array(vec![item.clone()]))
            });
            if role == "assistant" {
                let mut message = message;
                attach_pending_reasoning_to_assistant(&mut message, pending_reasoning);
                update_last_assistant_index(messages, &message, last_assistant_index);
                messages.push(message);
                return Ok(());
            } else {
                // Non-assistant turn boundary messages (user and friends): pending reasoning is no longer dropped
                // outright but attached backwards to the previous assistant, appending when that message already
                // has reasoning_content. Reasoning must not leak across a user turn into a later assistant message;
                // with no previous assistant to attach to it is simply dropped (as before).
                attach_pending_reasoning_to_previous_assistant(
                    messages,
                    *last_assistant_index,
                    pending_reasoning,
                );
            }
            update_last_assistant_index(messages, &message, last_assistant_index);
            messages.push(message);
        }
        Some("message") | None => {
            if item.get("role").is_some() || item.get("content").is_some() {
                flush_pending_tool_calls(
                    messages,
                    pending_tool_calls,
                    pending_media,
                    pending_reasoning,
                    last_assistant_index,
                );
                flush_pending_chat_tool_media(messages, pending_media);
                let message = responses_message_item_to_chat_message(
                    item,
                    pending_reasoning,
                    messages,
                    *last_assistant_index,
                );
                update_last_assistant_index(messages, &message, last_assistant_index);
                messages.push(message);
            } else if pending_media.is_empty() {
                // Preserve legacy no-media ordering: inert message-like items
                // used to close a pending tool-call batch.
                flush_pending_tool_calls(
                    messages,
                    pending_tool_calls,
                    pending_media,
                    pending_reasoning,
                    last_assistant_index,
                );
            }
        }
        _ => {
            if item.get("role").is_some() || item.get("content").is_some() {
                flush_pending_tool_calls(
                    messages,
                    pending_tool_calls,
                    pending_media,
                    pending_reasoning,
                    last_assistant_index,
                );
                flush_pending_chat_tool_media(messages, pending_media);
                let message = responses_message_item_to_chat_message(
                    item,
                    pending_reasoning,
                    messages,
                    *last_assistant_index,
                );
                update_last_assistant_index(messages, &message, last_assistant_index);
                messages.push(message);
            } else if pending_media.is_empty() {
                // Preserve legacy no-media ordering without letting an inert
                // unknown item flush a media-bearing result batch.
                flush_pending_tool_calls(
                    messages,
                    pending_tool_calls,
                    pending_media,
                    pending_reasoning,
                    last_assistant_index,
                );
            }
        }
    }

    Ok(())
}

fn flush_pending_tool_calls(
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    pending_media: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
    last_assistant_index: &mut Option<usize>,
) {
    if pending_tool_calls.is_empty() {
        return;
    }

    // One Responses model turn can contain an assistant commentary message
    // directly followed by function-call items. Keep them on the same Chat
    // assistant message: a standalone text-only assistant turn teaches Chat
    // models to stop after a progress update instead of emitting the expected
    // tool call.
    if merge_pending_tool_calls_into_adjacent_assistant(
        messages,
        pending_tool_calls,
        pending_reasoning,
    ) {
        *last_assistant_index = Some(messages.len() - 1);
        return;
    }

    // Media from the preceding tool-result batch must be presented before a
    // new assistant tool-call turn. Consecutive outputs do not enter here
    // because `pending_tool_calls` is empty after the first output.
    flush_pending_chat_tool_media(messages, pending_media);
    let mut message = json!({
        "role": "assistant",
        "content": null,
        "tool_calls": std::mem::take(pending_tool_calls)
    });
    attach_pending_reasoning_to_assistant(&mut message, pending_reasoning);
    *last_assistant_index = Some(messages.len());
    messages.push(message);
}

/// Merge pending Chat tool calls into a directly adjacent assistant message
/// that does not carry tool calls yet. The Responses input preserves both the
/// commentary text and the calls as separate items of the same model turn;
/// serializing them as two consecutive assistant messages lets Chat models
/// imitate the first text-only message as a complete turn.
fn merge_pending_tool_calls_into_adjacent_assistant(
    messages: &mut [Value],
    pending_tool_calls: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
) -> bool {
    let Some(message) = messages.last_mut() else {
        return false;
    };
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return false;
    }
    let has_tool_calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .is_some_and(|calls| !calls.is_empty());
    if has_tool_calls {
        return false;
    }

    let Some(obj) = message.as_object_mut() else {
        return false;
    };
    obj.insert(
        "tool_calls".to_string(),
        Value::Array(std::mem::take(pending_tool_calls)),
    );
    attach_pending_reasoning_to_assistant_unique(message, pending_reasoning);
    true
}

fn attach_pending_reasoning_to_assistant_unique(
    message: &mut Value,
    pending_reasoning: &mut Option<String>,
) {
    let Some(reasoning) = pending_reasoning.take() else {
        return;
    };
    let reasoning = reasoning.trim();
    if reasoning.is_empty() {
        return;
    }

    let existing_text = message
        .get("reasoning_content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let existing_segments: Vec<&str> = existing_text
        .split("\n\n")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    let missing_segments: Vec<&str> = reasoning
        .split("\n\n")
        .map(str::trim)
        .filter(|segment| !segment.is_empty() && !existing_segments.contains(segment))
        .collect();
    if missing_segments.is_empty() {
        return;
    }

    let merged = if existing_segments.is_empty() {
        missing_segments.join("\n\n")
    } else {
        let existing = existing_segments.join("\n\n");
        let missing = missing_segments.join("\n\n");
        format!("{existing}\n\n{missing}")
    };
    if let Some(obj) = message.as_object_mut() {
        obj.insert("reasoning_content".to_string(), Value::String(merged));
    }
}

fn responses_message_item_to_chat_message(
    item: &Value,
    pending_reasoning: &mut Option<String>,
    messages: &mut [Value],
    last_assistant_index: Option<usize>,
) -> Value {
    let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
    let chat_role = responses_role_to_chat_role(role);
    let content = item
        .get("content")
        .map(|value| responses_content_to_chat_content(chat_role, value))
        .unwrap_or(Value::Null);

    let mut message = json!({
        "role": chat_role,
        "content": content
    });

    if chat_role == "assistant" {
        append_pending_reasoning(pending_reasoning, responses_message_reasoning_text(item));
        attach_pending_reasoning_to_assistant(&mut message, pending_reasoning);
    } else {
        // Non-assistant turn boundary messages (user and friends): pending reasoning is no longer dropped
        // outright but attached backwards to the previous assistant, appending trailing reasoning when it
        // already has reasoning_content, which also stops reasoning leaking across a user turn.
        attach_pending_reasoning_to_previous_assistant(
            messages,
            last_assistant_index,
            pending_reasoning,
        );
    }

    message
}

fn responses_role_to_chat_role(role: &str) -> &'static str {
    match role {
        "system" | "developer" => "system",
        "assistant" => "assistant",
        "tool" => "tool",
        "user" | "latest_reminder" => "user",
        _ => "user",
    }
}

fn update_last_assistant_index(
    messages: &[Value],
    message: &Value,
    last_assistant_index: &mut Option<usize>,
) {
    match message.get("role").and_then(|v| v.as_str()) {
        Some("assistant") => {
            *last_assistant_index = Some(messages.len());
        }
        Some("tool") => {}
        _ => {
            *last_assistant_index = None;
        }
    }
}

fn append_pending_reasoning(pending_reasoning: &mut Option<String>, reasoning: Option<String>) {
    let Some(reasoning) = reasoning else {
        return;
    };
    let reasoning = reasoning.trim();
    if reasoning.is_empty() {
        return;
    }

    match pending_reasoning {
        Some(existing) if !existing.is_empty() => {
            existing.push_str("\n\n");
            existing.push_str(reasoning);
        }
        _ => {
            *pending_reasoning = Some(reasoning.to_string());
        }
    }
}

fn append_unique_pending_reasoning(
    pending_reasoning: &mut Option<String>,
    reasoning: Option<String>,
) {
    let Some(reasoning) = reasoning else {
        return;
    };
    let reasoning = reasoning.trim();
    if reasoning.is_empty() {
        return;
    }

    match pending_reasoning {
        Some(existing) if existing.contains(reasoning) => {}
        Some(existing) if !existing.is_empty() => {
            existing.push_str("\n\n");
            existing.push_str(reasoning);
        }
        _ => {
            *pending_reasoning = Some(reasoning.to_string());
        }
    }
}

fn attach_pending_reasoning_to_assistant(
    message: &mut Value,
    pending_reasoning: &mut Option<String>,
) {
    let Some(reasoning) = pending_reasoning.take() else {
        return;
    };
    if reasoning.trim().is_empty() {
        return;
    }

    if let Some(obj) = message.as_object_mut() {
        append_reasoning_content(obj, &reasoning);
    }
}

/// After the whole input is processed, fills a placeholder into assistant tool-call messages that still lack `reasoning_content`.
/// This must run as the very last fallback of the pipeline: real reasoning may arrive as a trailing
/// `reasoning` item via `attach_pending_reasoning_to_previous_assistant`, and injecting the
/// placeholder too early would let `append_reasoning_content` append to it and pollute the real thinking.
fn backfill_tool_call_reasoning_placeholders(messages: &mut [Value]) {
    for message in messages.iter_mut() {
        let is_assistant_tool_call = message.get("role").and_then(|value| value.as_str())
            == Some("assistant")
            && message
                .get("tool_calls")
                .and_then(|value| value.as_array())
                .is_some_and(|calls| !calls.is_empty());
        if is_assistant_tool_call {
            ensure_tool_call_reasoning_content(message);
        }
    }
}

/// Thinking models such as kimi/Moonshot and DeepSeek require every assistant message carrying
/// `tool_calls` to also carry a non-empty `reasoning_content`. On a cross-turn history restore miss
/// (proxy restart losing the in-memory cache, ambiguous call_id, or a turn where upstream produced
/// no thinking) a placeholder is filled in to avoid `reasoning_content is missing in assistant tool call message`.
/// Symmetric with the placeholder behaviour of `transform::anthropic_to_openai_with_reasoning_content`.
fn ensure_tool_call_reasoning_content(message: &mut Value) {
    let Some(obj) = message.as_object_mut() else {
        return;
    };
    let has_reasoning = obj
        .get("reasoning_content")
        .and_then(|value| value.as_str())
        .is_some_and(|text| !text.trim().is_empty());
    if !has_reasoning {
        obj.insert(
            "reasoning_content".to_string(),
            Value::String("tool call".to_string()),
        );
    }
}

/// Attaches still-unconsumed pending reasoning backwards onto the previous assistant message.
///
/// Only two genuinely trailing situations may call this:
/// 1. pending_reasoning is left over after the whole input was processed, with nothing left to
///    attach it forward to;
/// 2. pending_reasoning is non-empty when a turn boundary message (user and friends) arrives, since
///    reasoning must not leak across a user turn, nor may attributable thinking simply be dropped.
///
/// This is the trailing/boundary wrap-up point, not the normal forward attribution path for reasoning.
/// If the target already has reasoning_content, the trailing reasoning is appended so both embedded
/// and trailing reasoning of the same assistant turn survive. Whether or not attaching succeeds,
/// pending is consumed (taken) and never carried over to the next assistant.
fn attach_pending_reasoning_to_previous_assistant(
    messages: &mut [Value],
    last_assistant_index: Option<usize>,
    pending_reasoning: &mut Option<String>,
) {
    let Some(reasoning) = pending_reasoning.take() else {
        return;
    };
    let reasoning = reasoning.trim();
    if reasoning.is_empty() {
        return;
    }
    let Some(message) = last_assistant_index.and_then(|index| messages.get_mut(index)) else {
        return;
    };
    if message.get("role").and_then(|v| v.as_str()) != Some("assistant") {
        return;
    }
    if let Some(obj) = message.as_object_mut() {
        append_reasoning_content(obj, reasoning);
    }
}

fn responses_message_reasoning_text(item: &Value) -> Option<String> {
    responses_item_reasoning_text(item)
}

fn responses_item_reasoning_text(item: &Value) -> Option<String> {
    extract_reasoning_field_text(item)
}

fn responses_reasoning_item_text(item: &Value) -> Option<String> {
    extract_reasoning_summary_text(item)
}

fn responses_content_to_chat_content(_role: &str, content: &Value) -> Value {
    if content.is_null() || content.is_string() {
        return content.clone();
    }

    let Some(parts) = content.as_array() else {
        return content.clone();
    };

    let mut chat_parts: Vec<Value> = Vec::new();
    let mut has_non_text_part = false;

    for part in parts {
        let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match part_type {
            "input_text" | "output_text" | "text" => {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        chat_parts.push(json!({
                            "type": "text",
                            "text": text
                        }));
                    }
                }
            }
            "refusal" => {
                if let Some(text) = part.get("refusal").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        chat_parts.push(json!({
                            "type": "text",
                            "text": text
                        }));
                    }
                }
            }
            "input_image" => {
                if let Some(image_url) = part.get("image_url") {
                    let image_url = if image_url.is_object() {
                        image_url.clone()
                    } else {
                        json!({ "url": image_url.as_str().unwrap_or_default() })
                    };
                    chat_parts.push(json!({
                        "type": "image_url",
                        "image_url": image_url
                    }));
                    has_non_text_part = true;
                }
            }
            "input_file" => {
                if let Some(file) = responses_input_file_to_chat_file(part) {
                    chat_parts.push(json!({
                        "type": "file",
                        "file": file
                    }));
                    has_non_text_part = true;
                }
            }
            "input_audio" => {
                if let Some(input_audio) = part.get("input_audio") {
                    chat_parts.push(json!({
                        "type": "input_audio",
                        "input_audio": input_audio.clone()
                    }));
                    has_non_text_part = true;
                }
            }
            _ => {}
        }
    }

    if !has_non_text_part {
        return Value::String(
            chat_parts
                .iter()
                .filter_map(|part| part.get("text").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    Value::Array(chat_parts)
}

fn responses_input_file_to_chat_file(part: &Value) -> Option<Value> {
    chat_file_from_input_file(part)
}

fn collect_tool_search_output_tools(value: &Value, context: &mut CodexToolContext) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_tool_search_output_tools(item, context);
            }
        }
        Value::Object(obj) => {
            if obj.get("type").and_then(|v| v.as_str()) == Some("tool_search_output") {
                if let Some(tools) = obj.get("tools").and_then(|v| v.as_array()) {
                    for tool in tools {
                        context.add_response_tool(tool);
                    }
                }
            }
            for value in obj.values() {
                collect_tool_search_output_tools(value, context);
            }
        }
        _ => {}
    }
}

pub(crate) fn flatten_namespace_tool_name(namespace: &str, name: &str) -> String {
    let full_name = format!("{namespace}__{name}");
    if full_name.len() <= CHAT_TOOL_NAME_MAX_LEN {
        return full_name;
    }

    let hash = short_sha256_hex(full_name.as_bytes());
    let suffix = format!("__{hash}");
    let prefix_len = CHAT_TOOL_NAME_MAX_LEN.saturating_sub(suffix.len());
    let mut prefix = String::new();
    for ch in full_name.chars() {
        if prefix.len() + ch.len_utf8() > prefix_len {
            break;
        }
        prefix.push(ch);
    }
    format!("{prefix}{suffix}")
}

fn responses_tool_name(tool: &Value) -> Option<String> {
    tool.get("function")
        .and_then(|function| function.get("name"))
        .or_else(|| tool.get("name"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn responses_custom_tool_description(tool: &Value) -> String {
    let mut description = String::new();
    description.push_str(CUSTOM_TOOL_PRESERVED_METADATA_HEADING);
    description.push_str("\n```json\n");
    description.push_str(&serialize_tool_definition_for_description(tool));
    description.push_str("\n```");
    description
}

fn serialize_tool_definition_for_description(tool: &Value) -> String {
    // Keep the embedded definition compact to reduce tool-description token
    // overhead for chat-only upstreams, while remaining stable across map
    // storage order.
    canonical_json_string(tool)
}

/// Normalize a function's `parameters` JSON Schema so `type` is always `"object"`.
///
/// Some Responses tools carry `parameters: null` or `parameters: {"type": null}`,
/// but OpenAI Chat Completions strictly requires `{"type": "object", "properties": {...}}`.
fn normalize_function_parameters(params: Option<&Value>) -> Value {
    let mut params = match params {
        Some(Value::Object(obj)) => Value::Object(obj.clone()),
        _ => json!({"type": "object", "properties": {}}),
    };
    if let Some(obj) = params.as_object_mut() {
        match obj.get("type").and_then(|v| v.as_str()) {
            Some("object") => {}
            _ => {
                obj.insert("type".to_string(), json!("object"));
            }
        }
    }
    params
}

fn responses_function_tool_to_chat_tool(tool: &Value, chat_name: &str) -> Option<Value> {
    if tool.get("type").and_then(|v| v.as_str()) != Some("function") {
        return None;
    }

    if let Some(function) = tool.get("function") {
        let mut chat_tool = json!({
            "type": "function",
            "function": function.clone()
        });
        if let Some(obj) = chat_tool
            .get_mut("function")
            .and_then(|value| value.as_object_mut())
        {
            // Ensure parameters.type is "object" for strict OpenAI-compatible providers
            let parameters = normalize_function_parameters(obj.get("parameters"));
            obj.insert("parameters".to_string(), parameters);

            obj.insert("name".to_string(), json!(chat_name));
            if let Some(strict) = tool.get("strict").cloned() {
                obj.entry("strict".to_string()).or_insert(strict);
            }
        }
        return Some(chat_tool);
    }

    let mut function = json!({
        "name": chat_name,
        "description": tool.get("description").cloned().unwrap_or(Value::Null),
        "parameters": normalize_function_parameters(tool.get("parameters"))
    });
    if let Some(strict) = tool.get("strict") {
        function["strict"] = strict.clone();
    }

    Some(json!({
        "type": "function",
        "function": function
    }))
}

fn responses_function_call_to_chat_tool_call(
    item: &Value,
    tool_context: &CodexToolContext,
) -> Value {
    let call_id = item
        .get("call_id")
        .or_else(|| item.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let namespace = item.get("namespace").and_then(|v| v.as_str());
    let chat_name = tool_context.chat_name_for_response_function(name, namespace);
    let arguments = canonicalize_tool_arguments(item.get("arguments"));

    json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": chat_name,
            "arguments": arguments
        }
    })
}

fn responses_custom_tool_call_to_chat_tool_call(item: &Value) -> Value {
    let call_id = item
        .get("call_id")
        .or_else(|| item.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let input = item.get("input").cloned().unwrap_or_else(|| json!(""));

    json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": name,
            "arguments": canonical_json_string(&json!({ CUSTOM_TOOL_INPUT_FIELD: input }))
        }
    })
}

fn responses_tool_search_call_to_chat_tool_call(item: &Value) -> Value {
    let call_id = item
        .get("call_id")
        .or_else(|| item.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let arguments = item
        .get("arguments")
        .map(canonical_json_string)
        .unwrap_or_else(|| "{}".to_string());

    json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": TOOL_SEARCH_PROXY_NAME,
            "arguments": arguments
        }
    })
}

fn responses_tool_choice_to_chat(tool_choice: &Value, tool_context: &CodexToolContext) -> Value {
    match tool_choice {
        Value::Object(obj) if obj.get("type").and_then(|v| v.as_str()) == Some("function") => {
            let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let namespace = obj.get("namespace").and_then(|v| v.as_str());
            let chat_name = tool_context.chat_name_for_response_function(name, namespace);
            json!({
                "type": "function",
                "function": {
                    "name": chat_name
                }
            })
        }
        Value::Object(obj) if obj.get("type").and_then(|v| v.as_str()) == Some("tool_search") => {
            json!({
                "type": "function",
                "function": {
                    "name": TOOL_SEARCH_PROXY_NAME
                }
            })
        }
        Value::Object(obj) if obj.get("type").and_then(|v| v.as_str()) == Some("custom") => {
            let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
            json!({
                "type": "function",
                "function": {
                    "name": name
                }
            })
        }
        _ => tool_choice.clone(),
    }
}

/// Convert a non-streaming Chat Completions response into a Responses response,
/// restoring Codex-specific tool names using the original Responses request.
pub(crate) fn chat_completion_to_response_with_context(
    body: Value,
    tool_context: &CodexToolContext,
) -> Result<Value, ProxyError> {
    let choices = body
        .get("choices")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ProxyError::TransformError("No choices in chat response".to_string()))?;
    let choice = choices
        .first()
        .ok_or_else(|| ProxyError::TransformError("Empty choices in chat response".to_string()))?;
    let message = choice
        .get("message")
        .ok_or_else(|| ProxyError::TransformError("No message in chat choice".to_string()))?;

    let response_id = response_id_from_chat_id(body.get("id").and_then(|v| v.as_str()));
    let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("");
    let created_at = body.get("created").and_then(|v| v.as_u64()).unwrap_or(0);
    let finish_reason = choice.get("finish_reason").and_then(|v| v.as_str());

    let reasoning = chat_reasoning_text(message);
    let mut output = Vec::new();
    if let Some(reasoning_item) =
        chat_reasoning_to_response_output_item(reasoning.as_deref(), &response_id)
    {
        output.push(reasoning_item);
    }
    if let Some(message_item) = chat_message_to_response_output_item(message, &response_id) {
        output.push(message_item);
    }
    let tool_calls =
        chat_tool_calls_to_response_output_items(message, reasoning.as_deref(), tool_context);

    // When tool calls were dropped and none survive, Codex receives a turn with
    // "status=completed but no tool call in output" and the agent loop inevitably ends silently
    // (#4341). Report the failure honestly instead of faking success. As long as one valid tool call
    // remains Codex would have continued, the condition does not hold, and behaviour is unchanged.
    //
    // As in the streaming branch, this applies only to turns that should be `completed`:
    // `finish_reason=length` is truncation, a tool call missing its name is the consequence of that
    // rather than malformed upstream data, and reporting tool_call_dropped would attribute it wrongly.
    if response_status_from_finish_reason(finish_reason) == "completed"
        && tool_calls.dropped > 0
        && tool_calls.items.is_empty()
    {
        return Err(ProxyError::TransformError(format!(
            "Upstream returned {} tool call(s) without a function name, \
             leaving no usable tool call in this turn",
            tool_calls.dropped
        )));
    }
    output.extend(tool_calls.items);

    let mut response = json!({
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": response_status_from_finish_reason(finish_reason),
        "model": model,
        "output": output,
        "usage": chat_usage_to_responses_usage(body.get("usage"))
    });

    if finish_reason == Some("length") {
        response["incomplete_details"] = json!({ "reason": "max_output_tokens" });
    }

    Ok(response)
}

fn chat_reasoning_to_response_output_item(
    reasoning: Option<&str>,
    response_id: &str,
) -> Option<Value> {
    let reasoning = reasoning?;
    if reasoning.is_empty() {
        return None;
    }

    Some(json!({
        "id": format!("rs_{response_id}"),
        "type": "reasoning",
        "summary": [{
            "type": "summary_text",
            "text": reasoning
        }]
    }))
}

fn chat_reasoning_text(message: &Value) -> Option<String> {
    if let Some(reasoning) = extract_reasoning_field_text(message) {
        return Some(reasoning);
    }

    if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
        if let Some((reasoning, _answer)) = split_leading_think_block(content) {
            if !reasoning.is_empty() {
                return Some(reasoning);
            }
        }
    }

    None
}

fn chat_message_to_response_output_item(message: &Value, response_id: &str) -> Option<Value> {
    let mut content = Vec::new();

    if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
        let text = split_leading_think_block(text)
            .map(|(_reasoning, answer)| answer)
            .unwrap_or_else(|| text.to_string());
        if !text.is_empty() {
            content.push(json!({
                "type": "output_text",
                "text": text,
                "annotations": []
            }));
        }
    } else if let Some(parts) = message.get("content").and_then(|v| v.as_array()) {
        for part in parts {
            let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match part_type {
                "text" | "output_text" => {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            content.push(json!({
                                "type": "output_text",
                                "text": text,
                                "annotations": []
                            }));
                        }
                    }
                }
                "refusal" => {
                    if let Some(text) = part.get("refusal").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            content.push(json!({
                                "type": "refusal",
                                "refusal": text
                            }));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(refusal) = message.get("refusal").and_then(|v| v.as_str()) {
        if !refusal.is_empty() {
            content.push(json!({
                "type": "refusal",
                "refusal": refusal
            }));
        }
    }

    if content.is_empty() {
        return None;
    }

    Some(json!({
        "id": format!("{response_id}_msg"),
        "type": "message",
        "status": "completed",
        "role": "assistant",
        "content": content
    }))
}

/// Result of a non-streaming tool call conversion. `dropped` counts entries discarded for lacking a
/// valid function name, letting the caller tell whether Codex can still continue this turn (see #4341).
struct ChatToolCallItems {
    items: Vec<Value>,
    dropped: usize,
}

fn chat_tool_calls_to_response_output_items(
    message: &Value,
    reasoning: Option<&str>,
    tool_context: &CodexToolContext,
) -> ChatToolCallItems {
    let mut output = Vec::new();
    let mut dropped = 0usize;

    if let Some(tool_calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for (index, tool_call) in tool_calls.iter().enumerate() {
            // Skip tool calls with missing function names (defensive: some models
            // may generate tool calls without providing a valid name)
            let function = tool_call.get("function").unwrap_or(&Value::Null);
            let name = function.get("name").and_then(|v| v.as_str()).unwrap_or("");
            // A whitespace-only name matches no published tool either, so treat it like an empty name.
            if name.trim().is_empty() {
                dropped += 1;
                // Log structure only, never the arguments content (it may contain user code).
                let call_id_empty = tool_call
                    .get("id")
                    .and_then(|v| v.as_str())
                    .is_none_or(str::is_empty);
                let args_bytes = function
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .map(str::len)
                    .unwrap_or(0);
                log::warn!(
                    "[Codex] dropped tool call: index={index} call_id_empty={call_id_empty} \
                     args_bytes={args_bytes} tools_total={}",
                    tool_calls.len()
                );
                continue;
            }
            output.push(chat_tool_call_to_response_item(
                tool_call,
                index,
                reasoning,
                tool_context,
            ));
        }
    } else if let Some(function_call) = message.get("function_call") {
        match chat_legacy_function_call_to_response_item(function_call, reasoning, tool_context) {
            Some(item) => output.push(item),
            None => dropped += 1,
        }
    }

    ChatToolCallItems {
        items: output,
        dropped,
    }
}

fn chat_tool_call_to_response_item(
    tool_call: &Value,
    index: usize,
    reasoning: Option<&str>,
    tool_context: &CodexToolContext,
) -> Value {
    let call_id = tool_call
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("call_{index}"));
    let function = tool_call.get("function").unwrap_or(&Value::Null);
    let name = function.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let arguments = canonicalize_tool_arguments(function.get("arguments"));

    let item_id = response_tool_call_item_id_from_chat_name(&call_id, name, tool_context);
    response_tool_call_item_from_chat_name(
        &item_id,
        "completed",
        &call_id,
        name,
        &arguments,
        reasoning,
        tool_context,
    )
}

fn chat_legacy_function_call_to_response_item(
    function_call: &Value,
    reasoning: Option<&str>,
    tool_context: &CodexToolContext,
) -> Option<Value> {
    let call_id = function_call
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .unwrap_or("call_0");
    let name = function_call
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Skip legacy function calls with missing names (defensive: some models
    // may generate function_call without providing a valid name).
    // A whitespace-only name matches no published tool either, so treat it like an empty name.
    if name.trim().is_empty() {
        // Log structure only, never the arguments content (it may contain user code).
        let args_bytes = function_call
            .get("arguments")
            .and_then(|v| v.as_str())
            .map(str::len)
            .unwrap_or(0);
        log::warn!(
            "[Codex] dropped legacy function_call: call_id={call_id} args_bytes={args_bytes}"
        );
        return None;
    }

    let arguments = canonicalize_tool_arguments(function_call.get("arguments"));

    let item_id = response_tool_call_item_id_from_chat_name(call_id, name, tool_context);
    Some(response_tool_call_item_from_chat_name(
        &item_id,
        "completed",
        call_id,
        name,
        &arguments,
        reasoning,
        tool_context,
    ))
}

pub(crate) fn response_tool_call_item_id_from_chat_name(
    call_id: &str,
    chat_name: &str,
    tool_context: &CodexToolContext,
) -> String {
    if tool_context.is_custom_tool_chat_name(chat_name) {
        format!("ctc_{call_id}")
    } else {
        format!("fc_{call_id}")
    }
}

pub(crate) fn response_tool_call_item_from_chat_name(
    item_id: &str,
    status: &str,
    call_id: &str,
    chat_name: &str,
    arguments: &str,
    reasoning: Option<&str>,
    tool_context: &CodexToolContext,
) -> Value {
    match tool_context.lookup_chat_name(chat_name) {
        Some(spec) if spec.kind == CodexToolKind::ToolSearch => {
            response_tool_search_call_item(call_id, status, arguments, reasoning)
        }
        Some(spec) if spec.kind == CodexToolKind::Custom => response_custom_tool_call_item(
            item_id, status, call_id, &spec.name, arguments, reasoning,
        ),
        Some(spec) => response_function_call_item_with_namespace(
            item_id,
            status,
            call_id,
            &spec.name,
            spec.namespace.as_deref(),
            arguments,
            reasoning,
        ),
        None => {
            response_function_call_item(item_id, status, call_id, chat_name, arguments, reasoning)
        }
    }
}

fn response_tool_search_call_item(
    call_id: &str,
    status: &str,
    arguments: &str,
    reasoning: Option<&str>,
) -> Value {
    let parsed_arguments = parse_tool_arguments_object(arguments);
    let mut item = json!({
        "type": "tool_search_call",
        "call_id": call_id,
        "status": status,
        "execution": "client",
        "arguments": parsed_arguments
    });
    super::codex_chat_common::attach_optional_reasoning_content_field(&mut item, reasoning);
    item
}

fn response_custom_tool_call_item(
    item_id: &str,
    status: &str,
    call_id: &str,
    name: &str,
    arguments: &str,
    reasoning: Option<&str>,
) -> Value {
    let input = custom_tool_input_from_chat_arguments(arguments);
    let mut item = json!({
        "id": item_id,
        "type": "custom_tool_call",
        "status": status,
        "call_id": call_id,
        "name": name,
        "input": input
    });
    super::codex_chat_common::attach_optional_reasoning_content_field(&mut item, reasoning);
    item
}

fn parse_tool_arguments_object(arguments: &str) -> Value {
    if arguments.trim().is_empty() {
        return json!({});
    }
    serde_json::from_str::<Value>(arguments)
        .ok()
        .filter(|value| value.is_object())
        .unwrap_or_else(|| json!({ "query": arguments }))
}

pub(crate) fn custom_tool_input_from_chat_arguments(arguments: &str) -> String {
    if arguments.trim().is_empty() {
        return String::new();
    }
    match serde_json::from_str::<Value>(arguments) {
        Ok(Value::Object(obj)) => obj
            .get(CUSTOM_TOOL_INPUT_FIELD)
            .and_then(|value| value.as_str())
            .unwrap_or(arguments)
            .to_string(),
        _ => arguments.to_string(),
    }
}

pub(crate) fn chat_usage_to_responses_usage(usage: Option<&Value>) -> Value {
    let Some(usage) = usage.filter(|value| value.is_object() && !value.is_null()) else {
        return json!({
            "input_tokens": 0,
            "input_tokens_details": { "cached_tokens": 0 },
            "output_tokens": 0,
            "total_tokens": 0,
            "output_tokens_details": { "reasoning_tokens": 0 }
        });
    };

    let input_tokens = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(input_tokens + output_tokens);

    let mut result = json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": total_tokens
    });

    let direct_cache_read = usage.get("cache_read_input_tokens").and_then(Value::as_u64);
    let cached = direct_cache_read
        .or_else(|| {
            usage
                .pointer("/prompt_tokens_details/cached_tokens")
                .and_then(Value::as_u64)
        })
        .or_else(|| {
            usage
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(Value::as_u64)
        })
        // DeepSeek Chat's documented cache-hit field (mirroring usage/parser.rs), used as the last fallback.
        // The official endpoint currently mirrors the same value into the undocumented
        // prompt_tokens_details.cached_tokens (matched by the standard field above), so this only applies
        // when upstream sends the documented field without the mirror (some relays) and guards against the mirror disappearing; nothing changes when upstream sends any standard field.
        .or_else(|| usage.get("prompt_cache_hit_tokens").and_then(Value::as_u64))
        .unwrap_or(0);
    let cache_write = usage
        .pointer("/prompt_tokens_details/cache_write_tokens")
        .or_else(|| usage.pointer("/input_tokens_details/cache_write_tokens"))
        .and_then(|v| v.as_u64())
        .or_else(|| {
            usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
        })
        .unwrap_or(0);
    if cached > 0 || cache_write > 0 {
        result["input_tokens_details"] = json!({
            "cached_tokens": cached,
            "cache_write_tokens": cache_write
        });
    } else {
        result["input_tokens_details"] = json!({ "cached_tokens": 0 });
    }

    if let Some(details) = usage
        .get("completion_tokens_details")
        .filter(|v| v.is_object())
    {
        let mut details = details.clone();
        if details.get("reasoning_tokens").is_none() {
            details["reasoning_tokens"] = json!(0);
        }
        result["output_tokens_details"] = details;
    } else {
        result["output_tokens_details"] = json!({ "reasoning_tokens": 0 });
    }

    if let Some(cache_read) = direct_cache_read {
        result["cache_read_input_tokens"] = json!(cache_read);
    }
    if cache_write > 0 {
        result["cache_creation_input_tokens"] = json!(cache_write);
    }

    result
}

pub(crate) fn response_id_from_chat_id(id: Option<&str>) -> String {
    let id = id.unwrap_or("ccswitch");
    if id.starts_with("resp_") {
        id.to_string()
    } else {
        format!("resp_{id}")
    }
}

pub(crate) fn response_status_from_finish_reason(finish_reason: Option<&str>) -> &'static str {
    match finish_reason {
        Some("length") => "incomplete",
        _ => "completed",
    }
}

/// Normalizes a Chat Completions upstream error body into an OpenAI Responses API style error object.
///
/// Handles three kinds of input:
/// 1. The standard OpenAI shape `{"error": {"message": "...", "type": "...", "code": ...}}`
/// 2. Non-standard shapes such as MiniMax's `{"base_resp": {"status_code": 2013, "status_msg": "..."}}`
/// 3. Minimal errors with only a top-level `message` / `detail`, or a bare string
///
/// The output is always `{"error": {"message", "type", "code", "param"}}`, matching OpenAI Responses
/// API error responses; the Codex client's error handling recognises only this shape.
pub fn chat_error_to_response_error(body: Option<&Value>) -> Value {
    let Some(value) = body else {
        return json!({
            "error": {
                "message": "Upstream returned an empty error response",
                "type": "upstream_error",
                "code": serde_json::Value::Null,
                "param": serde_json::Value::Null,
            }
        });
    };

    if let Some(text) = value.as_str() {
        return json!({
            "error": {
                "message": text,
                "type": "upstream_error",
                "code": serde_json::Value::Null,
                "param": serde_json::Value::Null,
            }
        });
    }

    let source = value.get("error").unwrap_or(value);

    let message = source
        .get("message")
        .or_else(|| source.get("detail"))
        .or_else(|| source.get("status_msg"))
        .or_else(|| source.pointer("/base_resp/status_msg"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .or_else(|| source.as_str().map(ToString::to_string))
        .unwrap_or_else(|| {
            // If no text can be extracted from any field, serialize the whole JSON back so users can debug it.
            serde_json::to_string(source).unwrap_or_else(|_| "Upstream error".to_string())
        });

    let error_type = source
        .get("type")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| "upstream_error".to_string());

    let code = source
        .get("code")
        .cloned()
        .or_else(|| source.pointer("/base_resp/status_code").cloned())
        .unwrap_or(serde_json::Value::Null);

    let param = source
        .get("param")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    json!({
        "error": {
            "message": message,
            "type": error_type,
            "code": code,
            "param": param,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_reasoning_effort_handles_ultra_per_mode() {
        // passthrough targets generic upstreams with unknown enums: ultra passes through like max/xhigh
        // so an upstream declaring that tier receives the user's choice.
        assert_eq!(map_reasoning_effort("ultra", None, None), Some("ultra"));
        // Dedicated modes with known enums clamp to their own highest valid tier rather than silently dropping to None.
        assert_eq!(
            map_reasoning_effort("ultra", Some("deepseek"), None),
            Some("max")
        );
        assert_eq!(
            map_reasoning_effort("ultra", Some("low_high"), None),
            Some("high")
        );
        assert_eq!(
            map_reasoning_effort("ultra", Some("openrouter"), None),
            Some("xhigh")
        );
        // Genuinely unknown values are still dropped, to avoid an upstream 400.
        assert_eq!(map_reasoning_effort("turbo", None, None), None);
    }

    #[test]
    fn responses_request_to_chat_uses_provider_reasoning_effort_for_deepseek_model() {
        let input = json!({
            "model": "deepseek-v4-pro",
            "input": "hello",
            "reasoning": {"effort": "xhigh"}
        });
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("deepseek".to_string()),
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        };

        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert_eq!(result["thinking"]["type"], "enabled");
        assert_eq!(result["reasoning_effort"], "max");
    }

    #[test]
    fn chat_usage_to_responses_usage_maps_deepseek_cache_hit_tokens() {
        // DeepSeek Chat's documented cache-hit field must also reach the Responses
        // input_tokens_details (related to issue #6073): when upstream sends only that field without
        // mirroring prompt_tokens_details.cached_tokens (some relays), missing this fallback leaves
        // cached_tokens at 0 in the synthesized response.completed under routing mode, so neither the
        // Codex session record nor the local log sees the cache hit.
        let usage = json!({
            "prompt_tokens": 1000,
            "completion_tokens": 100,
            "total_tokens": 1100,
            "prompt_cache_hit_tokens": 600,
            "prompt_cache_miss_tokens": 400
        });

        let result = chat_usage_to_responses_usage(Some(&usage));
        assert_eq!(result["input_tokens"], 1000);
        assert_eq!(result["output_tokens"], 100);
        assert_eq!(result["input_tokens_details"]["cached_tokens"], 600);
        assert_eq!(result["input_tokens_details"]["cache_write_tokens"], 0);
    }

    #[test]
    fn responses_request_to_chat_maps_openrouter_to_native_reasoning_object() {
        // OpenRouter platform shape: the native reasoning:{effort} object plus the "openrouter" value map
        // (matching the config inferred by infer_aggregator_platform_config).
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(false),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning.effort".to_string()),
            effort_value_mode: Some("openrouter".to_string()),
            output_format: Some("auto".to_string()),
            effort_levels: None,
        };

        // max is outside the OpenRouter enum (see openclaw#77350), so it must clamp to xhigh and be
        // written into the native reasoning object rather than the top-level reasoning_effort alias.
        let input = json!({
            "model": "deepseek/deepseek-chat-v3.1",
            "input": "hello",
            "reasoning": {"effort": "max"}
        });
        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert_eq!(result["reasoning"]["effort"], "xhigh");
        assert!(result.get("reasoning_effort").is_none());
        // thinking_param=none: even if supports_effort drags supports_thinking to true, no thinking field
        // is written (OpenRouter does not accept thinking:{type}).
        assert!(result.get("thinking").is_none());

        // Valid tiers pass through unchanged.
        let input_high = json!({
            "model": "deepseek/deepseek-chat-v3.1",
            "input": "hello",
            "reasoning": {"effort": "high"}
        });
        let result_high =
            responses_to_chat_completions_with_reasoning(input_high, Some(&config)).unwrap();
        assert_eq!(result_high["reasoning"]["effort"], "high");
        assert!(result_high.get("reasoning_effort").is_none());
    }

    #[test]
    fn responses_request_to_chat_passes_explicit_none_through_for_openrouter() {
        // The native OpenRouter reasoning object supports an explicit off: effort=none must forward
        // faithfully as {"reasoning":{"effort":"none"}} instead of being swallowed, otherwise models that
        // think by default cannot be turned off, skewing behaviour and cost.
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(false),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning.effort".to_string()),
            effort_value_mode: Some("openrouter".to_string()),
            output_format: Some("auto".to_string()),
            effort_levels: None,
        };

        let input = json!({
            "model": "openai/gpt-5",
            "input": "hello",
            "reasoning": {"effort": "none"}
        });
        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert_eq!(result["reasoning"]["effort"], "none");
        // none is not valid in the top-level OpenAI reasoning_effort enum, so neither the alias nor thinking is written.
        assert!(result.get("reasoning_effort").is_none());
        assert!(result.get("thinking").is_none());
    }

    #[test]
    fn responses_request_to_chat_drops_explicit_none_for_top_level_effort_provider() {
        // By contrast, platforms using top-level reasoning_effort (DeepSeek/OpenAI style) have no none in
        // their enum, so an explicit none must not become reasoning_effort:"none" (upstream rejects it) and only takes the thinking-off path.
        // This pins the boundary that none only passes through in the reasoning.effort shape, preventing regressions.
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("deepseek".to_string()),
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        };

        let input = json!({
            "model": "deepseek-v4-pro",
            "input": "hello",
            "reasoning": {"effort": "none"}
        });
        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        // The thinking-off signal is still sent, but neither reasoning_effort nor the native reasoning object is written.
        assert_eq!(result["thinking"]["type"], "disabled");
        assert!(result.get("reasoning_effort").is_none());
        assert!(result.get("reasoning").is_none());
    }

    #[test]
    fn responses_request_to_chat_clamps_zen_effort_to_model_declared_levels() {
        // OpenCode Zen platform shape plus per-model tier clamping (the table mirrors models.dev:
        // glm-5.2 declares only high|max). Pinned because a union mapping would send Codex's default
        // medium to glm-5.2, which declares only high|max (and happens to be the preset default model),
        // and a strictly validating gateway would error. Lower tiers clamp up to the nearest valid tier, and anything above the top (including the Codex extension tier ultra) takes the highest valid tier.
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("zen".to_string()),
            output_format: Some("reasoning_content".to_string()),
            effort_levels: Some(vec!["high".to_string(), "max".to_string()]),
        };

        for (input_effort, expected) in [
            ("minimal", "high"),
            ("low", "high"),
            ("medium", "high"),
            ("high", "high"),
            ("xhigh", "max"),
            ("max", "max"),
            ("ultra", "max"),
        ] {
            let input = json!({
                "model": "glm-5.2",
                "input": "hello",
                "reasoning": {"effort": input_effort}
            });
            let result =
                responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();
            assert_eq!(
                result["reasoning_effort"], expected,
                "effort={input_effort}"
            );
            // Written as top-level reasoning_effort, with no thinking field at all
            // (the gateway accepts only the platform-normalized parameter, not the vendor thinking shape).
            assert!(result.get("thinking").is_none());
        }
    }

    #[test]
    fn responses_request_to_chat_zen_preserves_low_when_model_declares_it() {
        // deepseek-v4-flash declares low|high|max: low is valid and passes through, medium clamps up to
        // high, and xhigh clamps up to max. Regression lock: the old deepseek vendor branch mapped
        // everything but max to high, and the per-model clamp must be no worse, nor emit undeclared values.
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("zen".to_string()),
            output_format: Some("reasoning_content".to_string()),
            effort_levels: Some(vec![
                "low".to_string(),
                "high".to_string(),
                "max".to_string(),
            ]),
        };

        for (input_effort, expected) in [
            ("minimal", "low"),
            ("low", "low"),
            ("medium", "high"),
            ("xhigh", "max"),
        ] {
            let input = json!({
                "model": "deepseek-v4-flash",
                "input": "hello",
                "reasoning": {"effort": input_effort}
            });
            let result =
                responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();
            assert_eq!(
                result["reasoning_effort"], expected,
                "effort={input_effort}"
            );
        }
    }

    #[test]
    fn responses_request_to_chat_zen_single_level_model_clamps_everything_to_max() {
        // kimi-k3 declares only max (no intersection exists across all models, so a union mapping cannot
        // cover it and per-model clamping is required): every requested tier clamps to max.
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("zen".to_string()),
            output_format: Some("reasoning_content".to_string()),
            effort_levels: Some(vec!["max".to_string()]),
        };

        for input_effort in ["minimal", "medium", "high", "max"] {
            let input = json!({
                "model": "kimi-k3",
                "input": "hello",
                "reasoning": {"effort": input_effort}
            });
            let result =
                responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();
            assert_eq!(result["reasoning_effort"], "max", "effort={input_effort}");
        }
    }

    #[test]
    fn responses_request_to_chat_omits_zen_effort_without_model_levels() {
        // A model that declares no effort in the catalog (toggle/budget models such as glm-5.1 and the qwen
        // family, or a model absent from the catalog) sends no reasoning_effort at all, so strictly validating gateways never receive unverifiable values.
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("zen".to_string()),
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        };

        let input = json!({
            "model": "glm-5.1",
            "input": "hello",
            "reasoning": {"effort": "medium"}
        });
        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert!(result.get("reasoning_effort").is_none());
        assert!(result.get("thinking").is_none());
    }

    #[test]
    fn responses_request_to_chat_maps_thinking_only_provider_without_effort() {
        let input = json!({
            "model": "kimi-k2.6",
            "input": "hello",
            "reasoning": {"effort": "high"}
        });
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        };

        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert_eq!(result["thinking"]["type"], "enabled");
        assert!(result.get("reasoning_effort").is_none());
    }

    #[test]
    fn responses_request_to_chat_maps_enable_thinking_provider() {
        let input = json!({
            "model": "qwen3-max",
            "input": "hello",
            "reasoning": {"effort": "medium"}
        });
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("enable_thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        };

        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert_eq!(result["enable_thinking"], true);
        assert!(result.get("reasoning_effort").is_none());
    }

    #[test]
    fn collapse_system_messages_preserves_non_system_order() {
        let input = vec![
            json!({"role": "system", "content": "S1"}),
            json!({"role": "user", "content": "U1"}),
            json!({"role": "assistant", "content": "A1"}),
            json!({"role": "system", "content": "S2"}),
            json!({"role": "user", "content": "U2"}),
        ];
        let out = collapse_system_messages_to_head(input);

        assert_eq!(out.len(), 4);
        assert_eq!(out[0]["role"], "system");
        assert_eq!(out[0]["content"], "S1\n\nS2");
        assert_eq!(out[1]["content"], "U1");
        assert_eq!(out[2]["content"], "A1");
        assert_eq!(out[3]["content"], "U2");
    }

    #[test]
    fn chat_usage_to_responses_includes_required_input_token_details() {
        let usage = json!({
            "prompt_tokens": 13,
            "completion_tokens": 245,
            "total_tokens": 258,
            "prompt_tokens_details": { "cached_tokens": 0 }
        });

        let converted = chat_usage_to_responses_usage(Some(&usage));
        assert_eq!(
            converted["input_tokens_details"],
            json!({ "cached_tokens": 0 })
        );

        let fallback = chat_usage_to_responses_usage(None);
        assert_eq!(
            fallback["input_tokens_details"],
            json!({ "cached_tokens": 0 })
        );
    }

    #[test]
    fn chat_usage_to_responses_resolves_cache_read_precedence() {
        let direct_cache_usage = json!({
            "prompt_tokens": 100,
            "completion_tokens": 5,
            "prompt_tokens_details": { "cached_tokens": 0 },
            "cache_read_input_tokens": 40
        });
        let converted = chat_usage_to_responses_usage(Some(&direct_cache_usage));
        assert_eq!(converted["input_tokens_details"]["cached_tokens"], 40);
        assert_eq!(converted["cache_read_input_tokens"], 40);

        let direct_zero = json!({
            "prompt_tokens_details": { "cached_tokens": 12 },
            "cache_read_input_tokens": 0
        });
        let converted = chat_usage_to_responses_usage(Some(&direct_zero));
        assert_eq!(converted["input_tokens_details"]["cached_tokens"], 0);
        assert_eq!(converted["cache_read_input_tokens"], 0);

        let invalid_direct = json!({
            "prompt_tokens_details": { "cached_tokens": 12 },
            "cache_read_input_tokens": "invalid"
        });
        let converted = chat_usage_to_responses_usage(Some(&invalid_direct));
        assert_eq!(converted["input_tokens_details"]["cached_tokens"], 12);
        assert!(converted.get("cache_read_input_tokens").is_none());

        let invalid_prompt_details = json!({
            "prompt_tokens_details": { "cached_tokens": "invalid" },
            "input_tokens_details": { "cached_tokens": 7 }
        });
        let converted = chat_usage_to_responses_usage(Some(&invalid_prompt_details));
        assert_eq!(converted["input_tokens_details"]["cached_tokens"], 7);
    }

    #[test]
    fn chat_response_to_responses_restores_loaded_namespace_tool_call() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{"type": "tool_search"}],
            "input": [{
                "type": "tool_search_output",
                "call_id": "call_tool_search_1",
                "status": "completed",
                "execution": "client",
                "tools": [{
                    "type": "namespace",
                    "name": "mcp__codex_apps__gmail",
                    "description": "Find and reference emails from your inbox.",
                    "tools": [{
                        "type": "function",
                        "name": "_search_emails",
                        "description": "Search Gmail for emails matching a query.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "query": {"type": "string"},
                                "label_ids": {"type": "array", "items": {"type": "string"}},
                                "max_results": {"type": "integer"}
                            }
                        }
                    }]
                }]
            }]
        });
        let context = build_codex_tool_context_from_request(&request);
        let chat = json!({
            "id": "chatcmpl_gmail",
            "object": "chat.completion",
            "created": 123,
            "model": "gpt-5.4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_gmail",
                        "type": "function",
                        "function": {
                            "name": "mcp__codex_apps__gmail___search_emails",
                            "arguments": "{\"query\":\"-in:spam -in:trash\",\"label_ids\":[\"UNREAD\"],\"max_results\":5}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let result = chat_completion_to_response_with_context(chat, &context).unwrap();

        assert_eq!(result["output"][0]["type"], "function_call");
        assert_eq!(result["output"][0]["call_id"], "call_gmail");
        assert_eq!(result["output"][0]["namespace"], "mcp__codex_apps__gmail");
        assert_eq!(result["output"][0]["name"], "_search_emails");
        assert_eq!(
            result["output"][0]["arguments"],
            r#"{"label_ids":["UNREAD"],"max_results":5,"query":"-in:spam -in:trash"}"#
        );
    }

    #[test]
    fn chat_response_to_responses_restores_tool_search_call() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{"type": "tool_search"}],
            "input": "Find tools."
        });
        let context = build_codex_tool_context_from_request(&request);
        let chat = json!({
            "id": "chatcmpl_tool_search",
            "object": "chat.completion",
            "created": 123,
            "model": "gpt-5.4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_tool_search_1",
                        "type": "function",
                        "function": {
                            "name": "tool_search",
                            "arguments": "{\"query\":\"Gmail search emails\",\"limit\":10}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let result = chat_completion_to_response_with_context(chat, &context).unwrap();

        assert_eq!(result["output"][0]["type"], "tool_search_call");
        assert_eq!(result["output"][0]["call_id"], "call_tool_search_1");
        assert_eq!(result["output"][0]["execution"], "client");
        assert_eq!(
            result["output"][0]["arguments"]["query"],
            "Gmail search emails"
        );
        assert_eq!(result["output"][0]["arguments"]["limit"], 10);
    }

    #[test]
    fn chat_response_to_responses_restores_custom_tool_call() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{"type": "custom", "name": "apply_patch"}],
            "input": "Patch it."
        });
        let context = build_codex_tool_context_from_request(&request);
        let chat = json!({
            "id": "chatcmpl_custom",
            "object": "chat.completion",
            "created": 123,
            "model": "gpt-5.4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_patch",
                        "type": "function",
                        "function": {
                            "name": "apply_patch",
                            "arguments": "{\"input\":\"*** Begin Patch\\n*** End Patch\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let result = chat_completion_to_response_with_context(chat, &context).unwrap();

        assert_eq!(result["output"][0]["type"], "custom_tool_call");
        assert_eq!(result["output"][0]["id"], "ctc_call_patch");
        assert_eq!(result["output"][0]["call_id"], "call_patch");
        assert_eq!(result["output"][0]["name"], "apply_patch");
        assert_eq!(
            result["output"][0]["input"],
            "*** Begin Patch\n*** End Patch"
        );
    }

    /// #4341 (non-streaming path): when nothing remains after dropping, report the failure honestly
    /// instead of returning an empty turn that Codex treats as a normal completion.
    #[test]
    fn chat_response_with_only_unnamed_tool_call_is_an_error() {
        let chat = json!({
            "id": "chatcmpl_drop",
            "object": "chat.completion",
            "created": 123,
            "model": "kimi-k3",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Let me keep working on this file",
                    "tool_calls": [{
                        "id": "call_bad",
                        "type": "function",
                        "function": {"arguments": "{}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let err = chat_completion_to_response_with_context(chat, &CodexToolContext::default())
            .unwrap_err();
        assert!(matches!(err, ProxyError::TransformError(_)));
        assert!(err.to_string().contains("without a function name"));
    }

    /// As long as one valid tool call remains Codex would have continued, so behaviour is unchanged.
    #[test]
    fn chat_response_keeps_valid_tool_call_beside_unnamed_one() {
        let chat = json!({
            "id": "chatcmpl_mixed",
            "object": "chat.completion",
            "created": 123,
            "model": "kimi-k3",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [
                        {"id": "call_bad", "type": "function", "function": {"arguments": "{}"}},
                        {
                            "id": "call_good",
                            "type": "function",
                            "function": {"name": "exec_command", "arguments": "{\"cmd\":\"ls\"}"}
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let result =
            chat_completion_to_response_with_context(chat, &CodexToolContext::default()).unwrap();
        let output = result["output"].as_array().unwrap();

        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["name"], "exec_command");
        assert_eq!(output[0]["call_id"], "call_good");
        assert_eq!(result["status"], "completed");
    }

    /// The legacy `function_call` shape is covered by the same check.
    #[test]
    fn chat_response_with_unnamed_legacy_function_call_is_an_error() {
        let chat = json!({
            "id": "chatcmpl_legacy",
            "object": "chat.completion",
            "created": 123,
            "model": "kimi-k3",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "function_call": {"id": "call_legacy", "arguments": "{}"}
                },
                "finish_reason": "function_call"
            }]
        });

        let err = chat_completion_to_response_with_context(chat, &CodexToolContext::default())
            .unwrap_err();
        assert!(matches!(err, ProxyError::TransformError(_)));
    }

    /// `finish_reason=length` is truncation, not malformed upstream data, so the attribution must stay
    /// incomplete rather than tool_call_dropped.
    #[test]
    fn chat_response_truncated_stays_incomplete_instead_of_error() {
        let chat = json!({
            "id": "chatcmpl_trunc",
            "object": "chat.completion",
            "created": 123,
            "model": "kimi-k3",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Let me take a look",
                    "tool_calls": [{
                        "id": "call_cut",
                        "type": "function",
                        "function": {"arguments": "{\"pa"}
                    }]
                },
                "finish_reason": "length"
            }]
        });

        let result =
            chat_completion_to_response_with_context(chat, &CodexToolContext::default()).unwrap();
        assert_eq!(result["status"], "incomplete");
        assert_eq!(result["incomplete_details"]["reason"], "max_output_tokens");
    }

    /// A whitespace-only function name must be treated like an empty one, or it masquerades as "this turn still has a tool call".
    #[test]
    fn chat_response_whitespace_only_tool_name_is_an_error() {
        let chat = json!({
            "id": "chatcmpl_ws",
            "object": "chat.completion",
            "created": 123,
            "model": "kimi-k3",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_ws",
                        "type": "function",
                        "function": {"name": "   ", "arguments": "{}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let err = chat_completion_to_response_with_context(chat, &CodexToolContext::default())
            .unwrap_err();
        assert!(matches!(err, ProxyError::TransformError(_)));
    }

    /// A pure text turn (no tool call ever appeared) is unaffected by the check.
    #[test]
    fn chat_response_text_only_still_completes() {
        let chat = json!({
            "id": "chatcmpl_text",
            "object": "chat.completion",
            "created": 123,
            "model": "kimi-k3",
            "choices": [{
                "message": {"role": "assistant", "content": "Done"},
                "finish_reason": "stop"
            }]
        });

        let result =
            chat_completion_to_response_with_context(chat, &CodexToolContext::default()).unwrap();
        assert_eq!(result["status"], "completed");
    }

    #[test]
    fn chat_error_to_response_error_normalizes_standard_openai_shape() {
        let input = json!({
            "error": {
                "message": "Invalid API key",
                "type": "invalid_request_error",
                "code": "invalid_api_key",
                "param": "api_key"
            }
        });

        let result = chat_error_to_response_error(Some(&input));

        assert_eq!(result["error"]["message"], "Invalid API key");
        assert_eq!(result["error"]["type"], "invalid_request_error");
        assert_eq!(result["error"]["code"], "invalid_api_key");
        assert_eq!(result["error"]["param"], "api_key");
    }

    #[test]
    fn chat_error_to_response_error_normalizes_minimax_base_resp() {
        // MiniMax stuffs the error into base_resp, with a numeric rather than string code
        let input = json!({
            "base_resp": {
                "status_code": 2013,
                "status_msg": "invalid params, chat content has invalid message role: system"
            }
        });

        let result = chat_error_to_response_error(Some(&input));

        assert_eq!(
            result["error"]["message"],
            "invalid params, chat content has invalid message role: system"
        );
        assert_eq!(result["error"]["code"], 2013);
        // No explicit type, so it must fall back to upstream_error
        assert_eq!(result["error"]["type"], "upstream_error");
    }

    #[test]
    fn chat_error_to_response_error_handles_plain_text_body() {
        let input = json!("Upstream timeout");

        let result = chat_error_to_response_error(Some(&input));

        assert_eq!(result["error"]["message"], "Upstream timeout");
        assert_eq!(result["error"]["type"], "upstream_error");
        assert!(result["error"]["code"].is_null());
        assert!(result["error"]["param"].is_null());
    }

    #[test]
    fn chat_error_to_response_error_handles_missing_body() {
        let result = chat_error_to_response_error(None);

        assert_eq!(
            result["error"]["message"],
            "Upstream returned an empty error response"
        );
        assert_eq!(result["error"]["type"], "upstream_error");
    }

    #[test]
    fn chat_error_to_response_error_falls_back_to_detail_field() {
        // Some relays stuff the error into a top-level detail field (common in OpenAI compatibility layers)
        let input = json!({
            "detail": "rate limit exceeded"
        });

        let result = chat_error_to_response_error(Some(&input));

        assert_eq!(result["error"]["message"], "rate limit exceeded");
        assert_eq!(result["error"]["type"], "upstream_error");
    }
    // Regression tests for tool_choice without tools guard
    // https://github.com/farion1231/cc-switch/issues/3557
}
