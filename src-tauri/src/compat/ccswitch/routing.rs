//! Product-safe facade over the inherited CC Switch routing engine (ADR-0007).
//!
//! This is the only product module that may see proxy configuration, provider
//! records and health rows. The Domain projection excludes credentials, live
//! backups, notes, filesystem paths and raw request errors.

use std::sync::Arc;

use serde_json::Value;

use crate::database::Database;
use crate::domain::{
    AppError, ErrorCode, RoutingOverview, RoutingPickup, RoutingProvider, RoutingTarget, ToolId,
};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::provider::Provider;
use crate::services::ProxyService;
use crate::store::AppState;

/// One tool the inherited gateway can route, and how its live config is
/// read and compared (ADR-0054).
struct RoutingApp {
    tool: ToolId,
    app: &'static str,
    /// When an open session of the tool sees a change to its settings file.
    pickup: RoutingPickup,
    /// How the live config (as the inherited proxy reads it) is compared.
    layout: &'static [LivePart],
    read_live: fn() -> Result<Value, String>,
}

fn read_claude_live() -> Result<Value, String> {
    let path = crate::config::get_claude_settings_path();
    crate::config::read_json_file::<Value>(&path).map_err(|error| error.to_string())
}

fn read_codex_live() -> Result<Value, String> {
    crate::codex_config::read_codex_live_settings().map_err(|error| error.to_string())
}

fn read_gemini_live() -> Result<Value, String> {
    crate::gemini_config::read_gemini_env()
        .map(|env| crate::gemini_config::env_to_json(&env))
        .map_err(|error| error.to_string())
}

fn read_grok_live() -> Result<Value, String> {
    crate::grok_config::read_grok_live_settings().map_err(|error| error.to_string())
}

const ROUTING_APPS: [RoutingApp; 4] = [
    RoutingApp {
        tool: ToolId::ClaudeCode,
        app: "claude",
        pickup: RoutingPickup::Live,
        layout: &[LivePart::Document],
        read_live: read_claude_live,
    },
    RoutingApp {
        tool: ToolId::Codex,
        app: "codex",
        pickup: RoutingPickup::AtStart,
        // auth.json holds the ChatGPT login; the inherited restore owns it.
        layout: &[LivePart::Toml("config"), LivePart::AsBackedUp("auth")],
        read_live: read_codex_live,
    },
    RoutingApp {
        tool: ToolId::GeminiCli,
        app: "gemini",
        pickup: RoutingPickup::AtStart,
        layout: &[LivePart::Document],
        read_live: read_gemini_live,
    },
    RoutingApp {
        tool: ToolId::GrokBuild,
        app: "grokbuild",
        pickup: RoutingPickup::AtStart,
        layout: &[LivePart::Toml("config")],
        read_live: read_grok_live,
    },
];

fn detail<E: std::fmt::Display>(error: E) -> String {
    truncate_tail(&redact_secrets(&error.to_string()), 20, 2000)
}

fn read_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.routing.readFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn change_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.routing.changeFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn conflict(message_key: &'static str, technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::OperationConflict, message_key)
        .with_technical(detail(technical.into()))
}

/// Which tools have an inherited local-routing data plane. Lightweight
/// provider failover is only offered for these, so both answers come from
/// this one table.
pub fn supports_local_routing(tool: ToolId) -> bool {
    ROUTING_APPS.iter().any(|entry| entry.tool == tool)
}

fn routing_app(tool: ToolId) -> Result<&'static RoutingApp, AppError> {
    ROUTING_APPS
        .iter()
        .find(|entry| entry.tool == tool)
        .ok_or_else(|| {
            conflict(
                "error.routing.unsupportedTool",
                format!(
                    "{} has no inherited local-routing data plane",
                    tool.as_str()
                ),
            )
        })
}

fn provider_id(raw: &str) -> Result<&str, AppError> {
    let normalized = raw.trim();
    if normalized.is_empty() || normalized.len() > 256 {
        return Err(conflict(
            "error.routing.providerUnavailable",
            "provider id is empty or exceeds 256 bytes",
        ));
    }
    Ok(normalized)
}

