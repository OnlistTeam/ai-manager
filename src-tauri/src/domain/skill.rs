use serde::{Deserialize, Serialize};

use crate::domain::{AppError, ErrorCode, OperationId};

/// Product-facing provenance for a catalog Skill. The UI shows only the
/// friendly repository label; the compatibility layer owns the conversion to
/// CC Switch's `DiscoverableSkill`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillSource {
    pub owner: String,
    pub repository: String,
    pub branch: String,
    pub directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillCatalogItem {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source: SkillSource,
    pub installed: bool,
    /// True only when this catalog snapshot used the fixed, verified jsDelivr
    /// GitHub transport fallback. It is presentation evidence, not authority.
    pub mirror_used: bool,
}

/// A known update for one managed Skill. Hashes and repository coordinates
/// remain inside the CC Switch compatibility layer; the renderer only needs a
/// stable target and a friendly label.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdate {
    pub id: String,
    pub name: String,
}

/// A configured GitHub source. The opaque id is resolved again inside the
/// compatibility layer before removal; renderer input never selects a DB row
/// by owner/name alone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillRepository {
    pub id: String,
    pub owner: String,
    pub repository: String,
    pub branch: String,
    pub enabled: bool,
}

/// Inbound-only source settings. URL parsing is presentation work, while the
/// upstream service remains the authoritative GitHub/ref validator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillRepositoryDraft {
    pub owner: String,
    pub repository: String,
    pub branch: String,
    pub enabled: bool,
}

/// Safe projection of an uninstall recovery copy. Filesystem paths, original
/// directory names, repository metadata and per-tool flags remain native.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillBackup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub conflicts: bool,
}

/// Result of the native-only ZIP picker. A cancelled operating-system dialog
/// is an ordinary outcome; a started install returns only the task id. The
/// selected filesystem path never becomes renderer-visible state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SkillZipInstallOutcome {
    Cancelled,
    Started { operation: OperationId },
}

impl SkillCatalogItem {
    /// IPC callers are not trusted to preserve a catalog row. Keep the stable
    /// ID tied to its source coordinates before any download starts.
    pub fn validate_source(&self) -> Result<(), AppError> {
        let source = &self.source;
        let required = [
            self.name.trim(),
            source.owner.trim(),
            source.repository.trim(),
            source.branch.trim(),
            source.directory.trim(),
        ];
        let expected_id = format!(
            "{}/{}:{}",
            source.owner, source.repository, source.directory
        );

        if required.iter().any(|value| value.is_empty()) || self.id != expected_id {
            return Err(
                AppError::new(ErrorCode::InstallFailed, "error.skill.invalidSource")
                    .with_technical("skill catalog coordinates are incomplete or inconsistent")
                    .with_remediation("error.remediation.retryOrViewDetails"),
            );
        }
        Ok(())
    }

