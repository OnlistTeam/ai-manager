//! Copilot request optimizer
//!
//! Addresses abnormal GitHub Copilot quota consumption when proxying (issue #1813).
//!
//! Copilot uses the `x-initiator` header to tell a user-initiated turn from an agent continuation:
//! - `user`: counted as one premium interaction (quota is charged)
//! - `agent`: treated as a continuation of the previous interaction (no extra charge)
//!
//! Reference implementation: https://github.com/caozhiyuan/copilot-api

use std::collections::HashSet;

use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Request classification result
#[derive(Debug, Clone)]
pub struct CopilotClassification {
    /// "user" or "agent", mapped onto the x-initiator header
    pub initiator: &'static str,
    /// Whether this is a warmup/probe request (can be downgraded to a small model)
    pub is_warmup: bool,
    /// Whether this is a context compaction request
    pub is_compact: bool,
    /// Whether this is a Claude Code subagent request (spawned by the Agent tool)
    /// Subagent requests set x-interaction-type=conversation-subagent and are not counted as premium interactions
    pub is_subagent: bool,
}

/// Classifies an Anthropic-format request body to decide the Copilot headers.
///
/// Algorithm (only the last message is inspected, matching the caozhiyuan/copilot-api reference):
/// 1. No messages -> "user" (safe default for a first request)
/// 2. Last message has role=user:
///    - content holds a block that is not tool_result -> "user"
///    - content is entirely tool_result -> "agent"
///    - it matches the compact pattern -> "agent"
/// 3. Last message role is not user -> "user" (safe default)
///
/// Warmup detection (matching the reference implementation):
/// - an `anthropic-beta` header, no tools, and not compact -> warmup
///
/// `compact_detection`: whether compact detection runs. When false it is skipped, so the
/// `CopilotOptimizerConfig.compact_detection` switch really takes effect.
///
/// `subagent_detection`: whether subagent detection runs. When true it scans the first user
/// message for the `__SUBAGENT_MARKER__` tag and marks subagent requests as non-billable.
pub fn classify_request(
    body: &Value,
    has_anthropic_beta: bool,
    compact_detection: bool,
    subagent_detection: bool,
) -> CopilotClassification {
    let is_compact = compact_detection && is_compact_request(body);
    let is_subagent = subagent_detection && detect_subagent(body);

    let messages = match body.get("messages").and_then(|m| m.as_array()) {
        Some(msgs) if !msgs.is_empty() => msgs,
        _ => {
            return CopilotClassification {
                initiator: "user",
                is_warmup: is_warmup_request(body, has_anthropic_beta, false),
                is_compact: false,
                is_subagent,
            }
        }
    };

    let last_msg = &messages[messages.len() - 1];
    let role = last_msg.get("role").and_then(|r| r.as_str()).unwrap_or("");

    // Only role=user messages need further classification
    if role != "user" {
        return CopilotClassification {
            initiator: if is_subagent { "agent" } else { "user" },
            is_warmup: false,
            is_compact,
            is_subagent,
        };
    }

    // Decision logic (equivalent to copilot-api's merge-then-classify):
    // any tool_result in the content array means a tool continuation, hence agent.
    // This covers common cases such as skills, edit hooks, and plan follow-ups,
    // whose content is usually a mixed [tool_result, text] shape.
    // copilot-api gets the same result by merging first (text absorbed into tool_result) then
    // classifying; handling it in the classifier is sturdier and does not depend on whether merge is enabled or on ordering.
    let is_user_initiated = match last_msg.get("content") {
        Some(Value::Array(blocks)) => {
            // With a tool_result it is a tool continuation (agent), otherwise user-initiated (user)
            !blocks
                .iter()
                .any(|block| block.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
        }
        Some(Value::String(_)) => true,
        _ => false,
    };

    // Subagent requests are always marked agent (even when the first message holds user text)
    let initiator = if is_subagent || !is_user_initiated || is_compact {
        "agent"
    } else {
        "user"
    };

    CopilotClassification {
        initiator,
        is_warmup: initiator == "user" && is_warmup_request(body, has_anthropic_beta, is_compact),
        is_compact,
        is_subagent,
    }
}

/// Detects a warmup/probe request (a good candidate for downgrading to a small model).
///
/// Matching the reference implementation, all three must hold:
/// 1. an `anthropic-beta` header (the hallmark of a Claude Code warmup probe)
/// 2. no tools are defined
/// 3. it is not a compact request
fn is_warmup_request(body: &Value, has_anthropic_beta: bool, is_compact: bool) -> bool {
    if !has_anthropic_beta || is_compact {
        return false;
    }
    // No tools defined
    body.get("tools")
        .and_then(|tools| tools.as_array())
        .is_none_or(|tools| tools.is_empty())
}

/// Detects a Claude Code context compaction (compact) request.
///
/// Only machine-generated signals **produced internally** by Claude Code are matched, never generic
/// phrases a user might type, so real user requests are not mislabeled as agent.
///
/// Strong signals:
/// 1. system prompt: Claude Code compact mode sets a dedicated one that users cannot set by hand
/// 2. "CRITICAL: Respond with TEXT ONLY. Do NOT call any tools." - a machine instruction
/// 3. both "Pending Tasks:" and "Current Work:" - the structural markers of Claude Code compact
fn is_compact_request(body: &Value) -> bool {
    // Signal 1: the system prompt starts with the Claude Code compact prefix
    // Users cannot control the system prompt inside Claude Code, making this the most reliable signal
    let system_text = extract_system_text(body);
    if system_text
        .starts_with("You are a helpful AI assistant tasked with summarizing conversations")
    {
        return true;
    }

    // Signals 2 and 3: look for machine-generated markers in the last user message
    let messages = match body.get("messages").and_then(|m| m.as_array()) {
        Some(msgs) => msgs,
        None => return false,
    };

    if let Some(last_msg) = messages.last() {
        if last_msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            return false;
        }

        let text = extract_text_from_message(last_msg);

        // Signal 2: the Claude Code compact machine instruction (case-sensitive exact match)
        if text.contains("CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.") {
            return true;
        }

        // Signal 3: the Claude Code compact structural markers (both must be present)
        if text.contains("Pending Tasks:") && text.contains("Current Work:") {
            return true;
        }
    }

    false
}

