//! The display line and portability hint for one MCP connection (ADR-0062).
//!
//! The upstream unified spec is Claude's shape (`command`/`args`/`env`, or
//! `url`/`headers`), and every tool adapter converts into it on import, so one
//! reader serves managed rows and found ones alike. Only the command line or
//! address leaves this module; environment and header values are read to look
//! for `${…}` and nothing else.

use serde_json::Value;

use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::extension_detail::{
    command_line, has_env_reference, is_relative_path, remote_address,
};
use crate::domain::{ExtensionPortability, ExtensionScope, PortabilityReason, ToolId};

/// Whether a tool fills in `${NAME}` from the environment when it reads its
/// MCP configuration. None of the upstream adapters in `src-tauri/src/mcp/`
/// rewrite these references, so the text reaches each tool as written and
/// only the tool itself can expand it:
///
/// - Claude Code expands `${VAR}` and `${VAR:-default}` in `command`, `args`,
///   `env`, `url` and `headers` (code.claude.com/docs/en/mcp).
/// - Gemini CLI resolves `$VAR` / `${VAR}` in any `settings.json` string.
/// - Grok Build expands `${VAR}` in `url`, `command`, `args`, `env`, `headers`
///   (docs.x.ai/build/settings/reference).
/// - Hermes interpolates `${VAR}` in its `mcp_servers` section.
/// - Codex reads TOML literally (it has `env_vars` / `bearer_token_env_var`
///   instead), OpenCode's syntax is `{env:VAR}`, and Claude Desktop passes the
///   text through, so all three would start the server with the literal text.
///
/// Tools not listed are treated as not expanding, which only adds a hint.
fn expands_env_references(tool: ToolId) -> bool {
    matches!(
        tool,
        ToolId::ClaudeCode | ToolId::GeminiCli | ToolId::GrokBuild | ToolId::Hermes
    )
}

fn env_expanding_scopes() -> Vec<ExtensionScope> {
    ToolId::ALL
        .into_iter()
        .filter(|tool| capabilities_for(*tool).can_manage_mcp && expands_env_references(*tool))
        .map(ExtensionScope::tool)
        .collect()
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn values_of(spec: &Value, key: &str) -> Vec<String> {
    spec.get(key)
        .and_then(Value::as_object)
        .map(|object| object.values().filter_map(text).collect())
        .unwrap_or_default()
}

fn has_working_folder(spec: &Value) -> bool {
    ["cwd", "working_dir"].iter().any(|key| {
        spec.get(*key)
            .and_then(Value::as_str)
            .is_some_and(|folder| !folder.trim().is_empty())
    })
}

/// A spec is remote when it says so, or when it has an address and no
/// command (a found entry may omit `type`).
fn is_remote(spec: &Value) -> bool {
    match spec.get("type").and_then(Value::as_str) {
        Some("http" | "sse") => true,
        Some(_) => false,
        None => spec.get("command").is_none() && spec.get("url").is_some(),
    }
}

/// The row's detail line and, when it applies, the hint that the connection
/// depends on where it was set up.
pub(in crate::compat::ccswitch::extension) fn describe(
    spec: &Value,
) -> (Option<String>, Option<ExtensionPortability>) {
    let home = crate::config::get_home_dir();
    let home = home.to_string_lossy();

    if is_remote(spec) {
        let url = spec.get("url").and_then(Value::as_str).unwrap_or_default();
        let detail = (!url.trim().is_empty()).then(|| remote_address(url));
        let mut values = values_of(spec, "headers");
        values.extend(values_of(spec, "http_headers"));
        values.push(url.to_string());
        let portability = values
            .iter()
            .any(|value| has_env_reference(value))
            .then(|| ExtensionPortability {
                reason: PortabilityReason::EnvReference,
                works_in: env_expanding_scopes(),
            });
        return (detail, portability);
    }

    let command = spec
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let arguments = spec
        .get("args")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(text).collect::<Vec<_>>())
        .unwrap_or_default();
    let detail = (!command.is_empty()).then(|| command_line(&command, &arguments, &home));

    let relative = !has_working_folder(spec)
        && (is_relative_path(&command) || arguments.iter().any(|value| is_relative_path(value)));
    if relative {
        return (
            detail,
            Some(ExtensionPortability {
                reason: PortabilityReason::RelativePath,
                works_in: Vec::new(),
            }),
        );
    }

    let mut values = values_of(spec, "env");
    values.push(command);
    values.extend(arguments);
    let portability = values
        .iter()
        .any(|value| has_env_reference(value))
        .then(|| ExtensionPortability {
            reason: PortabilityReason::EnvReference,
            works_in: env_expanding_scopes(),
        });
    (detail, portability)
}

