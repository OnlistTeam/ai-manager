use std::sync::Arc;

use super::{
    backup_delete_failed, backup_reference, backup_restore_failed, catalog_item, install_failed,
    legacy_code, not_installed, project_backup, project_repository, remove_failed,
    repository_from_draft, repository_id, update_failed, zip_install_failed, SkillCatalogStore,
};
use crate::app_config::{InstalledSkill, SkillApps};
use crate::database::Database;
use crate::domain::{ErrorCode, SkillCatalogItem, SkillRepositoryDraft};
use crate::services::skill::{Skill, SkillBackupEntry, SkillRepo, SkillService};
use crate::store::AppState;

struct TestHome(Option<std::ffi::OsString>);

impl TestHome {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("AI_MANAGER_TEST_HOME");
        std::env::set_var("AI_MANAGER_TEST_HOME", path);
        Self(previous)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
            None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
        }
    }
}

fn store() -> SkillCatalogStore {
    SkillCatalogStore {
        state: AppState::new(Arc::new(Database::memory().expect("memory database"))),
        service: SkillService::new(),
    }
}

fn upstream(installed: bool) -> Skill {
    Skill {
        key: "anthropics/skills:skills/code-review".to_string(),
        name: "Code review".to_string(),
        description: "  Reviews a change.  ".to_string(),
        directory: "skills/code-review".to_string(),
        readme_url: Some("https://github.com/anthropics/skills".to_string()),
        installed,
        repo_owner: Some("anthropics".to_string()),
        repo_name: Some("skills".to_string()),
        repo_branch: Some("main".to_string()),
        mirror_used: false,
    }
}

fn installed_skill(id: &str, directory: &str) -> InstalledSkill {
    InstalledSkill {
        id: id.to_string(),
        name: "Code review".to_string(),
        description: Some("Reviews a change.".to_string()),
        directory: directory.to_string(),
        repo_owner: None,
        repo_name: None,
        repo_branch: None,
        readme_url: None,
        apps: SkillApps::default(),
        installed_at: 1,
        content_hash: None,
        updated_at: 0,
    }
}

#[test]
fn upstream_catalog_rows_become_small_product_rows() {
    let item = catalog_item(upstream(false)).expect("remote row is installable");
    assert_eq!(
        item,
        SkillCatalogItem {
            id: "anthropics/skills:skills/code-review".to_string(),
            name: "Code review".to_string(),
            description: Some("Reviews a change.".to_string()),
            source: crate::domain::SkillSource {
                owner: "anthropics".to_string(),
                repository: "skills".to_string(),
                branch: "main".to_string(),
                directory: "skills/code-review".to_string(),
            },
            installed: false,
            mirror_used: false,
        }
    );
    let json = serde_json::to_string(&item).expect("serialize product row");
    assert!(!json.contains("readmeUrl"));
}

#[test]
fn trusted_transport_evidence_is_projected_without_changing_source_identity() {
    let mut raw = upstream(false);
    raw.mirror_used = true;
    let item = catalog_item(raw).expect("remote row is installable");
    assert!(item.mirror_used);
    assert_eq!(item.source.owner, "anthropics");
    assert_eq!(item.source.repository, "skills");
}

#[test]
fn local_only_rows_do_not_pretend_to_be_downloadable_catalog_entries() {
    let mut local = upstream(true);
    local.repo_owner = None;
    assert!(catalog_item(local).is_none());
}

#[test]
fn corrupt_rows_are_filtered_and_an_empty_legacy_branch_uses_the_safe_sentinel() {
    let mut empty_branch = upstream(true);
    empty_branch.repo_branch = Some(String::new());
    assert_eq!(
        catalog_item(empty_branch)
            .expect("empty branch uses upstream's default-branch sentinel")
            .source
            .branch,
        "HEAD"
    );

    let mut mismatched = upstream(false);
    mismatched.key = "someone/else:payload".to_string();
    assert!(catalog_item(mismatched).is_none());
}

#[test]
fn legacy_network_errors_become_product_network_errors() {
    let raw =
        r#"{"code":"DOWNLOAD_TIMEOUT","context":{"timeout":"60"},"suggestion":"checkNetwork"}"#;
    assert_eq!(legacy_code(raw).as_deref(), Some("DOWNLOAD_TIMEOUT"));
    let error = install_failed(raw);
    assert_eq!(error.code, ErrorCode::NetworkError);
    assert_eq!(error.message_key, "error.skill.downloadFailed");
    assert_eq!(
        error.remediation.as_deref(),
        Some("error.remediation.checkInternetConnection")
    );
}

