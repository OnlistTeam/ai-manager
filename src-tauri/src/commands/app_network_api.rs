//! Thin outbound network-proxy product commands.

use crate::application::network_proxy::NetworkProxyService;
use crate::domain::{AppError, NetworkProxyMode, NetworkProxySettings};

use super::app_api::blocking;

#[tauri::command]
pub async fn app_network_proxy_get(
    app_handle: tauri::AppHandle,
) -> Result<NetworkProxySettings, AppError> {
    blocking(move || NetworkProxyService::load(&app_handle)).await
}

#[tauri::command]
pub async fn app_network_proxy_save(
    app_handle: tauri::AppHandle,
    mode: NetworkProxyMode,
    url: Option<String>,
) -> Result<NetworkProxySettings, AppError> {
    blocking(move || NetworkProxyService::save(&app_handle, mode, url)).await
}
