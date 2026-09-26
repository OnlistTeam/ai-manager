//! Product-owned MCP installation input.
//!
//! The renderer sends a small, typed draft instead of a free-form MCP JSON
//! object. The draft is inbound-only: it deliberately implements neither
//! `Serialize` nor `Debug`, because environment variables and request headers
//! usually carry an API key (ADR-0047). Upstream format synthesis stays behind
//! the compatibility boundary.

use serde::Deserialize;
use url::{Host, Url};

use super::{AppError, ErrorCode};

const MAX_NAME_CHARS: usize = 80;
const MAX_DESCRIPTION_CHARS: usize = 240;
const MAX_COMMAND_CHARS: usize = 512;
const MAX_ARGUMENTS: usize = 64;
const MAX_ARGUMENT_CHARS: usize = 2_048;
const MAX_URL_BYTES: usize = 2_048;
const MAX_VARIABLES: usize = 64;
const MAX_VARIABLE_NAME_CHARS: usize = 128;
const MAX_VARIABLE_VALUE_CHARS: usize = 8_192;

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpInstallDraft {
    pub name: String,
    pub description: Option<String>,
    pub connection: McpConnectionDraft,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "transport", rename_all = "camelCase", deny_unknown_fields)]
pub enum McpConnectionDraft {
    Stdio {
        command: String,
        #[serde(default)]
        arguments: Vec<String>,
        #[serde(default)]
        env: Vec<McpVariableDraft>,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: Vec<McpVariableDraft>,
    },
    Sse {
        url: String,
        #[serde(default)]
        headers: Vec<McpVariableDraft>,
    },
}

/// One environment variable (local) or request header (remote). An ordered
/// list rather than a map so a repeated name reaches validation instead of
/// being dropped silently by the JSON decoder. The value is often a secret:
/// it never appears in an error, log line, or technical message.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpVariableDraft {
    pub name: String,
    pub value: String,
}

impl McpVariableDraft {
    pub fn normalized_name(&self) -> &str {
        self.name.trim()
    }

    pub fn normalized_value(&self) -> &str {
        self.value.trim()
    }
}

#[derive(Clone, Copy)]
enum VariableKind {
    Env,
    Header,
}

impl VariableKind {
    fn message_key(self) -> &'static str {
        match self {
            Self::Env => "error.mcp.envInvalid",
            Self::Header => "error.mcp.headersInvalid",
        }
    }

    fn valid_name(self, name: &str) -> bool {
        match self {
            // POSIX portable names: every tool that launches the server accepts them.
            Self::Env => {
                let mut characters = name.chars();
                characters
                    .next()
                    .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
                    && characters.all(|next| next.is_ascii_alphanumeric() || next == '_')
            }
            // RFC 9110 `token`.
            Self::Header => {
                !name.is_empty()
                    && name.chars().all(|next| {
                        next.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(next)
                    })
            }
        }
    }

    fn same_name(self, left: &str, right: &str) -> bool {
        match self {
            Self::Env => left == right,
            Self::Header => left.eq_ignore_ascii_case(right),
        }
    }
}

impl McpInstallDraft {
    pub fn validate(&self) -> Result<(), AppError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(invalid("error.mcp.nameRequired", "MCP name is empty"));
        }
        if name.chars().count() > MAX_NAME_CHARS {
            return Err(invalid("error.mcp.nameTooLong", "MCP name is too long"));
        }
        if has_control(name) {
            return Err(invalid(
                "error.mcp.nameInvalid",
                "MCP name contains control characters",
            ));
        }

        if self
            .description
            .as_deref()
            .is_some_and(|value| value.trim().chars().count() > MAX_DESCRIPTION_CHARS)
        {
            return Err(invalid(
                "error.mcp.descriptionTooLong",
                "MCP description is too long",
            ));
        }

        match &self.connection {
            McpConnectionDraft::Stdio {
                command,
                arguments,
                env,
            } => {
                validate_stdio(command, arguments)?;
                validate_variables(env, VariableKind::Env)
            }
            McpConnectionDraft::Http { url, headers }
            | McpConnectionDraft::Sse { url, headers } => {
                validate_remote_url(url)?;
                validate_variables(headers, VariableKind::Header)
            }
        }
    }

    pub fn normalized_name(&self) -> String {
        self.name.trim().to_string()
    }

    pub fn normalized_description(&self) -> Option<String> {
        let value = self
            .description
            .as_deref()?
            .split_whitespace()
            .collect::<Vec<_>>();
        if value.is_empty() {
            None
        } else {
            Some(value.join(" "))
        }
    }
}

