use serde::{Deserialize, Serialize};

/// The product tool registry (ADR-0004). These strings go into the database, into i18n keys and
/// into frontend branching, so they are stable and must not change.
/// `rename_all` is not used: kebab-case would turn `OpenCode` into `open-code`, which does not
/// match the product vocabulary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolId {
    #[serde(rename = "claude-code")]
    ClaudeCode,
    #[serde(rename = "codex")]
    Codex,
    #[serde(rename = "opencode")]
    OpenCode,
    #[serde(rename = "gemini-cli")]
    GeminiCli,
    #[serde(rename = "grok-build")]
    GrokBuild,
    #[serde(rename = "openclaw")]
    OpenClaw,
    #[serde(rename = "hermes")]
    Hermes,
    #[serde(rename = "pi")]
    Pi,
    #[serde(rename = "kimi-code")]
    KimiCode,
    #[serde(rename = "deepseek-dsh")]
    DeepSeekDsh,
}

impl ToolId {
    pub const ALL: [ToolId; 10] = [
        ToolId::ClaudeCode,
        ToolId::Codex,
        ToolId::OpenCode,
        ToolId::GeminiCli,
        ToolId::GrokBuild,
        ToolId::OpenClaw,
        ToolId::Hermes,
        ToolId::Pi,
        ToolId::KimiCode,
        ToolId::DeepSeekDsh,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::GeminiCli => "gemini-cli",
            Self::GrokBuild => "grok-build",
            Self::OpenClaw => "openclaw",
            Self::Hermes => "hermes",
            Self::Pi => "pi",
            Self::KimiCode => "kimi-code",
            Self::DeepSeekDsh => "deepseek-dsh",
        }
    }

    pub fn from_str_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolStatus {
    NotInstalled,
    Installed,
    UpdateAvailable,
    Broken,
    Unknown,
}

/// Spec §11: the UI and the application layer always branch on capabilities; hardcoded special cases by ToolId are forbidden.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolCapabilities {
    pub can_install: bool,
    pub can_update: bool,
    pub can_uninstall: bool,
    pub can_repair: bool,
    pub can_launch: bool,
    pub can_manage_provider: bool,
    pub can_manage_mcp: bool,
    pub can_manage_skills: bool,
    pub can_manage_prompts: bool,
    pub can_manage_version: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolAccessRequirement {
    VendorOrProvider,
    Provider,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolUseCase {
    OfficialCoding,
    ModelChoice,
    PersonalAutomation,
}

/// Product-owned discovery facts. They are descriptive filters, not rankings
/// or endorsements, and keep renderer code free of ToolId branches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDiscovery {
    pub publisher: String,
    pub access: ToolAccessRequirement,
    pub use_cases: Vec<ToolUseCase>,
}

impl ToolDiscovery {
    pub fn for_tool(id: ToolId) -> Self {
        let (publisher, access, use_cases) = match id {
            ToolId::ClaudeCode => (
                "Anthropic",
                ToolAccessRequirement::VendorOrProvider,
                vec![ToolUseCase::OfficialCoding],
            ),
            ToolId::Codex => (
                "OpenAI",
                ToolAccessRequirement::VendorOrProvider,
                vec![ToolUseCase::OfficialCoding],
            ),
            ToolId::GeminiCli => (
                "Google",
                ToolAccessRequirement::VendorOrProvider,
                vec![ToolUseCase::OfficialCoding],
            ),
            ToolId::GrokBuild => (
                "xAI",
                ToolAccessRequirement::VendorOrProvider,
                vec![ToolUseCase::OfficialCoding],
            ),
            ToolId::OpenCode => (
                "OpenCode",
                ToolAccessRequirement::Provider,
                vec![ToolUseCase::ModelChoice],
            ),
            ToolId::OpenClaw => (
                "OpenClaw",
                ToolAccessRequirement::Provider,
                vec![ToolUseCase::ModelChoice, ToolUseCase::PersonalAutomation],
            ),
            ToolId::Hermes => (
                "Nous Research",
                ToolAccessRequirement::Provider,
                vec![ToolUseCase::ModelChoice, ToolUseCase::PersonalAutomation],
            ),
            ToolId::Pi => (
                "Pi",
                ToolAccessRequirement::Provider,
                vec![ToolUseCase::ModelChoice, ToolUseCase::PersonalAutomation],
            ),
            ToolId::KimiCode => (
                "Moonshot AI",
                ToolAccessRequirement::VendorOrProvider,
                vec![ToolUseCase::OfficialCoding],
            ),
            ToolId::DeepSeekDsh => (
                "DeepSeek",
                ToolAccessRequirement::VendorOrProvider,
                vec![ToolUseCase::OfficialCoding, ToolUseCase::PersonalAutomation],
            ),
        };
        Self {
            publisher: publisher.to_string(),
            access,
            use_cases,
        }
    }
}

