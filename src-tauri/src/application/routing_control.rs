//! Local routing orchestration (ADR-0007).

use std::sync::Arc;

use crate::compat::ccswitch::routing::RoutingStore;
use crate::domain::{
    AppError, ErrorCode, RoutingLiveModeOutcome, RoutingOverview, RoutingTraceSnapshot, ToolId,
};
use crate::infrastructure::RoutingTraceLog;

pub struct RoutingControl;

impl RoutingControl {
    fn store(app_handle: &tauri::AppHandle) -> Result<RoutingStore, AppError> {
        RoutingStore::open(app_handle)
    }

    pub async fn overview(app_handle: &tauri::AppHandle) -> Result<RoutingOverview, AppError> {
        Self::store(app_handle)?.overview().await
    }

    pub async fn set_takeover(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        enabled: bool,
    ) -> Result<RoutingOverview, AppError> {
        Self::store(app_handle)?.set_takeover(tool, enabled).await
    }

    pub async fn set_failover(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        enabled: bool,
    ) -> Result<RoutingOverview, AppError> {
        Self::store(app_handle)?.set_failover(tool, enabled).await
    }

    pub async fn add_to_queue(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        provider_id: &str,
    ) -> Result<RoutingOverview, AppError> {
        Self::store(app_handle)?
            .add_to_queue(tool, provider_id)
            .await
    }

    pub async fn remove_from_queue(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        provider_id: &str,
    ) -> Result<RoutingOverview, AppError> {
        Self::store(app_handle)?
            .remove_from_queue(tool, provider_id)
            .await
    }

    pub async fn switch_provider(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        provider_id: &str,
    ) -> Result<RoutingOverview, AppError> {
        Self::store(app_handle)?
            .switch_provider(tool, provider_id)
            .await
    }

    pub async fn stop_all(app_handle: &tauri::AppHandle) -> Result<RoutingOverview, AppError> {
        Self::store(app_handle)?.stop_all().await
    }

    /// Live routing on or off (ADR-0050); the renderer confirms first.
    pub async fn set_live_mode(
        app_handle: &tauri::AppHandle,
        enabled: bool,
    ) -> Result<RoutingLiveModeOutcome, AppError> {
        Self::store(app_handle)?.set_live_mode(enabled).await
    }

    /// The in-memory trace of recent requests (ADR-0050).
    pub fn trace(app_handle: &tauri::AppHandle) -> Result<RoutingTraceSnapshot, AppError> {
        let log =
            tauri::Manager::try_state::<Arc<RoutingTraceLog>>(app_handle).ok_or_else(|| {
                AppError::new(ErrorCode::Internal, "error.routing.storeUnavailable")
                    .with_remediation("error.remediation.retryOrViewDetails")
            })?;
        Ok(log.snapshot())
    }
}
