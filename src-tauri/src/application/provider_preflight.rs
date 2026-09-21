//! User-action-bound provider preflight and lightweight failover.
//!
//! This use case never installs a proxy or changes the request path. It checks saved provider
//! addresses and, only after an explicit Use/Open/Try Next action, delegates one configuration
//! switch to the inherited provider transaction.

use crate::application::product_settings::ProductSettingsService;
use crate::application::provider_directory::ProviderDirectory;
use crate::compat::ccswitch::routing::supports_local_routing;
use crate::domain::{
    AppError, ErrorCode, Provider, ProviderKind, ProviderPreflightOutcome, ProviderPreflightStatus,
    ProviderReachability, ProviderTestResult, ToolId, MAX_PROVIDER_ENDPOINT_CANDIDATES,
};

pub struct ProviderPreflightService;

impl ProviderPreflightService {
    pub async fn prepare_activation(
        app_handle: tauri::AppHandle,
        tool: ToolId,
        provider_id: String,
    ) -> Result<ProviderPreflightOutcome, AppError> {
        let (automatic, providers) = snapshot(&app_handle, tool).await?;
        let target = provider(&providers, tool, &provider_id)?;
        if !target.testable {
            let switched = switch_provider(&app_handle, tool, provider_id.clone()).await?;
            return Ok(outcome(
                ProviderPreflightStatus::NotChecked,
                Some(provider_id),
                switched,
                vec![],
            ));
        }

        let checked =
            ProviderDirectory::test_for_preflight(&app_handle, tool, &provider_id).await?;
        if healthy(&checked) {
            let switched = switch_provider(&app_handle, tool, provider_id.clone()).await?;
            return Ok(outcome(
                ProviderPreflightStatus::Ready,
                Some(provider_id),
                switched,
                vec![checked],
            ));
        }
        if automatic {
            return try_next_from(&app_handle, tool, provider_id, providers, vec![checked]).await;
        }
        Ok(outcome(
            ProviderPreflightStatus::Unreachable,
            Some(provider_id),
            providers,
            vec![checked],
        ))
    }

    pub async fn prepare_launch(
        app_handle: tauri::AppHandle,
        tool: ToolId,
    ) -> Result<ProviderPreflightOutcome, AppError> {
        let (automatic, providers) = snapshot(&app_handle, tool).await?;
        let Some(active) = providers.iter().find(|candidate| candidate.active) else {
            return Ok(outcome(
                ProviderPreflightStatus::NotChecked,
                None,
                providers,
                vec![],
            ));
        };
        let origin = active.id.clone();
        if !active.testable {
            return Ok(outcome(
                ProviderPreflightStatus::NotChecked,
                Some(origin),
                providers,
                vec![],
            ));
        }

        let checked = ProviderDirectory::test_for_preflight(&app_handle, tool, &origin).await?;
        if healthy(&checked) {
            return Ok(outcome(
                ProviderPreflightStatus::Ready,
                Some(origin),
                providers,
                vec![checked],
            ));
        }
        if automatic {
            return try_next_from(&app_handle, tool, origin, providers, vec![checked]).await;
        }
        Ok(outcome(
            ProviderPreflightStatus::Unreachable,
            Some(origin),
            providers,
            vec![checked],
        ))
    }

    pub async fn try_next_healthy(
        app_handle: tauri::AppHandle,
        tool: ToolId,
        failed_provider_id: String,
    ) -> Result<ProviderPreflightOutcome, AppError> {
        let (_, providers) = snapshot(&app_handle, tool).await?;
        provider(&providers, tool, &failed_provider_id)?;
        try_next_from(&app_handle, tool, failed_provider_id, providers, vec![]).await
    }
}

async fn snapshot(
    app_handle: &tauri::AppHandle,
    tool: ToolId,
) -> Result<(bool, Vec<Provider>), AppError> {
    let worker = app_handle.clone();
    worker_task(move || {
        let automatic = ProductSettingsService::load(&worker)?.automatic_provider_failover;
        let providers = ProviderDirectory::list(&worker, tool)?;
        Ok((automatic, providers))
    })
    .await
}

async fn switch_provider(
    app_handle: &tauri::AppHandle,
    tool: ToolId,
    provider_id: String,
) -> Result<Vec<Provider>, AppError> {
    let worker = app_handle.clone();
    worker_task(move || ProviderDirectory::switch(&worker, tool, &provider_id)).await
}