    pub fn validate_for_install(&self) -> Result<(), AppError> {
        self.validate_source()?;
        if self.installed {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.skill.alreadyInstalled",
            )
            .with_technical(self.id.clone()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SkillBackup, SkillCatalogItem, SkillRepository, SkillRepositoryDraft, SkillSource,
        SkillUpdate, SkillZipInstallOutcome,
    };
    use crate::domain::ErrorCode;

    fn item() -> SkillCatalogItem {
        SkillCatalogItem {
            id: "anthropics/skills:skills/code-review".to_string(),
            name: "Code review".to_string(),
            description: Some("Reviews a change before it ships.".to_string()),
            source: SkillSource {
                owner: "anthropics".to_string(),
                repository: "skills".to_string(),
                branch: "main".to_string(),
                directory: "skills/code-review".to_string(),
            },
            installed: false,
            mirror_used: false,
        }
    }

    #[test]
    fn catalog_item_uses_a_small_camel_case_wire_contract() {
        let json = serde_json::to_string(&item()).expect("serialize catalog item");
        assert_eq!(
            json,
            r#"{"id":"anthropics/skills:skills/code-review","name":"Code review","description":"Reviews a change before it ships.","source":{"owner":"anthropics","repository":"skills","branch":"main","directory":"skills/code-review"},"installed":false,"mirrorUsed":false}"#
        );
        assert!(!json.contains("readmeUrl"));
        assert!(!json.contains("content"));
    }

    #[test]
    fn install_validation_binds_the_id_to_the_source() {
        assert!(item().validate_for_install().is_ok());

        let mut tampered = item();
        tampered.id = "someone/else:payload".to_string();
        let error = tampered
            .validate_for_install()
            .expect_err("a mismatched source must not start a download");
        assert_eq!(error.code, ErrorCode::InstallFailed);
        assert_eq!(error.message_key, "error.skill.invalidSource");
    }

    #[test]
    fn installed_catalog_rows_cannot_start_a_second_install() {
        let mut installed = item();
        installed.installed = true;
        let error = installed
            .validate_for_install()
            .expect_err("installed row is not an install request");
        assert_eq!(error.code, ErrorCode::OperationConflict);
        assert_eq!(error.message_key, "error.skill.alreadyInstalled");
    }

    #[test]
    fn update_projection_never_exposes_hashes_or_repository_coordinates() {
        let update = SkillUpdate {
            id: "anthropics/skills:skills/code-review".to_string(),
            name: "Code review".to_string(),
        };
        let json = serde_json::to_string(&update).expect("serialize update");
        assert_eq!(
            json,
            r#"{"id":"anthropics/skills:skills/code-review","name":"Code review"}"#
        );
        for forbidden in ["hash", "branch", "repository", "directory", "content"] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn repository_contract_is_small_and_the_draft_rejects_unknown_fields() {
        let repository = SkillRepository {
            id: "opaque-source-id".to_string(),
            owner: "anthropics".to_string(),
            repository: "skills".to_string(),
            branch: "main".to_string(),
            enabled: true,
        };
        let json = serde_json::to_string(&repository).expect("serialize repository");
        assert_eq!(
            json,
            r#"{"id":"opaque-source-id","owner":"anthropics","repository":"skills","branch":"main","enabled":true}"#
        );
        assert!(!json.contains("token"));
        assert!(!json.contains("path"));

        let draft = r#"{"owner":"anthropics","repository":"skills","branch":"main","enabled":true,"url":"https://example.test"}"#;
        assert!(serde_json::from_str::<SkillRepositoryDraft>(draft).is_err());
    }

    #[test]
    fn zip_picker_outcomes_never_carry_a_filesystem_path() {
        let cancelled = serde_json::to_string(&SkillZipInstallOutcome::Cancelled)
            .expect("serialize cancelled picker");
        assert_eq!(cancelled, r#"{"status":"cancelled"}"#);

        let started = SkillZipInstallOutcome::Started {
            operation: crate::domain::OperationId("operation-1".to_string()),
        };
        let json = serde_json::to_string(&started).expect("serialize started install");
        assert_eq!(json, r#"{"status":"started","operation":"operation-1"}"#);
        for forbidden in ["path", "file", "/Users/", "C:\\\\"] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn backup_projection_contains_no_path_or_upstream_metadata() {
        let backup = SkillBackup {
            id: "a".repeat(64),
            name: "Code review".to_string(),
            description: Some("Reviews a change.".to_string()),
            created_at: 1_787_689_200,
            conflicts: false,
        };
        let json = serde_json::to_string(&backup).expect("serialize backup");
        assert_eq!(
            json,
            format!(
                r#"{{"id":"{}","name":"Code review","description":"Reviews a change.","createdAt":1787689200,"conflicts":false}}"#,
                "a".repeat(64)
            )
        );
        for forbidden in ["path", "directory", "repo", "content", "apps", "backupId"] {
            assert!(!json.contains(forbidden));
        }
    }
}
