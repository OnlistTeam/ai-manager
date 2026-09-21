//! Failover switch module
//!
//! Handles provider switching logic after a successful failover, including:
//! - Deduplication (avoid multiple requests triggering at once)
//! - Tray menu updates
//! - Frontend event emission

use crate::database::Database;
use crate::error::AppError;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

/// Failover switch manager
///
/// Handles provider switching after a successful failover, ensuring the UI directly reflects the provider currently in use.
#[derive(Clone)]
pub struct FailoverSwitchManager {
    /// Switches currently in progress (key = "app_type:provider_id")
    pending_switches: Arc<RwLock<HashSet<String>>>,
    db: Arc<Database>,
}

impl FailoverSwitchManager {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            pending_switches: Arc::new(RwLock::new(HashSet::new())),
            db,
        }
    }

    /// Attempt to perform a failover switch
    ///
    /// If the same switch is already in progress, skip it; otherwise run the switch logic.
    ///
    /// # Returns
    /// - `Ok(true)` - switch executed successfully
    /// - `Ok(false)` - switch already in progress, skipped
    /// - `Err(e)` - error occurred during the switch
    pub async fn try_switch(
        &self,
        app_handle: Option<&tauri::AppHandle>,
        app_type: &str,
        provider_id: &str,
        provider_name: &str,
    ) -> Result<bool, AppError> {
        let switch_key = format!("{app_type}:{provider_id}");

        // Dedup check: skip if the same switch is already in progress
        {
            let mut pending = self.pending_switches.write().await;
            if pending.contains(&switch_key) {
                log::debug!(
                    "[Failover] Switch already in progress, skipping: {app_type} -> {provider_id}"
                );
                return Ok(false);
            }
            pending.insert(switch_key.clone());
        }

        // Perform the switch (make sure the pending marker is cleaned up afterward)
        let result = self
            .do_switch(app_handle, app_type, provider_id, provider_name)
            .await;

        // Clean up the pending marker
        {
            let mut pending = self.pending_switches.write().await;
            pending.remove(&switch_key);
        }

        result
    }

    async fn do_switch(
        &self,
        app_handle: Option<&tauri::AppHandle>,
        app_type: &str,
        provider_id: &str,
        provider_name: &str,
    ) -> Result<bool, AppError> {
        // Check whether this app has been taken over by the proxy (enabled=true)
        // Only apps under proxy management are allowed to perform a failover switch
        let app_enabled = match self.db.get_proxy_config_for_app(app_type).await {
            Ok(config) => config.enabled,
            Err(e) => {
                log::warn!("[FO-002] Failed to read config for {app_type}: {e}, skipping switch");
                return Ok(false);
            }
        };

        if !app_enabled {
            log::debug!("[Failover] {app_type} does not have the proxy enabled, skipping switch");
            return Ok(false);
        }

        log::info!("[FO-001] Switching: {app_type} → {provider_name}");

        let mut switched = false;

        if let Some(app) = app_handle {
            if let Some(app_state) = app.try_state::<crate::store::AppState>() {
                switched = app_state
                    .proxy_service
                    .hot_switch_provider(app_type, provider_id)
                    .await
                    .map_err(AppError::Message)?
                    .logical_target_changed;

                if !switched {
                    return Ok(false);
                }

                if let Ok(new_menu) = crate::tray::create_tray_menu(app, app_state.inner()) {
                    if let Some(tray) = app.tray_by_id(crate::tray::TRAY_ID) {
                        if let Err(e) = tray.set_menu(Some(new_menu)) {
                            log::error!("[Failover] Failed to update tray menu: {e}");
                        }
                    }
                }
            }

            // Emit event to the frontend
            let event_data = serde_json::json!({
                "appType": app_type,
                "providerId": provider_id,
                "source": "failover"  // mark the source as failover
            });
            if let Err(e) = app.emit("provider-switched", event_data) {
                log::error!("[Failover] Failed to emit event: {e}");
            }
        }

        Ok(switched)
    }
}