async fn try_next_from(
    app_handle: &tauri::AppHandle,
    tool: ToolId,
    origin: String,
    providers: Vec<Provider>,
    mut checks: Vec<ProviderTestResult>,
) -> Result<ProviderPreflightOutcome, AppError> {
    // The failover set is the routing data-plane table, not a second list.
    if !supports_local_routing(tool) {
        return Ok(outcome(
            ProviderPreflightStatus::Unreachable,
            Some(origin),
            providers,
            checks,
        ));
    }
    let candidates = ordered_candidates(&providers, &origin)?;
    for batch in candidates.chunks(MAX_PROVIDER_ENDPOINT_CANDIDATES) {
        let ids = batch.to_vec();
        let mut measured =
            ProviderDirectory::test_failover_candidates(app_handle, tool, &ids).await?;
        let next = measured
            .iter()
            .find(|result| healthy(result))
            .map(|result| result.provider_id.clone());
        checks.append(&mut measured);
        if let Some(next) = next {
            let switched = switch_provider(app_handle, tool, next).await?;
            return Ok(outcome(
                ProviderPreflightStatus::FailedOver,
                Some(origin),
                switched,
                checks,
            ));
        }
    }

    Ok(outcome(
        ProviderPreflightStatus::Unreachable,
        Some(origin),
        providers,
        checks,
    ))
}

fn ordered_candidates(providers: &[Provider], origin: &str) -> Result<Vec<String>, AppError> {
    let start = providers
        .iter()
        .position(|provider| provider.id == origin)
        .ok_or_else(|| provider_not_found(origin))?;
    Ok((1..providers.len())
        .map(|offset| &providers[(start + offset) % providers.len()])
        .filter(|candidate| {
            !candidate.active
                && candidate.kind == ProviderKind::Custom
                && candidate.testable
                && candidate.id != origin
        })
        .map(|candidate| candidate.id.clone())
        .collect())
}

fn provider<'a>(
    providers: &'a [Provider],
    tool: ToolId,
    id: &str,
) -> Result<&'a Provider, AppError> {
    providers
        .iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| {
            provider_not_found(id).with_technical(format!("{}/provider-not-found", tool.as_str()))
        })
}

fn provider_not_found(id: &str) -> AppError {
    AppError::new(ErrorCode::ProviderNotFound, "error.provider.notFound")
        .with_technical(format!("provider id was not found ({})", id.len()))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn healthy(result: &ProviderTestResult) -> bool {
    result.reachability != ProviderReachability::Failed
}

fn outcome(
    status: ProviderPreflightStatus,
    origin_provider_id: Option<String>,
    providers: Vec<Provider>,
    checks: Vec<ProviderTestResult>,
) -> ProviderPreflightOutcome {
    let active_provider_id = providers
        .iter()
        .find(|provider| provider.active)
        .map(|provider| provider.id.clone());
    ProviderPreflightOutcome {
        status,
        origin_provider_id,
        active_provider_id,
        providers,
        checks,
    }
}

async fn worker_task<T, F>(work: F) -> Result<T, AppError>
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

#[cfg(test)]
mod tests {
    use super::ordered_candidates;
    use crate::domain::{Provider, ProviderKind, ToolId};

    fn provider(id: &str, kind: ProviderKind, active: bool, testable: bool) -> Provider {
        Provider {
            additive: false,
            id: id.to_string(),
            tool: ToolId::ClaudeCode,
            name: id.to_string(),
            kind,
            active,
            base_url: None,
            api_key: None,
            website_url: None,
            testable,
            can_remove: !active,
        }
    }

    #[test]
    fn candidates_wrap_after_the_failed_service_and_exclude_active_or_unsafe_rows() {
        let providers = vec![
            provider("official", ProviderKind::Official, true, true),
            provider("first", ProviderKind::Custom, false, true),
            provider("not-testable", ProviderKind::Custom, false, false),
            provider("second", ProviderKind::Custom, false, true),
        ];
        assert_eq!(
            ordered_candidates(&providers, "first").expect("ordered candidates"),
            vec!["second"]
        );
        assert_eq!(
            ordered_candidates(&providers, "official").expect("ordered candidates"),
            vec!["first", "second"]
        );
    }

    #[test]
    fn an_unknown_origin_fails_without_guessing_a_candidate() {
        let providers = vec![provider("compatible", ProviderKind::Custom, false, true)];
        assert!(ordered_candidates(&providers, "missing").is_err());
    }
}