fn supports_failover(app: &str, provider: &Provider) -> bool {
    crate::proxy::provider_router::provider_supports_failover(app, provider)
}

/// Handle over inherited routing state. All fields stay private to the facade.
pub struct RoutingStore {
    db: Arc<Database>,
    proxy: ProxyService,
    mutation_lock: Arc<tokio::sync::Mutex<()>>,
}

impl RoutingStore {
    pub fn open(app_handle: &tauri::AppHandle) -> Result<Self, AppError> {
        let state = tauri::Manager::try_state::<AppState>(app_handle).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.routing.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        Ok(Self::with_parts(
            state.db.clone(),
            state.proxy_service.clone(),
            state.routing_mutation_lock.clone(),
        ))
    }

    fn with_parts(
        db: Arc<Database>,
        proxy: ProxyService,
        mutation_lock: Arc<tokio::sync::Mutex<()>>,
    ) -> Self {
        Self {
            db,
            proxy,
            mutation_lock,
        }
    }

    pub async fn overview(&self) -> Result<RoutingOverview, AppError> {
        let status = self.proxy.get_status().await.map_err(read_failed)?;
        let mut targets = Vec::with_capacity(ROUTING_APPS.len());
        for entry in &ROUTING_APPS {
            targets.push(self.target(entry).await?);
        }

        Ok(RoutingOverview {
            running: status.running,
            address: status.running.then_some(status.address),
            port: status.running.then_some(status.port),
            active_connections: u64::try_from(status.active_connections).unwrap_or(u64::MAX),
            total_requests: status.total_requests,
            success_requests: status.success_requests,
            failed_requests: status.failed_requests,
            failover_count: status.failover_count,
            targets,
        })
    }

    async fn target(&self, entry: &RoutingApp) -> Result<RoutingTarget, AppError> {
        let app = entry.app;
        let config = self
            .db
            .get_proxy_config_for_app(app)
            .await
            .map_err(read_failed)?;
        let providers = self.db.get_all_providers(app).map_err(read_failed)?;
        let app_type = app.parse().map_err(read_failed)?;
        let current_id = crate::settings::get_effective_current_provider(&self.db, &app_type)
            .map_err(read_failed)?;
        let unavailable = forwarding::unavailable_reason(
            &app_type,
            current_id.as_deref().and_then(|id| providers.get(id)),
        );
        let upstream_queue = self.db.get_failover_queue(app).map_err(read_failed)?;

        let mut queue = Vec::new();
        for item in upstream_queue {
            let Some(provider) = providers.get(&item.provider_id) else {
                continue;
            };
            if !supports_failover(app, provider) {
                continue;
            }
            let health = self
                .db
                .get_provider_health(&provider.id, app)
                .await
                .map_err(read_failed)?;
            queue.push(RoutingProvider {
                id: provider.id.clone(),
                name: provider.name.clone(),
                priority: Some(u32::try_from(queue.len() + 1).unwrap_or(u32::MAX)),
                current: current_id.as_deref() == Some(provider.id.as_str()),
                healthy: health.is_healthy,
                consecutive_failures: health.consecutive_failures,
            });
        }

        let current_provider = match current_id.as_deref() {
            Some(id) => {
                if let Some(projected) = queue.iter().find(|provider| provider.id == id) {
                    Some(projected.clone())
                } else if let Some(provider) = providers.get(id) {
                    let health = self
                        .db
                        .get_provider_health(&provider.id, app)
                        .await
                        .map_err(read_failed)?;
                    Some(RoutingProvider {
                        id: provider.id.clone(),
                        name: provider.name.clone(),
                        priority: None,
                        current: true,
                        healthy: health.is_healthy,
                        consecutive_failures: health.consecutive_failures,
                    })
                } else {
                    None
                }
            }
            None => None,
        };

        let mut available = providers
            .values()
            .filter(|provider| !provider.in_failover_queue && supports_failover(app, provider))
            .map(|provider| RoutingProvider {
                id: provider.id.clone(),
                name: provider.name.clone(),
                priority: None,
                current: current_id.as_deref() == Some(provider.id.as_str()),
                healthy: true,
                consecutive_failures: 0,
            })
            .collect::<Vec<_>>();
        available.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(RoutingTarget {
            tool: entry.tool,
            takeover_enabled: config.enabled,
            auto_failover_enabled: config.auto_failover_enabled,
            current_provider,
            queue,
            available,
            unavailable,
            pickup: entry.pickup,
        })
    }

