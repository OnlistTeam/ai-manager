//! The masking engine: walks a JSON body, replaces sensitive substrings in
//! string values with placeholders, and puts real values back into replies.

use std::sync::{Mutex, PoisonError};

use serde_json::Value;

use super::placeholder::{HashKey, PLACEHOLDER};
use super::vault::Vault;
use super::KnownValue;

/// Keys whose string values must reach the provider byte for byte: opaque
/// signatures and encrypted blobs, identifiers, enums, model names, media,
/// and signed reasoning text (see [`SIGNED_TEXT_KEYS`]).
const SKIPPED_KEYS: [&str; 30] = [
    "signature",
    "encrypted_content",
    "data",
    "thoughtSignature",
    "thought_signature",
    "model",
    "id",
    "tool_use_id",
    "call_id",
    "item_id",
    "type",
    "role",
    "name",
    "previous_response_id",
    "prompt_cache_key",
    "media_type",
    "mime_type",
    "mimeType",
    "url",
    "image_url",
    "file_id",
    "fileUri",
    "file_uri",
    "reasoning_effort",
    "effort",
    "stop_reason",
    "finish_reason",
    "status",
    "object",
    "event",
];

/// Reasoning text the provider signs and verifies when it is sent back. It
/// is neither masked nor restored, so it always returns exactly as issued;
/// it only ever holds what the model itself wrote.
const SIGNED_TEXT_KEYS: [&str; 1] = ["thinking"];

/// Keys whose string value is itself JSON text (tool-call arguments).
const ENCODED_JSON_KEYS: [&str; 2] = ["arguments", "partial_json"];

pub(crate) fn is_encoded_json_key(key: &str) -> bool {
    ENCODED_JSON_KEYS.contains(&key)
}

pub(crate) fn is_signed_text_key(key: &str) -> bool {
    SIGNED_TEXT_KEYS.contains(&key)
}

fn is_skipped_key(key: &str) -> bool {
    SKIPPED_KEYS.contains(&key) || is_signed_text_key(key)
}

/// Escapes `value` for insertion inside a JSON string literal.
fn json_escaped(value: &str) -> String {
    let quoted = serde_json::Value::String(value.to_owned()).to_string();
    quoted[1..quoted.len() - 1].to_owned()
}

pub(crate) struct Engine {
    key: HashKey,
    vault: Mutex<Vault>,
}

impl Engine {
    pub(crate) fn new(key: HashKey, capacity: usize) -> Self {
        Self {
            key,
            vault: Mutex::new(Vault::new(capacity)),
        }
    }

    fn vault(&self) -> std::sync::MutexGuard<'_, Vault> {
        self.vault.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn has_values(&self) -> bool {
        !self.vault().is_empty()
    }

    /// Masks every string value of a JSON document in place.
    pub(crate) fn mask_value(&self, value: &mut Value, count: &mut usize) {
        let mut known = self.vault().known_values();
        self.mask_in(value, &mut known, count);
    }

    /// Masks one free-text string. Returns the new text when anything changed.
    #[cfg(test)]
    pub(crate) fn mask_text(&self, text: &str, count: &mut usize) -> Option<String> {
        let mut known = self.vault().known_values();
        self.mask_str(text, &mut known, count)
    }

    fn mask_str(
        &self,
        text: &str,
        known: &mut Vec<KnownValue>,
        count: &mut usize,
    ) -> Option<String> {
        if text.len() < 6 || text.starts_with("data:") {
            return None;
        }
        let spans = super::detect_with_known(text, known);
        if spans.is_empty() {
            return None;
        }
        let mut vault = self.vault();
        let mut output = String::with_capacity(text.len());
        let mut cursor = 0;
        for span in spans {
            let value = &text[span.start..span.end];
            let placeholder = self.key.placeholder(span.kind, value);
            vault.insert(placeholder.clone(), value.to_owned());
            vault.remember_known(value, span.kind);
            if span.kind.is_contextual() && !known.iter().any(|k| k.value == value) {
                known.push(KnownValue {
                    value: value.to_owned(),
                    kind: span.kind,
                });
            }
            output.push_str(&text[cursor..span.start]);
            output.push_str(&placeholder);
            cursor = span.end;
            *count += 1;
        }
        output.push_str(&text[cursor..]);
        Some(output)
    }

    /// Masks a string that holds JSON text: mask inside the decoded document
    /// so the stored value is the real one, not its escaped form.
    fn mask_encoded(
        &self,
        text: &str,
        known: &mut Vec<KnownValue>,
        count: &mut usize,
    ) -> Option<String> {
        match serde_json::from_str::<Value>(text) {
            Ok(mut document) if document.is_object() || document.is_array() => {
                let before = *count;
                self.mask_in(&mut document, known, count);
                (*count > before).then(|| document.to_string())
            }
            _ => self.mask_str(text, known, count),
        }
    }

    fn mask_in(&self, value: &mut Value, known: &mut Vec<KnownValue>, count: &mut usize) {
        match value {
            Value::String(text) => {
                if let Some(masked) = self.mask_str(text, known, count) {
                    *text = masked;
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.mask_in(item, known, count);
                }
            }
            Value::Object(map) => {
                for (key, item) in map.iter_mut() {
                    if is_skipped_key(key) {
                        continue;
                    }
                    match item {
                        Value::String(text) if is_encoded_json_key(key) => {
                            if let Some(masked) = self.mask_encoded(text, known, count) {
                                *text = masked;
                            }
                        }
                        _ => self.mask_in(item, known, count),
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    /// Replaces known placeholders in `text`. With `escape`, the values are
    /// escaped for a JSON string literal because `text` is JSON source.
    pub(crate) fn restore_text(&self, text: &str, escape: bool) -> Option<String> {
        if !text.contains("{{") {
            return None;
        }
        let vault = self.vault();
        let mut output = String::with_capacity(text.len());
        let mut cursor = 0;
        let mut changed = false;
        for found in PLACEHOLDER.find_iter(text) {
            let Some(real) = vault.get(found.as_str()) else {
                continue;
            };
            output.push_str(&text[cursor..found.start()]);
            if escape {
                output.push_str(&json_escaped(real));
            } else {
                output.push_str(real);
            }
            cursor = found.end();
            changed = true;
        }
        if !changed {
            return None;
        }
        output.push_str(&text[cursor..]);
        Some(output)
    }

    /// Restores every string in a parsed reply except signed reasoning text.
    /// Returns whether anything changed.
    pub(crate) fn restore_value(&self, value: &mut Value) -> bool {
        self.restore_value_in(value, false)
    }

    fn restore_value_in(&self, value: &mut Value, escape: bool) -> bool {
        match value {
            Value::String(text) => match self.restore_text(text, escape) {
                Some(restored) => {
                    *text = restored;
                    true
                }
                None => false,
            },
            Value::Array(items) => {
                let mut changed = false;
                for item in items {
                    changed |= self.restore_value_in(item, false);
                }
                changed
            }
            Value::Object(map) => {
                let mut changed = false;
                for (key, item) in map.iter_mut() {
                    if !is_signed_text_key(key) {
                        changed |= self.restore_value_in(item, is_encoded_json_key(key));
                    }
                }
                changed
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => false,
        }
    }
}
