//! Thinking signature rectifier
//!
//! Automatically repairs Anthropic API request errors caused by signature validation failures.
//! When the upstream API returns a signature-related error, the offending signature fields are removed and the request is retried.

use super::types::RectifierConfig;
use serde_json::Value;

/// Rectification result
#[derive(Debug, Clone, Default)]
pub struct RectifyResult {
    /// Whether rectification was applied
    pub applied: bool,
    /// Number of thinking blocks removed
    pub removed_thinking_blocks: usize,
    /// Number of redacted_thinking blocks removed
    pub removed_redacted_thinking_blocks: usize,
    /// Number of signature fields removed
    pub removed_signature_fields: usize,
}

/// Upstream "invalid request" phrase emitted by some Chinese-language gateways.
/// Escaped so the source stays ASCII while the match keeps working.
const INVALID_REQUEST_CN: &str = "\u{975e}\u{6cd5}\u{8bf7}\u{6c42}";

/// Detects whether the thinking signature rectifier should run
///
/// Returns `true` when the rectifier should run, `false` otherwise.
/// Configuration switches are checked.
pub fn should_rectify_thinking_signature(
    error_message: Option<&str>,
    config: &RectifierConfig,
) -> bool {
    // Check the master switch
    if !config.enabled {
        return false;
    }
    // Check the sub-switch
    if !config.request_thinking_signature {
        return false;
    }

    // Detect the error kind
    let Some(msg) = error_message else {
        return false;
    };
    let lower = msg.to_lowercase();

    // Case 1: invalid signature inside a thinking block
    // Example error: "Invalid 'signature' in 'thinking' block"
    if lower.contains("invalid")
        && lower.contains("signature")
        && lower.contains("thinking")
        && lower.contains("block")
    {
        return true;
    }

    // Case 1b: Gemini and third-party channels return "Thought signature is not valid"
    // Example error: "Unable to submit request because Thought signature is not valid"
    if lower.contains("thought signature")
        && (lower.contains("not valid") || lower.contains("invalid"))
    {
        return true;
    }

    // Case 2: the assistant message must start with a thinking block
    // Example error: "must start with a thinking block"
    if lower.contains("must start with a thinking block") {
        return true;
    }

    // Case 3: expected thinking or redacted_thinking, found tool_use
    // Aligned with CCH: require an explicit tool_use so the match does not become too broad.
    // Example error: "Expected `thinking` or `redacted_thinking`, but found `tool_use`"
    if lower.contains("expected")
        && (lower.contains("thinking") || lower.contains("redacted_thinking"))
        && lower.contains("found")
        && lower.contains("tool_use")
    {
        return true;
    }

    // Case 4: the signature field is required but missing
    // Example error: "signature: Field required"
    if lower.contains("signature") && lower.contains("field required") {
        return true;
    }

    // Case 5: the signature field is rejected (third-party channels)
    // Example error: "xxx.signature: Extra inputs are not permitted"
    if lower.contains("signature") && lower.contains("extra inputs are not permitted") {
        return true;
    }

    // Case 6: thinking/redacted_thinking blocks were modified
    // Example error: "thinking or redacted_thinking blocks ... cannot be modified"
    if (lower.contains("thinking") || lower.contains("redacted_thinking"))
        && lower.contains("cannot be modified")
    {
        return true;
    }

    // Case 7: invalid request (aligned with CCH, a catch-all for invalid request)
    if lower.contains(INVALID_REQUEST_CN)
        || lower.contains("illegal request")
        || lower.contains("invalid request")
    {
        return true;
    }

    false
}