/// Merges tool_result and text blocks inside user messages.
///
/// Matching the reference `mergeToolResultForClaude`:
///
/// **Within a message** (the core): inside one user message, text blocks are absorbed into
/// tool_result blocks so only tool_result blocks remain, and Copilot does not see a user-initiated interaction.
///
/// Context: for skill calls, edit hooks, plan reminders, and similar, Claude Code sends user
/// messages mixing tool_result and text, and the text block makes Copilot bill it as a premium request.
///
/// **Across messages** (supplementary): consecutive tool_result-only user messages merge into one.
pub fn merge_tool_results(mut body: Value) -> Value {
    let messages = match body.get_mut("messages").and_then(|m| m.as_array_mut()) {
        Some(msgs) if !msgs.is_empty() => msgs,
        _ => return body,
    };

    // Phase 1: merge within a message - absorb text blocks into tool_result blocks
    for msg in messages.iter_mut() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        let content = match msg.get("content").and_then(|c| c.as_array()) {
            Some(blocks) => blocks,
            None => continue,
        };

        // Separate tool_result and text blocks
        let mut tool_results: Vec<Value> = Vec::new();
        let mut text_blocks: Vec<Value> = Vec::new();
        let mut valid = true;

        for block in content {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("tool_result") => tool_results.push(block.clone()),
                Some("text") => text_blocks.push(block.clone()),
                _ => {
                    // Some other block type is present -> skip this message
                    valid = false;
                    break;
                }
            }
        }

        // A merge is only needed when both tool_result and text are present
        if !valid || tool_results.is_empty() || text_blocks.is_empty() {
            continue;
        }

        // Merge strategy (matching the reference implementation)
        let merged = merge_blocks_into_tool_results(tool_results, text_blocks);
        msg["content"] = Value::Array(merged);
    }

    // Phase 2: merge across messages - consecutive tool_result-only user messages
    let messages = match body.get("messages").and_then(|m| m.as_array()) {
        Some(messages) => messages.clone(),
        None => return body,
    };
    if messages.len() <= 1 {
        return body;
    }

    let mut merged_msgs: Vec<Value> = Vec::with_capacity(messages.len());
    let mut i = 0;

    while i < messages.len() {
        if is_tool_result_only_message(&messages[i]) {
            let mut combined_content: Vec<Value> = Vec::new();
            while i < messages.len() && is_tool_result_only_message(&messages[i]) {
                if let Some(content) = messages[i].get("content").and_then(|c| c.as_array()) {
                    combined_content.extend(content.iter().cloned());
                }
                i += 1;
            }
            if !combined_content.is_empty() {
                merged_msgs.push(serde_json::json!({
                    "role": "user",
                    "content": combined_content
                }));
            }
        } else {
            merged_msgs.push(messages[i].clone());
            i += 1;
        }
    }

    body["messages"] = Value::Array(merged_msgs);
    body
}

/// Builds a deterministic request ID from the content of the last user message.
///
/// An extra CC Switch policy (the copilot-api reference uses a random UUID):
/// - hash input: sessionId + lastUserContent (excluding tool_result and cache_control)
/// - identical content yields the same ID, which may help Copilot deduplicate
/// - falls back to a random UUID when no user content is found
/// - uses the UUID v4 format
pub fn deterministic_request_id(body: &Value, session_id: &str) -> String {
    let last_user_content = find_last_user_content(body);

    match last_user_content {
        Some(content) => {
            let mut hasher = Sha256::new();
            hasher.update(session_id.as_bytes());
            hasher.update(content.as_bytes());
            let result = hasher.finalize();

            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(&result[..16]);
            // UUID v4 version and variant bits (matching the reference implementation)
            bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
            bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant 1

            Uuid::from_bytes(bytes).to_string()
        }
        None => Uuid::new_v4().to_string(),
    }
}

