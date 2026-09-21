//! Preserve the user's other settings when switching endpoints (ADR-0035 decision 6).
//!
//! A CC Switch provider is a full config snapshot, so switching means overwriting the whole
//! file. Upstream's approach is not changed here; this module only pastes back the parts of
//! live that "the provider does not own" after upstream has written it. The rule is
//! `merged[K] = written[K]` (whatever was written wins); otherwise `previous[K]` (as long as
//! K is not a connection key). The set of connection keys comes from the keys the preset
//! templates actually use (see the 2026-09-07 plan §5).

use std::collections::HashMap;

use serde_json::{json, Value};
use toml_edit::DocumentMut;

use crate::config::{get_claude_settings_path, read_json_file, write_json_file};
use crate::domain::{AppError, ErrorCode, ToolId};
use crate::gemini_config::{get_gemini_env_path, read_gemini_env, write_gemini_env_atomic};
use crate::platform::redact::{looks_like_credential, redact_secrets, truncate_tail};

const PRESERVE_FAILED: &str = "error.provider.settingsPreserveFailed";

// ---------- Claude Code ----------

/// When the target provider does not write these three keys, fill in `""`: the official
/// docs state that an empty string in settings `env` cancels the variable, suppressing even
/// a shell export of the same name — otherwise "switch to official" would be a no-op for a
/// terminal that has relay variables set.
const CLAUDE_CANCEL_KEYS: [&str; 3] = [
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
];

/// Top-level keys whose value either is (or directly determines) the effective credential,
/// or is an internal field upstream strips before writing live
/// (`sanitize_claude_settings_for_live`, `services/provider/live.rs`). Neither kind may ever
/// be pasted back just because "previous has it and written does not": pasting back the
/// first three leaves the old provider's way of obtaining a token under the new provider's
/// name, and pasting back the last four resurrects internal state upstream deliberately
/// cleared.
const CLAUDE_TOP_LEVEL_CONNECTION_KEYS: [&str; 7] = [
    "apiKeyHelper",
    "awsAuthRefresh",
    "awsCredentialExport",
    "apiFormat",
    "api_format",
    "openrouterCompatMode",
    "openrouter_compat_mode",
];

/// Bedrock/Vertex go through `CLAUDE_CODE_USE_BEDROCK`/`CLAUDE_CODE_USE_VERTEX`, with the
/// credentials hanging off `AWS_*` (including `AWS_SECRET_ACCESS_KEY`) and
/// `GOOGLE_APPLICATION_CREDENTIALS`/`CLOUD_ML_REGION` respectively — those carry no
/// `ANTHROPIC_`/`CLAUDE_CODE_USE_` prefix but are just as much connection material that
/// "should change when the provider changes", so old values from previous must never be
/// pasted back.
fn claude_env_key_is_connection(key: &str) -> bool {
    key.starts_with("ANTHROPIC_")
        || key.starts_with("CLAUDE_CODE_USE_")
        || (key.starts_with("CLAUDE_CODE_SKIP_") && key.ends_with("_AUTH"))
        || key.starts_with("AWS_")
        || key == "GOOGLE_APPLICATION_CREDENTIALS"
        || key == "CLOUD_ML_REGION"
}

pub(super) fn merge_claude_settings(previous: &Value, written: &Value) -> Value {
    let mut merged = match written.as_object() {
        Some(object) => Value::Object(object.clone()),
        None => json!({}),
    };
    let target = merged
        .as_object_mut()
        .expect("merged was just built as an object");

    if let Some(previous_object) = previous.as_object() {
        for (key, value) in previous_object {
            if key != "env"
                && !CLAUDE_TOP_LEVEL_CONNECTION_KEYS.contains(&key.as_str())
                && !target.contains_key(key)
            {
                target.insert(key.clone(), value.clone());
            }
        }
    }

    if !target.get("env").is_some_and(Value::is_object) {
        target.insert("env".to_string(), json!({}));
    }
    let env = target
        .get_mut("env")
        .and_then(Value::as_object_mut)
        .expect("env was just normalized to an object");
    if let Some(previous_env) = previous.get("env").and_then(Value::as_object) {
        for (key, value) in previous_env {
            let value_str = value.as_str().unwrap_or_default();
            if !claude_env_key_is_connection(key)
                && !looks_like_credential(key, value_str)
                && !env.contains_key(key)
            {
                env.insert(key.clone(), value.clone());
            }
        }
    }
    for key in CLAUDE_CANCEL_KEYS {
        if !env.contains_key(key) {
            // This connection key from previous was skipped by the loop above precisely
            // because it is a connection key — but if its value came from a pure shell export
            // (the user never wrote it into any provider's settings_config), writing "" would
            // also suppress the credential in the shell. Only a log-side safety net is
            // possible here: this pure function cannot probe the shell, and explaining it in
            // the UI layer is known technical debt (see ARCHITECTURE.md).
            let previous_had_value = previous
                .get("env")
                .and_then(|env| env.get(key))
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
            if previous_had_value {
                log::warn!("switch cancels shell/previous {key} for Claude Code");
            }
            env.insert(key.to_string(), Value::String(String::new()));
        }
    }
    merged
}

