use std::sync::Arc;

use tauri::State;

use crate::application::app_update::AppUpdateManager;
use crate::domain::{AppError, AppUpdateStatus};

#[tauri::command]
pub async fn app_update_start(
    app_handle: tauri::AppHandle,
    updates: State<'_, Arc<AppUpdateManager>>,
    force: bool,
) -> Result<AppUpdateStatus, AppError> {
    Ok(updates.inner().start(app_handle, force))
}

#[tauri::command]
pub async fn app_update_status(
    updates: State<'_, Arc<AppUpdateManager>>,
) -> Result<AppUpdateStatus, AppError> {
    Ok(updates.status())
}

#[tauri::command]
pub async fn app_update_install_and_restart(
    app_handle: tauri::AppHandle,
    updates: State<'_, Arc<AppUpdateManager>>,
) -> Result<bool, AppError> {
    updates.install_and_restart(app_handle).await
}

#[tauri::command]
pub async fn app_update_open_download_page(
    app_handle: tauri::AppHandle,
    updates: State<'_, Arc<AppUpdateManager>>,
) -> Result<bool, AppError> {
    updates.open_download_page(&app_handle)
}
