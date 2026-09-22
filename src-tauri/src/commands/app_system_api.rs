//! Thin product commands for settings, backup, import and Quick Check.

use std::path::PathBuf;

use chrono::Local;
use tauri_plugin_dialog::DialogExt;

use crate::application::backup_directory::BackupDirectory;
use crate::application::backup_schedule::BackupScheduleService;
use crate::application::desktop_preferences::DesktopPreferencesService;
use crate::application::health_check::HealthCheck;
use crate::application::import_existing::ImportExisting;
use crate::application::product_settings::ProductSettingsService;
use crate::application::reveal::open_in_file_manager;
use crate::domain::{
    AppError, BackupExportOutcome, BackupImportOutcome, BackupList, BackupSchedule,
    DesktopPreferences, ErrorCode, HealthSnapshot, ImportOutcome, ImportPreview, ProductSettings,
    RestoreOutcome, TerminalAppId,
};

use super::app_api::{blocking, parse_tool};

#[tauri::command]
pub async fn app_settings_get(app_handle: tauri::AppHandle) -> Result<ProductSettings, AppError> {
    blocking(move || ProductSettingsService::load(&app_handle)).await
}

#[tauri::command]
pub async fn app_settings_save(
    app_handle: tauri::AppHandle,
    settings: ProductSettings,
) -> Result<ProductSettings, AppError> {
    let worker_handle = app_handle.clone();
    let saved = blocking(move || ProductSettingsService::save(&worker_handle, settings)).await?;
    crate::tray::schedule_tray_refresh(&app_handle);
    Ok(saved)
}

#[tauri::command]
pub async fn app_terminals_list() -> Result<Vec<TerminalAppId>, AppError> {
    blocking(|| Ok(ProductSettingsService::available_terminals())).await
}

#[tauri::command]
pub async fn app_desktop_preferences_get() -> Result<DesktopPreferences, AppError> {
    blocking(DesktopPreferencesService::load).await
}

#[tauri::command]
pub async fn app_desktop_preferences_save(
    app_handle: tauri::AppHandle,
    settings: DesktopPreferences,
) -> Result<DesktopPreferences, AppError> {
    let saved = blocking(move || DesktopPreferencesService::save(settings)).await?;
    if let Some(tray) = app_handle.tray_by_id(crate::tray::TRAY_ID) {
        if let Err(error) = tray.set_visible(saved.show_in_tray) {
            log::warn!("Could not apply tray visibility until restart: {error}");
        }
    }
    #[cfg(target_os = "macos")]
    if !saved.show_in_tray {
        crate::tray::apply_tray_policy(&app_handle, true);
    }
    Ok(saved)
}

#[tauri::command]
pub async fn app_backups_list() -> Result<BackupList, AppError> {
    blocking(BackupDirectory::list).await
}

#[tauri::command]
pub async fn app_backup_create(app_handle: tauri::AppHandle) -> Result<BackupList, AppError> {
    blocking(move || BackupDirectory::create(&app_handle)).await
}

#[tauri::command]
pub async fn app_backup_restore(
    app_handle: tauri::AppHandle,
    backup: String,
) -> Result<RestoreOutcome, AppError> {
    // Restore owns its async mutex and blocking-thread handoff.
    let restored = BackupDirectory::restore(&app_handle, backup).await?;
    crate::tray::schedule_tray_refresh(&app_handle);
    Ok(restored)
}

#[tauri::command]
pub async fn app_backup_delete(
    app_handle: tauri::AppHandle,
    backup: String,
) -> Result<BackupList, AppError> {
    blocking(move || BackupDirectory::delete(&app_handle, &backup)).await
}

#[tauri::command]
pub async fn app_backup_rename(
    app_handle: tauri::AppHandle,
    backup: String,
    name: String,
) -> Result<BackupList, AppError> {
    blocking(move || BackupDirectory::rename(&app_handle, &backup, &name)).await
}

#[tauri::command]
pub async fn app_backup_export(
    app_handle: tauri::AppHandle,
) -> Result<BackupExportOutcome, AppError> {
    let picker_handle = app_handle.clone();
    let default_name = format!(
        "ai-manager-config-{}.sql",
        Local::now().format("%Y%m%d-%H%M%S")
    );
    let target = blocking(move || {
        picker_handle
            .dialog()
            .file()
            .add_filter("AI Manager configuration", &["sql"])
            .set_file_name(default_name)
            .blocking_save_file()
            .map(|selected| {
                selected.simplified().into_path().map_err(|_| {
                    AppError::new(
                        ErrorCode::ConfigWriteFailed,
                        "error.backup.selectionUnavailable",
                    )
                    .with_technical("file picker returned a non-filesystem destination")
                    .with_remediation("error.remediation.retryOrViewDetails")
                })
            })
            .transpose()
    })
    .await?;

    let Some(target) = target else {
        return Ok(BackupExportOutcome::Cancelled);
    };
    blocking(move || BackupDirectory::export_archive(&app_handle, target)).await?;
    Ok(BackupExportOutcome::Exported)
}

