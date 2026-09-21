//! Transactional product wrapper around the upstream provider update path.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::app_config::AppType;
use crate::domain::{AppError, ErrorCode, Provider, ProviderDraft, ToolId};
use crate::provider::Provider as UpstreamProvider;
use crate::services::ProviderService;
use crate::store::AppState;

use super::{advanced, app_type_for, apply_draft, upstream_detail, ProviderStore};

pub(super) fn save(
    store: &ProviderStore,
    tool: ToolId,
    id: &str,
    draft: &ProviderDraft,
) -> Result<Vec<Provider>, AppError> {
    save_with(store, tool, id, draft, ProviderService::update)
}

pub(super) fn save_with<F>(
    store: &ProviderStore,
    tool: ToolId,
    id: &str,
    draft: &ProviderDraft,
    updater: F,
) -> Result<Vec<Provider>, AppError>
where
    F: FnOnce(
        &AppState,
        AppType,
        Option<&str>,
        UpstreamProvider,
    ) -> Result<bool, crate::error::AppError>,
{
    let original = store.find_raw(tool, id)?;
    let mut target = original.clone();
    apply_draft(tool, &mut target, draft)?;
    let app_type = app_type_for(tool);

    let update = catch_unwind(AssertUnwindSafe(|| {
        updater(&store.state, app_type.clone(), Some(id), target.clone())
    }));
    let outcome = match update {
        Ok(Ok(_)) => replace_endpoints_if_requested(store, tool, id, draft, &target)
            .and_then(|()| verify_saved(store, tool, id, draft, &target)),
        Ok(Err(error)) => Err(save_failed(error)),
        Err(payload) => Err(save_panicked(payload)),
    };

    match outcome {
        Ok(()) => match store.list(tool) {
            Ok(providers) => Ok(providers),
            Err(error) => Err(rollback_or_escalate(
                store, tool, id, draft, &original, error,
            )),
        },
        Err(error) => Err(rollback_or_escalate(
            store, tool, id, draft, &original, error,
        )),
    }
}

fn replace_endpoints_if_requested(
    store: &ProviderStore,
    tool: ToolId,
    id: &str,
    draft: &ProviderDraft,
    target: &UpstreamProvider,
) -> Result<(), AppError> {
    let requested = draft
        .advanced
        .as_ref()
        .is_some_and(|advanced| advanced.endpoint_candidates.is_some());
    if !requested {
        return Ok(());
    }
    let endpoints = target
        .meta
        .as_ref()
        .map(|meta| &meta.custom_endpoints)
        .ok_or_else(|| verify_failed("endpoint draft did not materialize provider metadata"))?;
    store
        .state
        .db
        .replace_custom_endpoints(app_type_for(tool).as_str(), id, endpoints)
        .map_err(save_failed)
}

fn verify_saved(
    store: &ProviderStore,
    tool: ToolId,
    id: &str,
    draft: &ProviderDraft,
    expected: &UpstreamProvider,
) -> Result<(), AppError> {
    let saved = store.find_raw(tool, id).map_err(|error| {
        verify_failed(
            error
                .technical_message
                .as_deref()
                .unwrap_or("saved service could not be read"),
        )
    })?;
    if saved.id != expected.id || saved.name != expected.name {
        return Err(verify_failed("saved service identity did not match"));
    }
    if draft.api_key.is_some() {
        let app_type = app_type_for(tool);
        let (_, expected_key) = expected.resolve_usage_credentials(&app_type);
        let (_, saved_key) = saved.resolve_usage_credentials(&app_type);
        if saved_key != expected_key {
            return Err(verify_failed("saved service key did not match"));
        }
    }
    if draft.models.is_some() {
        let expected_models = advanced::profile(tool, expected).models;
        let saved_models = advanced::profile(tool, &saved).models;
        if saved_models != expected_models {
            return Err(verify_failed("saved service models did not match"));
        }
    }
    if let Some(advanced_draft) = draft.advanced.as_ref() {
        if advanced_draft.base_url_changed {
            let expected_profile = advanced::profile(tool, expected);
            let saved_profile = advanced::profile(tool, &saved);
            if saved_profile.base_url != expected_profile.base_url {
                return Err(verify_failed("saved service endpoint did not match"));
            }
        }
        if advanced_draft.headers.is_some()
            && saved.settings_config.pointer("/options/headers")
                != expected.settings_config.pointer("/options/headers")
        {
            return Err(verify_failed("saved service headers did not match"));
        }
        if advanced_draft.endpoint_candidates.is_some() {
            let expected_profile = advanced::profile(tool, expected);
            let saved_profile = advanced::profile(tool, &saved);
            if saved_profile.endpoint_candidates != expected_profile.endpoint_candidates {
                return Err(verify_failed("saved service routes did not match"));
            }
        }
        if advanced_draft.endpoint_auto_select.is_some()
            && saved
                .meta
                .as_ref()
                .and_then(|meta| meta.endpoint_auto_select)
                != expected
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.endpoint_auto_select)
        {
            return Err(verify_failed(
                "saved service route preference did not match",
            ));
        }
    }
    verify_live_if_applicable(store, tool, id, draft, &saved)
}