    pub async fn set_takeover(
        &self,
        tool: ToolId,
        enabled: bool,
    ) -> Result<RoutingOverview, AppError> {
        let entry = routing_app(tool)?;
        let _guard = self.mutation_lock.lock().await;
        if !enabled {
            self.release_unlocked(entry).await?;
            return self.overview().await;
        }
        // A tool whose endpoint the gateway cannot carry is never taken over:
        // every request it sent would fail (ADR-0054).
        if let Some(reason) = self.target(entry).await?.unavailable {
            return Err(conflict(
                "error.routing.cannotForward",
                format!("{} cannot be forwarded: {reason:?}", entry.app),
            ));
        }
        self.proxy
            .set_takeover_for_app(entry.app, true)
            .await
            .map_err(change_failed)?;
        self.note_owned_settings(entry).await;
        self.overview().await
    }

    pub async fn set_failover(
        &self,
        tool: ToolId,
        enabled: bool,
    ) -> Result<RoutingOverview, AppError> {
        let entry = routing_app(tool)?;
        let app = entry.app;
        let _guard = self.mutation_lock.lock().await;
        if !enabled {
            self.clear_auto_failover(app).await?;
            return self.overview().await;
        }
        let mut config = self
            .db
            .get_proxy_config_for_app(app)
            .await
            .map_err(change_failed)?;
        if !config.enabled {
            return Err(conflict(
                "error.routing.takeoverRequired",
                format!("{app} takeover must be enabled before failover"),
            ));
        }

        let providers = self.db.get_all_providers(app).map_err(change_failed)?;
        let mut queue = self
            .db
            .get_failover_queue(app)
            .map_err(change_failed)?
            .into_iter()
            .filter(|item| {
                providers
                    .get(&item.provider_id)
                    .is_some_and(|provider| supports_failover(app, provider))
            })
            .collect::<Vec<_>>();
        let mut auto_added = None;

        if queue.is_empty() {
            let app_type = app.parse().map_err(change_failed)?;
            let current = crate::settings::get_effective_current_provider(&self.db, &app_type)
                .map_err(change_failed)?
                .ok_or_else(|| {
                    conflict(
                        "error.routing.providerUnavailable",
                        format!("{app} has no current provider to seed P1"),
                    )
                })?;
            let provider = providers.get(&current).ok_or_else(|| {
                conflict(
                    "error.routing.providerUnavailable",
                    format!("{app} current provider is missing"),
                )
            })?;
            if !supports_failover(app, provider) {
                return Err(conflict(
                    "error.routing.providerNotEligible",
                    format!("{app} provider {current} is not failover eligible"),
                ));
            }
            self.db
                .add_to_failover_queue(app, &current)
                .map_err(change_failed)?;
            auto_added = Some(current);
            queue = self
                .db
                .get_failover_queue(app)
                .map_err(change_failed)?
                .into_iter()
                .filter(|item| {
                    providers
                        .get(&item.provider_id)
                        .is_some_and(|provider| supports_failover(app, provider))
                })
                .collect();
        }

        let p1 = queue
            .first()
            .map(|item| item.provider_id.as_str())
            .ok_or_else(|| {
                conflict(
                    "error.routing.providerUnavailable",
                    format!("{app} failover queue has no eligible P1"),
                )
            })?;
        if let Err(error) = self.proxy.switch_proxy_target(app, p1).await {
            if let Some(id) = auto_added {
                let _ = self.db.remove_from_failover_queue(app, &id);
            }
            return Err(change_failed(error));
        }

        config.auto_failover_enabled = true;
        self.db
            .update_proxy_config_for_app(config)
            .await
            .map_err(change_failed)?;
        self.note_owned_settings(entry).await;
        self.overview().await
    }

