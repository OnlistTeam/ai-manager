// Unit tests for the pure merge functions in live_preservation.rs, split out to keep
// the production module under the AI_RULES.md < 500-line Rust Service cap.
use std::collections::HashMap;

use serde_json::json;

use super::{merge_claude_settings, merge_codex_config, merge_gemini_env};

#[test]
fn claude_keeps_everything_that_is_not_a_connection_key_and_cancels_shell_overrides() {
    let previous = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://relay.example.test",
            "ANTHROPIC_AUTH_TOKEN": "sk-relay",
            "CLAUDE_CODE_USE_BEDROCK": "1",
            "DISABLE_AUTO_COMPACT": "1",
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1"
        },
        "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "afplay x"}]}]},
        "statusLine": {"type": "command", "command": "npx ccstatusline"},
        "permissions": {"defaultMode": "bypassPermissions"},
        "model": "opus"
    });
    let written = json!({"env": {}});
    let merged = merge_claude_settings(&previous, &written);
    let env = merged.get("env").unwrap();
    assert_eq!(env.get("ANTHROPIC_BASE_URL"), Some(&json!("")));
    assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN"), Some(&json!("")));
    assert_eq!(env.get("ANTHROPIC_API_KEY"), Some(&json!("")));
    assert_eq!(env.get("CLAUDE_CODE_USE_BEDROCK"), None);
    assert_eq!(env.get("DISABLE_AUTO_COMPACT"), Some(&json!("1")));
    assert_eq!(
        env.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"),
        Some(&json!("1"))
    );
    assert_eq!(merged.get("hooks"), previous.get("hooks"));
    assert_eq!(merged.get("statusLine"), previous.get("statusLine"));
    assert_eq!(merged.get("permissions"), previous.get("permissions"));
    assert_eq!(merged.get("model"), Some(&json!("opus")));
}

#[test]
fn claude_written_values_always_win() {
    let previous = json!({"env": {"API_TIMEOUT_MS": "1000"}, "model": "opus"});
    let written = json!({
        "env": {"ANTHROPIC_BASE_URL": "https://b.example.test", "ANTHROPIC_API_KEY": "sk-b", "API_TIMEOUT_MS": "3000"},
        "model": "sonnet"
    });
    let merged = merge_claude_settings(&previous, &written);
    let env = merged.get("env").unwrap();
    assert_eq!(
        env.get("ANTHROPIC_BASE_URL"),
        Some(&json!("https://b.example.test"))
    );
    assert_eq!(env.get("ANTHROPIC_API_KEY"), Some(&json!("sk-b")));
    assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN"), Some(&json!("")));
    assert_eq!(env.get("API_TIMEOUT_MS"), Some(&json!("3000")));
    assert_eq!(merged.get("model"), Some(&json!("sonnet")));
}

#[test]
fn claude_handles_a_missing_env_block_on_either_side() {
    let merged = merge_claude_settings(&json!({"hooks": {}}), &json!({}));
    assert_eq!(merged.get("hooks"), Some(&json!({})));
    assert_eq!(
        merged
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_BASE_URL")),
        Some(&json!(""))
    );
}

#[test]
fn claude_never_copies_back_credential_or_upstream_stripped_top_level_keys() {
    let previous = json!({
        "apiKeyHelper": "/usr/local/bin/old-key-helper",
        "awsAuthRefresh": "aws sso login --profile old",
        "awsCredentialExport": "old-export-cmd",
        "apiFormat": "anthropic",
        "api_format": "anthropic",
        "openrouterCompatMode": true,
        "openrouter_compat_mode": true,
        "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "afplay x"}]}]}
    });
    let written = json!({"env": {}});
    let merged = merge_claude_settings(&previous, &written);
    assert_eq!(merged.get("apiKeyHelper"), None);
    assert_eq!(merged.get("awsAuthRefresh"), None);
    assert_eq!(merged.get("awsCredentialExport"), None);
    assert_eq!(merged.get("apiFormat"), None);
    assert_eq!(merged.get("api_format"), None);
    assert_eq!(merged.get("openrouterCompatMode"), None);
    assert_eq!(merged.get("openrouter_compat_mode"), None);
    assert_eq!(merged.get("hooks"), previous.get("hooks"));
}

