//! Switching or saving a tool's endpoint with its local route in mind
//! (ADR-0054 decision 8).
//!
//! Every entry point that changes which endpoint a tool uses (Home, the
//! endpoint list, the tray, the edit form) comes through here, so a routed
//! tool is never left pointing at the local gateway once its endpoint can no
//! longer go through it.

use crate::application::provider_directory::ProviderDirectory;
use crate::compat::ccswitch::routing::RoutingStore;
use crate::domain::{AppError, Provider, ProviderDraft, RoutingPickup, ToolId};

pub struct EndpointChange;

impl EndpointChange {
    /// Makes `provider_id` the tool's endpoint. The pickup is set when the
    /// switch ended the tool's route.
    pub async fn switch(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        provider_id: &str,
    ) -> Result<(Vec<Provider>, Option<RoutingPickup>), AppError> {
        let worker = app_handle.clone();
        let id = provider_id.to_string();
        RoutingStore::open(app_handle)?
            .switch_endpoint(tool, provider_id, move || {
                ProviderDirectory::switch(&worker, tool, &id)
            })
            .await
    }

    /// Saves one endpoint. The pickup is set when the save ended the tool's
    /// route.
    pub async fn save(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        provider_id: &str,
        draft: ProviderDraft,
    ) -> Result<(Vec<Provider>, Option<RoutingPickup>), AppError> {
        let worker = app_handle.clone();
        let id = provider_id.to_string();
        RoutingStore::open(app_handle)?
            .save_endpoint(tool, provider_id, move || {
                ProviderDirectory::save(&worker, tool, &id, &draft)
            })
            .await
    }
}