/// The stable wire result of `app_tool_launch`. Cancelling the system directory picker is not an
/// error, and the caller silently dismisses the instruction dialog on it; `Launched` is only
/// returned once the terminal has really taken over.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolLaunchOutcome {
    Launched,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub id: ToolId,
    pub name: String,
    pub description_key: String,
    pub discovery: ToolDiscovery,
    pub status: ToolStatus,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub capabilities: ToolCapabilities,
    /// Whether this tool keeps its chat history inside its own settings directory (the uninstall
    /// copy in spec §32 must state the knock-on effect honestly). **Not a capability** — it
    /// describes the disk layout rather than what the product can do to the tool, so it does not
    /// belong in `ToolCapabilities`.
    pub sessions_inside_settings: bool,
    /// Runtime environment identifier ("macos" / "windows" / "linux" / "wsl"). None when unknown.
    pub environment: Option<String>,
    /// An installed desktop application that reads and writes this tool's
    /// configuration directory.
    ///
    /// A tool whose command line is absent can still have a live configuration:
    /// the Codex desktop application uses the same `~/.codex` as the Codex CLI,
    /// so a machine with the application and without the CLI has services, MCP
    /// servers and prompts that are genuinely in use. The evidence is the
    /// *application's* installed state rather than the presence of the
    /// directory, which survives an uninstall and would keep claiming a tool
    /// nobody has.
    ///
    /// **Not a capability** — it says where a configuration comes from, not
    /// what the product may do to the tool.
    pub configuration_shared_with: Option<crate::domain::DesktopAppId>,
}

/// A read-only uninstall preview. These values are for display only: the
/// uninstall mutation re-detects the install owner and recomputes every target
/// instead of accepting a renderer-supplied path or command.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolUninstallTargetKind {
    Command,
    Directory,
    File,
    SymbolicLink,
    MissingPath,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolUninstallTarget {
    pub kind: ToolUninstallTargetKind,
    /// Exact local command or path shown to the user. It is never sent back to
    /// native code as part of the uninstall request.
    pub value: String,
    /// Commands have already passed the fixed planner/allowlist. Paths are true
    /// only when the native deletion boundary accepts them.
    pub can_remove_automatically: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolUninstallPreview {
    pub tool: ToolId,
    pub app: Vec<ToolUninstallTarget>,
    pub settings: Vec<ToolUninstallTarget>,
    pub cache: Vec<ToolUninstallTarget>,
}

/// Spec §32: uninstall has three items and only Uninstall App is ticked by default. The two flags
/// are independent, default to false, and deserialize to false when the field is missing — a
/// dangerous operation always takes the most conservative interpretation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct UninstallOptions {
    pub remove_settings: bool,
    pub remove_cache: bool,
}

#[cfg(test)]
mod tests {
    use super::{Tool, ToolCapabilities, ToolDiscovery, ToolId, ToolStatus};

    #[test]
    fn tool_id_strings_are_stable_and_reversible() {
        let pairs = [
            (ToolId::ClaudeCode, "claude-code"),
            (ToolId::Codex, "codex"),
            (ToolId::OpenCode, "opencode"),
            (ToolId::GeminiCli, "gemini-cli"),
            (ToolId::GrokBuild, "grok-build"),
            (ToolId::OpenClaw, "openclaw"),
            (ToolId::Hermes, "hermes"),
            (ToolId::Pi, "pi"),
            (ToolId::KimiCode, "kimi-code"),
            (ToolId::DeepSeekDsh, "deepseek-dsh"),
        ];
        for (id, expected) in pairs {
            assert_eq!(id.as_str(), expected);
            assert_eq!(ToolId::from_str_id(expected), Some(id));
            let json = serde_json::to_string(&id).expect("serialize tool id");
            assert_eq!(json, format!("\"{expected}\""));
            let parsed: ToolId = serde_json::from_str(&json).expect("deserialize tool id");
            assert_eq!(parsed, id);
        }
        assert_eq!(ToolId::ALL.len(), 10);
        assert_eq!(ToolId::from_str_id("claude"), None);
        assert_eq!(ToolId::from_str_id("open-code"), None);
        assert_eq!(ToolId::from_str_id(""), None);
    }

    #[test]
    fn status_and_capability_wire_format_is_camel_case() {
        let pairs = [
            (ToolStatus::NotInstalled, "notInstalled"),
            (ToolStatus::Installed, "installed"),
            (ToolStatus::UpdateAvailable, "updateAvailable"),
            (ToolStatus::Broken, "broken"),
            (ToolStatus::Unknown, "unknown"),
        ];
        for (status, expected) in pairs {
            let json = serde_json::to_string(&status).expect("serialize tool status");
            assert_eq!(json, format!("\"{expected}\""));
        }
        let json =
            serde_json::to_string(&ToolCapabilities::default()).expect("serialize capabilities");
        assert_eq!(
            json,
            r#"{"canInstall":false,"canUpdate":false,"canUninstall":false,"canRepair":false,"canLaunch":false,"canManageProvider":false,"canManageMcp":false,"canManageSkills":false,"canManagePrompts":false,"canManageVersion":false}"#
        );
    }