#[test]
fn codex_keeps_user_tables_and_drops_the_old_provider_tables() {
    let previous = r#"# my notes
model = "gpt-5-relay"
model_provider = "relay"
sandbox_mode = "workspace-write"
model_reasoning_effort = "high"

[model_providers.relay]
base_url = "https://relay.example.test/v1"

[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem"]
"#;
    let written = r#"model = "gpt-5"
model_provider = "official"

[model_providers.official]
base_url = "https://api.openai.com/v1"
"#;
    let merged = merge_codex_config(previous, written).expect("merge");
    let doc = merged.parse::<toml::Value>().expect("valid toml");
    assert_eq!(
        doc.get("model").and_then(toml::Value::as_str),
        Some("gpt-5")
    );
    assert_eq!(
        doc.get("model_provider").and_then(toml::Value::as_str),
        Some("official")
    );
    assert!(doc
        .get("model_providers")
        .and_then(|p| p.get("relay"))
        .is_none());
    assert!(doc
        .get("model_providers")
        .and_then(|p| p.get("official"))
        .is_some());
    assert_eq!(
        doc.get("sandbox_mode").and_then(toml::Value::as_str),
        Some("workspace-write")
    );
    assert_eq!(
        doc.get("model_reasoning_effort")
            .and_then(toml::Value::as_str),
        Some("high")
    );
    // mcp_servers is a projection of the mcp_servers table in the DB, and the stale projection in
    // previous must not be pasted back — doing so would resurrect MCP servers already deleted or
    // disabled in the app.
    assert!(doc.get("mcp_servers").is_none());
}

#[test]
fn codex_keeps_the_written_mcp_servers_projection_instead_of_the_previous_one() {
    let previous = r#"model_provider = "relay"

[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem"]
"#;
    let written = r#"model_provider = "official"

[mcp_servers.git]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-git"]
"#;
    let merged = merge_codex_config(previous, written).expect("merge");
    let doc = merged.parse::<toml::Value>().expect("valid toml");
    assert!(doc.get("mcp_servers").and_then(|m| m.get("git")).is_some());
    assert!(doc
        .get("mcp_servers")
        .and_then(|m| m.get("filesystem"))
        .is_none());
}

#[test]
fn codex_rejects_unparseable_input_instead_of_guessing() {
    assert!(merge_codex_config("model = ", "model = \"x\"").is_err());
    assert!(merge_codex_config("model = \"x\"", "[[broken").is_err());
}

#[test]
fn codex_does_not_restore_legacy_routing_or_credentials_after_switch() {
    let previous = r#"openai_base_url = "https://old.example.test/v1"
base_url = "https://legacy.example.test/v1"
experimental_bearer_token = "sk-old-test-token"
sandbox_mode = "workspace-write"
"#;
    for written in ["", "model_provider = \"openai\"\n"] {
        let merged = merge_codex_config(previous, written).expect("merge");
        let doc = merged.parse::<toml::Value>().expect("valid TOML");
        for key in ["openai_base_url", "base_url", "experimental_bearer_token"] {
            assert!(
                doc.get(key).is_none(),
                "restored old connection field: {key}"
            );
        }
        assert_eq!(doc["sandbox_mode"].as_str(), Some("workspace-write"));
    }
    let written = "openai_base_url = \"https://new.example.test/v1\"\nexperimental_bearer_token = \"sk-new-test-token\"\n";
    let merged = merge_codex_config(previous, written).expect("merge");
    let doc = merged.parse::<toml::Value>().unwrap();
    assert_eq!(
        doc["openai_base_url"].as_str(),
        Some("https://new.example.test/v1")
    );
    assert_eq!(
        doc["experimental_bearer_token"].as_str(),
        Some("sk-new-test-token")
    );
}

#[test]
fn codex_parse_errors_never_carry_the_secret_from_the_broken_line() {
    let secret = "sk-live-secret-0123456789ABCDEF";
    let previous = format!("[model_providers.relay]\nexperimental_bearer_token = \"{secret}\n");
    let written = "model = \"gpt-5\"\nmodel_provider = \"official\"\n";
    let error = merge_codex_config(&previous, written).expect_err("unterminated string");
    assert_eq!(error.message_key, "error.provider.settingsPreserveFailed");
    let technical = error.technical_message.expect("technical detail present");
    assert!(
        !technical.contains(secret),
        "technical_message leaked the secret: {technical}"
    );
}

#[test]
fn codex_arrays_of_tables_and_dotted_keys_survive_the_merge() {
    let previous = r#"model_provider = "relay"

network.proxy = "http://127.0.0.1:7890"

[[profiles]]
name = "a"

[[profiles]]
name = "b"
"#;
    let written = "model = \"gpt-5\"\nmodel_provider = \"official\"\n";
    let merged = merge_codex_config(previous, written).expect("merge");
    let doc = merged.parse::<toml::Value>().expect("valid toml");
    assert_eq!(
        doc.get("network")
            .and_then(|network| network.get("proxy"))
            .and_then(toml::Value::as_str),
        Some("http://127.0.0.1:7890")
    );
    assert_eq!(
        doc.get("profiles")
            .and_then(toml::Value::as_array)
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        doc.get("model_provider").and_then(toml::Value::as_str),
        Some("official")
    );
}

#[test]
fn gemini_keeps_project_variables_and_drops_old_connection_keys() {
    let previous: HashMap<String, String> = HashMap::from([
        ("GEMINI_API_KEY".into(), "k-old".into()),
        (
            "GOOGLE_GEMINI_BASE_URL".into(),
            "https://old.example.test".into(),
        ),
        ("GOOGLE_CLOUD_PROJECT".into(), "my-project".into()),
        ("GEMINI_SANDBOX".into(), "docker".into()),
    ]);
    let written: HashMap<String, String> =
        HashMap::from([("GEMINI_API_KEY".into(), "k-new".into())]);
    let merged = merge_gemini_env(&previous, &written);
    assert_eq!(
        merged.get("GEMINI_API_KEY").map(String::as_str),
        Some("k-new")
    );
    assert_eq!(merged.get("GOOGLE_GEMINI_BASE_URL"), None);
    assert_eq!(
        merged.get("GOOGLE_CLOUD_PROJECT").map(String::as_str),
        Some("my-project")
    );
    assert_eq!(
        merged.get("GEMINI_SANDBOX").map(String::as_str),
        Some("docker")
    );
}

#[test]
fn claude_never_carries_credential_shaped_env_across_a_switch() {
    let previous = json!({
        "env": {
            "AWS_SECRET_ACCESS_KEY": "aws-secret",
            "MY_PROXY_TOKEN": "abc123",
            "GOOGLE_APPLICATION_CREDENTIALS": "/tmp/gcp.json",
            "MY_CUSTOM_VAR": "sk-ant-abcdefghijklmnop",
            "DISABLE_AUTO_COMPACT": "1",
            "API_TIMEOUT_MS": "1000",
            "MAX_THINKING_TOKENS": "30000",
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "300000"
        }
    });
    let written = json!({"env": {}});
    let merged = merge_claude_settings(&previous, &written);
    let env = merged.get("env").unwrap();
    assert_eq!(env.get("AWS_SECRET_ACCESS_KEY"), None);
    assert_eq!(env.get("MY_PROXY_TOKEN"), None);
    assert_eq!(env.get("GOOGLE_APPLICATION_CREDENTIALS"), None);
    assert_eq!(env.get("MY_CUSTOM_VAR"), None);
    assert_eq!(env.get("DISABLE_AUTO_COMPACT"), Some(&json!("1")));
    assert_eq!(env.get("API_TIMEOUT_MS"), Some(&json!("1000")));
    // The plural `_TOKENS` is a unit of quantity, not a credential — it must not be caught by the singular `_TOKEN` rule.
    assert_eq!(env.get("MAX_THINKING_TOKENS"), Some(&json!("30000")));
    assert_eq!(
        env.get("CLAUDE_CODE_MAX_OUTPUT_TOKENS"),
        Some(&json!("300000"))
    );
}

#[test]
fn claude_written_credentials_still_win() {
    let previous = json!({"env": {"MY_PROXY_TOKEN": "stale"}});
    let written = json!({"env": {"MY_PROXY_TOKEN": "fresh"}});
    let merged = merge_claude_settings(&previous, &written);
    let env = merged.get("env").unwrap();
    assert_eq!(env.get("MY_PROXY_TOKEN"), Some(&json!("fresh")));
}

#[test]
fn gemini_never_carries_credential_shaped_env_across_a_switch() {
    let previous: HashMap<String, String> = HashMap::from([
        (
            "GOOGLE_APPLICATION_CREDENTIALS".into(),
            "/tmp/gcp.json".into(),
        ),
        ("MY_PROXY_TOKEN".into(), "abc".into()),
        ("GOOGLE_CLOUD_PROJECT".into(), "my-project".into()),
        ("GEMINI_SANDBOX".into(), "docker".into()),
    ]);
    let written: HashMap<String, String> =
        HashMap::from([("GEMINI_API_KEY".into(), "k-new".into())]);
    let merged = merge_gemini_env(&previous, &written);
    assert_eq!(merged.get("GOOGLE_APPLICATION_CREDENTIALS"), None);
    assert_eq!(merged.get("MY_PROXY_TOKEN"), None);
    assert_eq!(
        merged.get("GOOGLE_CLOUD_PROJECT").map(String::as_str),
        Some("my-project")
    );
    assert_eq!(
        merged.get("GEMINI_SANDBOX").map(String::as_str),
        Some("docker")
    );
}
