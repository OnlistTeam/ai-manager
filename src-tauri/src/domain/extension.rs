//! The product's extension model (spec §10 / §36 / §91).
//!
//! Upstream has three unrelated models: `crate::app_config::McpServer` (a global record plus a
//! per-tool switch), `crate::app_config::InstalledSkill` (the same), and
//! `crate::prompt::Prompt` (one table per tool with a single selection inside). They are unified
//! here into **one read model scoped to a (tool, kind) pair**: the list itself is read per tool,
//! so all three extension kinds share exactly the same list shape and one UI serves all three.
//!
//! **This type deliberately has no field that could hold a config payload** — no `server`, no
//! `content`, no paths. Spec §36 requires "do not show JSON on first entry", and deleting those
//! fields from the wire format is far more reliable than asking the frontend to "be careful not
//! to display them" (the same trick as the outbound secret channel in §19).
//!
//! The conversion happens only in the three modules under `compat/ccswitch/extension/`.

use serde::{Deserialize, Serialize};

use super::{DesktopAppId, ToolId};

/// Stable product identity for one extension surface. Desktop applications are
/// deliberately not folded into `ToolId` (ADR-0005 / ADR-0021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ExtensionScope {
    Tool { id: ToolId },
    DesktopApp { id: DesktopAppId },
}

impl ExtensionScope {
    pub const fn tool(id: ToolId) -> Self {
        Self::Tool { id }
    }

    pub const fn desktop_app(id: DesktopAppId) -> Self {
        Self::DesktopApp { id }
    }

    pub const fn tool_id(self) -> Option<ToolId> {
        match self {
            Self::Tool { id } => Some(id),
            Self::DesktopApp { .. } => None,
        }
    }

    pub const fn desktop_app_id(self) -> Option<DesktopAppId> {
        match self {
            Self::Tool { .. } => None,
            Self::DesktopApp { id } => Some(id),
        }
    }

    pub fn stable_key(self) -> String {
        match self {
            Self::Tool { id } => format!("tool:{}", id.as_str()),
            Self::DesktopApp { id } => format!("desktop-app:{}", id.as_str()),
        }
    }
}

/// The three inner pages of §36. The order is the display order on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionKind {
    Skill,
    Mcp,
    Prompt,
}

/// Whether AI Manager owns the source-of-truth row or merely found an
/// extension already present in a tool's local directory. Detected items stay
/// in place and are only exposed through safe locate/edit actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionManagement {
    Managed,
    Detected,
}

/// A safe, path-free action for a Skill that already exists in a tool's local
/// directory. The renderer can ask to locate or edit it, but never supplies a
/// filesystem path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DetectedSkillResourceAction {
    Browse,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DetectedSkillResourceOpenOutcome {
    FolderOpened,
    EditorOpened,
}

/// Whether one local inventory scope could be inspected. A failed scope stays
/// visible in the aggregate result so the UI never turns a read failure into a
/// misleading "nothing found" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalExtensionScopeStatus {
    Ready,
    Unavailable,
}

/// One safe `(tool, kind)` scan result. It deliberately carries no path,
/// configuration payload, error string, command or credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalExtensionScope {
    pub tool: ToolId,
    pub kind: ExtensionKind,
    pub status: LocalExtensionScopeStatus,
}

/// Cross-tool local inventory used by the Extensions landing surface. `items`
/// contains detected-only projections; managed rows remain in the existing
/// scoped lists. The hard cap is applied by the Application service and is
/// disclosed through `truncated` rather than silently hiding overflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalExtensionInventory {
    pub items: Vec<Extension>,
    pub scopes: Vec<LocalExtensionScope>,
    pub truncated: bool,
}

impl ExtensionKind {
    pub const ALL: [ExtensionKind; 3] = [
        ExtensionKind::Skill,
        ExtensionKind::Mcp,
        ExtensionKind::Prompt,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Mcp => "mcp",
            Self::Prompt => "prompt",
        }
    }

    pub fn from_str_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

/// One extension, as seen within the scope of a given tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub kind: ExtensionKind,
    pub id: String,
    /// Which CLI or desktop config surface scope this record was read in.
    /// Upstream stores MCP and skills as a global record plus a per-scope switch, and the primary
    /// key of the prompts table is already `(id, app_type)` — all three are covered by the single
    /// notion of a "scope".
    pub scope: ExtensionScope,
    pub name: String,
    /// A one-line human description the extension carries (written by its author or the user). An empty string counts as None.
    pub description: Option<String>,
    pub management: ExtensionManagement,
    pub enabled: bool,
    /// Whether this entry can be "turned off". Always false for prompts: upstream only supports
    /// selecting one of a tool's several instruction files, and turning off the last one would
    /// write that tool's instruction file as an empty file — that is deleting data, not flipping
    /// a switch. The UI picks its control from this (a toggle vs a "use" button) and never needs
    /// to know the kind.
    pub can_disable: bool,
}

#[cfg(test)]
mod tests {
    use super::{
        DetectedSkillResourceAction, DetectedSkillResourceOpenOutcome, Extension, ExtensionKind,
        ExtensionManagement, ExtensionScope, LocalExtensionInventory, LocalExtensionScope,
        LocalExtensionScopeStatus,
    };
    use crate::domain::ToolId;

    fn sample() -> Extension {
        Extension {
            kind: ExtensionKind::Mcp,
            id: "filesystem".to_string(),
            scope: ExtensionScope::tool(ToolId::ClaudeCode),
            name: "Filesystem".to_string(),
            description: Some("Reads and writes files you pick.".to_string()),
            management: ExtensionManagement::Managed,
            enabled: true,
            can_disable: true,
        }
    }