fn verify_live_if_applicable(
    store: &ProviderStore,
    tool: ToolId,
    id: &str,
    draft: &ProviderDraft,
    expected: &UpstreamProvider,
) -> Result<(), AppError> {
    let app_type = app_type_for(tool);
    let live_settings = if app_type.is_additive_mode() {
        let live_managed = expected
            .meta
            .as_ref()
            .and_then(|meta| meta.live_config_managed);
        if live_managed == Some(false)
            || matches!(expected.category.as_deref(), Some("omo") | Some("omo-slim"))
        {
            return Ok(());
        }
        match additive_live_node(&app_type, id)
            .map_err(|error| verify_failed(&upstream_detail(&error)))?
        {
            Some(settings) => settings,
            None if live_managed == Some(true) => {
                return Err(verify_failed("service is missing from live configuration"));
            }
            None => return Ok(()),
        }
    } else {
        let current = crate::settings::get_effective_current_provider(&store.state.db, &app_type)
            .map_err(|error| verify_failed(&upstream_detail(&error)))?;
        if current.as_deref() != Some(id) {
            return Ok(());
        }
        match futures::executor::block_on(store.state.db.get_live_backup(app_type.as_str()))
            .map_err(|error| verify_failed(&upstream_detail(&error)))?
        {
            Some(backup) => serde_json::from_str(&backup.original_config)
                .map_err(|_| verify_failed("live restore backup could not be parsed"))?,
            None => ProviderService::read_live_settings(app_type.clone())
                .map_err(|error| verify_failed(&upstream_detail(&error)))?,
        }
    };

    let mut actual = expected.clone();
    actual.settings_config = live_settings;
    verify_edited_projection(tool, draft, expected, &actual)
}

/// Each additive tool keeps its providers somewhere else in its live file
/// (`provider.{id}` for OpenCode, `models.providers.{id}` for OpenClaw, a
/// `custom_providers[]` YAML list for Hermes, Pi's own `models.json`), and
/// upstream `read_live_settings` returns the whole file or refuses (Pi).
/// Read the single node back through the same per-tool reader upstream uses
/// for `provider_exists_in_live_config`, so the node has the shape the
/// tool's writer produced and `None` means "not in live".
fn additive_live_node(
    app_type: &AppType,
    id: &str,
) -> Result<Option<serde_json::Value>, crate::error::AppError> {
    match app_type {
        AppType::OpenCode => Ok(crate::opencode_config::get_providers()?.get(id).cloned()),
        AppType::OpenClaw => crate::openclaw_config::get_provider(id),
        AppType::Hermes => crate::hermes_config::get_provider(id),
        AppType::Pi => crate::pi_config::read_pi_native_provider(id),
        AppType::Claude
        | AppType::ClaudeDesktop
        | AppType::Codex
        | AppType::Gemini
        | AppType::GrokBuild => Ok(None),
    }
}

fn verify_edited_projection(
    tool: ToolId,
    draft: &ProviderDraft,
    expected: &UpstreamProvider,
    actual: &UpstreamProvider,
) -> Result<(), AppError> {
    if draft.api_key.is_some() {
        let app_type = app_type_for(tool);
        let (_, expected_key) = expected.resolve_usage_credentials(&app_type);
        let (_, actual_key) = actual.resolve_usage_credentials(&app_type);
        if actual_key != expected_key {
            return Err(verify_failed("live service key did not match"));
        }
    }
    if draft.models.is_some()
        && advanced::profile(tool, actual).models != advanced::profile(tool, expected).models
    {
        return Err(verify_failed("live service models did not match"));
    }
    if let Some(advanced_draft) = draft.advanced.as_ref() {
        if advanced_draft.base_url_changed
            && advanced::profile(tool, actual).base_url
                != advanced::profile(tool, expected).base_url
        {
            return Err(verify_failed("live service endpoint did not match"));
        }
        if advanced_draft.headers.is_some()
            && actual.settings_config.pointer("/options/headers")
                != expected.settings_config.pointer("/options/headers")
        {
            return Err(verify_failed("live service headers did not match"));
        }
    }
    Ok(())
}