// ---------- Codex ----------

/// Connection keys (`model_provider`/`model_providers`/`model`) + the MCP projection keys
/// (`mcp_servers`, and the historically wrong `mcp`). The SSOT for MCP servers is the
/// mcp_servers table in the DB: `McpService::sync_enabled_for_app` at the end of
/// `ProviderService::switch` re-projects it into config.toml before restore() reads written,
/// so the `mcp_servers` in previous is only a stale projection snapshot from before the
/// switch, and pasting it back would resurrect MCP servers the user already disabled or
/// deleted in the app.
const CODEX_MANAGED_KEYS: [&str; 8] = [
    "model_provider",
    "model_providers",
    "model",
    "mcp_servers",
    "mcp",
    // Legacy top-level connection fields are still recognized by the upstream
    // reader/normalizer. Restoring them after a switch can reroute the built-in
    // provider or resurrect a credential that upstream intentionally removed.
    "openai_base_url",
    "base_url",
    "experimental_bearer_token",
];

pub(super) fn merge_codex_config(previous: &str, written: &str) -> Result<String, AppError> {
    let previous_doc = previous.parse::<DocumentMut>().map_err(|error| {
        AppError::new(ErrorCode::ConfigParseFailed, PRESERVE_FAILED).with_technical(truncate_tail(
            &redact_secrets(&format!("previous config.toml: {error}")),
            20,
            2000,
        ))
    })?;
    let mut written_doc = written.parse::<DocumentMut>().map_err(|error| {
        AppError::new(ErrorCode::ConfigParseFailed, PRESERVE_FAILED).with_technical(truncate_tail(
            &redact_secrets(&format!("written config.toml: {error}")),
            20,
            2000,
        ))
    })?;
    for (key, item) in previous_doc.iter() {
        if CODEX_MANAGED_KEYS.contains(&key) || written_doc.contains_key(key) {
            continue;
        }
        written_doc.insert(key, item.clone());
    }
    Ok(written_doc.to_string())
}

// ---------- Gemini CLI ----------

const GEMINI_CONNECTION_KEYS: [&str; 4] = [
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_GEMINI_BASE_URL",
    "GEMINI_MODEL",
];

pub(super) fn merge_gemini_env(
    previous: &HashMap<String, String>,
    written: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = written.clone();
    for (key, value) in previous {
        if !GEMINI_CONNECTION_KEYS.contains(&key.as_str())
            && !looks_like_credential(key, value)
            && !merged.contains_key(key)
        {
            merged.insert(key.clone(), value.clone());
        }
    }
    merged
}

// ---------- capture / restore ----------

pub(super) enum LivePreservation {
    None,
    Claude(Value),
    Codex(String),
    Gemini(HashMap<String, String>),
}

