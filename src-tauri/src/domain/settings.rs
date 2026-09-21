//! Product settings model (spec §25 / §58 / §59).
//!
//! Every field here is **this person's preference on this machine**, not business data that
//! gets backed up — a property that directly determines that they are preserved when a backup
//! is restored (`compat/ccswitch/backup.rs`).
//!
//! Where they are stored and what the keys are called is the business of
//! `compat/ccswitch/settings.rs`; this type does not know.

use serde::{Deserialize, Serialize};

use super::{ExtensionKind, ExtensionScope, ToolId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStrategy {
    /// Explicit off value for legacy settings and low-level tests; the product UI no longer exposes a region switch.
    OfficialOnly,
    /// The default automatic policy: official first, and only after an npm transport-level network failure try the community mirror once.
    #[default]
    #[serde(alias = "chinaResilient")]
    Automatic,
}

/// Which terminal takes over tool launching and session resumption. On macOS the user can
/// pick whatever they have installed; on other platforms the platform layer probes in a fixed
/// order and this choice is not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalAppId {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "iterm2")]
    ITerm2,
    #[serde(rename = "ghostty")]
    Ghostty,
    #[serde(rename = "kitty")]
    Kitty,
    #[serde(rename = "wezterm")]
    WezTerm,
    #[serde(rename = "alacritty")]
    Alacritty,
}

impl TerminalAppId {
    pub const ALL: [Self; 6] = [
        Self::System,
        Self::ITerm2,
        Self::Ghostty,
        Self::Kitty,
        Self::WezTerm,
        Self::Alacritty,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::ITerm2 => "iterm2",
            Self::Ghostty => "ghostty",
            Self::Kitty => "kitty",
            Self::WezTerm => "wezterm",
            Self::Alacritty => "alacritty",
        }
    }

    pub fn from_str_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSettings {
    /// Legacy compatibility field. The application layer always normalizes it to true; the UI
    /// no longer offers a mode switch and technical details are folded in place per feature.
    pub advanced_mode: bool,
    /// §17: the user was explicitly asked whether to import CC Switch. Set to true only
    /// after a successful import or an explicit skip; if no source database was found it stays
    /// false, so installing CC Switch later can still be detected.
    pub import_prompt_seen: bool,
    /// Which tool was last managed on the AI services / extensions pages. None = never chosen.
    /// The two pages share one value: the user's mental model is "I am managing Claude Code",
    /// not "I manage this one on this page and that one on that page". When the stored tool is
    /// not in the current page's selectable set, each page falls back to its own first entry
    /// (both pages already have that fallback today).
    pub tool_scope: Option<ToolId>,
    /// The extensions page's own full scope memory. CLI tools are still mirrored into
    /// `tool_scope`, while desktop apps are only written here, so Claude Desktop is never
    /// passed off as a command-line tool.
    pub extension_scope: Option<ExtensionScope>,
    /// Which kind tab the extensions page was last on. None = never chosen.
    pub extension_kind: Option<ExtensionKind>,
    /// Install/update compatibility field. The product defaults to official-first with limited automatic fallback and does not route by user region.
    pub download_strategy: DownloadStrategy,
    /// Whether a Use / Open action may switch to the next saved, reachable compatible service
    /// when the address is unreachable. Off by default; it starts no background probe and does
    /// not take over the request path.
    pub automatic_provider_failover: bool,
    /// Which terminal was last selected to take over. None = never chosen, so the platform
    /// default is used. Once chosen it is reused, so the user does not have to pick again every
    /// time they resume a session.
    pub terminal_app: Option<TerminalAppId>,
}

#[cfg(test)]
mod tests {
    use super::TerminalAppId;
    use super::{DownloadStrategy, ProductSettings};
    use crate::domain::{DesktopAppId, ExtensionKind, ExtensionScope, ToolId};

    #[test]
    fn defaults_preserve_the_legacy_wire_contract() {
        let defaults = ProductSettings::default();
        assert!(
            !defaults.advanced_mode,
            "the derived default remains compatible with older stored values"
        );
        assert!(
            !defaults.import_prompt_seen,
            "spec section 17: a fresh install has not answered the import prompt"
        );
        assert_eq!(defaults.tool_scope, None);
        assert_eq!(defaults.extension_scope, None);
        assert_eq!(defaults.extension_kind, None);
        assert_eq!(defaults.download_strategy, DownloadStrategy::Automatic);
        assert!(!defaults.automatic_provider_failover);
    }

    #[test]
    fn the_wire_format_is_camel_case_and_never_omits_a_null() {
        let json = serde_json::to_string(&ProductSettings::default()).expect("serialize");
        assert_eq!(
            json,
            r#"{"advancedMode":false,"importPromptSeen":false,"toolScope":null,"extensionScope":null,"extensionKind":null,"downloadStrategy":"automatic","automaticProviderFailover":false,"terminalApp":null}"#
        );

        let filled = ProductSettings {
            advanced_mode: true,
            import_prompt_seen: true,
            tool_scope: Some(ToolId::Codex),
            extension_scope: Some(ExtensionScope::desktop_app(DesktopAppId::ClaudeDesktop)),
            extension_kind: Some(ExtensionKind::Mcp),
            download_strategy: DownloadStrategy::Automatic,
            automatic_provider_failover: true,
            terminal_app: Some(TerminalAppId::Ghostty),
        };
        let json = serde_json::to_string(&filled).expect("serialize");
        assert_eq!(
            json,
            r#"{"advancedMode":true,"importPromptSeen":true,"toolScope":"codex","extensionScope":{"kind":"desktopApp","id":"claude-desktop"},"extensionKind":"mcp","downloadStrategy":"automatic","automaticProviderFailover":true,"terminalApp":"ghostty"}"#
        );
        assert_eq!(
            serde_json::from_str::<ProductSettings>(&json).expect("deserialize"),
            filled
        );
    }

    #[test]
    fn the_legacy_region_named_wire_value_migrates_to_automatic() {
        let legacy = r#"{"advancedMode":false,"importPromptSeen":false,"toolScope":null,"extensionScope":null,"extensionKind":null,"downloadStrategy":"chinaResilient","automaticProviderFailover":false,"terminalApp":null}"#;
        assert_eq!(
            serde_json::from_str::<ProductSettings>(legacy).expect("deserialize legacy settings"),
            ProductSettings::default()
        );
    }

    #[test]
    fn a_missing_field_is_rejected_rather_than_silently_defaulted() {
        // Omitting `#[serde(default)]` is deliberate: the frontend Zod type requires every
        // field, so a missing one means the two contracts drifted. Better to error than to
        // silently produce incomplete settings.
        assert!(serde_json::from_str::<ProductSettings>(r#"{"advancedMode":true}"#).is_err());
    }
}
