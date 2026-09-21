//! Thin Advanced Routing product commands (ADR-0007).

use crate::application::routing_control::RoutingControl;
use crate::domain::{AppError, RoutingOverview};

use super::app_api::parse_tool;

#[tauri::command]
pub async fn app_routing_overview(
    app_handle: tauri::AppHandle,
) -> Result<RoutingOverview, AppError> {
    RoutingControl::overview(&app_handle).await
}

#[tauri::command]
pub async fn app_routing_set_takeover(
    app_handle: tauri::AppHandle,
    tool: String,
    enabled: bool,
) -> Result<RoutingOverview, AppError> {
    RoutingControl::set_takeover(&app_handle, parse_tool(&tool)?, enabled).await
}

#[tauri::command]
pub async fn app_routing_set_failover(
    app_handle: tauri::AppHandle,
    tool: String,
    enabled: bool,
) -> Result<RoutingOverview, AppError> {
    RoutingControl::set_failover(&app_handle, parse_tool(&tool)?, enabled).await
}

#[tauri::command]
pub async fn app_routing_queue_add(
    app_handle: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<RoutingOverview, AppError> {
    RoutingControl::add_to_queue(&app_handle, parse_tool(&tool)?, &provider_id).await
}

#[tauri::command]
pub async fn app_routing_queue_remove(
    app_handle: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<RoutingOverview, AppError> {
    RoutingControl::remove_from_queue(&app_handle, parse_tool(&tool)?, &provider_id).await
}

#[tauri::command]
pub async fn app_routing_switch_provider(
    app_handle: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<RoutingOverview, AppError> {
    let tool = parse_tool(&tool)?;
    let overview = RoutingControl::switch_provider(&app_handle, tool, &provider_id).await?;
    crate::tray::provider_changed(&app_handle, tool);
    Ok(overview)
}

#[tauri::command]
pub async fn app_routing_stop_all(
    app_handle: tauri::AppHandle,
) -> Result<RoutingOverview, AppError> {
    RoutingControl::stop_all(&app_handle).await
}