/// Builds a stable interaction ID from the session ID.
///
/// Matching the reference (copilot-api session.ts):
/// - every request of one main conversation shares an interaction ID
/// - hash input: the session ID only (no message content, unlike the request ID)
/// - Copilot groups requests into one "interaction" by this ID, which drives premium billing
/// - an empty session ID returns None (never inject a random value, which fragments interactions)
pub fn deterministic_interaction_id(session_id: &str) -> Option<String> {
    if session_id.is_empty() {
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(b"interaction:");
    hasher.update(session_id.as_bytes());
    let result = hasher.finalize();

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&result[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant 1

    Some(Uuid::from_bytes(bytes).to_string())
}

/// Detects whether the request comes from a Claude Code subagent (spawned by the Agent tool).
///
/// The Claude Code Agent tool injects a `__SUBAGENT_MARKER__` JSON tag into the
/// `<system-reminder>` element of the subagent's first user message, shaped like:
/// ```json
/// {"__SUBAGENT_MARKER__": {"session_id": "...", "agent_id": "...", "agent_type": "..."}}
/// ```
///
/// Scan strategy (matching copilot-api's subagent-marker.ts):
/// 1. walk every user message (not just the first, since compaction may reorder them)
/// 2. look for the `__SUBAGENT_MARKER__` keyword in the message text
/// 3. a hit means this is a subagent request
fn detect_subagent(body: &Value) -> bool {
    // Signal 1: an explicit __SUBAGENT_MARKER__ (auto-injected by Claude Code 2.x and later)
    if extract_system_text(body).contains("__SUBAGENT_MARKER__") {
        return true;
    }

    if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
                continue;
            }
            let text = extract_text_from_message(msg);
            if text.contains("__SUBAGENT_MARKER__") {
                return true;
            }
        }
    }

    // Signal 2 (fallback): metadata.user_id carries a subagent marker.
    // The Claude Code Agent tool labels a subagent session as
    // "parentSessionId_agent_agentId", so the "_agent_" infix is what we look for
    if let Some(user_id) = body.pointer("/metadata/user_id").and_then(|v| v.as_str()) {
        // "_agent_" is the internal marker of the Claude Code Agent tool
        if user_id.contains("_agent_") {
            return true;
        }
    }

    // Signal 3 (fallback): the system prompt contains the typical Claude Code subagent framing.
    // A subagent spawned by the Agent tool carries the task description the tool injected, whereas
    // the main conversation's system prompt comes straight from the Claude Code CLI in a different shape.
    // This signal is not reliable enough (a user prompt may contain the same words), so it is only auxiliary
    // and is not enabled yet; the hook is reserved

    false
}

/// Sanitizes orphan tool_results: a tool_result with no matching tool_use becomes a text block.
///
/// Context: compaction or truncation can delete the tool_use from an assistant message while the
/// tool_result stays in the following user message, and upstream APIs may error or retry on the mismatch.
///
/// Matches copilot-api's `sanitizeOrphanToolResults`.
pub fn sanitize_orphan_tool_results(mut body: Value) -> Value {
    let messages = match body.get_mut("messages").and_then(|m| m.as_array_mut()) {
        Some(msgs) if msgs.len() >= 2 => msgs,
        _ => return body,
    };

    // The Anthropic protocol requires a tool_result to directly follow the assistant turn holding its
    // tool_use, so only messages[i-1] (the immediately preceding assistant) decides orphan status,
    // matching the reference sanitizeOrphanToolResults.
    for i in 1..messages.len() {
        if messages[i].get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }

        // Collect the tool_use ids of the immediately preceding assistant
        let prev_tool_use_ids: HashSet<String> =
            if messages[i - 1].get("role").and_then(|r| r.as_str()) == Some("assistant") {
                messages[i - 1]
                    .get("content")
                    .and_then(|c| c.as_array())
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
                            .filter_map(|b| b.get("id").and_then(|i| i.as_str()).map(String::from))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                // The previous message is not an assistant -> every tool_result here is an orphan
                HashSet::new()
            };

        let content = match messages[i]
            .get_mut("content")
            .and_then(|c| c.as_array_mut())
        {
            Some(blocks) => blocks,
            None => continue,
        };

        for block in content.iter_mut() {
            if block.get("type").and_then(|t| t.as_str()) != Some("tool_result") {
                continue;
            }
            let tool_use_id = block
                .get("tool_use_id")
                .and_then(|id| id.as_str())
                .unwrap_or("");
            // An empty tool_use_id, or one absent from the adjacent assistant's tool_use set -> orphan
            if tool_use_id.is_empty() || !prev_tool_use_ids.contains(tool_use_id) {
                let content_text = match block.get("content") {
                    Some(Value::String(text)) => text.clone(),
                    Some(Value::Array(blocks)) => blocks
                        .iter()
                        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                *block = serde_json::json!({
                    "type": "text",
                    "text": format!("[Tool result for {}]: {}", tool_use_id, content_text)
                });
            }
        }
    }

    body
}

/// Proactively strips every thinking / redacted_thinking block from assistant messages before the request
///
/// All three Copilot target endpoints (`/chat/completions`, `/v1/responses`, `/v1/chat/completions`)
/// are OpenAI-compatible and do not understand Anthropic thinking blocks. Forwarding them as is makes
/// upstream reject with invalid_request_error, after which `thinking_rectifier` cleans up reactively
/// and retries. That failed request still burns a premium quota, hence stripping it up front.
///
/// Differences from `thinking_rectifier::rectify_anthropic_request`:
/// - this function only strips thinking / redacted_thinking blocks, never touching signature and
///   never removing the top-level thinking field, which is aggressive error-path rectification.
/// - it keeps the same consume-body-return-new-body signature as `merge_tool_results` and
///   `sanitize_orphan_tool_results`, so it drops into the forwarder pipeline.
pub fn strip_thinking_blocks(mut body: Value) -> Value {
    let Some(messages) = body.get_mut("messages").and_then(|m| m.as_array_mut()) else {
        return body;
    };

    for msg in messages.iter_mut() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let Some(content) = msg.get_mut("content").and_then(|c| c.as_array_mut()) else {
            continue;
        };
        content.retain(|block| {
            !matches!(
                block.get("type").and_then(|t| t.as_str()),
                Some("thinking") | Some("redacted_thinking")
            )
        });
    }

    body
}

