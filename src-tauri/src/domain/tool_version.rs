use serde::{Deserialize, Serialize};

use crate::domain::{AppError, ErrorCode, OperationKind, ToolId};

pub const MAX_TOOL_VERSION_LENGTH: usize = 64;
pub const MAX_TOOL_VERSION_CATALOG: usize = 200;
pub const MAX_TOOL_VERSION_TAG_LENGTH: usize = 32;
pub const MAX_TOOL_VERSION_TAGS: usize = 32;
pub const MAX_TOOL_VERSION_HISTORY: usize = 100;

/// The install source exposes only the ownership category, never the real path, the Node version directory or the package name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolInstallSource {
    NotInstalled,
    Npm,
    Pnpm,
    Bun,
    Volta,
    Uv,
    Pipx,
    Brew,
    NativeInstaller,
    Unmanaged,
}

impl ToolInstallSource {
    pub fn as_str_id(self) -> &'static str {
        match self {
            Self::NotInstalled => "notInstalled",
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Bun => "bun",
            Self::Volta => "volta",
            Self::Uv => "uv",
            Self::Pipx => "pipx",
            Self::Brew => "brew",
            Self::NativeInstaller => "nativeInstaller",
            Self::Unmanaged => "unmanaged",
        }
    }

    pub fn from_str_id(raw: &str) -> Option<Self> {
        match raw {
            "notInstalled" => Some(Self::NotInstalled),
            "npm" => Some(Self::Npm),
            "pnpm" => Some(Self::Pnpm),
            "bun" => Some(Self::Bun),
            "volta" => Some(Self::Volta),
            "uv" => Some(Self::Uv),
            "pipx" => Some(Self::Pipx),
            "brew" => Some(Self::Brew),
            "nativeInstaller" => Some(Self::NativeInstaller),
            "unmanaged" => Some(Self::Unmanaged),
            _ => None,
        }
    }
}

/// Why the version catalog can be viewed but not executed. The UI explains it from the enum and never matches on technical error strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolVersionRestriction {
    NativeInstaller,
    Brew,
    Unmanaged,
}

/// A package-publisher controlled registry channel such as `latest`, `next`,
/// `beta`, or `alpha`. Tags are display metadata only; installation still
/// submits an exact validated version string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersionTag {
    pub tag: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersionCatalog {
    pub tool: ToolId,
    pub source: ToolInstallSource,
    pub can_change_version: bool,
    pub restriction: Option<ToolVersionRestriction>,
    pub latest_version: Option<String>,
    #[serde(default)]
    pub dist_tags: Vec<ToolVersionTag>,
    pub versions: Vec<String>,
    /// true only means this catalog query used the community mirror after a recoverable network
    /// failure against the official registry; it does not change the global npm configuration.
    pub mirror_used: bool,
}

/// A verified version transition performed by AI Manager itself. The database
/// row id is deliberately absent from the wire contract; ordering is the only
/// identity the product needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersionEvent {
    pub tool: ToolId,
    pub from_version: Option<String>,
    pub to_version: String,
    pub source: ToolInstallSource,
    pub operation_kind: OperationKind,
    pub occurred_at: i64,
}

