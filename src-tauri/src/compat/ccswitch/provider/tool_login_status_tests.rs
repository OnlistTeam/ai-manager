use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::database::Database;
use crate::store::AppState;

fn output(stdout: &str, stderr: &str, success: bool) -> CommandOutput {
    CommandOutput {
        success,
        exit_code: Some(if success { 0 } else { 1 }),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

fn signed_in(account: Option<&str>, plan: Option<&str>) -> ToolLoginStatus {
    ToolLoginStatus {
        state: ToolLoginState::SignedIn,
        account: account.map(str::to_string),
        plan: plan.map(str::to_string),
    }
}

const CLAUDE_AI: &str = r#"{
  "loggedIn": true,
  "authMethod": "claude.ai",
  "apiProvider": "firstParty",
  "email": "someone@example.com",
  "orgId": "org",
  "orgName": "Someone's Organization",
  "subscriptionType": "max"
}"#;

const SIGNED_OUT: &str = r#"{
  "loggedIn": false,
  "authMethod": "none",
  "apiProvider": "firstParty"
}"#;

/// What Claude Code prints while its settings point at a relay.
const RELAY_TOKEN: &str = r#"{
  "loggedIn": true,
  "authMethod": "oauth_token",
  "apiProvider": "firstParty"
}"#;

const SAVED_ACCOUNT: &str = r#"{
  "numStartups": 3,
  "oauthAccount": { "emailAddress": "someone@example.com", "organizationName": "Org" }
}"#;

#[test]
fn a_claude_ai_sign_in_names_the_account_and_plan() {
    assert_eq!(
        claude_status(CLAUDE_AI, None),
        signed_in(Some("someone@example.com"), Some("max"))
    );
}

#[test]
fn claude_signed_out_is_signed_out_whatever_the_config_file_remembers() {
    assert_eq!(
        claude_status(SIGNED_OUT, Some(SAVED_ACCOUNT)).state,
        ToolLoginState::SignedOut
    );
}

#[test]
fn a_relay_token_is_never_read_as_the_subscription() {
    assert_eq!(
        claude_status(RELAY_TOKEN, Some(r#"{ "numStartups": 3 }"#)).state,
        ToolLoginState::SignedOut
    );
    assert_eq!(
        claude_status(RELAY_TOKEN, None).state,
        ToolLoginState::SignedOut
    );
}

#[test]
fn behind_a_relay_the_saved_login_account_answers() {
    assert_eq!(
        claude_status(RELAY_TOKEN, Some(SAVED_ACCOUNT)),
        signed_in(Some("someone@example.com"), None)
    );
}

#[test]
fn unreadable_claude_answers_are_unknown() {
    assert_eq!(
        claude_status("Not JSON", None).state,
        ToolLoginState::Unknown
    );
    assert_eq!(
        claude_status(RELAY_TOKEN, Some("{ broken")).state,
        ToolLoginState::Unknown
    );
}

#[test]
fn codex_reports_each_way_it_can_be_signed_in() {
    let cases = [
        ("Logged in using ChatGPT", true, ToolLoginState::SignedIn),
        (
            "Logged in using an API key - sk-proj-***ABCD",
            true,
            ToolLoginState::ApiKey,
        ),
        ("Not logged in", false, ToolLoginState::SignedOut),
        (
            "Logged in using Amazon Bedrock API key",
            true,
            ToolLoginState::Unknown,
        ),
        ("", false, ToolLoginState::Unknown),
    ];
    for (stderr, success, state) in cases {
        assert_eq!(
            codex_status(&output("", stderr, success), false).state,
            state,
            "{stderr}"
        );
    }
}

#[test]
fn a_chatgpt_login_moved_aside_by_a_switch_still_counts() {
    let status = codex_status(&output("", "Not logged in", false), true);
    assert_eq!(status, ToolLoginStatus::of(ToolLoginState::SignedIn));
}

#[test]
fn codex_status_never_carries_the_key_it_printed() {
    let status = codex_status(
        &output("", "Logged in using an API key - sk-proj-***ABCD", true),
        false,
    );
    assert_eq!(status.account, None);
    assert_eq!(status.plan, None);
}

#[test]
fn only_chatgpt_tokens_count_as_a_saved_login() {
    assert!(holds_chatgpt_tokens(
        &json!({ "auth": { "tokens": { "id_token": "x" } } })
    ));
    assert!(!holds_chatgpt_tokens(&json!({ "auth": { "tokens": {} } })));
    assert!(!holds_chatgpt_tokens(
        &json!({ "auth": { "OPENAI_API_KEY": "sk-x" } })
    ));
    assert!(!holds_chatgpt_tokens(&json!({ "auth": {}, "config": "" })));
}

#[test]
fn the_seeded_official_entry_alone_is_not_a_saved_login() {
    let store = ProviderStore {
        state: AppState::new(Arc::new(Database::memory().expect("memory database"))),
    };
    super::super::tool_login::restore(&store, ToolId::Codex).expect("seed");
    assert!(!codex_login_stashed(&store));
}

#[test]
fn only_claude_code_and_codex_can_be_asked() {
    assert!(status_args(ToolId::ClaudeCode).is_some());
    assert!(status_args(ToolId::Codex).is_some());
    for tool in [
        ToolId::GeminiCli,
        ToolId::GrokBuild,
        ToolId::OpenCode,
        ToolId::OpenClaw,
        ToolId::Hermes,
        ToolId::Pi,
    ] {
        assert!(status_args(tool).is_none(), "{tool:?}");
    }
}

#[test]
fn gemini_is_signed_in_by_its_google_refresh_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(gemini_status(dir.path()).state, ToolLoginState::SignedOut);
    std::fs::write(
        dir.path().join("oauth_creds.json"),
        r#"{"access_token":"a","refresh_token":"r"}"#,
    )
    .expect("creds");
    std::fs::write(
        dir.path().join("google_accounts.json"),
        r#"{"active":"someone@example.com","old":[]}"#,
    )
    .expect("accounts");
    assert_eq!(
        gemini_status(dir.path()),
        signed_in(Some("someone@example.com"), None)
    );
    std::fs::write(dir.path().join("oauth_creds.json"), "{ broken").expect("broken");
    assert_eq!(gemini_status(dir.path()).state, ToolLoginState::Unknown);
}

#[test]
fn grok_is_signed_in_by_an_entry_holding_a_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(grok_status(dir.path()).state, ToolLoginState::SignedOut);
    std::fs::write(
        dir.path().join("auth.json"),
        r#"{"https://auth.x.ai":{"key":"","email":"old@example.com"}}"#,
    )
    .expect("empty");
    assert_eq!(grok_status(dir.path()).state, ToolLoginState::SignedOut);
    std::fs::write(
        dir.path().join("auth.json"),
        r#"{"https://auth.x.ai":{"key":"k","email":"someone@example.com"}}"#,
    )
    .expect("auth");
    assert_eq!(
        grok_status(dir.path()),
        signed_in(Some("someone@example.com"), None)
    );
}