    #[test]
    fn launch_outcome_wire_format_distinguishes_cancel_from_handoff() {
        use super::ToolLaunchOutcome;

        assert_eq!(
            serde_json::to_string(&ToolLaunchOutcome::Launched).expect("serialize launched"),
            r#""launched""#
        );
        assert_eq!(
            serde_json::to_string(&ToolLaunchOutcome::Cancelled).expect("serialize cancelled"),
            r#""cancelled""#
        );
    }

    #[test]
    fn tool_round_trips_with_camel_case_wire_format() {
        let tool = Tool {
            id: ToolId::ClaudeCode,
            name: "Claude Code".to_string(),
            description_key: "tool.claude-code.description".to_string(),
            discovery: ToolDiscovery::for_tool(ToolId::ClaudeCode),
            status: ToolStatus::UpdateAvailable,
            version: Some("2.1.0".to_string()),
            latest_version: Some("2.2.0".to_string()),
            capabilities: ToolCapabilities {
                can_install: true,
                ..ToolCapabilities::default()
            },
            sessions_inside_settings: true,
            environment: Some("macos".to_string()),
            configuration_shared_with: None,
        };
        let json = serde_json::to_string(&tool).expect("serialize tool");
        assert!(json.starts_with(r#"{"id":"claude-code","name":"Claude Code","descriptionKey":"tool.claude-code.description","discovery":{"publisher":"Anthropic","access":"vendorOrProvider","useCases":["officialCoding"]},"status":"updateAvailable","version":"2.1.0","latestVersion":"2.2.0","capabilities":{"canInstall":true"#));
        assert!(json.ends_with(r#""environment":"macos","configurationSharedWith":null}"#));
        let parsed: Tool = serde_json::from_str(&json).expect("deserialize tool");
        assert_eq!(parsed, tool);

        let empty = Tool {
            id: ToolId::Codex,
            name: "Codex".to_string(),
            description_key: "tool.codex.description".to_string(),
            discovery: ToolDiscovery::for_tool(ToolId::Codex),
            status: ToolStatus::NotInstalled,
            version: None,
            latest_version: None,
            capabilities: ToolCapabilities::default(),
            sessions_inside_settings: false,
            environment: None,
            configuration_shared_with: None,
        };
        let json = serde_json::to_string(&empty).expect("serialize tool");
        assert!(json.contains(r#""version":null,"latestVersion":null"#));
        assert!(json.ends_with(r#""environment":null,"configurationSharedWith":null}"#));
    }

    #[test]
    fn the_session_layout_fact_is_on_the_wire_between_capabilities_and_environment() {
        let tool = Tool {
            id: ToolId::ClaudeCode,
            name: "Claude Code".to_string(),
            description_key: "tool.claude-code.description".to_string(),
            discovery: ToolDiscovery::for_tool(ToolId::ClaudeCode),
            status: ToolStatus::Installed,
            version: None,
            latest_version: None,
            capabilities: ToolCapabilities::default(),
            sessions_inside_settings: true,
            environment: None,
            configuration_shared_with: None,
        };
        let json = serde_json::to_string(&tool).expect("serialize tool");
        assert!(json.contains(r#""sessionsInsideSettings":true,"environment":null"#));
        let parsed: Tool = serde_json::from_str(&json).expect("deserialize tool");
        assert!(parsed.sessions_inside_settings);
    }

    #[test]
    fn uninstall_defaults_to_app_only_and_uses_camel_case_on_the_wire() {
        use super::UninstallOptions;

        let defaults = UninstallOptions::default();
        assert!(
            !defaults.remove_settings && !defaults.remove_cache,
            "spec section 32: only Uninstall App is checked by default"
        );
        assert_eq!(
            serde_json::to_string(&defaults).expect("serialize options"),
            r#"{"removeSettings":false,"removeCache":false}"#
        );
        let parsed: UninstallOptions =
            serde_json::from_str(r#"{"removeSettings":true,"removeCache":false}"#)
                .expect("deserialize options");
        assert!(parsed.remove_settings && !parsed.remove_cache);
        // A missing field means do not delete — dangerous operations always default to the most conservative option
        let partial: UninstallOptions = serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(partial, UninstallOptions::default());
    }

    #[test]
    fn every_tool_has_bounded_discovery_metadata() {
        for id in ToolId::ALL {
            let discovery = ToolDiscovery::for_tool(id);
            assert!(!discovery.publisher.trim().is_empty());
            assert!(!discovery.use_cases.is_empty());
            assert!(discovery.use_cases.len() <= 2);
        }
    }
}
