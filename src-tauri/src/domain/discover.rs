//! Product models for the Discover section of the Skills and MCP pages
//! (ADR-0063).
//!
//! What crosses IPC is only what a card and its dialog show. The command a
//! server runs, its URL, the headers it sends and the package it starts stay
//! native: the renderer names a server by its id and gets back an operation.
//! The values the user types for a server's inputs are usually API keys, so
//! the inbound value type implements neither `Serialize` nor `Debug`
//! (ADR-0047).

use serde::{Deserialize, Serialize};

use super::{AppError, ExtensionScope};

/// How an MCP server is reached once added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoverTransport {
    Stdio,
    Http,
    Sse,
}

/// What starts a local server. Shown as a badge, never as a command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoverRunner {
    Npx,
    Uvx,
    Docker,
}

/// Where a value the user types ends up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoverInputTarget {
    Env,
    Header,
    Argument,
}

/// What a featured server's input asks for, so the renderer can name it in
/// the reader's language. Registry inputs carry only their own label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoverInputKind {
    ApiKey,
    AccessToken,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverInput {
    pub key: String,
    pub target: DiscoverInputTarget,
    /// The registry's own name for it, shown when `kind` is absent.
    pub label: String,
    pub kind: Option<DiscoverInputKind>,
    /// The registry's description, shown as returned.
    pub description: Option<String>,
    /// Where a featured server's key is issued, such as `brave.com/search/api`.
    pub site: Option<String>,
    pub placeholder: Option<String>,
    pub secret: bool,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverMcpServer {
    /// A featured id (`context7`) or a registry name (`io.github.owner/name`).
    pub id: String,
    /// What the new connection is called.
    pub name: String,
    pub title: String,
    /// Registry entries only; a featured server's one-liner is product copy
    /// and translated by the renderer from its id.
    pub description: Option<String>,
    pub publisher: Option<String>,
    /// Handed back to `app_discover_icon`, which fetches only icons the
    /// Discover section itself gave out.
    pub icon: Option<String>,
    /// Whether `app_discover_link_open` can open a homepage for it.
    pub homepage: bool,
    pub transport: DiscoverTransport,
    pub runs: Option<DiscoverRunner>,
    /// A remote server that asks the app to sign in, in the browser, on first use.
    pub sign_in: bool,
    pub featured: bool,
    pub inputs: Vec<DiscoverInput>,
    /// The name of the connection that already runs the same thing.
    pub added: Option<String>,
}

/// Which transports one app can take, so the dialog preselects only apps
/// that can run the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverMcpReach {
    pub scope: ExtensionScope,
    pub transports: Vec<DiscoverTransport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverMcpList {
    pub items: Vec<DiscoverMcpServer>,
    pub reach: Vec<DiscoverMcpReach>,
    /// A source that could not be reached; the items are what could be shown.
    pub source_error: Option<AppError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverSkill {
    /// `owner/repo/skillId`
    pub id: String,
    /// `owner/repo`
    pub source: String,
    pub skill_id: String,
    pub name: String,
    pub installs: u64,
    pub official: bool,
    pub icon: String,
    pub description: Option<String>,
    pub added: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverSkillList {
    pub items: Vec<DiscoverSkill>,
    pub source_error: Option<AppError>,
}

/// One value typed into a server's dialog. Usually a secret: no `Debug`, no
/// `Serialize`, and it never appears in an error or a log line.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoverInputValue {
    pub key: String,
    pub value: String,
}

/// Which page to open for a Discover item. The renderer names the page; the
/// native side owns the address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoverLink {
    Homepage,
    Page,
    Repository,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolId;

    #[test]
    fn a_server_crosses_ipc_without_any_way_to_carry_a_command_or_url() {
        let server = DiscoverMcpServer {
            id: "fetch".to_string(),
            name: "fetch".to_string(),
            title: "Fetch".to_string(),
            description: None,
            publisher: Some("Model Context Protocol".to_string()),
            icon: None,
            homepage: true,
            transport: DiscoverTransport::Stdio,
            runs: Some(DiscoverRunner::Uvx),
            sign_in: false,
            featured: true,
            inputs: Vec::new(),
            added: None,
        };
        let json = serde_json::to_value(&server).expect("serialize server");
        let keys = json
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for forbidden in ["command", "args", "url", "headers", "env", "server"] {
            assert!(!keys.iter().any(|key| key == forbidden), "{forbidden}");
        }
        assert_eq!(json["runs"], "uvx");
        assert_eq!(json["signIn"], false);
    }

    #[test]
    fn reach_names_its_scope_in_the_extension_wire_shape() {
        let reach = DiscoverMcpReach {
            scope: ExtensionScope::tool(ToolId::Codex),
            transports: vec![DiscoverTransport::Stdio, DiscoverTransport::Http],
        };
        let json = serde_json::to_value(&reach).expect("serialize reach");
        assert_eq!(json["scope"]["kind"], "tool");
        assert_eq!(json["scope"]["id"], "codex");
        assert_eq!(json["transports"], serde_json::json!(["stdio", "http"]));
    }

    #[test]
    fn an_input_value_rejects_unknown_fields() {
        let parsed =
            serde_json::from_str::<DiscoverInputValue>(r#"{"key":"TOKEN","value":"x","extra":1}"#);
        assert!(parsed.is_err());
    }
}
