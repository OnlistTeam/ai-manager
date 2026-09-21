//! Product facade over the mature upstream Skill discovery/install engine.
//!
//! Only this layer knows `SkillService`, `AppState`, `AppType`, or the legacy
//! structured-error string. Application and UI receive product domain types.

use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::Manager;

use crate::compat::ccswitch::extension::{detail, project_installed_skill};
use crate::compat::ccswitch::provider::app_type_for;
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, SkillBackup, SkillCatalogItem, SkillRepository,
    SkillRepositoryDraft, SkillSource, SkillUpdate, ToolId,
};
use crate::services::skill::{
    DiscoverableSkill, Skill as UpstreamSkill, SkillBackupEntry as UpstreamSkillBackup,
    SkillRepo as UpstreamSkillRepo, SkillService,
};
use crate::store::AppState;

mod updates;

pub struct SkillCatalogStore {
    state: AppState,
    service: SkillService,
}

impl SkillCatalogStore {
    pub fn open(app_handle: &tauri::AppHandle) -> Result<Self, AppError> {
        let state = app_handle.try_state::<AppState>().ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.skill.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        Ok(Self {
            state: state.inner().clone(),
            service: SkillService::new(),
        })
    }

    pub async fn list(&self) -> Result<Vec<SkillCatalogItem>, AppError> {
        let repos = self.state.db.get_skill_repos().map_err(catalog_failed)?;
        let skills = self
            .service
            .list_skills(repos, &self.state.db)
            .await
            .map_err(catalog_failed)?;
        Ok(skills.into_iter().filter_map(catalog_item).collect())
    }

    pub async fn install(
        &self,
        tool: ToolId,
        item: &SkillCatalogItem,
    ) -> Result<Extension, AppError> {
        let source = &item.source;
        let discoverable = DiscoverableSkill {
            key: item.id.clone(),
            name: item.name.clone(),
            description: item.description.clone().unwrap_or_default(),
            directory: source.directory.clone(),
            readme_url: None,
            repo_owner: source.owner.clone(),
            repo_name: source.repository.clone(),
            repo_branch: source.branch.clone(),
            mirror_used: false,
        };
        let app_type = app_type_for(tool);
        let installed = self
            .service
            .install(&self.state.db, &discoverable, &app_type)
            .await
            .map_err(install_failed)?;
        Ok(project_installed_skill(tool, &installed, &app_type))
    }

    /// Reuse the upstream local-archive engine unchanged. It owns extraction
    /// budgets, zip-slip/symlink protection, SSOT persistence and tool sync;
    /// this facade only projects its installed rows into the product model.
    pub fn install_zip(&self, tool: ToolId, path: &Path) -> Result<Vec<Extension>, AppError> {
        let app_type = app_type_for(tool);
        SkillService::install_from_zip(&self.state.db, path, &app_type)
            .map(|installed| {
                installed
                    .iter()
                    .map(|skill| project_installed_skill(tool, skill, &app_type))
                    .collect()
            })
            .map_err(zip_install_failed)
    }

    pub fn backups(&self) -> Result<Vec<SkillBackup>, AppError> {
        let installed =
            SkillService::get_all_installed(&self.state.db).map_err(backup_list_failed)?;
        SkillService::list_backups()
            .map_err(backup_list_failed)
            .map(|backups| {
                backups
                    .iter()
                    .filter_map(|backup| project_backup(backup, &installed))
                    .collect()
            })
    }

    pub fn backup_target(&self, id: &str) -> Result<SkillBackup, AppError> {
        self.backups()?
            .into_iter()
            .find(|backup| backup.id == id)
            .ok_or_else(|| backup_not_found(id))
    }

    pub fn restore_backup(&self, tool: ToolId, id: &str) -> Result<Extension, AppError> {
        let raw = self.require_raw_backup(id)?;
        let installed =
            SkillService::get_all_installed(&self.state.db).map_err(backup_restore_failed)?;
        if backup_conflicts(&raw, &installed) {
            return Err(
                AppError::new(ErrorCode::OperationConflict, "error.skill.backupConflict")
                    .with_technical("a managed Skill already uses this recovery copy's identity")
                    .with_remediation("error.remediation.retryOrViewDetails"),
            );
        }

        let app_type = app_type_for(tool);
        SkillService::restore_from_backup(&self.state.db, &raw.backup_id, &app_type)
            .map(|restored| project_installed_skill(tool, &restored, &app_type))
            .map_err(backup_restore_failed)
    }

    pub fn delete_backup(&self, id: &str) -> Result<Vec<SkillBackup>, AppError> {
        let raw = self.require_raw_backup(id)?;
        SkillService::delete_backup(&raw.backup_id).map_err(backup_delete_failed)?;
        self.backups()
    }

