//! Outbound proxy use case (ADR-0056).

use crate::compat::ccswitch::network_proxy::NetworkProxyStore;
use crate::domain::{AppError, NetworkProxyMode, NetworkProxySettings};

pub struct NetworkProxyService;

impl NetworkProxyService {
    pub fn load(app_handle: &tauri::AppHandle) -> Result<NetworkProxySettings, AppError> {
        NetworkProxyStore::open(app_handle)?.load()
    }

    pub fn save(
        app_handle: &tauri::AppHandle,
        mode: NetworkProxyMode,
        url: Option<String>,
    ) -> Result<NetworkProxySettings, AppError> {
        NetworkProxyStore::open(app_handle)?.save(mode, url)
    }
}
