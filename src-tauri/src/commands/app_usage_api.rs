//! Thin Advanced Usage product commands (ADR-0006).

use crate::application::usage_overview::UsageOverviewService;
use crate::domain::{AppError, UsageOverview, UsageRefreshResult};

use super::app_api::blocking;

#[tauri::command]
pub async fn app_usage_overview(app_handle: tauri::AppHandle) -> Result<UsageOverview, AppError> {
    blocking(move || UsageOverviewService::load(&app_handle)).await
}

#[tauri::command]
pub async fn app_usage_refresh(
    app_handle: tauri::AppHandle,
) -> Result<UsageRefreshResult, AppError> {
    UsageOverviewService::refresh(&app_handle).await
}