fn validate_stdio(command: &str, arguments: &[String]) -> Result<(), AppError> {
    let command = command.trim();
    if command.is_empty() {
        return Err(invalid(
            "error.mcp.commandRequired",
            "MCP local command is empty",
        ));
    }
    if command.chars().count() > MAX_COMMAND_CHARS || has_control(command) {
        return Err(invalid(
            "error.mcp.commandInvalid",
            "MCP local command is not a safe single-line value",
        ));
    }
    if arguments.len() > MAX_ARGUMENTS {
        return Err(invalid(
            "error.mcp.argumentsInvalid",
            "MCP local command has too many arguments",
        ));
    }
    if arguments
        .iter()
        .any(|argument| argument.chars().count() > MAX_ARGUMENT_CHARS || has_control(argument))
    {
        return Err(invalid(
            "error.mcp.argumentsInvalid",
            "MCP local command has an invalid argument",
        ));
    }
    Ok(())
}

fn validate_variables(variables: &[McpVariableDraft], kind: VariableKind) -> Result<(), AppError> {
    // Technical messages name the rule that failed, never the variable: a
    // header name such as `X-Api-Key-sk-...` is rare, a value never belongs here.
    let failed = |technical: &'static str| invalid(kind.message_key(), technical);
    if variables.len() > MAX_VARIABLES {
        return Err(failed("MCP connection has too many variables"));
    }
    for (index, variable) in variables.iter().enumerate() {
        let name = variable.normalized_name();
        if name.chars().count() > MAX_VARIABLE_NAME_CHARS || !kind.valid_name(name) {
            return Err(failed(
                "MCP variable name is empty or has invalid characters",
            ));
        }
        let value = variable.normalized_value();
        if value.chars().count() > MAX_VARIABLE_VALUE_CHARS || has_control(value) {
            return Err(failed(
                "MCP variable value is too long or not a single line",
            ));
        }
        if variables[..index]
            .iter()
            .any(|earlier| kind.same_name(earlier.normalized_name(), name))
        {
            return Err(failed("MCP variable name is repeated"));
        }
    }
    Ok(())
}

fn validate_remote_url(raw: &str) -> Result<(), AppError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(invalid("error.mcp.urlRequired", "MCP URL is empty"));
    }
    if raw.len() > MAX_URL_BYTES || has_control(raw) {
        return Err(invalid("error.mcp.urlInvalid", "MCP URL is invalid"));
    }

    let parsed = Url::parse(raw)
        .map_err(|_| invalid("error.mcp.urlInvalid", "MCP URL could not be parsed"))?;
    if parsed.host().is_none() || parsed.fragment().is_some() {
        return Err(invalid(
            "error.mcp.urlInvalid",
            "MCP URL needs a host and cannot contain a fragment",
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(invalid(
            "error.mcp.urlCredentials",
            "MCP URL contains embedded credentials",
        ));
    }

    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback_host(parsed.host()) => Ok(()),
        "http" => Err(invalid(
            "error.mcp.urlInsecure",
            "remote MCP HTTP is allowed only for a loopback host",
        )),
        _ => Err(invalid(
            "error.mcp.urlInvalid",
            "MCP URL scheme is not HTTP or HTTPS",
        )),
    }
}

fn is_loopback_host(host: Option<Host<&str>>) -> bool {
    match host {
        Some(Host::Domain(value)) => {
            value.eq_ignore_ascii_case("localhost")
                || value.to_ascii_lowercase().ends_with(".localhost")
        }
        Some(Host::Ipv4(value)) => value.is_loopback(),
        Some(Host::Ipv6(value)) => value.is_loopback(),
        None => false,
    }
}

fn has_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

fn invalid(message_key: &'static str, technical: &'static str) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, message_key)
        .with_technical(technical)
        .with_remediation("error.remediation.checkMcpSettings")
}

#[cfg(test)]
mod tests {
    use super::{McpConnectionDraft, McpInstallDraft, McpVariableDraft};