    pub async fn add_to_queue(
        &self,
        tool: ToolId,
        raw_provider_id: &str,
    ) -> Result<RoutingOverview, AppError> {
        let entry = routing_app(tool)?;
        let app = entry.app;
        let id = provider_id(raw_provider_id)?;
        let _guard = self.mutation_lock.lock().await;
        let provider = self
            .db
            .get_provider_by_id(id, app)
            .map_err(change_failed)?
            .ok_or_else(|| {
                conflict(
                    "error.routing.providerUnavailable",
                    format!("{app} provider {id} does not exist"),
                )
            })?;
        if !supports_failover(app, &provider) {
            return Err(conflict(
                "error.routing.providerNotEligible",
                format!("{app} provider {id} is not failover eligible"),
            ));
        }
        self.db
            .add_to_failover_queue(app, id)
            .map_err(change_failed)?;
        self.overview().await
    }

    pub async fn remove_from_queue(
        &self,
        tool: ToolId,
        raw_provider_id: &str,
    ) -> Result<RoutingOverview, AppError> {
        let entry = routing_app(tool)?;
        let app = entry.app;
        let id = provider_id(raw_provider_id)?;
        let _guard = self.mutation_lock.lock().await;
        let config = self
            .db
            .get_proxy_config_for_app(app)
            .await
            .map_err(change_failed)?;
        let providers = self.db.get_all_providers(app).map_err(change_failed)?;
        let queue = self
            .db
            .get_failover_queue(app)
            .map_err(change_failed)?
            .into_iter()
            .filter(|item| {
                providers
                    .get(&item.provider_id)
                    .is_some_and(|provider| supports_failover(app, provider))
            })
            .collect::<Vec<_>>();
        if !queue.iter().any(|item| item.provider_id == id) {
            return self.overview().await;
        }
        if config.auto_failover_enabled {
            if queue.len() <= 1 {
                return Err(conflict(
                    "error.routing.queueLocked",
                    format!("{app} cannot remove the final provider while failover is enabled"),
                ));
            }
            let app_type = app.parse().map_err(change_failed)?;
            let current = crate::settings::get_effective_current_provider(&self.db, &app_type)
                .map_err(change_failed)?;
            if current.as_deref() == Some(id) {
                return Err(conflict(
                    "error.routing.queueLocked",
                    format!("{app} cannot remove its active target while failover is enabled"),
                ));
            }
        }
        self.db
            .remove_from_failover_queue(app, id)
            .map_err(change_failed)?;
        self.overview().await
    }

    pub async fn switch_provider(
        &self,
        tool: ToolId,
        raw_provider_id: &str,
    ) -> Result<RoutingOverview, AppError> {
        let entry = routing_app(tool)?;
        let app = entry.app;
        let id = provider_id(raw_provider_id)?;
        let _guard = self.mutation_lock.lock().await;
        let config = self
            .db
            .get_proxy_config_for_app(app)
            .await
            .map_err(change_failed)?;
        if !config.enabled {
            return Err(conflict(
                "error.routing.takeoverRequired",
                format!("{app} takeover must be enabled before hot switching"),
            ));
        }
        self.proxy
            .switch_proxy_target(app, id)
            .await
            .map_err(change_failed)?;
        self.note_owned_settings(entry).await;
        self.overview().await
    }

    pub async fn stop_all(&self) -> Result<RoutingOverview, AppError> {
        let _guard = self.mutation_lock.lock().await;
        self.stop_all_unlocked().await?;
        self.overview().await
    }

    async fn clear_auto_failover(&self, app: &str) -> Result<(), AppError> {
        let mut config = self
            .db
            .get_proxy_config_for_app(app)
            .await
            .map_err(change_failed)?;
        if !config.auto_failover_enabled {
            return Ok(());
        }
        config.auto_failover_enabled = false;
        self.db
            .update_proxy_config_for_app(config)
            .await
            .map_err(change_failed)
    }
}

mod endpoint_change;
mod forwarding;
pub(crate) mod owned_settings;
mod release;
pub mod trace;

use owned_settings::LivePart;

#[cfg(test)]
#[path = "routing/tests.rs"]
mod tests;