    fn require_raw_backup(&self, id: &str) -> Result<UpstreamSkillBackup, AppError> {
        SkillService::list_backups()
            .map_err(backup_list_failed)?
            .into_iter()
            .find(|backup| backup_reference(&backup.backup_id) == id)
            .ok_or_else(|| backup_not_found(id))
    }

    /// Not upstream `check_updates`: that one turns an unreachable repository
    /// into an empty list, which the renderer would show as "up to date".
    pub async fn updates(&self) -> Result<Vec<SkillUpdate>, AppError> {
        let updates = updates::check(&self.service, &self.state.db).await?;
        Ok(updates
            .into_iter()
            .filter_map(|update| {
                let name = update.name.trim().to_string();
                (!update.id.trim().is_empty() && !name.is_empty()).then_some(SkillUpdate {
                    id: update.id,
                    name,
                })
            })
            .collect())
    }

    pub fn repositories(&self) -> Result<Vec<SkillRepository>, AppError> {
        self.state
            .db
            .get_skill_repos()
            .map(|repositories| repositories.iter().map(project_repository).collect())
            .map_err(repository_list_failed)
    }

    pub fn save_repository(
        &self,
        draft: SkillRepositoryDraft,
    ) -> Result<Vec<SkillRepository>, AppError> {
        let existing = self
            .state
            .db
            .get_skill_repos()
            .map_err(repository_list_failed)?;
        let repository = repository_from_draft(draft, &existing)?;
        self.state
            .db
            .save_skill_repo(&repository)
            .map_err(repository_save_failed)?;
        self.repositories()
    }

    pub fn remove_repository(&self, id: &str) -> Result<Vec<SkillRepository>, AppError> {
        let repositories = self
            .state
            .db
            .get_skill_repos()
            .map_err(repository_list_failed)?;
        let target = repositories
            .iter()
            .find(|repository| repository_id(&repository.owner, &repository.name) == id)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::ExtensionNotFound,
                    "error.skill.repositoryNotFound",
                )
                .with_technical(detail(id))
            })?;
        self.state
            .db
            .delete_skill_repo(&target.owner, &target.name)
            .map_err(repository_remove_failed)?;
        self.repositories()
    }

    /// Resolve the user-facing target from the authoritative installed list.
    /// The renderer sends only an id, so it cannot choose the task name or
    /// smuggle a different extension kind into a destructive operation.
    pub fn installed(&self, tool: ToolId, id: &str) -> Result<Extension, AppError> {
        let app_type = app_type_for(tool);
        let installed = SkillService::get_all_installed(&self.state.db).map_err(remove_failed)?;
        let raw = installed
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| not_installed(tool, id))?;
        Ok(project_installed_skill(tool, raw, &app_type))
    }

    pub fn update_target(&self, tool: ToolId, id: &str) -> Result<Extension, AppError> {
        let app_type = app_type_for(tool);
        let installed = SkillService::get_all_installed(&self.state.db).map_err(update_failed)?;
        let raw = installed
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| not_installed(tool, id))?;
        if raw
            .repo_owner
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
            || raw
                .repo_name
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(
                AppError::new(ErrorCode::UpdateFailed, "error.skill.updateUnsupported")
                    .with_technical("managed Skill has no repository provenance"),
            );
        }
        Ok(project_installed_skill(tool, raw, &app_type))
    }

    pub async fn update(&self, tool: ToolId, id: &str) -> Result<Extension, AppError> {
        let target = self.update_target(tool, id)?;
        let app_type = app_type_for(tool);
        let updated = self
            .service
            .update_skill(&self.state.db, id)
            .await
            .map_err(update_failed)?;
        let projected = project_installed_skill(tool, &updated, &app_type);
        if projected.kind != ExtensionKind::Skill || projected.id != target.id {
            return Err(
                AppError::new(ErrorCode::UpdateFailed, "error.skill.updateVerifyFailed")
                    .with_technical("updated Skill did not match the resolved target")
                    .with_remediation("error.remediation.retryOrViewDetails"),
            );
        }
        Ok(projected)
    }

    /// Remove one managed Skill everywhere, then independently prove that its
    /// database record is gone. The upstream service owns backup creation,
    /// path validation and preservation of unrecognized external copies.
    pub fn remove(&self, tool: ToolId, id: &str) -> Result<(), AppError> {
        let installed = SkillService::get_all_installed(&self.state.db).map_err(remove_failed)?;
        if !installed.iter().any(|entry| entry.id == id) {
            return Err(not_installed(tool, id));
        }

        let outcome = SkillService::uninstall(&self.state.db, id).map_err(remove_failed)?;
        if outcome.pi_cleanup_incomplete {
            log::warn!("Skill removal completed with an unrecognized external copy preserved");
        }

        let remaining = SkillService::get_all_installed(&self.state.db).map_err(remove_failed)?;
        if remaining.iter().any(|entry| entry.id == id) {
            return Err(AppError::new(
                ErrorCode::UninstallFailed,
                "error.skill.removeVerifyFailed",
            )
            .with_technical("Skill remained in the managed inventory after removal")
            .with_remediation("error.remediation.retryOrViewDetails"));
        }
        Ok(())
    }
}