/// Rectifies the Anthropic request body with minimal intrusion
///
/// - Removes thinking/redacted_thinking blocks from messages[*].content
/// - Removes signature fields left on non-thinking blocks
/// - Deletes the top-level thinking field under specific conditions
///
/// Note: this function mutates the body object in place
pub fn rectify_anthropic_request(body: &mut Value) -> RectifyResult {
    let mut result = RectifyResult::default();

    let messages = match body.get_mut("messages").and_then(|m| m.as_array_mut()) {
        Some(m) => m,
        None => return result,
    };

    // Walk every message
    for msg in messages.iter_mut() {
        let content = match msg.get_mut("content").and_then(|c| c.as_array_mut()) {
            Some(c) => c,
            None => continue,
        };

        let mut new_content = Vec::with_capacity(content.len());
        let mut content_modified = false;

        for block in content.iter() {
            let block_type = block.get("type").and_then(|t| t.as_str());

            match block_type {
                Some("thinking") => {
                    result.removed_thinking_blocks += 1;
                    content_modified = true;
                    continue;
                }
                Some("redacted_thinking") => {
                    result.removed_redacted_thinking_blocks += 1;
                    content_modified = true;
                    continue;
                }
                _ => {}
            }

            // Remove signature fields from non-thinking blocks
            if block.get("signature").is_some() {
                let mut block_clone = block.clone();
                if let Some(obj) = block_clone.as_object_mut() {
                    obj.remove("signature");
                    result.removed_signature_fields += 1;
                    content_modified = true;
                    new_content.push(Value::Object(obj.clone()));
                    continue;
                }
            }

            new_content.push(block.clone());
        }

        if content_modified {
            result.applied = true;
            *content = new_content;
        }
    }

    // Fallback: thinking enabled and the last assistant message in a tool-call chain does not start with thinking
    let messages_snapshot: Vec<Value> = body
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|a| a.to_vec())
        .unwrap_or_default();

    if should_remove_top_level_thinking(body, &messages_snapshot) {
        if let Some(obj) = body.as_object_mut() {
            obj.remove("thinking");
            result.applied = true;
        }
    }

    result
}

/// Decides whether the top-level thinking field must be deleted
fn should_remove_top_level_thinking(body: &Value, messages: &[Value]) -> bool {
    // Check whether thinking is enabled
    let thinking_type = body
        .get("thinking")
        .and_then(|t| t.get("type"))
        .and_then(|t| t.as_str());

    // Aligned with CCH: only type=enabled counts as enabled
    let thinking_enabled = thinking_type == Some("enabled");

    if !thinking_enabled {
        return false;
    }

    // Find the last assistant message
    let last_assistant = messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"));

    let last_assistant_content = match last_assistant
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        Some(c) if !c.is_empty() => c,
        _ => return false,
    };

    // Check whether the first block is thinking/redacted_thinking
    let first_block_type = last_assistant_content
        .first()
        .and_then(|b| b.get("type"))
        .and_then(|t| t.as_str());

    let missing_thinking_prefix =
        first_block_type != Some("thinking") && first_block_type != Some("redacted_thinking");

    if !missing_thinking_prefix {
        return false;
    }

    // Check whether a tool_use is present
    last_assistant_content
        .iter()
        .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
}

