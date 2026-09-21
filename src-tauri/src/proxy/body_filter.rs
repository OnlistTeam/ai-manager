//! Request body filtering module
//!
//! Filters out private parameters that should not pass through to upstream,
//! preventing internal information leaks.
//!
//! ## Filtering rules
//! - Fields starting with `_` are treated as private parameters and are filtered recursively
//! - Supports a whitelist mechanism to allow specific `_`-prefixed fields through
//! - Supports deep filtering of nested objects and arrays
//! - JSON Schema's properties / patternProperties / definitions / $defs names
//!   are user-defined field names and are not filtered as private parameters
//!
//! ## Use cases
//! - `_internal_id`: internal tracking ID
//! - `_debug_mode`: debug flag
//! - `_session_token`: session token
//! - `_client_version`: client version

use serde_json::Value;
use std::collections::HashSet;

/// Filters private parameters (fields starting with `_`)
///
/// Recursively walks the JSON structure, removing all fields that start with an underscore.
///
/// # Arguments
/// * `body` - the raw request body
///
/// # Returns
/// The filtered request body
///
/// # Example
/// ```ignore
/// let input = json!({
///     "model": "claude-3",
///     "_internal_id": "abc123",
///     "messages": [{"role": "user", "content": "hello", "_token": "secret"}]
/// });
/// let output = filter_private_params(input);
/// // output does not contain _internal_id or _token
/// ```
#[cfg(test)]
pub fn filter_private_params(body: Value) -> Value {
    filter_private_params_with_whitelist(body, &[])
}

/// Filters private parameters (with whitelist support)
///
/// Recursively walks the JSON structure, removing all fields that start with
/// an underscore, except for fields specified in the whitelist.
///
/// # Arguments
/// * `body` - the raw request body
/// * `whitelist` - the list of whitelisted fields (these are not filtered)
///
/// # Returns
/// The filtered request body
///
/// # Example
/// ```ignore
/// let input = json!({
///     "model": "claude-3",
///     "_metadata": {"key": "value"},  // whitelisted, kept
///     "_internal_id": "abc123"        // not whitelisted, filtered
/// });
/// let output = filter_private_params_with_whitelist(input, &["_metadata"]);
/// // output contains _metadata but not _internal_id
/// ```
pub fn filter_private_params_with_whitelist(body: Value, whitelist: &[String]) -> Value {
    let whitelist_set: HashSet<&str> = whitelist.iter().map(|s| s.as_str()).collect();
    filter_recursive_with_whitelist(body, &mut Vec::new(), &mut Vec::new(), &whitelist_set)
}

/// Recursive filtering implementation (with whitelist support)
fn filter_recursive_with_whitelist(
    value: Value,
    path: &mut Vec<String>,
    removed_keys: &mut Vec<String>,
    whitelist: &HashSet<&str>,
) -> Value {
    match value {
        Value::Object(map) => {
            let is_schema_name_map = path.last().is_some_and(|key| matches_schema_name_map(key));
            let filtered: serde_json::Map<String, Value> = map
                .into_iter()
                .filter_map(|(key, val)| {
                    // Fields starting with _ that aren't whitelisted are filtered out
                    if key.starts_with('_')
                        && !whitelist.contains(key.as_str())
                        && !is_schema_name_map
                    {
                        removed_keys.push(key);
                        None
                    } else {
                        path.push(key.clone());
                        let filtered_value =
                            filter_recursive_with_whitelist(val, path, removed_keys, whitelist);
                        path.pop();
                        Some((key, filtered_value))
                    }
                })
                .collect();

            // Only log when something was actually filtered (avoid logging on every request)
            if !removed_keys.is_empty() {
                log::debug!("[BodyFilter] Filtered private params: {removed_keys:?}");
                removed_keys.clear();
            }

            Value::Object(filtered)
        }
        Value::Array(arr) => Value::Array(
            arr.into_iter()
                .map(|v| filter_recursive_with_whitelist(v, path, removed_keys, whitelist))
                .collect(),
        ),
        other => other,
    }
}