fn catalog_item(raw: UpstreamSkill) -> Option<SkillCatalogItem> {
    let owner = raw.repo_owner?;
    let repository = raw.repo_name?;
    let branch = raw.repo_branch?;
    let branch = if branch.trim().is_empty() {
        "HEAD".to_string()
    } else {
        branch
    };
    let description = raw.description.trim().to_string();
    let item = SkillCatalogItem {
        id: raw.key,
        name: raw.name.trim().to_string(),
        description: (!description.is_empty()).then_some(description),
        source: SkillSource {
            owner,
            repository,
            branch,
            directory: raw.directory,
        },
        installed: raw.installed,
        mirror_used: raw.mirror_used,
    };
    item.validate_source().ok().map(|()| item)
}

fn repository_id(owner: &str, repository: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ai-manager:skill-repository\0");
    hasher.update(owner.as_bytes());
    hasher.update(b"\0");
    hasher.update(repository.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn backup_reference(backup_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ai-manager:skill-backup\0");
    hasher.update(backup_id.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn bounded_metadata(raw: &str, max_chars: usize) -> Option<String> {
    let value = raw
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(max_chars)
        .collect::<String>();
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn backup_conflicts(
    backup: &UpstreamSkillBackup,
    installed: &[crate::app_config::InstalledSkill],
) -> bool {
    installed.iter().any(|skill| {
        skill.id == backup.skill.id
            || skill
                .directory
                .eq_ignore_ascii_case(&backup.skill.directory)
    })
}

fn project_backup(
    raw: &UpstreamSkillBackup,
    installed: &[crate::app_config::InstalledSkill],
) -> Option<SkillBackup> {
    const MAX_BACKUP_UNIX_SECONDS: i64 = 253_402_300_799;
    if raw.backup_id.trim().is_empty() || !(1..=MAX_BACKUP_UNIX_SECONDS).contains(&raw.created_at) {
        return None;
    }
    let name = bounded_metadata(&raw.skill.name, 120)
        .or_else(|| bounded_metadata(&raw.skill.directory, 120))?;
    Some(SkillBackup {
        id: backup_reference(&raw.backup_id),
        name,
        description: raw
            .skill
            .description
            .as_deref()
            .and_then(|value| bounded_metadata(value, 500)),
        created_at: raw.created_at,
        conflicts: backup_conflicts(raw, installed),
    })
}

fn project_repository(raw: &UpstreamSkillRepo) -> SkillRepository {
    SkillRepository {
        id: repository_id(&raw.owner, &raw.name),
        owner: raw.owner.clone(),
        repository: raw.name.clone(),
        branch: raw.branch.clone(),
        enabled: raw.enabled,
    }
}

fn repository_from_draft(
    draft: SkillRepositoryDraft,
    existing: &[UpstreamSkillRepo],
) -> Result<UpstreamSkillRepo, AppError> {
    let owner = draft.owner.trim();
    let repository = draft.repository.trim();
    let branch = draft.branch.trim();
    SkillService::validate_repo_ref(owner, repository, branch).map_err(repository_invalid)?;

    let canonical = existing.iter().find(|candidate| {
        candidate.owner.eq_ignore_ascii_case(owner)
            && candidate.name.eq_ignore_ascii_case(repository)
    });
    Ok(UpstreamSkillRepo {
        owner: canonical
            .map(|candidate| candidate.owner.clone())
            .unwrap_or_else(|| owner.to_string()),
        name: canonical
            .map(|candidate| candidate.name.clone())
            .unwrap_or_else(|| repository.to_string()),
        branch: branch.to_string(),
        enabled: draft.enabled,
    })
}

fn catalog_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.skill.catalogFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.checkInternetConnection")
}

fn update_check_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.skill.updateCheckFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn repository_list_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.skill.repositoryListFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn repository_invalid<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(
        ErrorCode::ConfigParseFailed,
        "error.skill.repositoryInvalid",
    )
    .with_technical(detail(error))
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn repository_save_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.skill.repositorySaveFailed",
    )
    .with_technical(detail(error))
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn repository_remove_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.skill.repositoryRemoveFailed",
    )
    .with_technical(detail(error))
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn backup_list_failed<E: std::fmt::Display>(_error: E) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.skill.backupListFailed")
        .with_technical("upstream Skill recovery-copy inventory could not be read")
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn backup_not_found(id: &str) -> AppError {
    AppError::new(ErrorCode::BackupNotFound, "error.skill.backupNotFound")
        .with_technical(detail(id))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn backup_restore_failed<E: std::fmt::Display>(_error: E) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.skill.backupRestoreFailed",
    )
    .with_technical("upstream Skill recovery-copy restore failed")
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn backup_delete_failed<E: std::fmt::Display>(_error: E) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.skill.backupDeleteFailed",
    )
    .with_technical("upstream Skill recovery-copy deletion failed")
    .with_remediation("error.remediation.retryOrViewDetails")
}

