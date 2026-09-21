use serde::{Deserialize, Serialize};

use crate::domain::{AppError, ErrorCode, ToolId, ToolInstallSource};

pub const UPDATE_PREVIEW_FINGERPRINT_HEX_LEN: usize = 64;
pub const MAX_UPDATE_INSTALLATIONS: usize = 16;
pub const MAX_UPDATE_ATTEMPTS: usize = 8;
pub const MAX_UPDATE_COMMANDS_PER_ATTEMPT: usize = 8;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolUpdateMethod {
    NativeSelfUpdate,
    ToolSelfUpdate,
    OfficialInstaller,
    Homebrew,
    Npm,
    Pnpm,
    Bun,
    Volta,
    Uv,
    Pipx,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolUpdateBlockReason {
    NotInstalled,
    AmbiguousInstallation,
    UnsupportedInstallation,
    InspectionFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdateInstallation {
    pub source: ToolInstallSource,
    pub version: Option<String>,
    pub runnable: bool,
    pub is_default: bool,
    pub location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdateAttemptPreview {
    pub method: ToolUpdateMethod,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdateReadyPreview {
    pub tool: ToolId,
    pub preview_fingerprint: String,
    pub target_version: String,
    pub source: ToolInstallSource,
    pub installations: Vec<ToolUpdateInstallation>,
    pub attempts: Vec<ToolUpdateAttemptPreview>,
    pub multiple_installations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToolUpdatePreview {
    Ready {
        preview: ToolUpdateReadyPreview,
    },
    Blocked {
        tool: ToolId,
        reason: ToolUpdateBlockReason,
    },
}

impl ToolUpdatePreview {
    pub fn tool(&self) -> ToolId {
        match self {
            Self::Ready { preview } => preview.tool,
            Self::Blocked { tool, .. } => *tool,
        }
    }

    pub fn ready(&self) -> Option<&ToolUpdateReadyPreview> {
        match self {
            Self::Ready { preview } => Some(preview),
            Self::Blocked { .. } => None,
        }
    }

    pub fn ready_fingerprint(&self) -> Option<&str> {
        self.ready()
            .map(|preview| preview.preview_fingerprint.as_str())
    }
}

pub fn validate_update_preview_fingerprint(value: &str) -> Result<String, AppError> {
    let valid = value.len() == UPDATE_PREVIEW_FINGERPRINT_HEX_LEN
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    valid.then(|| value.to_string()).ok_or_else(|| {
        AppError::new(
            ErrorCode::UpdatePreviewStale,
            "error.tool.updatePreviewStale",
        )
        .with_technical("update preview fingerprint is not a lowercase SHA-256 digest")
        .with_remediation("error.remediation.recheckUpdate")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ToolId, ToolInstallSource};

    #[test]
    fn ready_preview_wire_format_is_tagged_camel_case_and_bounded() {
        let preview = ToolUpdatePreview::Ready {
            preview: ToolUpdateReadyPreview {
                tool: ToolId::ClaudeCode,
                preview_fingerprint: "a".repeat(64),
                target_version: "2.1.212".to_string(),
                source: ToolInstallSource::NativeInstaller,
                installations: vec![ToolUpdateInstallation {
                    source: ToolInstallSource::NativeInstaller,
                    version: Some("2.1.211".into()),
                    runnable: true,
                    is_default: true,
                    location: "/Users/test/.local/bin/claude".into(),
                }],
                attempts: vec![ToolUpdateAttemptPreview {
                    method: ToolUpdateMethod::NativeSelfUpdate,
                    commands: vec!["/Users/test/.local/bin/claude update".into()],
                }],
                multiple_installations: false,
            },
        };
        let json = serde_json::to_value(preview).expect("serialize update preview");
        assert_eq!(json["state"], "ready");
        assert_eq!(json["preview"]["previewFingerprint"], "a".repeat(64));
        assert_eq!(json["preview"]["targetVersion"], "2.1.212");
        assert_eq!(json["preview"]["attempts"][0]["method"], "nativeSelfUpdate");
        assert!(json["preview"].get("programPath").is_none());
        assert!(json["preview"].get("args").is_none());
    }

    #[test]
    fn blocked_preview_has_only_stable_product_facts() {
        let json = serde_json::to_value(ToolUpdatePreview::Blocked {
            tool: ToolId::Codex,
            reason: ToolUpdateBlockReason::AmbiguousInstallation,
        })
        .expect("serialize blocked preview");
        assert_eq!(
            json,
            serde_json::json!({
                "state": "blocked",
                "tool": "codex",
                "reason": "ambiguousInstallation"
            })
        );
    }

    #[test]
    fn fingerprint_input_accepts_only_lowercase_sha256_hex() {
        assert!(validate_update_preview_fingerprint(&"a".repeat(64)).is_ok());
        for invalid in [
            String::new(),
            "A".to_string(),
            "g".repeat(64),
            "a".repeat(63),
            "a".repeat(65),
        ] {
            assert!(
                validate_update_preview_fingerprint(&invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
    }
}