#[tauri::command]
pub async fn app_backup_import(
    app_handle: tauri::AppHandle,
) -> Result<BackupImportOutcome, AppError> {
    let picker_handle = app_handle.clone();
    let source = blocking(move || {
        picker_handle
            .dialog()
            .file()
            .add_filter("AI Manager configuration", &["sql"])
            .blocking_pick_file()
            .map(|selected| {
                selected.simplified().into_path().map_err(|_| {
                    AppError::new(
                        ErrorCode::ConfigParseFailed,
                        "error.backup.selectionUnavailable",
                    )
                    .with_technical("file picker returned a non-filesystem source")
                    .with_remediation("error.remediation.retryOrViewDetails")
                })
            })
            .transpose()
    })
    .await?;

    let Some(source) = source else {
        return Ok(BackupImportOutcome::Cancelled);
    };
    let imported = BackupDirectory::import_archive(&app_handle, source).await?;
    crate::tray::schedule_tray_refresh(&app_handle);
    Ok(BackupImportOutcome::Imported {
        backups: imported.backups,
        tools_out_of_sync: imported.tools_out_of_sync,
    })
}

#[tauri::command]
pub async fn app_backup_schedule_get() -> Result<BackupSchedule, AppError> {
    blocking(|| Ok(BackupScheduleService::load())).await
}

#[tauri::command]
pub async fn app_backup_schedule_save(
    schedule: BackupSchedule,
) -> Result<BackupSchedule, AppError> {
    blocking(move || BackupScheduleService::save(schedule)).await
}

#[tauri::command]
pub async fn app_import_preview(app_handle: tauri::AppHandle) -> Result<ImportPreview, AppError> {
    blocking(move || ImportExisting::preview(&app_handle)).await
}

#[tauri::command]
pub async fn app_import_run(app_handle: tauri::AppHandle) -> Result<ImportOutcome, AppError> {
    let imported = ImportExisting::run(&app_handle).await?;
    crate::tray::schedule_tray_refresh(&app_handle);
    Ok(imported)
}

#[tauri::command]
pub async fn app_health_snapshot(
    app_handle: tauri::AppHandle,
    tools: Vec<String>,
) -> Result<HealthSnapshot, AppError> {
    let installed = tools
        .iter()
        .map(|tool| parse_tool(tool))
        .collect::<Result<Vec<_>, _>>()?;
    blocking(move || HealthCheck::snapshot(&app_handle, &installed)).await
}

/// Shows a path in the system file manager.
///
/// The UI hands over a path it has just displayed to the user, so the only job
/// here is to survive the path having moved since: an existing item is revealed
/// (selected in Finder/Explorer), and anything else falls back to the nearest
/// folder that still exists. Revealing something the user cannot see the
/// location of is worse than opening one level up.
#[tauri::command]
pub async fn app_reveal_path(app_handle: tauri::AppHandle, path: String) -> Result<(), AppError> {
    let target = PathBuf::from(path);
    blocking(move || open_in_file_manager(&app_handle, &target).map_err(reveal_error)).await
}

fn reveal_error(technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::LaunchFailed, "error.system.revealFailed")
        .with_technical(technical)
        .with_remediation("error.remediation.checkPermissions")
}

/// The one pane that can restore a refused automation grant.
///
/// macOS asks for permission to control Terminal exactly once. After "Don't
/// Allow" every later attempt fails without a prompt, and the only way back is
/// this pane — which is deep enough that describing it in prose is not a
/// remedy. The destination is fixed here rather than passed in, for the same
/// reason as the legal notice links: the renderer names the intent, the native
/// side owns the address.
#[cfg(target_os = "macos")]
pub const AUTOMATION_PRIVACY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation";

/// Opens the system pane where a refused permission can be granted again.
///
/// Only macOS has a pane to open: Windows and Linux never ask for this grant,
/// so the command reports that there is nothing to open rather than pretending
/// to succeed.
#[tauri::command]
pub async fn app_open_automation_settings(
    #[allow(unused_variables)] app_handle: tauri::AppHandle,
) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_opener::OpenerExt;
        app_handle
            .opener()
            .open_url(AUTOMATION_PRIVACY_SETTINGS_URL, None::<String>)
            .map_err(|error| {
                AppError::new(ErrorCode::LaunchFailed, "error.system.openSettingsFailed")
                    .with_technical(error.to_string())
                    .with_remediation("error.remediation.retryOrViewDetails")
            })?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    Err(
        AppError::new(ErrorCode::Unsupported, "error.system.openSettingsFailed")
            .with_technical("no automation privacy pane exists on this platform"),
    )
}