#[cfg(test)]
mod tests {
    use super::{describe, env_expanding_scopes};
    use crate::domain::{ExtensionScope, PortabilityReason, ToolId};
    use serde_json::json;

    #[test]
    fn a_local_server_shows_its_command_line_and_never_its_environment() {
        let (detail, portability) = describe(&json!({
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@playwright/mcp@latest", "--api-key", "abc123"],
            "env": {"GITHUB_TOKEN": "ghp_should_not_show"}
        }));
        let detail = detail.expect("detail");
        assert_eq!(detail, "npx -y @playwright/mcp@latest --api-key ••••");
        assert!(!detail.contains("ghp_") && !detail.contains("GITHUB_TOKEN"));
        assert!(portability.is_none());
    }

    #[test]
    fn a_remote_server_shows_its_address_and_never_its_headers() {
        let (detail, portability) = describe(&json!({
            "type": "http",
            "url": "https://mcp.example.com/mcp",
            "headers": {"Authorization": "Bearer abc"}
        }));
        assert_eq!(detail.as_deref(), Some("https://mcp.example.com/mcp"));
        assert!(portability.is_none());

        // A found entry may omit the type.
        let (detail, _) = describe(&json!({"url": "https://mcp.example.com/sse"}));
        assert_eq!(detail.as_deref(), Some("https://mcp.example.com/sse"));
    }

    #[test]
    fn a_relative_path_without_a_working_folder_is_flagged_for_every_app() {
        let (_, portability) = describe(&json!({
            "command": "node",
            "args": ["./dist/index.js"]
        }));
        let portability = portability.expect("relative path is flagged");
        assert_eq!(portability.reason, PortabilityReason::RelativePath);
        assert!(portability.works_in.is_empty());

        for spec in [
            json!({"command": "node", "args": ["./dist/index.js"], "cwd": "/srv/mcp"}),
            json!({"command": "node", "args": ["./dist/index.js"], "working_dir": "/srv"}),
            json!({"command": "npx", "args": ["-y", "@scope/pkg@1.0"]}),
        ] {
            assert!(describe(&spec).1.is_none(), "{spec}");
        }
    }

    #[test]
    fn an_env_reference_is_safe_only_in_the_apps_that_expand_it() {
        for spec in [
            json!({"command": "npx", "args": ["server"], "env": {"TOKEN": "${MY_TOKEN}"}}),
            json!({"command": "${HOME}/bin/server"}),
            json!({"type": "sse", "url": "https://x.test/sse", "headers": {"A": "Bearer ${T}"}}),
        ] {
            let portability = describe(&spec).1.expect("env reference flagged");
            assert_eq!(
                portability.reason,
                PortabilityReason::EnvReference,
                "{spec}"
            );
            assert_eq!(portability.works_in, env_expanding_scopes());
        }

        let works_in = env_expanding_scopes();
        assert!(works_in.contains(&ExtensionScope::tool(ToolId::ClaudeCode)));
        assert!(works_in.contains(&ExtensionScope::tool(ToolId::GeminiCli)));
        assert!(!works_in.contains(&ExtensionScope::tool(ToolId::Codex)));
        assert!(!works_in.contains(&ExtensionScope::tool(ToolId::OpenCode)));
        assert!(!works_in
            .iter()
            .any(|scope| scope.desktop_app_id().is_some()));
    }
}