/// Never fails: if it cannot be read, nothing is preserved (the upstream switch reports its
/// own error). A missing Claude file counts as `{}`, so even the first switch writes the
/// empty strings that cancel the shell variables.
///
/// `std::fs::read_to_string` is used directly (rather than the `read_json_file` /
/// `read_gemini_env` convenience wrappers) to get at the raw `io::ErrorKind`: a missing file
/// (`NotFound`) is an expected state (the tool was never configured) and is silently treated
/// as "nothing to preserve"; any other read failure (permissions, a broken mount point, ...)
/// must likewise not block the switch, but is worth a log line, otherwise the user's settings
/// vanish silently with no clue to investigate.
pub(super) fn capture(tool: ToolId) -> LivePreservation {
    match tool {
        ToolId::ClaudeCode => {
            let path = get_claude_settings_path();
            LivePreservation::Claude(match std::fs::read_to_string(&path) {
                Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
                    log::warn!(
                        "live_preservation capture: Claude Code settings.json did not parse as JSON, preserving nothing"
                    );
                    json!({})
                }),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
                Err(error) => {
                    log::warn!(
                        "live_preservation capture: reading Claude Code settings.json failed ({:?}), preserving nothing",
                        error.kind()
                    );
                    json!({})
                }
            })
        }
        ToolId::Codex => {
            let path = crate::codex_config::get_codex_config_path();
            match std::fs::read_to_string(&path) {
                Ok(text) => LivePreservation::Codex(text),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    LivePreservation::None
                }
                Err(error) => {
                    log::warn!(
                        "live_preservation capture: reading Codex config.toml failed ({:?}), preserving nothing",
                        error.kind()
                    );
                    LivePreservation::None
                }
            }
        }
        ToolId::GeminiCli => {
            let path = get_gemini_env_path();
            match std::fs::read_to_string(&path) {
                Ok(text) => LivePreservation::Gemini(crate::gemini_config::parse_env_file(&text)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    LivePreservation::None
                }
                Err(error) => {
                    log::warn!(
                        "live_preservation capture: reading Gemini .env failed ({:?}), preserving nothing",
                        error.kind()
                    );
                    LivePreservation::None
                }
            }
        }
        ToolId::OpenCode
        | ToolId::GrokBuild
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => LivePreservation::None,
    }
}

impl LivePreservation {
    /// Called after upstream has written live: re-read the file, merge, and atomically write
    /// back only when something changed. An unreadable file (`NotFound`) counts as "this
    /// switch did not write live at all" and is not a preservation failure — there is nothing
    /// to merge, so it is let through.
    ///
    /// There is no separate backup file: upstream's own write is already "temp file + atomic
    /// rename", so even if the merge write here fails, what remains on disk is the copy
    /// upstream just wrote containing nothing but connection information, not a half-written
    /// broken file. That is simpler than managing another backup artifact, and a failure is
    /// still reported explicitly via `settingsPreserveFailed` rather than silently losing
    /// user data.
    pub(super) fn restore(&self) -> Result<(), AppError> {
        match self {
            Self::None => Ok(()),
            Self::Claude(previous) => {
                let path = get_claude_settings_path();
                match std::fs::metadata(&path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                    Err(error) => {
                        return Err(preserve_failed(crate::error::AppError::io(&path, error)))
                    }
                    Ok(_) => {}
                }
                let written = read_json_file::<Value>(&path).map_err(preserve_failed)?;
                let merged = merge_claude_settings(previous, &written);
                if merged != written {
                    write_json_file(&path, &merged).map_err(preserve_failed)?;
                }
                Ok(())
            }
            Self::Codex(previous) => {
                let path = crate::codex_config::get_codex_config_path();
                let written = match std::fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(());
                    }
                    Err(error) => {
                        return Err(preserve_failed(crate::error::AppError::io(&path, error)));
                    }
                };
                let merged = merge_codex_config(previous, &written)?;
                if merged != written {
                    crate::codex_config::write_codex_live_config_atomic(Some(&merged))
                        .map_err(preserve_failed)?;
                }
                Ok(())
            }
            Self::Gemini(previous) => {
                let path = get_gemini_env_path();
                match std::fs::metadata(&path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                    Err(error) => {
                        return Err(preserve_failed(crate::error::AppError::io(&path, error)))
                    }
                    Ok(_) => {}
                }
                let written = read_gemini_env().map_err(preserve_failed)?;
                let merged = merge_gemini_env(previous, &written);
                if merged != written {
                    write_gemini_env_atomic(&merged).map_err(preserve_failed)?;
                }
                Ok(())
            }
        }
    }
}

fn preserve_failed(error: crate::error::AppError) -> AppError {
    // `upstream_detail` already redacts and truncates; redacting its output again
    // would be dead work (`upstream_detail` in `provider.rs` calls `redact_secrets`
    // internally).
    AppError::new(ErrorCode::ConfigWriteFailed, PRESERVE_FAILED)
        .with_technical(truncate_tail(&super::upstream_detail(&error), 20, 2000))
        .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
#[path = "live_preservation_tests.rs"]
mod tests;
