//! Changing a routed tool's endpoint (ADR-0054 decision 8).
//!
//! A routed tool keeps pointing at the local gateway, which forwards to the
//! tool's current endpoint. When that endpoint changes to one the gateway
//! cannot carry (an own-account login, an endpoint without an address or
//! key), keeping the route would leave the tool pointing at an address that
//! can only fail. The route ends first, exactly as turning it off does, and
//! the new endpoint is then applied directly. A change to another endpoint
//! the gateway can carry keeps the route and only moves its upstream.

use crate::app_config::AppType;
use crate::domain::{AppError, ErrorCode, RoutingPickup, ToolId};

use super::{change_failed, forwarding, RoutingApp, RoutingStore, ROUTING_APPS};

fn routing_entry(tool: ToolId) -> Option<&'static RoutingApp> {
    ROUTING_APPS.iter().find(|entry| entry.tool == tool)
}

async fn run_blocking<T, F>(work: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            AppError::new(ErrorCode::Internal, "error.command.joinFailed")
                .with_technical(error.to_string())
                .with_remediation("error.remediation.retryOrViewDetails")
        })?
}

impl RoutingStore {
    async fn is_routed(&self, entry: &RoutingApp) -> Result<bool, AppError> {
        self.db
            .get_proxy_config_for_app(entry.app)
            .await
            .map(|config| config.enabled)
            .map_err(change_failed)
    }

    /// Whether the gateway could carry `provider_id` for this tool. An
    /// endpoint that does not exist is left to the switch to report.
    fn cannot_forward(&self, entry: &RoutingApp, provider_id: &str) -> Result<bool, AppError> {
        let app_type: AppType = entry.app.parse().map_err(change_failed)?;
        let provider = self
            .db
            .get_provider_by_id(provider_id, entry.app)
            .map_err(change_failed)?;
        Ok(provider.is_some_and(|provider| {
            forwarding::unavailable_reason(&app_type, Some(&provider)).is_some()
        }))
    }

    /// Makes `provider_id` the tool's endpoint through `switch`, ending the
    /// tool's route first when the gateway could not carry the new endpoint.
    /// Returns what `switch` returned and, when the route ended, how an open
    /// session of the tool picks that up.
    ///
    /// Should the switch then fail, the route stays off: the tool is back on
    /// its previous endpoint directly, which never leaves it pointing at an
    /// address that cannot serve it.
    pub async fn switch_endpoint<T, F>(
        &self,
        tool: ToolId,
        provider_id: &str,
        switch: F,
    ) -> Result<(T, Option<RoutingPickup>), AppError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, AppError> + Send + 'static,
    {
        let Some(entry) = routing_entry(tool) else {
            return Ok((run_blocking(switch).await?, None));
        };
        let _guard = self.mutation_lock.lock().await;
        if !self.is_routed(entry).await? {
            return Ok((run_blocking(switch).await?, None));
        }
        if self.cannot_forward(entry, provider_id)? {
            self.release_unlocked(entry).await?;
            let switched = run_blocking(switch).await?;
            return Ok((switched, Some(entry.pickup)));
        }
        let switched = run_blocking(switch).await?;
        // The hot switch rewrote the routed config for the new upstream.
        self.note_owned_settings(entry).await;
        Ok((switched, None))
    }

    /// Saves `provider_id` through `save`. When it is the endpoint a routed
    /// tool uses and the gateway can no longer carry it (its key was removed,
    /// say), the route then ends. Returns what `save` returned and, when the
    /// route ended, how an open session of the tool picks that up.
    pub async fn save_endpoint<T, F>(
        &self,
        tool: ToolId,
        provider_id: &str,
        save: F,
    ) -> Result<(T, Option<RoutingPickup>), AppError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, AppError> + Send + 'static,
    {
        let Some(entry) = routing_entry(tool) else {
            return Ok((run_blocking(save).await?, None));
        };
        let _guard = self.mutation_lock.lock().await;
        let saved = run_blocking(save).await?;
        if !self.is_routed(entry).await? {
            return Ok((saved, None));
        }
        let app_type: AppType = entry.app.parse().map_err(change_failed)?;
        let current = crate::settings::get_effective_current_provider(&self.db, &app_type)
            .map_err(change_failed)?;
        if current.as_deref() != Some(provider_id) || !self.cannot_forward(entry, provider_id)? {
            return Ok((saved, None));
        }
        // The inherited save already moved the backup to the edited endpoint,
        // so the route's own settings go back to its new values.
        self.release_unlocked(entry).await?;
        Ok((saved, Some(entry.pickup)))
    }
}

#[cfg(test)]
#[path = "tests_endpoint_change.rs"]
mod tests;