// --- Internal helpers -------------------------

/// Extracts text from the request body's `system` field (handling both string and array forms).
fn extract_system_text(body: &Value) -> String {
    match body.get("system") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

/// Finds the non-tool_result text content of the last user message.
///
/// Matching the reference `findLastUserContent`:
/// - walk the messages backwards
/// - exclude tool_result blocks
/// - exclude the cache_control field
fn find_last_user_content(body: &Value) -> Option<String> {
    let messages = body.get("messages").and_then(|m| m.as_array())?;

    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        let content = msg.get("content")?;

        if let Some(s) = content.as_str() {
            return Some(s.to_string());
        }

        if let Some(blocks) = content.as_array() {
            // Filter out tool_result and keep the rest (dropping cache_control)
            let filtered: Vec<Value> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) != Some("tool_result"))
                .map(|b| {
                    let mut b = b.clone();
                    if let Some(obj) = b.as_object_mut() {
                        obj.remove("cache_control");
                    }
                    b
                })
                .collect();

            if !filtered.is_empty() {
                return Some(serde_json::to_string(&filtered).unwrap_or_default());
            }
        }
    }

    None
}

/// Merges text blocks into tool_result blocks.
///
/// Two strategies (matching the reference implementation):
/// - equal counts: pair them up and append each text to the matching tool_result's content
/// - unequal counts: append every text to the last tool_result's content
fn merge_blocks_into_tool_results(
    mut tool_results: Vec<Value>,
    text_blocks: Vec<Value>,
) -> Vec<Value> {
    if tool_results.len() == text_blocks.len() {
        // Pairwise merge
        for (tr, tb) in tool_results.iter_mut().zip(text_blocks.iter()) {
            append_text_to_tool_result(tr, tb);
        }
    } else {
        // Append every text to the last tool_result
        if let Some(last_tr) = tool_results.last_mut() {
            for tb in &text_blocks {
                append_text_to_tool_result(last_tr, tb);
            }
        }
    }
    tool_results
}

/// Appends the content of a text block to a tool_result's content
fn append_text_to_tool_result(tool_result: &mut Value, text_block: &Value) {
    let text = text_block
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("");
    if text.trim().is_empty() {
        return;
    }

    // A tool_result's content may be a string or an array
    match tool_result.get_mut("content") {
        Some(Value::String(existing)) => {
            existing.push('\n');
            existing.push_str(text);
        }
        Some(Value::Array(arr)) => {
            arr.push(serde_json::json!({"type": "text", "text": text}));
        }
        _ => {
            // content is missing or null - just set it
            tool_result["content"] = Value::String(text.to_string());
        }
    }
}

