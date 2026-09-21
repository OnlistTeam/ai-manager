//! Outbound download-proxy use case.

use crate::compat::ccswitch::network_proxy::NetworkProxyStore;
use crate::domain::{AppError, NetworkProxySettings};

pub struct NetworkProxyService;

impl NetworkProxyService {
    pub fn load(app_handle: &tauri::AppHandle) -> Result<NetworkProxySettings, AppError> {
        NetworkProxyStore::open(app_handle)?.load()
    }

    pub fn save(
        app_handle: &tauri::AppHandle,
        url: Option<String>,
    ) -> Result<NetworkProxySettings, AppError> {
        NetworkProxyStore::open(app_handle)?.save(url)
    }
}
