//! Whether a tool is signed in to its own account right now (ADR-0060).
//!
//! The tool is asked in its own words, `claude auth status --json` and
//! `codex login status`, anchored to the installation a launch would use.
//! Only the state, the account's email and the plan leave this module;
//! command output is parsed here and never logged or passed on.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::commands::misc::wsl_distro_for_tool;
use crate::compat::ccswitch::install_probe::probe;
use crate::compat::ccswitch::lifecycle_specs::{anchored, tool_program};
use crate::compat::ccswitch::tools::tool_id_to_cli_name;
use crate::domain::{ToolId, ToolLoginState, ToolLoginStatus};
use crate::platform::executor::{CommandExecutor, CommandOutput, SystemExecutor};

use super::ProviderStore;

/// Both commands read local files only and answer in well under a second.
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);

/// The command that reports the sign-in, for the tools that have one.
///
/// Exhaustive on purpose (no `_ =>`): a new `ToolId` forces a conscious call.
fn status_args(tool: ToolId) -> Option<&'static [&'static str]> {
    match tool {
        ToolId::ClaudeCode => Some(&["auth", "status", "--json"]),
        ToolId::Codex => Some(&["login", "status"]),
        ToolId::GeminiCli
        | ToolId::GrokBuild
        | ToolId::OpenCode
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => None,
    }
}

pub(super) async fn read(store: &ProviderStore, tool: ToolId) -> ToolLoginStatus {
    // Gemini CLI and Grok Build have no status command; their sign-in is a
    // file of their own, read for its presence and the account's name only
    // (ADR-0061).
    match tool {
        ToolId::GeminiCli => return gemini_status(&crate::gemini_config::get_gemini_dir()),
        ToolId::GrokBuild => return grok_status(&crate::grok_config::get_grok_config_dir()),
        _ => {}
    }
    let Some(args) = status_args(tool) else {
        return unknown();
    };
    // Read before any await: the store is not held across the probe.
    let stashed = tool == ToolId::Codex && codex_login_stashed(store);
    let Some(output) = run_status(tool, args).await else {
        return unknown();
    };
    match tool {
        ToolId::ClaudeCode => {
            let config = std::fs::read_to_string(crate::config::get_claude_mcp_path()).ok();
            claude_status(&output.stdout, config.as_deref())
        }
        _ => codex_status(&output, stashed),
    }
}

fn unknown() -> ToolLoginStatus {
    ToolLoginStatus::of(ToolLoginState::Unknown)
}

async fn run_status(tool: ToolId, args: &[&str]) -> Option<CommandOutput> {
    let name = tool_id_to_cli_name(tool);
    // A tool inside WSL keeps its sign-in in the Linux home; there is nothing
    // here to anchor to.
    if wsl_distro_for_tool(name).is_some() {
        return None;
    }
    let probed = probe(tool).await.ok()?;
    let entry = probed.entry.as_ref().filter(|entry| entry.runnable)?;
    let args = args.iter().map(|arg| arg.to_string()).collect();
    // The binary's own directory goes first on PATH so an npm launcher
    // (`#!/usr/bin/env node`) finds the Node it was installed with.
    let spec = anchored(tool_program(tool), entry, name, args, &probed, true)?
        .with_timeout(STATUS_TIMEOUT);
    SystemExecutor.execute(spec).await.ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeAuthStatus {
    #[serde(default)]
    logged_in: bool,
    auth_method: Option<String>,
    email: Option<String>,
    subscription_type: Option<String>,
}

/// `authMethod` follows the settings in force, so while Claude Code points
/// at a relay it names the relay's token. Only `claude.ai` and `none` say
/// anything about the `/login` account; otherwise the account `/login` left
/// in the Claude config file answers.
fn claude_status(stdout: &str, claude_config: Option<&str>) -> ToolLoginStatus {
    let Ok(status) = serde_json::from_str::<ClaudeAuthStatus>(stdout) else {
        return unknown();
    };
    match status.auth_method.as_deref() {
        Some("claude.ai") if status.logged_in => ToolLoginStatus {
            state: ToolLoginState::SignedIn,
            account: non_empty(status.email),
            plan: non_empty(status.subscription_type),
        },
        Some("none") | None if !status.logged_in => ToolLoginStatus::of(ToolLoginState::SignedOut),
        _ => claude_saved_account(claude_config),
    }
}

fn claude_saved_account(claude_config: Option<&str>) -> ToolLoginStatus {
    let Some(config) = claude_config else {
        return ToolLoginStatus::of(ToolLoginState::SignedOut);
    };
    let Ok(config) = serde_json::from_str::<Value>(config) else {
        return unknown();
    };
    match config
        .get("oauthAccount")
        .filter(|account| account.is_object())
    {
        Some(account) => ToolLoginStatus {
            state: ToolLoginState::SignedIn,
            account: non_empty(
                account
                    .get("emailAddress")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            ),
            plan: None,
        },
        None => ToolLoginStatus::of(ToolLoginState::SignedOut),
    }
}

fn read_json(path: &std::path::Path) -> Option<Result<Value, ()>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes).map_err(|_| ())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => Some(Err(())),
    }
}