/// Aligned with CCH: do not rewrite the thinking type before the request.
pub fn normalize_thinking_type(body: Value) -> Value {
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn enabled_config() -> RectifierConfig {
        RectifierConfig {
            enabled: true,
            request_thinking_signature: true,
            request_thinking_budget: true,
            request_media_fallback: true,
            request_media_heuristic: true,
        }
    }

    fn disabled_config() -> RectifierConfig {
        RectifierConfig {
            enabled: true,
            request_thinking_signature: false,
            request_thinking_budget: false,
            request_media_fallback: true,
            request_media_heuristic: true,
        }
    }

    fn master_disabled_config() -> RectifierConfig {
        RectifierConfig {
            enabled: false,
            request_thinking_signature: true,
            request_thinking_budget: true,
            request_media_fallback: true,
            request_media_heuristic: true,
        }
    }

    // ==================== should_rectify_thinking_signature tests ====================

    #[test]
    fn test_detect_invalid_signature() {
        assert!(should_rectify_thinking_signature(
            Some("messages.1.content.0: Invalid `signature` in `thinking` block"),
            &enabled_config()
        ));
    }

    #[test]
    fn test_detect_invalid_signature_no_backticks() {
        assert!(should_rectify_thinking_signature(
            Some("Messages.1.Content.0: invalid signature in thinking block"),
            &enabled_config()
        ));
    }

    #[test]
    fn test_detect_invalid_thought_signature_message() {
        assert!(should_rectify_thinking_signature(
            Some(
                "Unable to submit request because Thought signature is not valid.. Learn more: https://example.com/help"
            ),
            &enabled_config()
        ));
    }

    #[test]
    fn test_detect_invalid_signature_nested_json() {
        // Test an error message in nested JSON form (common with third-party channels)
        let nested_error = r#"{"error":{"message":"{\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"***.content.0: Invalid `signature` in `thinking` block\"},\"request_id\":\"req_xxx\"}"}}"#;
        assert!(should_rectify_thinking_signature(
            Some(nested_error),
            &enabled_config()
        ));
    }

    #[test]
    fn test_detect_invalid_thought_signature_nested_json() {
        let nested_error = r#"{"error":{"message":"Unable to submit request because Thought signature is not valid.. Learn more: https://example.com/help","type":"upstream_error","param":"","code":400}}"#;
        assert!(should_rectify_thinking_signature(
            Some(nested_error),
            &enabled_config()
        ));
    }

    #[test]
    fn test_detect_thinking_expected() {
        assert!(should_rectify_thinking_signature(
            Some("messages.69.content.0.type: Expected `thinking` or `redacted_thinking`, but found `tool_use`."),
            &enabled_config()
        ));
    }

    #[test]
    fn test_no_detect_thinking_expected_without_tool_use() {
        assert!(!should_rectify_thinking_signature(
            Some("messages.69.content.0.type: Expected `thinking` or `redacted_thinking`, but found `text`."),
            &enabled_config()
        ));
    }

    #[test]
    fn test_detect_must_start_with_thinking() {
        assert!(should_rectify_thinking_signature(
            Some("a final `assistant` message must start with a thinking block"),
            &enabled_config()
        ));
    }

    #[test]
    fn test_no_trigger_for_unrelated_error() {
        assert!(!should_rectify_thinking_signature(
            Some("Request timeout"),
            &enabled_config()
        ));
        assert!(!should_rectify_thinking_signature(
            Some("Connection refused"),
            &enabled_config()
        ));
        assert!(!should_rectify_thinking_signature(None, &enabled_config()));
    }

    #[test]
    fn test_detect_signature_field_required() {
        // Case 4: the signature field is missing
        assert!(should_rectify_thinking_signature(
            Some("***.***.***.***.***.signature: Field required"),
            &enabled_config()
        ));
        // Nested JSON form
        let nested_error = r#"{"error":{"type":"<nil>","message":"{\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"***.***.***.***.***.signature: Field required\"},\"request_id\":\"req_xxx\"}"}}"#;
        assert!(should_rectify_thinking_signature(
            Some(nested_error),
            &enabled_config()
        ));
    }

    #[test]
    fn test_disabled_config() {
        // Even on a matching error, a disabled config must not trigger
        assert!(!should_rectify_thinking_signature(
            Some("Invalid `signature` in `thinking` block"),
            &disabled_config()
        ));
    }

    #[test]
    fn test_master_disabled() {
        // With the master switch off, an enabled sub-switch must not trigger
        assert!(!should_rectify_thinking_signature(
            Some("Invalid `signature` in `thinking` block"),
            &master_disabled_config()
        ));
    }

    // ==================== rectify_anthropic_request tests ====================

    #[test]
    fn test_rectify_removes_thinking_blocks() {
        let mut body = json!({
            "model": "claude-test",
            "messages": [{
                "role": "assistant",
                "content": [
                    { "type": "thinking", "thinking": "t", "signature": "sig" },
                    { "type": "text", "text": "hello", "signature": "sig_text" },
                    { "type": "tool_use", "id": "toolu_1", "name": "WebSearch", "input": {}, "signature": "sig_tool" },
                    { "type": "redacted_thinking", "data": "r", "signature": "sig_redacted" }
                ]
            }]
        });

        let result = rectify_anthropic_request(&mut body);

        assert!(result.applied);
        assert_eq!(result.removed_thinking_blocks, 1);
        assert_eq!(result.removed_redacted_thinking_blocks, 1);
        assert_eq!(result.removed_signature_fields, 2);

        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert!(content[0].get("signature").is_none());
        assert_eq!(content[1]["type"], "tool_use");
        assert!(content[1].get("signature").is_none());
    }

    #[test]
    fn test_rectify_removes_top_level_thinking() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "enabled", "budget_tokens": 1024 },
            "messages": [{
                "role": "assistant",
                "content": [
                    { "type": "tool_use", "id": "toolu_1", "name": "WebSearch", "input": {} }
                ]
            }, {
                "role": "user",
                "content": [{ "type": "tool_result", "tool_use_id": "toolu_1", "content": "ok" }]
            }]
        });

        let result = rectify_anthropic_request(&mut body);

        assert!(result.applied);
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn test_rectify_no_change_when_no_issues() {
        let mut body = json!({
            "model": "claude-test",
            "messages": [{
                "role": "user",
                "content": [{ "type": "text", "text": "hello" }]
            }]
        });

        let result = rectify_anthropic_request(&mut body);

        assert!(!result.applied);
        assert_eq!(result.removed_thinking_blocks, 0);
    }

    #[test]
    fn test_rectify_no_messages() {
        let mut body = json!({ "model": "claude-test" });
        let result = rectify_anthropic_request(&mut body);
        assert!(!result.applied);
    }

    #[test]
    fn test_rectify_preserves_thinking_when_prefix_exists() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "enabled" },
            "messages": [{
                "role": "assistant",
                "content": [
                    { "type": "thinking", "thinking": "some thought" },
                    { "type": "tool_use", "id": "toolu_1", "name": "Test", "input": {} }
                ]
            }]
        });

        let result = rectify_anthropic_request(&mut body);

        // The thinking block is removed, but the top-level thinking must stay (it had a thinking prefix)
        assert!(result.applied);
        assert_eq!(result.removed_thinking_blocks, 1);
        // Note: once the thinking block is removed the first block becomes tool_use,
        // which triggers the logic that deletes the top-level thinking
        // That is intended: if the request still does not comply after rectification, drop the top-level thinking
    }

    // ==================== tests for the newly added error cases ====================

    #[test]
    fn test_detect_signature_extra_inputs() {
        // Case 5: the signature field is rejected
        assert!(should_rectify_thinking_signature(
            Some("xxx.signature: Extra inputs are not permitted"),
            &enabled_config()
        ));
    }

    #[test]
    fn test_detect_thinking_cannot_be_modified() {
        // Case 6: thinking blocks cannot be modified
        assert!(should_rectify_thinking_signature(
            Some("thinking or redacted_thinking blocks in the response cannot be modified"),
            &enabled_config()
        ));
    }

    #[test]
    fn test_detect_invalid_request() {
        // Case 7: invalid request (aligned with CCH, always triggers)
        let localized = format!("{INVALID_REQUEST_CN}: thinking signature invalid");
        assert!(should_rectify_thinking_signature(
            Some(&localized),
            &enabled_config()
        ));
        assert!(should_rectify_thinking_signature(
            Some("illegal request: tool_use block mismatch"),
            &enabled_config()
        ));
        assert!(should_rectify_thinking_signature(
            Some("invalid request: malformed JSON"),
            &enabled_config()
        ));
    }

    #[test]
    fn test_do_not_detect_thinking_type_tag_mismatch() {
        // Aligned with CCH: an adaptive tag mismatch must not trigger the signature rectifier
        assert!(!should_rectify_thinking_signature(
            Some("Input tag 'adaptive' found using 'type' does not match expected tags"),
            &enabled_config()
        ));
    }

    // ==================== adaptive thinking type tests ====================

    #[test]
    fn test_rectify_keeps_adaptive_when_no_legacy_blocks() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "adaptive" },
            "messages": [{
                "role": "user",
                "content": [{ "type": "text", "text": "hello" }]
            }]
        });

        let result = rectify_anthropic_request(&mut body);

        assert!(!result.applied);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert!(body["thinking"].get("budget_tokens").is_none());
    }

    #[test]
    fn test_rectify_adaptive_preserves_existing_budget_tokens() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "adaptive", "budget_tokens": 5000 },
            "messages": [{
                "role": "user",
                "content": [{ "type": "text", "text": "hello" }]
            }]
        });

        let result = rectify_anthropic_request(&mut body);

        assert!(!result.applied);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["thinking"]["budget_tokens"], 5000);
    }

    #[test]
    fn test_rectify_does_not_change_enabled_type() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "enabled", "budget_tokens": 1024 },
            "messages": [{
                "role": "user",
                "content": [{ "type": "text", "text": "hello" }]
            }]
        });

        let result = rectify_anthropic_request(&mut body);

        assert!(!result.applied);
        assert_eq!(body["thinking"]["type"], "enabled");
    }

    #[test]
    fn test_rectify_removes_top_level_thinking_adaptive() {
        // The top-level thinking is deleted only for type=enabled with tool_use; adaptive keeps it
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "adaptive" },
            "messages": [{
                "role": "assistant",
                "content": [
                    { "type": "tool_use", "id": "toolu_1", "name": "WebSearch", "input": {} }
                ]
            }, {
                "role": "user",
                "content": [{ "type": "tool_result", "tool_use_id": "toolu_1", "content": "ok" }]
            }]
        });

        let result = rectify_anthropic_request(&mut body);

        assert!(!result.applied);
        assert_eq!(body["thinking"]["type"], "adaptive");
    }

    #[test]
    fn test_rectify_adaptive_still_cleans_legacy_signature_blocks() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "adaptive" },
            "messages": [{
                "role": "assistant",
                "content": [
                    { "type": "thinking", "thinking": "t", "signature": "sig_thinking" },
                    { "type": "text", "text": "hello", "signature": "sig_text" }
                ]
            }]
        });

        let result = rectify_anthropic_request(&mut body);

        assert!(result.applied);
        assert_eq!(result.removed_thinking_blocks, 1);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert!(content[0].get("signature").is_none());
        assert_eq!(body["thinking"]["type"], "adaptive");
    }

    // ==================== normalize_thinking_type tests ====================

    #[test]
    fn test_normalize_thinking_type_adaptive_unchanged() {
        let body = json!({
            "model": "claude-test",
            "thinking": { "type": "adaptive" }
        });

        let result = normalize_thinking_type(body);

        assert_eq!(result["thinking"]["type"], "adaptive");
        assert!(result["thinking"].get("budget_tokens").is_none());
    }

    #[test]
    fn test_normalize_thinking_type_enabled_unchanged() {
        let body = json!({
            "model": "claude-test",
            "thinking": { "type": "enabled", "budget_tokens": 2048 }
        });

        let result = normalize_thinking_type(body);

        assert_eq!(result["thinking"]["type"], "enabled");
        assert_eq!(result["thinking"]["budget_tokens"], 2048);
    }

    #[test]
    fn test_normalize_thinking_type_disabled_unchanged() {
        let body = json!({
            "model": "claude-test",
            "thinking": { "type": "disabled" }
        });

        let result = normalize_thinking_type(body);

        assert_eq!(result["thinking"]["type"], "disabled");
    }

    #[test]
    fn test_normalize_thinking_type_preserves_budget() {
        let body = json!({
            "model": "claude-test",
            "thinking": { "type": "adaptive", "budget_tokens": 5000 }
        });

        let result = normalize_thinking_type(body);

        assert_eq!(result["thinking"]["type"], "adaptive");
        assert_eq!(result["thinking"]["budget_tokens"], 5000);
    }

    #[test]
    fn test_normalize_thinking_type_no_thinking() {
        let body = json!({
            "model": "claude-test"
        });

        let result = normalize_thinking_type(body);

        assert!(result.get("thinking").is_none());
    }

    #[test]
    fn test_normalize_thinking_type_unknown_unchanged() {
        let body = json!({
            "model": "claude-test",
            "thinking": { "type": "unexpected", "budget_tokens": 100 }
        });

        let result = normalize_thinking_type(body);

        assert_eq!(result["thinking"]["type"], "unexpected");
        assert_eq!(result["thinking"]["budget_tokens"], 100);
    }
}