impl ToolVersionEvent {
    pub fn verified(
        tool: ToolId,
        from_version: Option<String>,
        to_version: String,
        source: ToolInstallSource,
        operation_kind: OperationKind,
        occurred_at: i64,
    ) -> Result<Self, AppError> {
        let from_version = from_version
            .map(|version| validate_observed_tool_version(&version))
            .transpose()?;
        let to_version = validate_observed_tool_version(&to_version)?;
        if source == ToolInstallSource::NotInstalled {
            return Err(version_history_invalid(
                "verified installed tool cannot use notInstalled source",
            ));
        }
        if !matches!(
            operation_kind,
            OperationKind::Install | OperationKind::Update | OperationKind::ChangeVersion
        ) {
            return Err(version_history_invalid(format!(
                "unsupported version history operation {operation_kind:?}"
            )));
        }
        if occurred_at < 0 {
            return Err(version_history_invalid("occurred_at cannot be negative"));
        }
        Ok(Self {
            tool,
            from_version,
            to_version,
            source,
            operation_kind,
            occurred_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersionHistory {
    pub tool: ToolId,
    pub previous_version: Option<String>,
    pub events: Vec<ToolVersionEvent>,
}

impl ToolVersionHistory {
    pub fn from_newest_first(tool: ToolId, events: Vec<ToolVersionEvent>) -> Self {
        let previous_version = events.iter().find_map(|event| {
            event
                .from_version
                .as_ref()
                .filter(|from| *from != &event.to_version)
                .cloned()
        });
        Self {
            tool,
            previous_version,
            events,
        }
    }
}

/// Observed versions are display and audit data, never command fragments. They
/// still stay ASCII, bounded and control-free so a hostile executable cannot
/// persist terminal escapes or an unbounded payload in product storage.
pub fn validate_observed_tool_version(raw: &str) -> Result<String, AppError> {
    if raw.is_empty()
        || raw.len() > MAX_TOOL_VERSION_LENGTH
        || raw != raw.trim()
        || !raw.is_ascii()
        || !raw.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(version_history_invalid(
            "observed tool version is empty, unsafe, or too long",
        ));
    }
    Ok(raw.to_string())
}

fn version_history_invalid(detail: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::UpdateFailed,
        "error.tool.versionHistoryWriteFailed",
    )
    .with_technical(detail)
    .with_remediation("error.remediation.retryOrViewDetails")
}

/// The package name is fixed by the registry, so the only dynamic fragment the renderer can supply
/// is the version. This allows the usual npm SemVer / prerelease / build characters while rejecting
/// `@`, slashes, whitespace and control characters, so it can only ever be the version part of
/// `package@version` and can never change the structure of the package spec.
pub fn validate_tool_version(raw: &str) -> Result<String, AppError> {
    if raw.is_empty()
        || raw.len() > MAX_TOOL_VERSION_LENGTH
        || raw != raw.trim()
        || !raw.is_ascii()
        || !raw.as_bytes()[0].is_ascii_digit()
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return Err(
            AppError::new(ErrorCode::UpdateFailed, "error.tool.versionInvalid")
                .with_remediation("error.remediation.chooseListedVersion"),
        );
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        validate_observed_tool_version, validate_tool_version, ToolInstallSource,
        ToolVersionCatalog, ToolVersionEvent, ToolVersionHistory, ToolVersionRestriction,
        MAX_TOOL_VERSION_LENGTH,
    };
    use crate::domain::{OperationKind, ToolId};

    #[test]
    fn version_target_accepts_semver_shapes_and_rejects_package_spec_injection() {
        for version in ["1.2.3", "0.1.0-beta.2", "2.0.0+darwin-arm64", "1"] {
            assert_eq!(validate_tool_version(version).unwrap(), version);
        }
        for version in [
            "",
            " latest",
            "latest",
            "1.2.3 ",
            "1.2.3@evil",
            "1.2.3/../../x",
            "1.2.3 --force",
        ] {
            assert!(validate_tool_version(version).is_err(), "{version:?}");
        }
        assert!(
            validate_tool_version(&format!("1{}", "0".repeat(MAX_TOOL_VERSION_LENGTH))).is_err()
        );
    }

    #[test]
    fn catalog_wire_format_is_bounded_product_data_without_paths_or_package_names() {
        let catalog = ToolVersionCatalog {
            tool: ToolId::ClaudeCode,
            source: ToolInstallSource::NativeInstaller,
            can_change_version: false,
            restriction: Some(ToolVersionRestriction::NativeInstaller),
            latest_version: None,
            dist_tags: Vec::new(),
            versions: Vec::new(),
            mirror_used: false,
        };
        let json = serde_json::to_string(&catalog).unwrap();
        assert_eq!(
            json,
            r#"{"tool":"claude-code","source":"nativeInstaller","canChangeVersion":false,"restriction":"nativeInstaller","latestVersion":null,"distTags":[],"versions":[],"mirrorUsed":false}"#
        );
        assert!(!json.contains("/Users/"));
        assert!(!json.contains("anthropic-ai"));
    }

    #[test]
    fn install_source_storage_ids_round_trip_without_aliases() {
        for source in [
            ToolInstallSource::NotInstalled,
            ToolInstallSource::Npm,
            ToolInstallSource::Pnpm,
            ToolInstallSource::Bun,
            ToolInstallSource::Volta,
            ToolInstallSource::Uv,
            ToolInstallSource::Pipx,
            ToolInstallSource::Brew,
            ToolInstallSource::NativeInstaller,
            ToolInstallSource::Unmanaged,
        ] {
            assert_eq!(
                ToolInstallSource::from_str_id(source.as_str_id()),
                Some(source)
            );
        }
        assert_eq!(ToolInstallSource::from_str_id("native"), None);
    }

    #[test]
    fn history_uses_only_verified_version_operations_and_skips_noop_for_previous() {
        let noop = ToolVersionEvent::verified(
            ToolId::Codex,
            Some("1.2.0".to_string()),
            "1.2.0".to_string(),
            ToolInstallSource::Pnpm,
            OperationKind::Update,
            20,
        )
        .unwrap();
        let changed = ToolVersionEvent::verified(
            ToolId::Codex,
            Some("1.1.0".to_string()),
            "1.2.0".to_string(),
            ToolInstallSource::Pnpm,
            OperationKind::ChangeVersion,
            10,
        )
        .unwrap();
        let history = ToolVersionHistory::from_newest_first(
            ToolId::Codex,
            vec![noop.clone(), changed.clone()],
        );

        assert_eq!(history.previous_version.as_deref(), Some("1.1.0"));
        assert_eq!(history.events, vec![noop, changed]);
        assert!(ToolVersionEvent::verified(
            ToolId::Codex,
            Some("1.0.0".to_string()),
            "1.2.0".to_string(),
            ToolInstallSource::Pnpm,
            OperationKind::Repair,
            30,
        )
        .is_err());
    }

    #[test]
    fn observed_versions_reject_control_sequences_and_unbounded_values() {
        for version in ["1.2.3", "v1.2.3", "2026.08.28-beta+arm64"] {
            assert_eq!(validate_observed_tool_version(version).unwrap(), version);
        }
        for version in ["", " 1.2.3", "1.2.3\nnext", "1.2.3\u{1b}[31m"] {
            assert!(validate_observed_tool_version(version).is_err());
        }
    }
}