    fn variable(name: &str, value: &str) -> McpVariableDraft {
        McpVariableDraft {
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    fn remote(headers: Vec<McpVariableDraft>) -> McpInstallDraft {
        McpInstallDraft {
            name: "Remote".to_string(),
            description: None,
            connection: McpConnectionDraft::Http {
                url: "https://mcp.example.test".to_string(),
                headers,
            },
        }
    }

    fn local_with_env(env: Vec<McpVariableDraft>) -> McpInstallDraft {
        let mut draft = local();
        if let McpConnectionDraft::Stdio { env: slot, .. } = &mut draft.connection {
            *slot = env;
        }
        draft
    }

    fn local() -> McpInstallDraft {
        McpInstallDraft {
            name: "Filesystem".to_string(),
            description: Some("  Read selected files.  ".to_string()),
            connection: McpConnectionDraft::Stdio {
                command: "npx".to_string(),
                arguments: vec!["-y".to_string(), "server-filesystem".to_string()],
                env: Vec::new(),
            },
        }
    }

    #[test]
    fn a_small_typed_local_draft_is_valid_and_normalized() {
        let draft = local();
        draft.validate().expect("valid draft");
        assert_eq!(draft.normalized_name(), "Filesystem");
        assert_eq!(
            draft.normalized_description().as_deref(),
            Some("Read selected files.")
        );
    }

    #[test]
    fn remote_connections_require_https_except_on_loopback() {
        for url in [
            "https://mcp.example.test/v1",
            "http://localhost:3000/mcp",
            "http://127.0.0.1:4000/mcp",
            "http://[::1]:5000/mcp",
        ] {
            let draft = McpInstallDraft {
                name: "Remote".to_string(),
                description: None,
                connection: McpConnectionDraft::Http {
                    url: url.to_string(),
                    headers: Vec::new(),
                },
            };
            draft.validate().expect(url);
        }

        let insecure = McpInstallDraft {
            name: "Remote".to_string(),
            description: None,
            connection: McpConnectionDraft::Sse {
                url: "http://mcp.example.test/events".to_string(),
                headers: Vec::new(),
            },
        }
        .validate()
        .expect_err("public HTTP is rejected");
        assert_eq!(insecure.message_key, "error.mcp.urlInsecure");
    }

    #[test]
    fn embedded_url_credentials_and_free_form_fields_are_rejected() {
        let credentials = McpInstallDraft {
            name: "Remote".to_string(),
            description: None,
            connection: McpConnectionDraft::Http {
                url: "https://user:secret@mcp.example.test".to_string(),
                headers: Vec::new(),
            },
        }
        .validate()
        .expect_err("embedded credentials are rejected");
        assert_eq!(credentials.message_key, "error.mcp.urlCredentials");

        for payload in [
            r#"{"name":"Remote","description":null,"connection":{"transport":"http","url":"https://mcp.example.test","token":"secret"}}"#,
            r#"{"name":"Remote","description":null,"connection":{"transport":"http","url":"https://mcp.example.test","headers":{"Authorization":"secret"}}}"#,
            r#"{"name":"Files","description":null,"connection":{"transport":"stdio","command":"npx","env":[{"name":"A","value":"b","secret":true}]}}"#,
        ] {
            assert!(
                serde_json::from_str::<McpInstallDraft>(payload).is_err(),
                "{payload}"
            );
        }
    }

    #[test]
    fn local_env_and_remote_headers_travel_as_ordered_name_value_pairs() {
        let local = serde_json::from_str::<McpInstallDraft>(
            r#"{"name":"GitHub","description":null,"connection":{"transport":"stdio","command":"npx","arguments":[],"env":[{"name":"GITHUB_TOKEN","value":" ghp_example "},{"name":"_DEBUG","value":""}]}}"#,
        )
        .expect("env list decodes");
        local.validate().expect("env is valid");
        let McpConnectionDraft::Stdio { env, .. } = &local.connection else {
            panic!("stdio draft");
        };
        assert_eq!(env[0].normalized_name(), "GITHUB_TOKEN");
        assert_eq!(env[0].normalized_value(), "ghp_example");

        remote(vec![
            variable("Authorization", "Bearer abc"),
            variable("X-Api-Key", "abc"),
        ])
        .validate()
        .expect("headers are valid");
    }

    #[test]
    fn variable_names_values_and_repeats_are_checked_without_echoing_them() {
        for env in [
            vec![variable("1TOKEN", "x")],
            vec![variable("MY-TOKEN", "x")],
            vec![variable("  ", "x")],
            vec![variable("TOKEN", "line\nbreak")],
            vec![variable("TOKEN", "a"), variable("TOKEN", "b")],
            vec![variable("TOKEN", &"x".repeat(8_193))],
        ] {
            let error = local_with_env(env)
                .validate()
                .expect_err("invalid env is rejected");
            assert_eq!(error.message_key, "error.mcp.envInvalid");
            let technical = error.technical_message.unwrap_or_default();
            assert!(!technical.contains("TOKEN") && !technical.contains("break"));
        }

        let repeated = remote(vec![
            variable("Authorization", "a"),
            variable("authorization", "b"),
        ])
        .validate()
        .expect_err("header names are case-insensitive");
        assert_eq!(repeated.message_key, "error.mcp.headersInvalid");

        let spaced = remote(vec![variable("X Api Key", "a")])
            .validate()
            .expect_err("a header name is a single token");
        assert_eq!(spaced.message_key, "error.mcp.headersInvalid");

        let many = local_with_env(
            (0..65)
                .map(|index| variable(&format!("VAR_{index}"), "x"))
                .collect(),
        )
        .validate()
        .expect_err("variable count is bounded");
        assert_eq!(many.message_key, "error.mcp.envInvalid");
    }

    #[test]
    fn local_values_are_bounded_and_single_line() {
        let mut draft = local();
        if let McpConnectionDraft::Stdio { command, .. } = &mut draft.connection {
            *command = "npx\nrm".to_string();
        }
        assert_eq!(
            draft
                .validate()
                .expect_err("newline is rejected")
                .message_key,
            "error.mcp.commandInvalid"
        );

        let mut draft = local();
        if let McpConnectionDraft::Stdio { arguments, .. } = &mut draft.connection {
            arguments.resize(65, "arg".to_string());
        }
        assert_eq!(
            draft
                .validate()
                .expect_err("argument limit is enforced")
                .message_key,
            "error.mcp.argumentsInvalid"
        );
    }
}