#[derive(Deserialize)]
struct LegacySkillError {
    code: String,
}

fn legacy_code(raw: &str) -> Option<String> {
    let parsed: LegacySkillError = serde_json::from_str(raw).ok()?;
    Some(parsed.code)
}

fn install_failed<E: std::fmt::Display>(error: E) -> AppError {
    let raw = error.to_string();
    let code = legacy_code(&raw);
    let (error_code, message_key, remediation) = match code.as_deref() {
        Some("DOWNLOAD_TIMEOUT" | "DOWNLOAD_FAILED") => (
            ErrorCode::NetworkError,
            "error.skill.downloadFailed",
            "error.remediation.checkInternetConnection",
        ),
        Some("SKILL_DIRECTORY_CONFLICT") => (
            ErrorCode::InstallFailed,
            "error.skill.conflict",
            "error.remediation.retryOrViewDetails",
        ),
        Some(
            "INVALID_REPO_REF"
            | "INVALID_SKILL_DIRECTORY"
            | "SKILL_DIR_NOT_FOUND"
            | "SKILL_NOT_FOUND",
        ) => (
            ErrorCode::InstallFailed,
            "error.skill.invalidSource",
            "error.remediation.retryOrViewDetails",
        ),
        _ => (
            ErrorCode::InstallFailed,
            "error.skill.installFailed",
            "error.remediation.retryOrViewDetails",
        ),
    };
    AppError::new(error_code, message_key)
        .with_technical(detail(raw))
        .with_remediation(remediation)
}

fn zip_install_failed<E: std::fmt::Display>(error: E) -> AppError {
    let raw = error.to_string();
    let legacy = legacy_code(&raw);
    let invalid_archive = raw.contains("Failed to read ZIP file")
        || raw.to_ascii_lowercase().contains("invalid zip archive")
        || raw.to_ascii_lowercase().contains("unsupported zip archive")
        || raw.to_ascii_lowercase().contains("checksum failed");
    let (message_key, technical) = match legacy.as_deref() {
        Some("EMPTY_ARCHIVE" | "NO_SKILLS_IN_ZIP") => (
            "error.skill.zipNoSkills",
            "local Skill archive contains no installable Skill",
        ),
        Some("ARCHIVE_TOO_LARGE" | "ARCHIVE_TOO_MANY_ENTRIES" | "INVALID_SKILL_DIRECTORY") => (
            "error.skill.zipInvalid",
            "local Skill archive failed structural validation",
        ),
        _ if invalid_archive => (
            "error.skill.zipInvalid",
            "local Skill archive could not be parsed safely",
        ),
        _ => (
            "error.skill.zipInstallFailed",
            "upstream local Skill archive installation failed",
        ),
    };
    // Upstream I/O errors can contain the selected path or a temporary path.
    // Keep those paths behind the native boundary even in Task details.
    AppError::new(ErrorCode::InstallFailed, message_key)
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn not_installed(tool: ToolId, id: &str) -> AppError {
    AppError::new(ErrorCode::ExtensionNotFound, "error.skill.notInstalled")
        .with_technical(detail(format!("{}/{id}", tool.as_str())))
}

fn remove_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UninstallFailed, "error.skill.removeFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn update_failed<E: std::fmt::Display>(error: E) -> AppError {
    let raw = error.to_string();
    let code = legacy_code(&raw);
    let (error_code, message_key, remediation) = match code.as_deref() {
        Some("DOWNLOAD_TIMEOUT" | "DOWNLOAD_FAILED") => (
            ErrorCode::NetworkError,
            "error.skill.updateDownloadFailed",
            "error.remediation.checkInternetConnection",
        ),
        Some(
            "INVALID_REPO_REF"
            | "INVALID_SKILL_DIRECTORY"
            | "SKILL_DIR_NOT_FOUND"
            | "SKILL_NOT_FOUND",
        ) => (
            ErrorCode::UpdateFailed,
            "error.skill.updateSourceInvalid",
            "error.remediation.retryOrViewDetails",
        ),
        _ => (
            ErrorCode::UpdateFailed,
            "error.skill.updateFailed",
            "error.remediation.retryOrViewDetails",
        ),
    };
    AppError::new(error_code, message_key)
        .with_technical(detail(raw))
        .with_remediation(remediation)
}

#[cfg(test)]
#[path = "skill_catalog/tests.rs"]
mod tests;
