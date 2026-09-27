//! What the MCP edit form is prefilled with (ADR-0062).
//!
//! `McpInstallDraft` is inbound-only and deliberately implements neither
//! `Serialize` nor `Debug` (ADR-0047). Editing needs the opposite direction,
//! so it gets its own outbound type rather than weakening that one. This type
//! is `Serialize` because it has to cross IPC to fill the form, and still not
//! `Debug`, so an environment variable or header value cannot end up in a log
//! line through `{:?}`. It is read only when a person opens the edit form.

use serde::Serialize;

/// One saved MCP connection in the shape of the guided form. The id is
/// fixed: saving updates this connection in place.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpEditForm {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub connection: McpConnectionForm,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "transport", rename_all = "camelCase")]
pub enum McpConnectionForm {
    Stdio {
        command: String,
        arguments: Vec<String>,
        env: Vec<McpVariableForm>,
    },
    Http {
        url: String,
        headers: Vec<McpVariableForm>,
    },
    Sse {
        url: String,
        headers: Vec<McpVariableForm>,
    },
}

/// One environment variable or request header, value included, because the
/// person editing the connection is the one who typed it (ADR-0047 shows
/// values in the form as it shows saved API keys).
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpVariableForm {
    pub name: String,
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::{McpConnectionForm, McpEditForm, McpVariableForm};

    #[test]
    fn the_edit_form_serializes_in_the_same_shape_the_install_draft_is_read_in() {
        let form = McpEditForm {
            id: "github-a1b2c3d4".to_string(),
            name: "GitHub".to_string(),
            description: None,
            connection: McpConnectionForm::Stdio {
                command: "npx".to_string(),
                arguments: vec!["-y".to_string()],
                env: vec![McpVariableForm {
                    name: "GITHUB_TOKEN".to_string(),
                    value: "ghp_example".to_string(),
                }],
            },
        };
        assert_eq!(
            serde_json::to_value(&form).expect("serialize form"),
            serde_json::json!({
                "id": "github-a1b2c3d4",
                "name": "GitHub",
                "description": null,
                "connection": {
                    "transport": "stdio",
                    "command": "npx",
                    "arguments": ["-y"],
                    "env": [{"name": "GITHUB_TOKEN", "value": "ghp_example"}]
                }
            })
        );
    }
}