fn rollback_or_escalate(
    store: &ProviderStore,
    tool: ToolId,
    id: &str,
    draft: &ProviderDraft,
    original: &UpstreamProvider,
    failure: AppError,
) -> AppError {
    match restore(store, tool, id, draft, original) {
        Ok(()) => failure,
        Err(restore) => {
            let mutation = failure
                .technical_message
                .as_deref()
                .unwrap_or("service save failed");
            AppError::new(
                ErrorCode::ConfigWriteFailed,
                "error.provider.saveRestoreFailed",
            )
            .with_technical(safe_detail(format!("save: {mutation}; restore: {restore}")))
            .with_remediation("error.remediation.retryOrViewDetails")
        }
    }
}

fn restore(
    store: &ProviderStore,
    tool: ToolId,
    id: &str,
    draft: &ProviderDraft,
    original: &UpstreamProvider,
) -> Result<(), String> {
    let app_type = app_type_for(tool);
    let restored = catch_unwind(AssertUnwindSafe(|| {
        ProviderService::update(&store.state, app_type.clone(), Some(id), original.clone())
    }));
    let live_restore = match restored {
        Ok(Ok(_)) => store
            .state
            .db
            .get_provider_by_id(id, app_type.as_str())
            .map_err(|error| upstream_detail(&error))
            .and_then(|restored| {
                restored.ok_or_else(|| "restored service row is missing".to_string())
            })
            .and_then(|normalized| {
                verify_live_if_applicable(store, tool, id, draft, &normalized)
                    .map_err(domain_detail)
            }),
        Ok(Err(error)) => Err(upstream_detail(&error)),
        Err(payload) => Err(panic_detail(payload)),
    };
    // Upstream normalization may materialize semantically empty metadata while
    // restoring Live. Live is now back; put the exact pre-save DB snapshot back
    // as the final commit so a failed edit cannot create unrelated row drift.
    store
        .state
        .db
        .save_provider(app_type_for(tool).as_str(), original)
        .map_err(|error| upstream_detail(&error))?;
    let original_endpoints = original
        .meta
        .as_ref()
        .map(|meta| &meta.custom_endpoints)
        .cloned()
        .unwrap_or_default();
    store
        .state
        .db
        .replace_custom_endpoints(app_type_for(tool).as_str(), id, &original_endpoints)
        .map_err(|error| upstream_detail(&error))?;
    if !row_matches(&store.state, tool, id, original)? {
        return Err("service row does not match the pre-save snapshot".to_string());
    }
    live_restore
}

fn row_matches(
    state: &AppState,
    tool: ToolId,
    id: &str,
    expected: &UpstreamProvider,
) -> Result<bool, String> {
    let saved = state
        .db
        .get_all_providers(app_type_for(tool).as_str())
        .map_err(|error| upstream_detail(&error))?
        .get(id)
        .cloned();
    let Some(saved) = saved else {
        return Ok(false);
    };
    let saved = serde_json::to_value(saved).map_err(safe_detail)?;
    let expected = serde_json::to_value(expected).map_err(safe_detail)?;
    Ok(saved == expected)
}

fn save_failed(error: crate::error::AppError) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.saveFailed")
        .with_technical(upstream_detail(&error))
        .with_remediation("error.remediation.checkServiceSettings")
}

fn verify_failed(detail: &str) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.saveFailed")
        .with_technical(safe_detail(detail))
        .with_remediation("error.remediation.checkServiceSettings")
}

fn domain_detail(error: AppError) -> String {
    error
        .technical_message
        .as_deref()
        .map(safe_detail)
        .unwrap_or_else(|| error.message_key.to_string())
}

fn save_panicked(payload: Box<dyn Any + Send>) -> AppError {
    AppError::new(ErrorCode::Internal, "error.provider.saveFailed")
        .with_technical(panic_detail(payload))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn panic_detail(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        safe_detail(message)
    } else if let Some(message) = payload.downcast_ref::<String>() {
        safe_detail(message)
    } else {
        "service save panicked with a non-string payload".to_string()
    }
}

fn safe_detail(value: impl std::fmt::Display) -> String {
    crate::platform::redact::truncate_tail(
        &crate::platform::redact::redact_secrets(&value.to_string()),
        10,
        1000,
    )
}