#[test]
fn unknown_upstream_failures_are_redacted_and_stay_behind_details() {
    let error = install_failed("clone failed with token=ghp_secret_value");
    assert_eq!(error.code, ErrorCode::InstallFailed);
    assert_eq!(error.message_key, "error.skill.installFailed");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("ghp_secret_value"));
}

#[test]
fn local_zip_failures_are_classified_without_exposing_native_paths() {
    for code in ["EMPTY_ARCHIVE", "NO_SKILLS_IN_ZIP"] {
        let error = zip_install_failed(format!(r#"{{"code":"{code}"}}"#));
        assert_eq!(error.message_key, "error.skill.zipNoSkills");
    }

    for raw in [
        r#"{"code":"ARCHIVE_TOO_LARGE"}"#,
        r#"{"code":"ARCHIVE_TOO_MANY_ENTRIES"}"#,
        "invalid Zip archive: invalid central directory",
    ] {
        let error = zip_install_failed(raw);
        assert_eq!(error.message_key, "error.skill.zipInvalid");
    }

    let unknown =
        zip_install_failed("copy failed at /Users/private/skills.zip token=ghp_zip_secret_value");
    assert_eq!(unknown.message_key, "error.skill.zipInstallFailed");
    let detail = unknown.technical_message.unwrap_or_default();
    assert!(!detail.contains("/Users/private"));
    assert!(!detail.contains("ghp_zip_secret_value"));
}

#[test]
fn recovery_copy_projection_is_opaque_bounded_and_conflict_aware() {
    let raw = SkillBackupEntry {
        backup_id: "20260826_120000_code-review".to_string(),
        backup_path: "/Users/private/skill-backups/code-review".to_string(),
        created_at: 1_787_689_200,
        skill: InstalledSkill {
            name: format!("{}\0hidden", "A".repeat(140)),
            description: Some(format!("{}\nsecret", "D".repeat(520))),
            ..installed_skill("local:code-review", "code-review")
        },
    };
    let projected = project_backup(&raw, &[]).expect("safe projection");
    assert_eq!(projected.id, backup_reference(&raw.backup_id));
    assert_eq!(projected.id.len(), 64);
    assert_eq!(projected.name.chars().count(), 120);
    assert_eq!(
        projected
            .description
            .as_deref()
            .unwrap_or_default()
            .chars()
            .count(),
        500
    );
    assert!(!projected.conflicts);

    let conflict = project_backup(&raw, &[installed_skill("someone:else", "CODE-REVIEW")])
        .expect("conflicting projection");
    assert!(conflict.conflicts);

    let json = serde_json::to_string(&projected).expect("serialize recovery copy");
    for forbidden in ["/Users/private", "backupPath", "directory", "apps", "repo"] {
        assert!(!json.contains(forbidden));
    }
}

#[test]
fn corrupt_recovery_copy_metadata_is_not_projected() {
    let mut raw = SkillBackupEntry {
        backup_id: "backup-1".to_string(),
        backup_path: "/private/backup".to_string(),
        created_at: 0,
        skill: installed_skill("local:code-review", "code-review"),
    };
    assert!(project_backup(&raw, &[]).is_none());
    raw.created_at = 1_787_689_200;
    raw.skill.name = "\0\n".to_string();
    raw.skill.directory = "\0".to_string();
    assert!(project_backup(&raw, &[]).is_none());
}

#[test]
fn recovery_copy_mutation_errors_never_disclose_native_paths() {
    let restore =
        backup_restore_failed("restore failed at /Users/private/backup token=ghp_backup_secret");
    let delete =
        backup_delete_failed("delete failed at /Users/private/backup token=ghp_backup_secret");
    assert_eq!(restore.message_key, "error.skill.backupRestoreFailed");
    assert_eq!(delete.message_key, "error.skill.backupDeleteFailed");
    for error in [restore, delete] {
        let detail = error.technical_message.unwrap_or_default();
        assert!(!detail.contains("/Users/private"));
        assert!(!detail.contains("ghp_backup_secret"));
    }
}

#[test]
fn a_missing_managed_skill_has_a_stable_product_error() {
    let error = not_installed(
        crate::domain::ToolId::ClaudeCode,
        "anthropics/skills:missing",
    );
    assert_eq!(error.code, ErrorCode::ExtensionNotFound);
    assert_eq!(error.message_key, "error.skill.notInstalled");

    let secret = not_installed(
        crate::domain::ToolId::ClaudeCode,
        "missing?token=ghp_secret_value",
    );
    assert!(!secret
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("ghp_secret_value"));
}

#[test]
fn removal_failures_are_action_specific_and_redacted() {
    let error = remove_failed("backup failed with token=ghp_secret_value");
    assert_eq!(error.code, ErrorCode::UninstallFailed);
    assert_eq!(error.message_key, "error.skill.removeFailed");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("ghp_secret_value"));
}

#[test]
fn update_failures_keep_network_and_source_problems_distinct() {
    let network = update_failed(
        r#"{"code":"DOWNLOAD_TIMEOUT","context":{"timeout":"60"},"suggestion":"checkNetwork"}"#,
    );
    assert_eq!(network.code, ErrorCode::NetworkError);
    assert_eq!(network.message_key, "error.skill.updateDownloadFailed");

    let source = update_failed(
        r#"{"code":"SKILL_DIR_NOT_FOUND","context":{"path":"missing"},"suggestion":"checkRepoUrl"}"#,
    );
    assert_eq!(source.code, ErrorCode::UpdateFailed);
    assert_eq!(source.message_key, "error.skill.updateSourceInvalid");

    let unknown = update_failed("replace failed token=ghp_secret_value");
    assert_eq!(unknown.message_key, "error.skill.updateFailed");
    assert!(!unknown
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("ghp_secret_value"));
}

#[test]
fn repository_rows_use_opaque_stable_ids_and_small_product_fields() {
    let raw = SkillRepo {
        owner: "anthropics".to_string(),
        name: "skills".to_string(),
        branch: "main".to_string(),
        enabled: true,
    };
    let projected = project_repository(&raw);
    assert_eq!(projected.id.len(), 64);
    assert_eq!(projected.id, repository_id("anthropics", "skills"));
    assert_eq!(projected.owner, "anthropics");
    assert_eq!(projected.repository, "skills");
    let json = serde_json::to_string(&projected).expect("serialize repository");
    for forbidden in ["token", "path", "content", "readme"] {
        assert!(!json.contains(forbidden));
    }
}

#[test]
fn repository_save_reuses_existing_casing_and_upstream_validation() {
    let existing = vec![SkillRepo {
        owner: "ExampleOrg".to_string(),
        name: "Example-Skills".to_string(),
        branch: "main".to_string(),
        enabled: true,
    }];
    let updated = repository_from_draft(
        SkillRepositoryDraft {
            owner: "exampleorg".to_string(),
            repository: "example-skills".to_string(),
            branch: "release/v2".to_string(),
            enabled: false,
        },
        &existing,
    )
    .expect("valid repository");
    assert_eq!(updated.owner, "ExampleOrg");
    assert_eq!(updated.name, "Example-Skills");
    assert_eq!(updated.branch, "release/v2");
    assert!(!updated.enabled);

    let error = repository_from_draft(
        SkillRepositoryDraft {
            owner: "exampleorg".to_string(),
            repository: "../escape".to_string(),
            branch: "main".to_string(),
            enabled: true,
        },
        &[],
    )
    .expect_err("unsafe repository must be rejected");
    assert_eq!(error.code, ErrorCode::ConfigParseFailed);
    assert_eq!(error.message_key, "error.skill.repositoryInvalid");
}

#[tokio::test]
#[serial_test::serial]
async fn an_unreachable_repository_is_a_failed_check_not_an_empty_answer() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let store = store();
    // Coordinates upstream refuses before touching the network: the repository
    // cannot be checked, which is not the same as "checked, nothing new".
    let mut unreachable = installed_skill("bad/skills:code-review", "code-review");
    unreachable.repo_owner = Some("../bad".to_string());
    unreachable.repo_name = Some("skills".to_string());
    unreachable.repo_branch = Some("main".to_string());
    store
        .state
        .db
        .save_skill(&unreachable)
        .expect("seed repository-backed Skill");

    let error = store
        .updates()
        .await
        .expect_err("an unreachable repository must not read as 'no updates'");
    assert_eq!(error.code, ErrorCode::UpstreamError);
    assert_eq!(error.message_key, "error.skill.updateCheckFailed");
}

#[tokio::test]
#[serial_test::serial]
async fn local_only_skills_have_nothing_to_check_and_report_no_updates() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let store = store();
    store
        .state
        .db
        .save_skill(&installed_skill("local", "local-skill"))
        .expect("seed local Skill");

    let updates = store.updates().await.expect("nothing to check succeeds");
    assert!(updates.is_empty());
}
