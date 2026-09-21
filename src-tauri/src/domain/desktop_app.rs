use serde::{Deserialize, Serialize};

use super::ToolId;

/// Desktop applications are a separate product surface from CLI tools.
/// These identifiers are stable on the native wire and must not be folded into `ToolId`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DesktopAppId {
    #[serde(rename = "codex-app")]
    CodexApp,
    #[serde(rename = "claude-desktop")]
    ClaudeDesktop,
    #[serde(rename = "cursor")]
    Cursor,
    #[serde(rename = "zcode")]
    ZCode,
    #[serde(rename = "cherry-studio")]
    CherryStudio,
}

impl DesktopAppId {
    pub const ALL: [Self; 5] = [
        Self::CodexApp,
        Self::ClaudeDesktop,
        Self::Cursor,
        Self::ZCode,
        Self::CherryStudio,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CodexApp => "codex-app",
            Self::ClaudeDesktop => "claude-desktop",
            Self::Cursor => "cursor",
            Self::ZCode => "zcode",
            Self::CherryStudio => "cherry-studio",
        }
    }

    pub fn from_str_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesktopAppStatus {
    Installed,
    UpdateAvailable,
    NotInstalled,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesktopAppConfigurationRelationship {
    SharedConfiguration,
    SeparateConfiguration,
    StandaloneApplication,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesktopAppLaunchOutcome {
    Launched,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesktopAppInstallerHandoff {
    DirectOfficialPackage,
    OfficialDownloadPage,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesktopAppUninstallHandoff {
    RevealApplication,
    SystemSettings,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAppOfficialDownloadOutcome {
    pub handoff: DesktopAppInstallerHandoff,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesktopAppUninstallOutcome {
    Opened,
}

/// Safe renderer projection. Bundle paths, package-family names and launch commands
/// remain entirely inside the native platform boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopApp {
    pub id: DesktopAppId,
    pub name: String,
    pub status: DesktopAppStatus,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub related_tool: Option<ToolId>,
    pub configuration_relationship: DesktopAppConfigurationRelationship,
    pub can_launch: bool,
    /// Whether this installed desktop configuration surface can safely manage
    /// MCP connections. This is separate from CLI `ToolCapabilities`.
    pub can_manage_mcp: bool,
    pub environment: String,
    pub installer_handoff: DesktopAppInstallerHandoff,
    pub uninstall_handoff: DesktopAppUninstallHandoff,
    pub updates_managed_by_vendor: bool,
    pub can_rollback: bool,
}

#[cfg(test)]
mod tests {
    use super::{
        DesktopApp, DesktopAppConfigurationRelationship, DesktopAppId, DesktopAppInstallerHandoff,
        DesktopAppLaunchOutcome, DesktopAppOfficialDownloadOutcome, DesktopAppStatus,
        DesktopAppUninstallHandoff, DesktopAppUninstallOutcome,
    };
    use crate::domain::ToolId;

    #[test]
    fn desktop_app_ids_are_stable_and_separate_from_cli_ids() {
        for (id, expected) in [
            (DesktopAppId::CodexApp, "codex-app"),
            (DesktopAppId::ClaudeDesktop, "claude-desktop"),
            (DesktopAppId::Cursor, "cursor"),
            (DesktopAppId::ZCode, "zcode"),
            (DesktopAppId::CherryStudio, "cherry-studio"),
        ] {
            assert_eq!(id.as_str(), expected);
            assert_eq!(DesktopAppId::from_str_id(expected), Some(id));
            assert_eq!(
                serde_json::to_string(&id).expect("serialize"),
                format!("\"{expected}\"")
            );
            assert!(ToolId::from_str_id(expected).is_none());
        }
        assert_eq!(DesktopAppId::from_str_id("codex"), None);
    }

    #[test]
    fn desktop_app_projection_round_trips_without_native_paths() {
        let app = DesktopApp {
            id: DesktopAppId::CodexApp,
            name: "ChatGPT / Codex".to_string(),
            status: DesktopAppStatus::Installed,
            version: Some("26.527.60818".to_string()),
            latest_version: Some("26.527.60818".to_string()),
            related_tool: Some(ToolId::Codex),
            configuration_relationship: DesktopAppConfigurationRelationship::SharedConfiguration,
            can_launch: true,
            can_manage_mcp: false,
            environment: "macos".to_string(),
            installer_handoff: DesktopAppInstallerHandoff::DirectOfficialPackage,
            uninstall_handoff: DesktopAppUninstallHandoff::RevealApplication,
            updates_managed_by_vendor: true,
            can_rollback: false,
        };
        let json = serde_json::to_string(&app).expect("serialize app");
        assert_eq!(
            json,
            r#"{"id":"codex-app","name":"ChatGPT / Codex","status":"installed","version":"26.527.60818","latestVersion":"26.527.60818","relatedTool":"codex","configurationRelationship":"sharedConfiguration","canLaunch":true,"canManageMcp":false,"environment":"macos","installerHandoff":"directOfficialPackage","uninstallHandoff":"revealApplication","updatesManagedByVendor":true,"canRollback":false}"#
        );
        assert!(!json.contains("Applications"));
        assert_eq!(
            serde_json::from_str::<DesktopApp>(&json).expect("parse"),
            app
        );
        assert_eq!(
            serde_json::to_string(&DesktopAppLaunchOutcome::Launched).expect("serialize outcome"),
            r#""launched""#
        );
        assert_eq!(
            serde_json::to_string(&DesktopAppOfficialDownloadOutcome {
                handoff: DesktopAppInstallerHandoff::DirectOfficialPackage,
            })
            .expect("serialize official download outcome"),
            r#"{"handoff":"directOfficialPackage"}"#
        );
        assert_eq!(
            serde_json::to_string(&DesktopAppUninstallOutcome::Opened)
                .expect("serialize uninstall outcome"),
            r#""opened""#
        );
    }

    #[test]
    fn every_status_and_relationship_has_a_stable_camel_case_wire_value() {
        for (value, expected) in [
            (DesktopAppStatus::Installed, "installed"),
            (DesktopAppStatus::UpdateAvailable, "updateAvailable"),
            (DesktopAppStatus::NotInstalled, "notInstalled"),
            (DesktopAppStatus::Unsupported, "unsupported"),
            (DesktopAppStatus::Unknown, "unknown"),
        ] {
            assert_eq!(
                serde_json::to_string(&value).expect("serialize"),
                format!("\"{expected}\"")
            );
        }
        assert_eq!(
            serde_json::to_string(&DesktopAppConfigurationRelationship::SeparateConfiguration)
                .expect("serialize"),
            r#""separateConfiguration""#
        );
        assert_eq!(
            serde_json::to_string(&DesktopAppConfigurationRelationship::StandaloneApplication)
                .expect("serialize"),
            r#""standaloneApplication""#
        );
    }
}
