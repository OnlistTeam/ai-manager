//! Product commands for checking and applying managed Skill updates.

use std::sync::Arc;

use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::application::extension_directory::ExtensionDirectory;
use crate::application::skill_backup::{
    SkillBackupDirectory, SkillBackupRestoreRequest, SkillBackupRestoreService,
};
use crate::application::skill_repository::SkillRepositoryDirectory;
use crate::application::skill_update::{SkillUpdateRequest, SkillUpdateService};
use crate::application::skill_zip_installation::{
    SkillZipInstallRequest, SkillZipInstallationService,
};
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionScope, OperationId, SkillBackup, SkillRepository,
    SkillRepositoryDraft, SkillUpdate, SkillZipInstallOutcome,
};
use crate::infrastructure::OperationManager;

use super::app_api::{blocking, parse_tool};

#[tauri::command]
pub async fn app_skill_updates_check(
    app_handle: tauri::AppHandle,
) -> Result<Vec<SkillUpdate>, AppError> {
    SkillUpdateService::check(&app_handle).await
}

#[tauri::command]
pub async fn app_skill_update(
    app_handle: tauri::AppHandle,
    tool: String,
    skill: String,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<OperationId, AppError> {
    let tool = parse_tool(&tool)?;
    let resolved_handle = app_handle.clone();
    let request =
        blocking(move || SkillUpdateService::resolve(&resolved_handle, tool, &skill)).await?;
    start_skill_update(
        SkillUpdateService::system(app_handle, operations.inner().clone()),
        request,
    )
}

#[tauri::command]
pub async fn app_skill_backups_list(
    app_handle: tauri::AppHandle,
) -> Result<Vec<SkillBackup>, AppError> {
    blocking(move || SkillBackupDirectory::list(&app_handle)).await
}

#[tauri::command]
pub async fn app_skill_backup_restore(
    app_handle: tauri::AppHandle,
    tool: String,
    backup: String,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<OperationId, AppError> {
    let tool = parse_tool(&tool)?;
    let resolved_handle = app_handle.clone();
    let request =
        blocking(move || SkillBackupDirectory::resolve(&resolved_handle, tool, &backup)).await?;
    start_skill_backup_restore(
        SkillBackupRestoreService::system(app_handle, operations.inner().clone()),
        request,
    )
}

#[tauri::command]
pub async fn app_skill_backup_delete(
    app_handle: tauri::AppHandle,
    backup: String,
) -> Result<Vec<SkillBackup>, AppError> {
    blocking(move || SkillBackupDirectory::delete(&app_handle, &backup)).await
}

#[tauri::command]
pub async fn app_skill_zip_install(
    app_handle: tauri::AppHandle,
    tool: String,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<SkillZipInstallOutcome, AppError> {
    let tool = parse_tool(&tool)?;
    SkillZipInstallationService::ensure_supported(tool)?;
    let picker_handle = app_handle.clone();
    let request = blocking(move || {
        picker_handle
            .dialog()
            .file()
            .add_filter("Skill ZIP", &["zip"])
            .blocking_pick_file()
            .map(|selected| {
                selected
                    .simplified()
                    .into_path()
                    .map_err(|_| {
                        AppError::new(
                            ErrorCode::InstallFailed,
                            "error.skill.zipSelectionUnavailable",
                        )
                        .with_technical("file picker returned a non-filesystem location")
                        .with_remediation("error.remediation.retryOrViewDetails")
                    })
                    .and_then(|path| SkillZipInstallRequest::selected(tool, path))
            })
            .transpose()
    })
    .await?;

    let Some(request) = request else {
        return Ok(SkillZipInstallOutcome::Cancelled);
    };
    let operation = start_skill_zip_install(
        SkillZipInstallationService::system(app_handle, operations.inner().clone()),
        request,
    )?;
    Ok(SkillZipInstallOutcome::Started { operation })
}

#[tauri::command]
pub async fn app_skill_repositories_list(
    app_handle: tauri::AppHandle,
) -> Result<Vec<SkillRepository>, AppError> {
    blocking(move || SkillRepositoryDirectory::list(&app_handle)).await
}

#[tauri::command]
pub async fn app_skill_repository_save(
    app_handle: tauri::AppHandle,
    repository: SkillRepositoryDraft,
) -> Result<Vec<SkillRepository>, AppError> {
    blocking(move || SkillRepositoryDirectory::save(&app_handle, repository)).await
}

#[tauri::command]
pub async fn app_skill_repository_remove(
    app_handle: tauri::AppHandle,
    repository: String,
) -> Result<Vec<SkillRepository>, AppError> {
    blocking(move || SkillRepositoryDirectory::remove(&app_handle, &repository)).await
}

/// Copy one detected Skill into another tool. The renderer sends a stable Skill
/// id and a tool id only — never a path — and gets the target tool's refreshed
/// inventory back, same shape as every other extension write.
#[tauri::command]
pub async fn app_detected_skill_copy(
    app_handle: tauri::AppHandle,
    scope: ExtensionScope,
    target: String,
    skill: String,
) -> Result<Vec<Extension>, AppError> {
    let target = parse_tool(&target)?;
    blocking(move || ExtensionDirectory::copy_detected_skill(&app_handle, scope, target, &skill))
        .await
}

fn start_skill_update(
    service: SkillUpdateService,
    request: SkillUpdateRequest,
) -> Result<OperationId, AppError> {
    let id = service.begin(&request)?;
    let spawned = id.clone();
    tokio::spawn(async move { service.run(spawned, request).await });
    Ok(id)
}

fn start_skill_backup_restore(
    service: SkillBackupRestoreService,
    request: SkillBackupRestoreRequest,
) -> Result<OperationId, AppError> {
    let id = service.begin(&request)?;
    let spawned = id.clone();
    tokio::spawn(async move { service.run(spawned, request).await });
    Ok(id)
}

fn start_skill_zip_install(
    service: SkillZipInstallationService,
    request: SkillZipInstallRequest,
) -> Result<OperationId, AppError> {
    let id = service.begin(&request)?;
    let spawned = id.clone();
    tokio::spawn(async move { service.run(spawned, request).await });
    Ok(id)
}

#[cfg(test)]
#[path = "app_skill_api/tests.rs"]
mod tests;