/// Extracts the text content of a message
fn extract_text_from_message(msg: &Value) -> String {
    match msg.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    block.get("text").and_then(|t| t.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

/// Checks whether a message is a tool_result-only user message
fn is_tool_result_only_message(msg: &Value) -> bool {
    if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
        return false;
    }
    match msg.get("content").and_then(|c| c.as_array()) {
        Some(blocks) if !blocks.is_empty() => blocks
            .iter()
            .all(|block| block.get("type").and_then(|t| t.as_str()) == Some("tool_result")),
        _ => false,
    }
}

// --- Tests ------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // === classify_request tests ===

    #[test]
    fn test_classify_user_text_message() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "user", "content": "Hello, please help me write some code"}
            ]
        });
        let result = classify_request(&body, false, true, false);
        assert_eq!(result.initiator, "user");
        assert!(!result.is_compact);
    }

    #[test]
    fn test_classify_user_text_array_message() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "Please explain this code"}
                ]}
            ]
        });
        let result = classify_request(&body, false, true, false);
        assert_eq!(result.initiator, "user");
    }

    #[test]
    fn test_classify_tool_result_only() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "tools": [{"name": "Read", "description": "Read a file", "input_schema": {}}],
            "messages": [
                {"role": "user", "content": "Read the file"},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "I'll read that file."},
                    {"type": "tool_use", "id": "toolu_123", "name": "Read", "input": {"path": "/tmp/test.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_123", "content": "file contents here"}
                ]}
            ]
        });
        let result = classify_request(&body, true, true, false);
        assert_eq!(result.initiator, "agent");
        assert!(!result.is_warmup);
    }

    #[test]
    fn test_classify_tool_result_with_text_block() {
        // tool_result + text block (the usual shape for skills, edit hooks, plan follow-ups)
        // a tool_result is present -> tool continuation -> agent
        // equivalent to copilot-api's merge-then-classify
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_123", "content": "file contents"},
                    {"type": "text", "text": "Now please refactor this code"}
                ]}
            ]
        });
        let result = classify_request(&body, false, true, false);
        assert_eq!(result.initiator, "agent");
    }

    #[test]
    fn test_classify_empty_messages() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": []
        });
        let result = classify_request(&body, false, true, false);
        assert_eq!(result.initiator, "user");
    }

    #[test]
    fn test_classify_no_messages() {
        let body = json!({"model": "claude-sonnet-4-20250514"});
        let result = classify_request(&body, false, true, false);
        assert_eq!(result.initiator, "user");
    }

    #[test]
    fn test_classify_compact_request_system_prompt() {
        // compact detected via the strong system prompt signal
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "system": "You are a helpful AI assistant tasked with summarizing conversations. Please create a summary.",
            "messages": [
                {"role": "user", "content": "Here is the conversation history to summarize..."}
            ]
        });
        let result = classify_request(&body, false, true, false);
        assert_eq!(result.initiator, "agent");
        assert!(result.is_compact);
    }

    #[test]
    fn test_classify_compact_request_critical_marker() {
        // compact detected via the CRITICAL machine instruction
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "CRITICAL: Respond with TEXT ONLY. Do NOT call any tools. Summarize the conversation."}
                ]}
            ]
        });
        let result = classify_request(&body, false, true, false);
        assert_eq!(result.initiator, "agent");
        assert!(result.is_compact);
    }

    #[test]
    fn test_classify_compact_disabled_by_config() {
        // With compact_detection=false, matching content must not be marked compact
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "system": "You are a helpful AI assistant tasked with summarizing conversations.",
            "messages": [
                {"role": "user", "content": "Summarize"}
            ]
        });
        let result = classify_request(&body, false, false, false); // compact_detection=false
        assert_eq!(result.initiator, "user"); // must not be marked agent
        assert!(!result.is_compact);
    }

    #[test]
    fn test_no_false_positive_on_user_summarize_request() {
        // P1 fix check: a user typing "summarize the conversation" must not be misread as compact
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "user", "content": "Please summarize the conversation so far into a concise summary."}
            ]
        });
        let result = classify_request(&body, false, true, false);
        // No strong system prompt signal and no CRITICAL instruction -> not compact -> user
        assert_eq!(result.initiator, "user");
        assert!(!result.is_compact);
    }

    // === warmup tests (matching the reference implementation) ===

    #[test]
    fn test_warmup_with_anthropic_beta_no_tools() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        });
        // has_anthropic_beta=true and no tools -> warmup
        let result = classify_request(&body, true, true, false);
        assert!(result.is_warmup);
    }

    #[test]
    fn test_not_warmup_without_anthropic_beta() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        });
        // has_anthropic_beta=false -> not warmup
        let result = classify_request(&body, false, true, false);
        assert!(!result.is_warmup);
    }

    #[test]
    fn test_not_warmup_with_tools() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "tools": [{"name": "Read", "description": "Read a file", "input_schema": {}}],
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        });
        // tools present -> not warmup (even with anthropic-beta)
        let result = classify_request(&body, true, true, false);
        assert!(!result.is_warmup);
    }

    #[test]
    fn test_not_warmup_when_agent() {
        // tool_result -> agent -> never warmup
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_123", "content": "ok"}
                ]}
            ]
        });
        let result = classify_request(&body, true, true, false);
        assert_eq!(result.initiator, "agent");
        assert!(!result.is_warmup);
    }

    // === merge_tool_results tests ===

    #[test]
    fn test_merge_intra_message_tool_result_text() {
        // Core case: tool_result + text within a message -> text absorbed into tool_result
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "file contents"},
                    {"type": "text", "text": "skill output here"}
                ]}
            ]
        });
        let result = merge_tool_results(body);
        let content = result["messages"][0]["content"].as_array().unwrap();
        // Only 1 tool_result block must remain (the text was absorbed)
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "tool_result");
        // The tool_result content must hold the original plus the absorbed text
        let tr_content = content[0]["content"].as_str().unwrap();
        assert!(tr_content.contains("file contents"));
        assert!(tr_content.contains("skill output here"));
    }

    #[test]
    fn test_merge_intra_message_equal_count() {
        // Equal counts: pairwise merge
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "result1"},
                    {"type": "text", "text": "text1"},
                    {"type": "tool_result", "tool_use_id": "t2", "content": "result2"},
                    {"type": "text", "text": "text2"}
                ]}
            ]
        });
        let result = merge_tool_results(body);
        let content = result["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert!(content[0]["content"].as_str().unwrap().contains("text1"));
        assert!(content[1]["content"].as_str().unwrap().contains("text2"));
    }

    #[test]
    fn test_merge_intra_message_empty_text_ignored() {
        // An empty text block appends nothing
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "result"},
                    {"type": "text", "text": ""}
                ]}
            ]
        });
        let result = merge_tool_results(body);
        let content = result["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        // An empty text leaves the original content unchanged
        assert_eq!(content[0]["content"], "result");
    }

    #[test]
    fn test_merge_intra_skips_other_block_types() {
        // A block that is neither tool_result nor text -> skip the whole message
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "result"},
                    {"type": "image", "source": {"data": "..."}},
                    {"type": "text", "text": "caption"}
                ]}
            ]
        });
        let result = merge_tool_results(body);
        let content = result["messages"][0]["content"].as_array().unwrap();
        // Not merged, still 3 blocks
        assert_eq!(content.len(), 3);
    }

    #[test]
    fn test_merge_cross_message_consecutive() {
        // Cross-message merge: consecutive tool_result-only user messages
        let body = json!({
            "messages": [
                {"role": "user", "content": "Read files"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {}},
                    {"type": "tool_use", "id": "t2", "name": "Read", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "file1"}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t2", "content": "file2"}
                ]}
            ]
        });
        let result = merge_tool_results(body);
        let messages = result["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        let merged_content = messages[2]["content"].as_array().unwrap();
        assert_eq!(merged_content.len(), 2);
    }

    #[test]
    fn test_merge_does_not_affect_normal_messages() {
        let body = json!({
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi!"},
                {"role": "user", "content": "How are you?"}
            ]
        });
        let result = merge_tool_results(body.clone());
        assert_eq!(result["messages"], body["messages"]);
    }

    // === deterministic_request_id tests ===

    #[test]
    fn test_deterministic_id_stable() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let id1 = deterministic_request_id(&body, "session1");
        let id2 = deterministic_request_id(&body, "session1");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_deterministic_id_varies_by_content() {
        let body1 = json!({
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let body2 = json!({
            "messages": [{"role": "user", "content": "Goodbye"}]
        });
        let id1 = deterministic_request_id(&body1, "session1");
        let id2 = deterministic_request_id(&body2, "session1");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_deterministic_id_varies_by_session() {
        let body = json!({
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let id1 = deterministic_request_id(&body, "session1");
        let id2 = deterministic_request_id(&body, "session2");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_deterministic_id_ignores_tool_result() {
        // Different tool_result content but identical user text -> identical ID
        let body1 = json!({
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi"},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "version_A"}
                ]},
                {"role": "user", "content": "do something"}
            ]
        });
        let body2 = json!({
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi"},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "version_B"}
                ]},
                {"role": "user", "content": "do something"}
            ]
        });
        let id1 = deterministic_request_id(&body1, "s");
        let id2 = deterministic_request_id(&body2, "s");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_deterministic_id_fallback_when_no_user_content() {
        // No user message -> falls back to a random UUID (different every time)
        let body = json!({
            "messages": [
                {"role": "assistant", "content": "Hi"}
            ]
        });
        let id1 = deterministic_request_id(&body, "s");
        let id2 = deterministic_request_id(&body, "s");
        // A random UUID must differ each time
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_deterministic_id_is_valid_uuid() {
        let body = json!({
            "messages": [{"role": "user", "content": "test"}]
        });
        let id = deterministic_request_id(&body, "session");
        assert!(Uuid::parse_str(&id).is_ok());
    }

    // === deterministic_interaction_id tests ===

    #[test]
    fn test_interaction_id_stable_for_same_session() {
        let id1 = deterministic_interaction_id("session_abc");
        let id2 = deterministic_interaction_id("session_abc");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_interaction_id_differs_across_sessions() {
        let id1 = deterministic_interaction_id("session_abc");
        let id2 = deterministic_interaction_id("session_def");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_interaction_id_differs_from_request_id() {
        let body = json!({
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let interaction = deterministic_interaction_id("session_abc").unwrap();
        let request = deterministic_request_id(&body, "session_abc");
        assert_ne!(interaction, request);
    }

    #[test]
    fn test_interaction_id_empty_session_is_none() {
        // Without a session no interaction ID must be produced (to avoid fragmentation)
        assert!(deterministic_interaction_id("").is_none());
    }

    #[test]
    fn test_interaction_id_is_valid_uuid() {
        let id = deterministic_interaction_id("test_session").unwrap();
        assert!(Uuid::parse_str(&id).is_ok());
    }

    // === enhanced compact detection tests ===

    #[test]
    fn test_compact_detection_system_prompt() {
        let body = json!({
            "system": "You are a helpful AI assistant tasked with summarizing conversations. Please provide a concise summary.",
            "messages": [
                {"role": "user", "content": "Here is the conversation to summarize..."}
            ]
        });
        assert!(is_compact_request(&body));
    }

    #[test]
    fn test_compact_detection_critical_keyword() {
        let body = json!({
            "messages": [
                {"role": "user", "content": "CRITICAL: Respond with TEXT ONLY. Do NOT call any tools. Summarize this conversation."}
            ]
        });
        assert!(is_compact_request(&body));
    }

    #[test]
    fn test_compact_detection_structural_markers() {
        // The structural markers unique to Claude Code compact
        let body = json!({
            "messages": [
                {"role": "user", "content": "Summary of conversation:\n\nPending Tasks:\n- Fix bug\n\nCurrent Work:\n- Implementing feature"}
            ]
        });
        assert!(is_compact_request(&body));
    }

    #[test]
    fn test_compact_no_false_positive_on_generic_summary() {
        // Generic phrases must not trigger compact detection
        let body = json!({
            "messages": [
                {"role": "user", "content": "Your task is to create a detailed summary of the conversation so far."}
            ]
        });
        assert!(!is_compact_request(&body));
    }

    #[test]
    fn test_compact_detection_negative() {
        let body = json!({
            "messages": [
                {"role": "user", "content": "What is the weather today?"}
            ]
        });
        assert!(!is_compact_request(&body));
    }

    #[test]
    fn test_compact_detection_system_array() {
        let body = json!({
            "system": [
                {"type": "text", "text": "You are a helpful AI assistant tasked with summarizing conversations."}
            ],
            "messages": [
                {"role": "user", "content": "Summarize"}
            ]
        });
        assert!(is_compact_request(&body));
    }

    // === detect_subagent tests ===

    #[test]
    fn test_detect_subagent_with_marker_in_user_message() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "<system-reminder>\n{\"__SUBAGENT_MARKER__\":{\"session_id\":\"abc123\",\"agent_id\":\"explore-1\",\"agent_type\":\"Explore\"}}\n</system-reminder>\nPlease search the codebase for auth handlers"}
                ]}
            ]
        });
        assert!(detect_subagent(&body));
    }

    #[test]
    fn test_detect_subagent_with_marker_in_system() {
        let body = json!({
            "system": "You are an agent. {\"__SUBAGENT_MARKER__\":{\"session_id\":\"abc\",\"agent_id\":\"plan-1\",\"agent_type\":\"Plan\"}}",
            "messages": [
                {"role": "user", "content": "Design the implementation plan"}
            ]
        });
        assert!(detect_subagent(&body));
    }

    #[test]
    fn test_detect_subagent_no_marker() {
        let body = json!({
            "messages": [
                {"role": "user", "content": "Hello, please help me write code"}
            ]
        });
        assert!(!detect_subagent(&body));
    }

    #[test]
    fn test_detect_subagent_via_metadata_user_id() {
        // fallback signal: metadata.user_id carries the "_agent_" marker
        let body = json!({
            "metadata": {
                "user_id": "session_abc123_agent_explore-1"
            },
            "messages": [
                {"role": "user", "content": "Search for files"}
            ]
        });
        assert!(detect_subagent(&body));
    }

    #[test]
    fn test_detect_subagent_normal_user_id_not_matched() {
        // An ordinary session ID must not be misread
        let body = json!({
            "metadata": {
                "user_id": "session_abc123"
            },
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        });
        assert!(!detect_subagent(&body));
    }

    #[test]
    fn test_classify_subagent_sets_agent_initiator() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "<system-reminder>\n{\"__SUBAGENT_MARKER__\":{\"session_id\":\"abc\",\"agent_id\":\"explore-1\",\"agent_type\":\"Explore\"}}\n</system-reminder>\nSearch for files"}
                ]}
            ]
        });
        let result = classify_request(&body, false, true, true);
        assert_eq!(result.initiator, "agent");
        assert!(result.is_subagent);
    }

    #[test]
    fn test_classify_subagent_disabled_flag() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "<system-reminder>\n{\"__SUBAGENT_MARKER__\":{\"session_id\":\"abc\",\"agent_id\":\"explore-1\",\"agent_type\":\"Explore\"}}\n</system-reminder>\nSearch for files"}
                ]}
            ]
        });
        // subagent_detection=false -> no subagent detection
        let result = classify_request(&body, false, true, false);
        assert_eq!(result.initiator, "user");
        assert!(!result.is_subagent);
    }

    // === sanitize_orphan_tool_results tests ===

    #[test]
    fn test_sanitize_orphan_tool_results_converts_orphans() {
        let body = json!({
            "messages": [
                {"role": "user", "content": "Help me"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "tool_1", "name": "read_file", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "tool_1", "content": "file contents"},
                    {"type": "tool_result", "tool_use_id": "tool_orphan", "content": "orphan data"}
                ]}
            ]
        });
        let result = sanitize_orphan_tool_results(body);
        let msgs = result["messages"].as_array().unwrap();
        let last_content = msgs[2]["content"].as_array().unwrap();
        // tool_1 stays a tool_result
        assert_eq!(last_content[0]["type"], "tool_result");
        // tool_orphan becomes text
        assert_eq!(last_content[1]["type"], "text");
        assert!(last_content[1]["text"]
            .as_str()
            .unwrap()
            .contains("tool_orphan"));
    }

    #[test]
    fn test_sanitize_orphan_tool_results_no_orphans() {
        let body = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "tool_1", "name": "read_file", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "tool_1", "content": "ok"}
                ]}
            ]
        });
        let result = sanitize_orphan_tool_results(body.clone());
        // No orphan tool_result, so nothing changes
        assert_eq!(result["messages"][1]["content"][0]["type"], "tool_result");
    }

    #[test]
    fn test_sanitize_orphan_non_adjacent_assistant_tool_use_is_orphan() {
        // The tool_use lives in an earlier assistant while the message before the tool_result is a different assistant,
        // so under the Anthropic protocol this tool_result is an orphan
        let body = json!({
            "messages": [
                {"role": "user", "content": "step 1"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "old_tool", "name": "search", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "old_tool", "content": "found it"}
                ]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "OK, now let me think..."}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "old_tool", "content": "stale ref"}
                ]}
            ]
        });
        let result = sanitize_orphan_tool_results(body);
        let msgs = result["messages"].as_array().unwrap();
        // messages[2]: the adjacent assistant has old_tool -> kept
        assert_eq!(msgs[2]["content"][0]["type"], "tool_result");
        // messages[4]: the adjacent assistant has no tool_use -> orphan -> text
        assert_eq!(msgs[4]["content"][0]["type"], "text");
    }

    #[test]
    fn test_sanitize_orphan_prev_not_assistant() {
        // The message before the tool_result is a user rather than an assistant -> all orphans
        let body = json!({
            "messages": [
                {"role": "user", "content": "first"},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "data"}
                ]}
            ]
        });
        let result = sanitize_orphan_tool_results(body);
        assert_eq!(result["messages"][1]["content"][0]["type"], "text");
    }

    /// Key case: an orphan tool_result (compaction lost the adjacent tool_use) must still count as an
    /// agent continuation at classification time, and must not become a user request just because a
    /// later sanitize turns it into text.
    ///
    /// This test checks that classify_request correctly reads an orphan tool_result as agent on the
    /// original, un-sanitized body.
    #[test]
    fn test_orphan_tool_result_classified_as_agent_before_sanitize() {
        // Case: the last user message is all tool_result, but the adjacent assistant message has no
        // matching tool_use (lost to context compaction)
        let body = json!({
            "messages": [
                {"role": "assistant", "content": "I'll help you with that."},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "orphan_tool_1", "content": "file contents here"},
                    {"type": "tool_result", "tool_use_id": "orphan_tool_2", "content": "another result"}
                ]}
            ]
        });
        // Classifying the original body -> all tool_result -> agent
        let classification = classify_request(&body, false, false, false);
        assert_eq!(classification.initiator, "agent");

        // After sanitize -> tool_result becomes text -> reclassifying would yield user
        let sanitized = sanitize_orphan_tool_results(body);
        let classification_after = classify_request(&sanitized, false, false, false);
        assert_eq!(
            classification_after.initiator, "user",
            "after sanitize an orphan tool_result becomes text and classification flips to user, \
             which is exactly why classification must run before sanitize"
        );
    }

    /// Mixed orphan tool_result and text case:
    /// the classifier treats any message containing a tool_result as agent (text block or not) and
    /// does not depend on merge ordering. Even if the orphan tool_result is later turned into text by
    /// sanitize, the classification was already fixed as agent beforehand.
    #[test]
    fn test_orphan_tool_result_with_text_classified_as_agent() {
        let body = json!({
            "messages": [
                {"role": "assistant", "content": "Processing..."},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "orphan_1", "content": "result data"},
                    {"type": "text", "text": "Here's the output from the tool"}
                ]}
            ]
        });
        // A tool_result is present -> agent (text block or not)
        let classification = classify_request(&body, false, false, false);
        assert_eq!(classification.initiator, "agent");

        // After sanitize the orphan tool_result becomes text -> pure text -> classification would be user
        // but the correct order is classify then sanitize, so this is not a problem
        let sanitized = sanitize_orphan_tool_results(body);
        let classification_after = classify_request(&sanitized, false, false, false);
        assert_eq!(classification_after.initiator, "user");
    }

    #[test]
    fn test_sanitize_orphan_empty_tool_use_id_is_orphan() {
        // An empty or missing tool_use_id matches no tool_use -> orphan
        let body = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "tool_1", "name": "read", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "", "content": "empty id"},
                    {"type": "tool_result", "content": "missing id field"}
                ]}
            ]
        });
        let result = sanitize_orphan_tool_results(body);
        let content = result["messages"][1]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "text");
    }

    // === strip_thinking_blocks tests ===

    #[test]
    fn test_strip_thinking_removes_assistant_thinking_blocks() {
        let body = serde_json::json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "let me ponder", "signature": "sig"},
                    {"type": "redacted_thinking", "data": "opaque"},
                    {"type": "text", "text": "hello"},
                    {"type": "tool_use", "id": "t1", "name": "read", "input": {}}
                ]}
            ]
        });
        let result = strip_thinking_blocks(body);
        let content = result["messages"][1]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "tool_use");
    }

    #[test]
    fn test_strip_thinking_leaves_user_messages_untouched() {
        // Only assistant messages are processed; thinking blocks on user messages (rare but possible) stay
        let body = serde_json::json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "thinking", "thinking": "x"},
                    {"type": "text", "text": "hi"}
                ]}
            ]
        });
        let result = strip_thinking_blocks(body);
        let content = result["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
    }

    #[test]
    fn test_strip_thinking_handles_missing_messages() {
        let body = serde_json::json!({ "model": "claude-3-5-sonnet" });
        let result = strip_thinking_blocks(body.clone());
        assert_eq!(result, body);
    }

    #[test]
    fn test_strip_thinking_leaves_empty_content_array() {
        // An assistant message with only thinking ends up with empty content; leave it for upstream to handle
        let body = serde_json::json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "solo"}
                ]}
            ]
        });
        let result = strip_thinking_blocks(body);
        let content = result["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 0);
    }

    #[test]
    fn test_strip_thinking_preserves_signature_on_non_thinking_blocks() {
        // signature is left to thinking_rectifier on the error path and untouched here
        let body = serde_json::json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "x", "input": {}, "signature": "s"}
                ]}
            ]
        });
        let result = strip_thinking_blocks(body);
        let block = &result["messages"][0]["content"][0];
        assert_eq!(block["signature"], "s");
    }

    #[test]
    fn test_strip_thinking_multiple_assistant_turns() {
        let body = serde_json::json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "q1"}]},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "a"},
                    {"type": "text", "text": "r1"}
                ]},
                {"role": "user", "content": [{"type": "text", "text": "q2"}]},
                {"role": "assistant", "content": [
                    {"type": "redacted_thinking", "data": "x"},
                    {"type": "text", "text": "r2"}
                ]}
            ]
        });
        let result = strip_thinking_blocks(body);
        let a1 = result["messages"][1]["content"].as_array().unwrap();
        let a2 = result["messages"][3]["content"].as_array().unwrap();
        assert_eq!(a1.len(), 1);
        assert_eq!(a1[0]["text"], "r1");
        assert_eq!(a2.len(), 1);
        assert_eq!(a2[0]["text"], "r2");
    }

    #[test]
    fn test_strip_thinking_ignores_string_content() {
        // assistant.content is a string rather than a block array, as legacy requests and minimal clients do
        // It must not crash and must not change the structure
        let body = serde_json::json!({
            "messages": [
                {"role": "assistant", "content": "plain text response"}
            ]
        });
        let result = strip_thinking_blocks(body.clone());
        assert_eq!(result, body);
    }

    #[test]
    fn test_strip_thinking_preserves_block_order() {
        let body = serde_json::json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "pre"},
                    {"type": "text", "text": "A"},
                    {"type": "tool_use", "id": "t1", "name": "x", "input": {}},
                    {"type": "redacted_thinking", "data": "mid"},
                    {"type": "text", "text": "B"}
                ]}
            ]
        });
        let result = strip_thinking_blocks(body);
        let content = result["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 3);
        assert_eq!(content[0]["text"], "A");
        assert_eq!(content[1]["type"], "tool_use");
        assert_eq!(content[2]["text"], "B");
    }
}