fn matches_schema_name_map(key: &str) -> bool {
    matches!(
        key,
        "properties" | "patternProperties" | "definitions" | "$defs"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_top_level_private_params() {
        let input = json!({
            "model": "claude-3",
            "_internal_id": "abc123",
            "_debug": true,
            "max_tokens": 1024
        });

        let output = filter_private_params(input);

        assert!(output.get("model").is_some());
        assert!(output.get("max_tokens").is_some());
        assert!(output.get("_internal_id").is_none());
        assert!(output.get("_debug").is_none());
    }

    #[test]
    fn test_filter_nested_private_params() {
        let input = json!({
            "model": "claude-3",
            "messages": [
                {
                    "role": "user",
                    "content": "hello",
                    "_session_token": "secret"
                }
            ],
            "metadata": {
                "user_id": "user-1",
                "_tracking_id": "track-1"
            }
        });

        let output = filter_private_params(input);

        // Top-level fields are kept
        assert!(output.get("model").is_some());
        assert!(output.get("messages").is_some());
        assert!(output.get("metadata").is_some());

        // Private params inside the messages array are filtered out
        let messages = output.get("messages").unwrap().as_array().unwrap();
        assert!(messages[0].get("role").is_some());
        assert!(messages[0].get("content").is_some());
        assert!(messages[0].get("_session_token").is_none());

        // Private params inside the metadata object are filtered out
        let metadata = output.get("metadata").unwrap();
        assert!(metadata.get("user_id").is_some());
        assert!(metadata.get("_tracking_id").is_none());
    }

    #[test]
    fn test_filter_deeply_nested() {
        let input = json!({
            "level1": {
                "level2": {
                    "level3": {
                        "keep": "value",
                        "_remove": "secret"
                    }
                }
            }
        });

        let output = filter_private_params(input);

        let level3 = output
            .get("level1")
            .unwrap()
            .get("level2")
            .unwrap()
            .get("level3")
            .unwrap();

        assert!(level3.get("keep").is_some());
        assert!(level3.get("_remove").is_none());
    }

    #[test]
    fn test_filter_array_of_objects() {
        let input = json!({
            "items": [
                {"id": 1, "_secret": "a"},
                {"id": 2, "_secret": "b"},
                {"id": 3, "_secret": "c"}
            ]
        });

        let output = filter_private_params(input);
        let items = output.get("items").unwrap().as_array().unwrap();

        for item in items {
            assert!(item.get("id").is_some());
            assert!(item.get("_secret").is_none());
        }
    }

    #[test]
    fn test_no_private_params() {
        let input = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "hello"}]
        });

        let output = filter_private_params(input.clone());

        // With no private params, the output should equal the input
        assert_eq!(input, output);
    }

    #[test]
    fn test_empty_object() {
        let input = json!({});
        let output = filter_private_params(input);
        assert_eq!(output, json!({}));
    }

    #[test]
    fn test_primitive_values() {
        // The original value must not be modified
        assert_eq!(filter_private_params(json!(42)), json!(42));
        assert_eq!(filter_private_params(json!("string")), json!("string"));
        assert_eq!(filter_private_params(json!(true)), json!(true));
        assert_eq!(filter_private_params(json!(null)), json!(null));
    }

    #[test]
    fn test_whitelist_preserves_private_params() {
        let input = json!({
            "model": "claude-3",
            "_metadata": {"key": "value"},
            "_internal_id": "abc123",
            "_stream_options": {"include_usage": true}
        });

        let whitelist = vec!["_metadata".to_string(), "_stream_options".to_string()];
        let output = filter_private_params_with_whitelist(input, &whitelist);

        // Allowlisted fields are kept
        assert!(output.get("_metadata").is_some());
        assert!(output.get("_stream_options").is_some());
        // Private fields that are not allowlisted are filtered out
        assert!(output.get("_internal_id").is_none());
        // Ordinary fields are kept
        assert!(output.get("model").is_some());
    }

    #[test]
    fn test_whitelist_nested() {
        let input = json!({
            "data": {
                "_allowed": "keep",
                "_forbidden": "remove",
                "normal": "value"
            }
        });

        let whitelist = vec!["_allowed".to_string()];
        let output = filter_private_params_with_whitelist(input, &whitelist);

        let data = output.get("data").unwrap();
        assert!(data.get("_allowed").is_some());
        assert!(data.get("_forbidden").is_none());
        assert!(data.get("normal").is_some());
    }

    #[test]
    fn test_preserves_json_schema_property_names_with_underscore() {
        let input = json!({
            "tools": [
                {
                    "name": "lookup",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "_id": {"type": "string", "_internal_note": "remove"},
                            "_meta": {"type": "object"}
                        },
                        "_private_schema_note": "remove"
                    }
                }
            ]
        });

        let output = filter_private_params(input);
        let schema = &output["tools"][0]["input_schema"];

        assert!(schema["properties"].get("_id").is_some());
        assert!(schema["properties"].get("_meta").is_some());
        assert!(schema["properties"]["_id"].get("_internal_note").is_none());
        assert!(schema.get("_private_schema_note").is_none());
    }

    #[test]
    fn test_empty_whitelist_same_as_default() {
        let input = json!({
            "model": "claude-3",
            "_internal_id": "abc123"
        });

        let output1 = filter_private_params(input.clone());
        let output2 = filter_private_params_with_whitelist(input, &[]);

        assert_eq!(output1, output2);
    }
}