/// Gemini CLI's Google sign-in: a refresh token in `oauth_creds.json`, the
/// account named active in `google_accounts.json`.
fn gemini_status(dir: &std::path::Path) -> ToolLoginStatus {
    let credentials = match read_json(&dir.join("oauth_creds.json")) {
        None => return ToolLoginStatus::of(ToolLoginState::SignedOut),
        Some(Err(())) => return unknown(),
        Some(Ok(credentials)) => credentials,
    };
    let signed_in = credentials
        .get("refresh_token")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.is_empty());
    if !signed_in {
        return ToolLoginStatus::of(ToolLoginState::SignedOut);
    }
    let account = read_json(&dir.join("google_accounts.json"))
        .and_then(Result::ok)
        .and_then(|accounts| {
            accounts
                .get("active")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    ToolLoginStatus {
        state: ToolLoginState::SignedIn,
        account: non_empty(account),
        plan: None,
    }
}

/// Grok Build's sign-in: an entry holding a key in its `auth.json`.
fn grok_status(dir: &std::path::Path) -> ToolLoginStatus {
    let all = match read_json(&dir.join("auth.json")) {
        None => return ToolLoginStatus::of(ToolLoginState::SignedOut),
        Some(Err(())) => return unknown(),
        Some(Ok(all)) => all,
    };
    let Some(entries) = all.as_object() else {
        return unknown();
    };
    let signed_in = entries.values().find(|entry| {
        entry
            .get("key")
            .and_then(Value::as_str)
            .is_some_and(|key| !key.is_empty())
    });
    match signed_in {
        Some(entry) => ToolLoginStatus {
            state: ToolLoginState::SignedIn,
            account: non_empty(
                entry
                    .get("email")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            ),
            plan: None,
        },
        None => ToolLoginStatus::of(ToolLoginState::SignedOut),
    }
}

/// Codex prints one line on stderr. Credentials other than ChatGPT and an
/// API key (Bedrock, workload identity, access tokens) are neither the
/// subscription nor a key the user typed, so they read as unknown.
fn codex_status(output: &CommandOutput, stashed: bool) -> ToolLoginStatus {
    let text = format!("{}\n{}", output.stderr, output.stdout);
    let line = |prefix: &str| text.lines().any(|line| line.trim().starts_with(prefix));
    let state = if line("Logged in using ChatGPT") {
        ToolLoginState::SignedIn
    } else if line("Logged in using an API key") {
        ToolLoginState::ApiKey
    } else if line("Not logged in") {
        // Switching to another endpoint moves the ChatGPT login out of
        // `auth.json` into the official record, and switching back writes it
        // again (ADR-0040), so it is still there to come back to.
        if stashed {
            ToolLoginState::SignedIn
        } else {
            ToolLoginState::SignedOut
        }
    } else {
        ToolLoginState::Unknown
    };
    ToolLoginStatus::of(state)
}

/// Whether an official Codex record holds ChatGPT tokens while another entry
/// is in use. Only their presence is read.
fn codex_login_stashed(store: &ProviderStore) -> bool {
    let Ok((raw, current)) = store.raw_inventory(ToolId::Codex) else {
        return false;
    };
    raw.values().any(|entry| {
        entry.category.as_deref() == Some("official")
            && entry.id != current
            && holds_chatgpt_tokens(&entry.settings_config)
    })
}

fn holds_chatgpt_tokens(settings: &Value) -> bool {
    settings
        .pointer("/auth/tokens")
        .and_then(Value::as_object)
        .is_some_and(|tokens| !tokens.is_empty())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "tool_login_status_tests.rs"]
mod tests;
