//! Base64 payload decoding and credential detection for deep links.

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use serde_json::Value;

use super::{invalid, MAX_PAYLOAD_BYTES};
use crate::domain::AppError;
use crate::platform::redact::{looks_like_credential, redact_secrets};

/// Decodes a `config` / `content` parameter.
///
/// Vendors in the wild emit all four Base64 spellings, and a `+` that was not
/// percent-escaped arrives here as a space. Restoring the space and re-padding
/// before trying the standard and URL-safe alphabets keeps those links working
/// while still refusing anything that is not Base64 at all.
pub fn decode_payload(field: &'static str, raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    let restored = trimmed.replace(' ', "+");
    let padded = pad(&restored);

    let bytes = [restored.as_str(), padded.as_str()]
        .into_iter()
        .find_map(|candidate| {
            [&STANDARD, &STANDARD_NO_PAD]
                .into_iter()
                .find_map(|engine| engine.decode(candidate).ok())
                .or_else(|| {
                    [&URL_SAFE, &URL_SAFE_NO_PAD]
                        .into_iter()
                        .find_map(|engine| engine.decode(candidate).ok())
                })
        })
        .ok_or_else(|| {
            invalid(
                "error.deepLink.invalidEncoding",
                format!("deep link parameter {field} is not valid Base64"),
            )
        })?;

    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(invalid(
            "error.deepLink.valueTooLong",
            format!("deep link parameter {field} decodes to too much data"),
        ));
    }

    let text = String::from_utf8(bytes).map_err(|_| {
        invalid(
            "error.deepLink.invalidEncoding",
            format!("deep link parameter {field} does not decode to UTF-8"),
        )
    })?;
    if text.contains('\0') {
        return Err(invalid(
            "error.deepLink.invalidEncoding",
            format!("deep link parameter {field} decodes to text containing NUL"),
        ));
    }
    Ok(text)
}

fn pad(value: &str) -> String {
    let remainder = value.len() % 4;
    if remainder == 0 {
        return value.to_string();
    }
    let mut padded = value.to_string();
    padded.extend(std::iter::repeat_n('=', 4 - remainder));
    padded
}

/// Decodes a `config` parameter into JSON.
///
/// `configFormat=toml` is part of the upstream format; TOML is converted to the
/// same JSON shape so everything downstream sees one representation.
pub fn decode_config(raw: &str, format: Option<&str>) -> Result<Value, AppError> {
    let text = decode_payload("config", raw)?;
    match format.unwrap_or("json") {
        "json" => serde_json::from_str(&text).map_err(|_| {
            invalid(
                "error.deepLink.invalidEncoding",
                "deep link config is not valid JSON",
            )
        }),
        "toml" => {
            let parsed: toml::Value = toml::from_str(&text).map_err(|_| {
                invalid(
                    "error.deepLink.invalidEncoding",
                    "deep link config is not valid TOML",
                )
            })?;
            serde_json::to_value(parsed).map_err(|_| {
                invalid(
                    "error.deepLink.invalidEncoding",
                    "deep link TOML config has no JSON representation",
                )
            })
        }
        other => Err(invalid(
            "error.deepLink.invalidLink",
            format!("deep link configFormat {other} is not supported"),
        )),
    }
}

/// Whether a decoded config carries something credential-shaped.
///
/// This deliberately reuses `platform::redact`, the product's existing and
/// tested credential detector, instead of introducing a second table of secret
/// field names. It is format-agnostic, so it also sees a key embedded in a TOML
/// blob stored as a JSON string, or an MCP `--api-key` argument, neither of
/// which a per-tool field table would reach.
///
/// A present-but-empty `ANTHROPIC_AUTH_TOKEN` is how vendor templates mark the
/// slot the user must fill in, so only non-blank values count.
pub fn carries_credential(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, child)| match child {
            Value::String(text) => !text.trim().is_empty() && looks_like_credential(key, text),
            other => carries_credential(other),
        }),
        // Argument vectors put the label and the secret in two adjacent
        // elements (`["--api-key", "abc123"]`). Scanning them joined keeps the
        // adjacency `redact_secrets` needs to mask a value whose own shape
        // gives nothing away.
        Value::Array(items) => {
            let joined = items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            (!joined.trim().is_empty() && redact_secrets(&joined) != joined)
                || items.iter().any(carries_credential)
        }
        Value::String(text) => !text.trim().is_empty() && redact_secrets(text) != *text,
        _ => false,
    }
}
