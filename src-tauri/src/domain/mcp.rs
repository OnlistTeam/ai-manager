//! Product-owned MCP installation input.
//!
//! The renderer sends a small, typed draft instead of a free-form MCP JSON
//! object. The draft is inbound-only: it deliberately does not implement
//! `Serialize`, and it has no environment/header/token fields. Upstream format
//! synthesis stays behind the compatibility boundary.

use serde::Deserialize;
use url::{Host, Url};

use super::{AppError, ErrorCode};

const MAX_NAME_CHARS: usize = 80;
const MAX_DESCRIPTION_CHARS: usize = 240;
const MAX_COMMAND_CHARS: usize = 512;
const MAX_ARGUMENTS: usize = 64;
const MAX_ARGUMENT_CHARS: usize = 2_048;
const MAX_URL_BYTES: usize = 2_048;

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
    },
    Http {
        url: String,
    },
    Sse {
        url: String,
    },
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
            McpConnectionDraft::Stdio { command, arguments } => validate_stdio(command, arguments),
            McpConnectionDraft::Http { url } | McpConnectionDraft::Sse { url } => {
                validate_remote_url(url)
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
    use super::{McpConnectionDraft, McpInstallDraft};

    fn local() -> McpInstallDraft {
        McpInstallDraft {
            name: "Filesystem".to_string(),
            description: Some("  Read selected files.  ".to_string()),
            connection: McpConnectionDraft::Stdio {
                command: "npx".to_string(),
                arguments: vec!["-y".to_string(), "server-filesystem".to_string()],
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
                },
            };
            draft.validate().expect(url);
        }

        let insecure = McpInstallDraft {
            name: "Remote".to_string(),
            description: None,
            connection: McpConnectionDraft::Sse {
                url: "http://mcp.example.test/events".to_string(),
            },
        }
        .validate()
        .expect_err("public HTTP is rejected");
        assert_eq!(insecure.message_key, "error.mcp.urlInsecure");
    }

    #[test]
    fn embedded_url_credentials_and_free_form_secret_fields_are_rejected() {
        let credentials = McpInstallDraft {
            name: "Remote".to_string(),
            description: None,
            connection: McpConnectionDraft::Http {
                url: "https://user:secret@mcp.example.test".to_string(),
            },
        }
        .validate()
        .expect_err("embedded credentials are rejected");
        assert_eq!(credentials.message_key, "error.mcp.urlCredentials");

        let unknown = serde_json::from_str::<McpInstallDraft>(
            r#"{"name":"Remote","description":null,"connection":{"transport":"http","url":"https://mcp.example.test","headers":{"Authorization":"secret"}}}"#,
        )
        .err()
        .expect("headers are outside the product draft");
        assert!(unknown.to_string().contains("unknown field"));
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