    #[test]
    fn kinds_use_stable_lower_case_wire_values() {
        for (kind, expected) in [
            (ExtensionKind::Skill, "skill"),
            (ExtensionKind::Mcp, "mcp"),
            (ExtensionKind::Prompt, "prompt"),
        ] {
            assert_eq!(kind.as_str(), expected);
            assert_eq!(
                serde_json::to_string(&kind).expect("serialize kind"),
                format!("\"{expected}\"")
            );
            assert_eq!(ExtensionKind::from_str_id(expected), Some(kind));
        }
        assert_eq!(ExtensionKind::from_str_id("mcpServer"), None);
        assert_eq!(ExtensionKind::from_str_id("skills"), None);
        assert_eq!(ExtensionKind::from_str_id(""), None);
    }

    #[test]
    fn the_kind_order_is_the_order_spec_36_lists_the_inner_pages() {
        // The inner pages of §36 are literally Skills / MCP / Prompts. This array decides the page
        // order, so changing the order has to be deliberate.
        assert_eq!(
            ExtensionKind::ALL,
            [
                ExtensionKind::Skill,
                ExtensionKind::Mcp,
                ExtensionKind::Prompt
            ]
        );
    }

    #[test]
    fn the_wire_format_is_camel_case_and_carries_no_configuration_payload() {
        let json = serde_json::to_string(&sample()).expect("serialize extension");
        assert_eq!(
            json,
            r#"{"kind":"mcp","id":"filesystem","scope":{"kind":"tool","id":"claude-code"},"name":"Filesystem","description":"Reads and writes files you pick.","management":"managed","enabled":true,"canDisable":true}"#
        );
        // Spec §36: the wire format simply has no key that could hold a config payload, so the frontend has no JSON to display.
        for forbidden in ["server", "config", "content", "path", "command", "args"] {
            assert!(
                !json.contains(forbidden),
                "the wire format grew a field that could carry raw configuration: {forbidden}"
            );
        }
    }

    #[test]
    fn an_absent_description_serializes_as_null_rather_than_disappearing() {
        // serde contract: Option<T> must serialize to null so the frontend Zod can use .nullable().
        let bare = Extension {
            description: None,
            ..sample()
        };
        let json = serde_json::to_string(&bare).expect("serialize extension");
        assert!(json.contains(r#""description":null"#));
    }

    #[test]
    fn an_extension_round_trips_through_json() {
        let json = serde_json::to_string(&sample()).expect("serialize");
        let parsed: Extension = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, sample());
    }

    #[test]
    fn extension_scopes_keep_cli_and_desktop_app_ids_disjoint() {
        let tool = ExtensionScope::tool(ToolId::ClaudeCode);
        let desktop = ExtensionScope::desktop_app(crate::domain::DesktopAppId::ClaudeDesktop);
        assert_eq!(tool.stable_key(), "tool:claude-code");
        assert_eq!(desktop.stable_key(), "desktop-app:claude-desktop");
        assert_eq!(tool.tool_id(), Some(ToolId::ClaudeCode));
        assert_eq!(desktop.tool_id(), None);
        assert_eq!(
            serde_json::to_string(&desktop).expect("serialize desktop scope"),
            r#"{"kind":"desktopApp","id":"claude-desktop"}"#
        );
        assert!(serde_json::from_str::<ExtensionScope>(
            r#"{"kind":"desktopApp","id":"claude-desktop","tool":"claude-code"}"#
        )
        .is_err());
    }

    #[test]
    fn detected_skill_resource_actions_are_path_free_stable_enums() {
        assert_eq!(
            serde_json::to_string(&DetectedSkillResourceAction::Browse)
                .expect("serialize browse action"),
            r#""browse""#
        );
        assert_eq!(
            serde_json::to_string(&DetectedSkillResourceAction::Edit)
                .expect("serialize edit action"),
            r#""edit""#
        );
        assert_eq!(
            serde_json::to_string(&DetectedSkillResourceOpenOutcome::FolderOpened)
                .expect("serialize folder outcome"),
            r#""folderOpened""#
        );
        assert!(
            serde_json::from_str::<DetectedSkillResourceAction>(r#"{"path":"/tmp/private"}"#)
                .is_err()
        );
    }

    #[test]
    fn local_inventory_reports_partial_scans_without_growing_a_payload_channel() {
        let mut detected = sample();
        detected.management = ExtensionManagement::Detected;
        detected.can_disable = false;
        let inventory = LocalExtensionInventory {
            items: vec![detected],
            scopes: vec![
                LocalExtensionScope {
                    tool: ToolId::ClaudeCode,
                    kind: ExtensionKind::Mcp,
                    status: LocalExtensionScopeStatus::Ready,
                },
                LocalExtensionScope {
                    tool: ToolId::Codex,
                    kind: ExtensionKind::Mcp,
                    status: LocalExtensionScopeStatus::Unavailable,
                },
            ],
            truncated: false,
        };

        let json = serde_json::to_string(&inventory).expect("serialize inventory");
        assert!(json.contains(r#""status":"ready""#));
        assert!(json.contains(r#""status":"unavailable""#));
        for forbidden in ["server", "config", "content", "path", "command", "args"] {
            assert!(
                !json.contains(forbidden),
                "local inventory grew a private payload field: {forbidden}"
            );
        }
    }
}
