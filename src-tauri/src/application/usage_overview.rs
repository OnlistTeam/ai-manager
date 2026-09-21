//! Advanced Usage use-case orchestration (ADR-0006).

use crate::compat::ccswitch::usage::UsageStore;
use crate::domain::{AppError, UsageOverview, UsageRefreshResult};

pub struct UsageOverviewService;

impl UsageOverviewService {
    pub fn load(app_handle: &tauri::AppHandle) -> Result<UsageOverview, AppError> {
        UsageStore::open(app_handle)?.overview()
    }

    pub async fn refresh(app_handle: &tauri::AppHandle) -> Result<UsageRefreshResult, AppError> {
        UsageStore::open(app_handle)?.refresh().await
    }
}
