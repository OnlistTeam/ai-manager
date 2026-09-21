use serde::{Deserialize, Serialize};

use crate::domain::ToolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderRuntimeResourceKind {
    Configuration,
    Instructions,
    Memory,
    UserProfile,
    SessionData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderRuntimeResourceScope {
    Global,
    Project,
}

/// The product deliberately distinguishes resources that are safe to edit from
/// files that should only be located in Finder/Explorer. In particular, live
/// credential files are never opened straight into an editor from this panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderRuntimeResourceAction {
    Edit,
    Browse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRuntimeResource {
    /// Stable allow-list identifier. The frontend sends this ID back; it never
    /// sends a filesystem path to the open command.
    pub id: String,
    pub kind: ProviderRuntimeResourceKind,
    pub scope: ProviderRuntimeResourceScope,
    pub path: String,
    pub exists: bool,
    pub action: ProviderRuntimeResourceAction,
    /// Local byte count only. File names, project paths, and contents never
    /// cross the product boundary. `None` means the resource is absent or
    /// could not be measured safely.
    pub size_bytes: Option<u64>,
    /// True when the byte count is a lower bound because the bounded scan hit
    /// an unreadable entry or its safety budget.
    pub measurement_limited: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderRuntimeResourceOpenOutcome {
    EditorOpened,
    FolderOpened,
}

/// Where one field of the effective connection (endpoint or credential) actually came from. There
/// is no field for the raw value, and a credential can never cross IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum EffectiveConnectionSource {
    LiveConfig { path: String },
    ShellFile { variable: String, path: String },
    Environment { variable: String },
    ToolDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectiveCredential {
    Configured,
    ToolLogin,
    Missing,
    Unknown,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectiveSelection {
    #[default]
    Configuration,
    DefaultModel,
    RecentModel,
    Unknown,
}

/// Read-only connection evidence from global configuration and local state. `selection`
/// distinguishes configured defaults, recent history and unknown state; it is not a live
/// session probe and cannot account for arbitrary project or command-line overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveConnection {
    #[serde(default)]
    pub selection: EffectiveSelection,
    #[serde(default)]
    pub model: Option<String>,
    /// Only http(s) URLs without userinfo/query/fragment are allowed; anything else is None while the source is still reported honestly.
    pub endpoint: Option<String>,
    pub endpoint_source: EffectiveConnectionSource,
    pub credential: EffectiveCredential,
    pub credential_source: EffectiveConnectionSource,
    /// The saved endpoint matching the effective connection; None = nothing matched.
    pub provider_id: Option<String>,
    /// false = the login shell did not start, so only the config files and the GUI process environment were checked.
    pub shell_inspected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRuntimeStorage {
    pub total_bytes: u64,
    pub session_bytes: u64,
    pub session_count: u32,
    pub measurement_limited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRuntimeContext {
    pub tool: ToolId,
    pub live_config_paths: Vec<String>,
    pub resources: Vec<ProviderRuntimeResource>,
    pub storage: ProviderRuntimeStorage,
    pub effective_connection: Option<EffectiveConnection>,
}

#[cfg(test)]
mod tests {
    use super::{
        EffectiveConnection, EffectiveConnectionSource, EffectiveCredential,
        ProviderRuntimeContext, ProviderRuntimeResource, ProviderRuntimeResourceAction,
        ProviderRuntimeResourceKind, ProviderRuntimeResourceScope, ProviderRuntimeStorage,
    };
    use crate::domain::ToolId;

    #[test]
    fn wire_model_has_no_value_or_secret_field() {
        let context = ProviderRuntimeContext {
            tool: ToolId::ClaudeCode,
            live_config_paths: vec!["~/.claude/settings.json".to_string()],
            resources: vec![ProviderRuntimeResource {
                id: "global-instructions".to_string(),
                kind: ProviderRuntimeResourceKind::Instructions,
                scope: ProviderRuntimeResourceScope::Global,
                path: "~/.claude/CLAUDE.md".to_string(),
                exists: true,
                action: ProviderRuntimeResourceAction::Edit,
                size_bytes: Some(128),
                measurement_limited: false,
            }],
            storage: ProviderRuntimeStorage {
                total_bytes: 128,
                session_bytes: 0,
                session_count: 2,
                measurement_limited: false,
            },
            effective_connection: Some(EffectiveConnection {
                selection: super::EffectiveSelection::Configuration,
                model: None,
                endpoint: Some("https://relay.example.test".to_string()),
                endpoint_source: EffectiveConnectionSource::ShellFile {
                    variable: "ANTHROPIC_BASE_URL".to_string(),
                    path: "~/.zshrc:12".to_string(),
                },
                credential: EffectiveCredential::Configured,
                credential_source: EffectiveConnectionSource::Environment {
                    variable: "ANTHROPIC_AUTH_TOKEN".to_string(),
                },
                provider_id: None,
                shell_inspected: true,
            }),
        };
        let json = serde_json::to_string(&context).unwrap();
        assert!(json.contains(r#""endpointSource":{"kind":"shellFile","variable":"ANTHROPIC_BASE_URL","path":"~/.zshrc:12"}"#));
        assert!(json.contains(r#""credential":"configured""#));
        assert!(json.contains(
            r#""credentialSource":{"kind":"environment","variable":"ANTHROPIC_AUTH_TOKEN"}"#
        ));
        assert!(json.contains(r#""providerId":null"#));
        assert!(!json.contains("varValue"));
        assert!(!json.contains("apiKey"));
        assert!(!json.contains("sk-"));
        assert!(json.contains(r#""sessionCount":2"#));
    }

    #[test]
    fn tool_default_serializes_as_a_tagged_unit() {
        let json = serde_json::to_string(&EffectiveConnectionSource::ToolDefault).unwrap();
        assert_eq!(json, r#"{"kind":"toolDefault"}"#);
    }
}
