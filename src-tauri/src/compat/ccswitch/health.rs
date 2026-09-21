//! Read-only upstream projection for Quick Check (spec §37 / §38).
//!
//! It only aggregates facts that already exist in the database and the live config files. It runs
//! no commands, makes no network requests and never tries to "repair" the state it reads; in
//! particular it never calls the provider stream check.

use crate::app_config::AppType;
use crate::domain::{
    AppError, ConfigHealth, ConfigReadStatus, ErrorCode, HealthProviderTarget, HealthSnapshot,
    McpHealth, ProviderHealth, ToolId,
};
use crate::services::{McpService, ProviderService};
use crate::store::AppState;

use super::provider::{app_type_for, provider_from_upstream, upstream_detail};

fn snapshot_failed(error: &crate::error::AppError) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.health.snapshotFailed")
        .with_technical(upstream_detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

/// The same "local first, database as fallback" selection semantics as `ProviderService::current`,
/// but without cleaning up a stale local setting. Quick Check is a strictly read-only path and does
/// not even perform that self-healing write.
fn selected_provider_id(
    state: &AppState,
    app_type: &AppType,
    providers: &indexmap::IndexMap<String, crate::provider::Provider>,
) -> Result<String, AppError> {
    if app_type.is_additive_mode() {
        return Ok(String::new());
    }
    if let Some(local) = crate::settings::get_current_provider(app_type) {
        if providers.contains_key(&local) {
            return Ok(local);
        }
    }
    Ok(state
        .db
        .get_current_provider(app_type.as_str())
        .map_err(|error| snapshot_failed(&error))?
        .filter(|id| providers.contains_key(id))
        .unwrap_or_default())
}

fn provider_health(state: &AppState, tool: ToolId) -> Result<ProviderHealth, AppError> {
    let app_type = app_type_for(tool);
    let providers = ProviderService::list(state, app_type.clone()).map_err(|error| {
        AppError::new(ErrorCode::UpstreamError, "error.health.snapshotFailed")
            .with_technical(upstream_detail(&error))
            .with_remediation("error.remediation.retryOrViewDetails")
    })?;
    let current = selected_provider_id(state, &app_type, &providers)?;

    let configured: Vec<_> = if app_type.is_additive_mode() {
        providers
            .values()
            // `None` is legacy/unknown and is treated as live-managed everywhere
            // else; only an explicit false means “database only”.
            .filter(|provider| {
                provider
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.live_config_managed)
                    != Some(false)
            })
            .collect()
    } else {
        providers.get(&current).into_iter().collect()
    };

    let check_targets = configured
        .iter()
        .filter_map(|raw| {
            let projected = provider_from_upstream(tool, raw, &current);
            projected.testable.then_some(HealthProviderTarget {
                provider_id: projected.id,
                name: projected.name,
            })
        })
        .collect();
    let configured_count = u32::try_from(configured.len()).unwrap_or(u32::MAX);

    Ok(ProviderHealth {
        tool,
        configured: configured_count > 0,
        configured_count,
        check_targets,
    })
}

fn mcp_health(state: &AppState) -> Result<McpHealth, AppError> {
    let servers = McpService::get_all_servers(state).map_err(|error| {
        AppError::new(ErrorCode::UpstreamError, "error.health.snapshotFailed")
            .with_technical(upstream_detail(&error))
            .with_remediation("error.remediation.retryOrViewDetails")
    })?;
    let product_apps = [
        AppType::Claude,
        AppType::Codex,
        AppType::OpenCode,
        AppType::Gemini,
    ];
    let enabled = servers
        .values()
        .filter(|server| {
            product_apps
                .iter()
                .any(|app_type| server.apps.is_enabled_for(app_type))
        })
        .count();

    Ok(McpHealth {
        total: u32::try_from(servers.len()).unwrap_or(u32::MAX),
        enabled: u32::try_from(enabled).unwrap_or(u32::MAX),
    })
}

/// Whether the file(s) the live reader would open exist. `None` means this
/// app has no known on-disk layout here, so only the reader can judge it.
///
/// The upstream readers report a missing file as an error, and a tool that
/// has simply never been configured must not show up as "unreadable".
fn live_config_present(app_type: &AppType) -> Option<bool> {
    match app_type {
        AppType::Claude => Some(crate::config::get_claude_settings_path().exists()),
        AppType::Codex => Some(
            crate::codex_config::get_codex_auth_path().exists()
                || crate::codex_config::get_codex_config_path().exists(),
        ),
        AppType::Gemini => Some(crate::gemini_config::get_gemini_env_path().exists()),
        AppType::OpenCode => Some(crate::opencode_config::get_opencode_config_path().exists()),
        _ => None,
    }
}

fn config_status<P, F>(tool: ToolId, config_present: &mut P, read_live: &mut F) -> ConfigReadStatus
where
    P: FnMut(AppType) -> Option<bool>,
    F: FnMut(AppType) -> Result<serde_json::Value, crate::error::AppError>,
{
    let app_type = app_type_for(tool);
    if config_present(app_type.clone()) == Some(false) {
        return ConfigReadStatus::Missing;
    }
    match read_live(app_type) {
        Ok(_) => ConfigReadStatus::Readable,
        Err(error) => {
            log::warn!(
                "Quick Check could not read the {} live config: {}",
                tool.as_str(),
                upstream_detail(&error)
            );
            ConfigReadStatus::Unreadable
        }
    }
}

fn snapshot_with_reader<P, F>(
    state: &AppState,
    installed: &[ToolId],
    mut config_present: P,
    mut read_live: F,
) -> Result<HealthSnapshot, AppError>
where
    P: FnMut(AppType) -> Option<bool>,
    F: FnMut(AppType) -> Result<serde_json::Value, crate::error::AppError>,
{
    let mut providers = Vec::with_capacity(installed.len());
    let mut configs = Vec::with_capacity(installed.len());
    for &tool in installed {
        providers.push(provider_health(state, tool)?);
        let status = config_status(tool, &mut config_present, &mut read_live);
        configs.push(ConfigHealth { tool, status });
    }

    Ok(HealthSnapshot {
        providers,
        configs,
        mcp: mcp_health(state)?,
    })
}

/// Handle for upstream health reads. It only holds a cloned `AppState` and leaks no upstream type to the layers above.
pub struct HealthStore {
    state: AppState,
}

impl HealthStore {
    pub fn open(app_handle: &tauri::AppHandle) -> Result<Self, AppError> {
        let state = tauri::Manager::try_state::<AppState>(app_handle).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.health.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        Ok(Self {
            state: state.inner().clone(),
        })
    }

    pub fn snapshot(&self, installed: &[ToolId]) -> Result<HealthSnapshot, AppError> {
        snapshot_with_reader(
            &self.state,
            installed,
            |app_type| live_config_present(&app_type),
            ProviderService::read_live_settings,
        )
    }
}

#[cfg(test)]
#[path = "health/tests.rs"]
mod tests;
