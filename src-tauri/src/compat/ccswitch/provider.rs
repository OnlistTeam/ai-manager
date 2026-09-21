//! Compatibility layer for the CC Switch provider engine (ADR-0003 / spec §90).
//!
//! This is the **only** place that knows about `AppState` / `AppType` / the upstream
//! `Provider` / the upstream `AppError`. The signatures the layers above see contain only
//! `crate::domain` types.
//!
//! Upstream files are unmodified by this layer: `mod provider;` / `mod services;` /
//! `mod store;` are modules private to the crate root, and this module is a descendant of
//! the crate root, so they are visible here naturally.

use crate::app_config::AppType;
use crate::domain::{
    AppError, ErrorCode, Provider, ProviderCreateDraft, ProviderCreateResult,
    ProviderCustomCreateDraft, ProviderDraft, ProviderEditProfile, ProviderEndpointCandidate,
    ProviderKind, ProviderReachability, ProviderTestResult, ToolId,
};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::provider::Provider as UpstreamProvider;
use crate::services::stream_check::{HealthStatus, StreamCheckConfig, StreamCheckService};
use crate::services::ProviderService;
use crate::store::AppState;
use futures::future::join_all;
use indexmap::IndexMap;
use std::sync::MutexGuard;

mod advanced;
mod codex_identity;
mod create;
pub mod deep_link;
mod live_preservation;
mod long_tail;
mod presets;
mod removal;
mod saving;
mod switching;
pub use create::connection_profile_for;
use create::{
    create_error, is_connection_attempt_provider, provider_for_create, provider_for_custom_create,
    provider_id_for_attempt, ConnectionCreateMode,
};

struct ConnectionAttempt {
    id: String,
    mode: ConnectionCreateMode,
    existing: bool,
}

/// Upstream error strings are bare localized text that may carry addresses and
/// parameters. Never hand them over as is: redact and truncate first, and only put them
/// into `technical_message` (View Details in §42).
pub(super) fn upstream_detail(error: &crate::error::AppError) -> String {
    truncate_tail(&redact_secrets(&error.to_string()), 20, 2000)
}

/// Product `ToolId` -> upstream `AppType`. This and `tool_id_to_app_type` in `tools.rs`
/// are two shapes of the same knowledge; the tests compare them entry by entry to prevent
/// drift (ADR-0004).
pub(super) fn app_type_for(tool: ToolId) -> AppType {
    match tool {
        ToolId::ClaudeCode => AppType::Claude,
        ToolId::Codex => AppType::Codex,
        ToolId::OpenCode => AppType::OpenCode,
        ToolId::GeminiCli => AppType::Gemini,
        ToolId::GrokBuild => AppType::GrokBuild,
        ToolId::OpenClaw => AppType::OpenClaw,
        ToolId::Hermes => AppType::Hermes,
        ToolId::Pi => AppType::Pi,
        // Provider use cases are capability-gated before reaching this
        // compatibility layer. These lifecycle-only tools deliberately have
        // no upstream AppType and must never borrow another tool's format.
        ToolId::KimiCode | ToolId::DeepSeekDsh => {
            unreachable!("lifecycle-only tool reached CC Switch provider boundary")
        }
    }
}

/// Upstream has written "no key configured" both as an empty string and as whitespace.
/// The product model only accepts `None`, otherwise a card with no key would look like it
/// holds an invisible empty key.
fn non_empty_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Upstream record -> product model. The key is pulled out of that free-form JSON blob
/// here and becomes a structured field; apart from normalizing empty strings the content
/// is unchanged.
pub(super) fn provider_from_upstream(
    tool: ToolId,
    raw: &UpstreamProvider,
    current_id: &str,
) -> Provider {
    let app_type = app_type_for(tool);
    // Upstream matches exhaustively over the 9 AppTypes and hands over the most
    // error-prone piece of knowledge — which slot of settings_config the key and address
    // live in — in one place. We do not reimplement it.
    let (base_url, api_key) = raw.resolve_usage_credentials(&app_type);
    let kind = if raw.category.as_deref() == Some("official") {
        ProviderKind::Official
    } else {
        ProviderKind::Custom
    };
    let base_url = advanced::safe_base_url(base_url);
    let testable = match kind {
        ProviderKind::Official => official_probe_target(tool).is_ok_and(|target| target.is_some()),
        ProviderKind::Custom => base_url.is_some(),
    };

    let active = raw.id == current_id;
    Provider {
        additive: app_type.is_additive_mode(),
        id: raw.id.clone(),
        tool,
        name: raw.name.clone(),
        kind,
        active,
        // Official targets come from the audited preset catalog and custom targets from
        // the record itself; both project capability bits only and never expose the
        // official URL to the renderer or write it back into the live config.
        testable,
        base_url,
        api_key: non_empty_key(&api_key),
        website_url: raw.website_url.clone(),
        can_remove: removal::can_remove(raw, active),
    }
}

/// The empty placeholder created by the first-launch import (ADR-0035 decision 5): no
/// endpoint, no key, not official. It exists only as a write-back target when switching
/// and is noise to ordinary users.
fn is_import_placeholder(provider: &Provider) -> bool {
    provider.id == "default"
        && provider.kind == ProviderKind::Custom
        && provider.base_url.is_none()
        && provider.api_key.is_none()
}

fn official_probe_target(tool: ToolId) -> Result<Option<String>, AppError> {
    presets::official_endpoint_for(tool)
}

pub(super) fn reviewed_endpoint_candidates(
    tool: ToolId,
) -> Result<Vec<ProviderEndpointCandidate>, AppError> {
    presets::endpoint_candidates_for(tool)
}

fn probe_target_for(tool: ToolId, raw: &UpstreamProvider) -> Result<Option<String>, AppError> {
    if raw.category.as_deref() == Some("official") {
        return official_probe_target(tool);
    }
    let (base_url, _) = raw.resolve_usage_credentials(&app_type_for(tool));
    Ok(advanced::safe_base_url(base_url))
}

/// Backend-only probe credentials (ADR-0041).
///
/// Unlike `probe_target_for` this also carries the key, so the result stays
/// inside the backend: it is assembled, used for one outbound request and
/// dropped. It never becomes part of a product model or an IPC payload.
///
/// An empty key is not an error. Local gateways commonly accept unauthenticated
/// requests, and the caller simply omits the credential header.
pub(super) fn probe_credentials_for(
    tool: ToolId,
    raw: &UpstreamProvider,
) -> Result<Option<(String, String)>, AppError> {
    let Some(target) = probe_target_for(tool, raw)? else {
        return Ok(None);
    };
    let (_, api_key) = raw.resolve_usage_credentials(&app_type_for(tool));
    Ok(Some((target, api_key.trim().to_string())))
}

fn log_switch_warnings(tool: ToolId, id: &str, warnings: &[String]) {
    // Upstream warnings are bare localized strings that may contain addresses or
    // parameters. As with a normal switch, only the redacted and truncated diagnostic goes
    // into the log; it never goes into the user-facing Connect error card.
    for warning in warnings {
        log::warn!(
            "switching {} to {id} reported: {}",
            tool.as_str(),
            truncate_tail(&redact_secrets(warning), 5, 500)
        );
    }
}

/// Upstream tri-state -> product tri-state. Upstream counts any HTTP response as
/// reachable and only DNS / connection refused / TLS / timeout as a failure; `degraded`
/// means "reachable but slow". The semantics pass through unchanged and the copy says so.
pub(super) fn reachability_from(status: &HealthStatus) -> ProviderReachability {
    match status {
        HealthStatus::Operational => ProviderReachability::Operational,
        HealthStatus::Degraded => ProviderReachability::Degraded,
        HealthStatus::Failed => ProviderReachability::Failed,
    }
}

/// Which slot of `settings_config` holds each tool's key. The order matches the read
/// order of the upstream `Provider::resolve_usage_credentials` **verbatim**, so
/// "write the first currently non-empty slot, or the first slot when all are empty" is
/// guaranteed to be read back by upstream.
///
/// This is the only per-tool branching knowledge in the whole phase, and the round-trip
/// tests pin it down tool by tool (a shape AI_RULES rule 8 allows: one table with tests,
/// not scattered ifs).
fn api_key_slots(tool: ToolId) -> &'static [[&'static str; 2]] {
    match tool {
        ToolId::ClaudeCode => &[
            ["env", "ANTHROPIC_AUTH_TOKEN"],
            ["env", "ANTHROPIC_API_KEY"],
            ["env", "OPENROUTER_API_KEY"],
            ["env", "GOOGLE_API_KEY"],
        ],
        ToolId::Codex => &[["auth", "OPENAI_API_KEY"]],
        ToolId::OpenCode => &[["options", "apiKey"]],
        ToolId::GeminiCli => &[["env", "GEMINI_API_KEY"], ["env", "GOOGLE_API_KEY"]],
        ToolId::GrokBuild
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => &[],
    }
}

fn slot_value<'a>(settings: &'a serde_json::Value, slot: &[&str; 2]) -> Option<&'a str> {
    // A bare `!value.is_empty()`, matching the upstream `first_non_empty` verbatim
    // (src-tauri/src/provider.rs:149-161): upstream does not trim when reading the key, so
    // a whitespace-only value counts as "non-empty" to upstream. Trimming here would treat
    // a slot upstream considers "filled" as an "empty slot", write the new key into a
    // different slot, and upstream would still read back the old unreplaced value (or the
    // whitespace) — both sides must agree on which slot holds a value.
    settings
        .get(slot[0])?
        .get(slot[1])?
        .as_str()
        .filter(|value| !value.is_empty())
}

/// The error returned whenever `settings_config` is not an object (at any level down the
/// chain). Never silently substitute an empty object — that would launder a corrupted
/// state into a seemingly valid empty config that not even upstream's own
/// `validate_provider_settings` could catch (it would only see the empty object we just
/// created), while the user's hand-written fields, addresses and permissions are destroyed
/// by that coercion and persisted by the save. Corrupted rows really are reachable via the
/// dao's `unwrap_or(Null)` (database/dao/providers.rs).
fn settings_not_object_error() -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.saveFailed")
        .with_remediation("error.remediation.checkServiceSettings")
}

fn write_slot(
    settings: &mut serde_json::Value,
    slot: &[&str; 2],
    value: &str,
) -> Result<(), AppError> {
    let root = settings
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?;
    let section = root
        .entry(slot[0].to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    let map = section
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?;
    map.insert(
        slot[1].to_string(),
        serde_json::Value::String(value.to_string()),
    );
    Ok(())
}

/// Apply form changes in place to an existing upstream record.
///
/// All changes land on a clone first and only replace the caller's snapshot once every
/// validation passes; if any advanced field is invalid, the name, key and original config
/// all stay untouched.
pub(super) fn apply_draft(
    tool: ToolId,
    raw: &mut UpstreamProvider,
    draft: &ProviderDraft,
) -> Result<(), AppError> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err(
            AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.nameRequired")
                .with_remediation("error.remediation.checkServiceSettings"),
        );
    }

    let mut next = raw.clone();

    if let Some(key) = draft.api_key.as_deref() {
        let key = key.trim();
        if key.is_empty() {
            // "unchanged" is null on the wire; an empty string means the user cleared the
            // box, which is neither "unchanged" nor a valid key, so it must be rejected
            // rather than silently wiping the stored one.
            return Err(
                AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.saveFailed")
                    .with_remediation("error.remediation.checkServiceSettings"),
            );
        }
        if matches!(
            tool,
            ToolId::GrokBuild | ToolId::OpenClaw | ToolId::Hermes | ToolId::Pi
        ) {
            long_tail::write_api_key(tool, &mut next.settings_config, key)?;
        } else {
            let slots = api_key_slots(tool);
            let target = slots
                .iter()
                .find(|slot| slot_value(&next.settings_config, slot).is_some())
                .unwrap_or(&slots[0]);
            // A non-object settings_config is rejected here instead of being silently
            // coerced into an empty object by write_slot: not a single byte has been
            // written before this step, so `raw` is returned untouched on rejection.
            write_slot(&mut next.settings_config, target, key)?;
        }
    }

    advanced::apply_settings(tool, &mut next, draft)?;
    next.name = name.to_string();
    *raw = next;
    Ok(())
}

/// Handle to the upstream provider store. The fields are private, so the layers above can never reach `AppState`.
pub struct ProviderStore {
    state: AppState,
}

impl ProviderStore {
    pub fn open(app_handle: &tauri::AppHandle) -> Result<Self, AppError> {
        let state = tauri::Manager::try_state::<AppState>(app_handle).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.provider.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        Ok(Self {
            state: state.inner().clone(),
        })
    }

    /// The effective-connection resolver needs the raw record (endpoint values, category);
    /// the product `Provider` has already projected those away, so this returns the
    /// upstream shape. Used only inside `compat/ccswitch`.
    ///
    /// Additive-mode tools (OpenCode and friends) have no notion of a "current service",
    /// so upstream returns an empty string and every card has active = false. This is
    /// not evidence of non-use: the runtime resolver supplies model/history evidence.
    pub(crate) fn raw_inventory(
        &self,
        tool: ToolId,
    ) -> Result<(IndexMap<String, UpstreamProvider>, String), AppError> {
        let app_type = app_type_for(tool);
        let list_failed = |error: &crate::error::AppError| {
            AppError::new(ErrorCode::UpstreamError, "error.provider.listFailed")
                .with_technical(upstream_detail(error))
                .with_remediation("error.remediation.retryOrViewDetails")
        };
        let raw = ProviderService::list(&self.state, app_type.clone())
            .map_err(|error| list_failed(&error))?;
        let current =
            ProviderService::current(&self.state, app_type).map_err(|error| list_failed(&error))?;
        Ok((raw, current))
    }

    pub fn list(&self, tool: ToolId) -> Result<Vec<Provider>, AppError> {
        let (raw, current) = self.raw_inventory(tool)?;
        Ok(raw
            .values()
            .map(|entry| provider_from_upstream(tool, entry, &current))
            .filter(|provider| !is_import_placeholder(provider))
            .collect())
    }

    pub fn edit_profile(&self, tool: ToolId, id: &str) -> Result<ProviderEditProfile, AppError> {
        Ok(advanced::profile(tool, &self.find_raw(tool, id)?))
    }

    pub fn create(
        &self,
        tool: ToolId,
        request_id: &str,
        draft: &ProviderCreateDraft,
    ) -> Result<ProviderCreateResult, AppError> {
        let _mutation = self.lock_mutation();
        let attempt = self.connection_attempt(tool, request_id)?;
        let raw = provider_for_create(tool, &attempt.id, draft)?;

        self.persist_connection(tool, attempt, raw)
    }

    pub fn create_custom(
        &self,
        tool: ToolId,
        request_id: &str,
        draft: &ProviderCustomCreateDraft,
    ) -> Result<ProviderCreateResult, AppError> {
        let _mutation = self.lock_mutation();
        let attempt = self.connection_attempt(tool, request_id)?;
        let raw = provider_for_custom_create(tool, &attempt.id, draft)?;

        self.persist_connection(tool, attempt, raw)
    }

    fn persist_connection(
        &self,
        tool: ToolId,
        attempt: ConnectionAttempt,
        mut raw: UpstreamProvider,
    ) -> Result<ProviderCreateResult, AppError> {
        codex_identity::prepare(self, tool, &attempt, &mut raw)?;
        if attempt.existing {
            return self.resume_connection_attempt(tool, attempt, raw);
        }

        let app_type = app_type_for(tool);
        if let Err(error) = ProviderService::add(&self.state, app_type.clone(), raw.clone(), true) {
            // Upstream add can fail to write live after the DB commit, in both normal and
            // additive mode. Only continue converging this operation when a row with the
            // same deterministic ID and marked by this product really did land in the DB;
            // never guess by name, and never overwrite an external record that happens to
            // share the ID.
            let committed = ProviderService::list(&self.state, app_type)
                .ok()
                .and_then(|providers| providers.get(&attempt.id).cloned());
            if committed
                .as_ref()
                .is_some_and(is_connection_attempt_provider)
            {
                return self.resume_connection_attempt(tool, attempt, raw);
            }
            return Err(create_error().with_technical(upstream_detail(&error)));
        }

        Ok(ProviderCreateResult {
            providers: self.list(tool)?,
            created_provider_id: attempt.id,
        })
    }

    fn connection_attempt(
        &self,
        tool: ToolId,
        request_id: &str,
    ) -> Result<ConnectionAttempt, AppError> {
        let activate_id = provider_id_for_attempt(request_id, ConnectionCreateMode::Activate)?;
        let store_only_id = provider_id_for_attempt(request_id, ConnectionCreateMode::StoreOnly)?;
        let app_type = app_type_for(tool);
        let providers = ProviderService::list(&self.state, app_type.clone())
            .map_err(|error| create_error().with_technical(upstream_detail(&error)))?;
        let activate = providers.get(&activate_id);
        let store_only = providers.get(&store_only_id);

        let existing = match (activate, store_only) {
            (Some(provider), None) => {
                Some((&activate_id, ConnectionCreateMode::Activate, provider))
            }
            (None, Some(provider)) => {
                Some((&store_only_id, ConnectionCreateMode::StoreOnly, provider))
            }
            (Some(_), Some(_)) => {
                return Err(create_error()
                    .with_technical("both connection-attempt ids already exist".to_string()));
            }
            (None, None) => None,
        };

        if let Some((id, mode, provider)) = existing {
            if !is_connection_attempt_provider(provider) {
                return Err(
                    create_error().with_technical(format!("connection-attempt id collision: {id}"))
                );
            }
            return Ok(ConnectionAttempt {
                id: id.clone(),
                mode,
                existing: true,
            });
        }

        let mode = if app_type.is_additive_mode()
            || ProviderService::current(&self.state, app_type)
                .map_err(|error| create_error().with_technical(upstream_detail(&error)))?
                .is_empty()
        {
            ConnectionCreateMode::Activate
        } else {
            ConnectionCreateMode::StoreOnly
        };
        let id = match mode {
            ConnectionCreateMode::Activate => activate_id,
            ConnectionCreateMode::StoreOnly => store_only_id,
        };
        Ok(ConnectionAttempt {
            id,
            mode,
            existing: false,
        })
    }

    fn resume_connection_attempt(
        &self,
        tool: ToolId,
        attempt: ConnectionAttempt,
        raw: UpstreamProvider,
    ) -> Result<ProviderCreateResult, AppError> {
        let app_type = app_type_for(tool);
        ProviderService::update(&self.state, app_type.clone(), Some(&attempt.id), raw)
            .map_err(|error| create_error().with_technical(upstream_detail(&error)))?;
        if attempt.mode == ConnectionCreateMode::Activate {
            let outcome = ProviderService::switch(&self.state, app_type, &attempt.id)
                .map_err(|error| create_error().with_technical(upstream_detail(&error)))?;
            log_switch_warnings(tool, &attempt.id, &outcome.warnings);
        }
        Ok(ProviderCreateResult {
            providers: self.list(tool)?,
            created_provider_id: attempt.id,
        })
    }

    pub fn save(
        &self,
        tool: ToolId,
        id: &str,
        draft: &ProviderDraft,
    ) -> Result<Vec<Provider>, AppError> {
        let _mutation = self.lock_mutation();
        saving::save(self, tool, id, draft)
    }

    /// Remove one non-current service and return the already-verified list.
    /// The compatibility layer owns target resolution, rollback, and live-file
    /// verification; no raw provider configuration leaves this boundary.
    pub fn remove(&self, tool: ToolId, id: &str) -> Result<Vec<Provider>, AppError> {
        let _mutation = self.lock_mutation();
        removal::remove(&self.state, tool, id)
    }

    /// Switch the current service and return the already-verified list. The
    /// compatibility layer undoes a half-committed upstream switch whose live
    /// write failed, so `current` never names an unapplied service.
    pub fn switch(&self, tool: ToolId, id: &str) -> Result<Vec<Provider>, AppError> {
        let _mutation = self.lock_mutation();
        switching::switch(self, tool, id)
    }

    /// The lock guards `()` and only orders writers, so a poisoned lock is still
    /// a valid lock: recover it (as upstream `settings.rs` does) instead of
    /// failing every later provider write until the app restarts.
    fn lock_mutation(&self) -> MutexGuard<'_, ()> {
        self.state
            .provider_mutation_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Probe whether this service's address is reachable. **Returns the result
    /// synchronously and does not go through the OperationManager** (decision 2): this is a
    /// read-only probe, hooking it in would also lock the tool's install/update buttons,
    /// and an operation's terminal payload is `()`, which cannot carry latency or a status
    /// code.
    pub async fn test(&self, tool: ToolId, id: &str) -> Result<ProviderTestResult, AppError> {
        let raw = self.find_raw(tool, id)?;
        Self::test_raw(tool, raw, StreamCheckConfig::default()).await
    }

    /// One bounded address check used only inside an explicit Use/Open action.
    pub async fn test_for_preflight(
        &self,
        tool: ToolId,
        id: &str,
    ) -> Result<ProviderTestResult, AppError> {
        let raw = self.find_raw(tool, id)?;
        Self::test_raw(
            tool,
            raw,
            StreamCheckConfig {
                timeout_secs: 4,
                max_retries: 0,
                ..StreamCheckConfig::default()
            },
        )
        .await
    }

    /// Concurrently checks only backend-resolved saved providers. The renderer supplies no URL.
    ///
    /// One candidate never aborts the round: a service deleted since the
    /// renderer snapshot is skipped, and a probe that cannot run counts as a
    /// failed result for that service, so the next candidate is still tried.
    pub async fn test_for_failover(
        &self,
        tool: ToolId,
        ids: &[String],
    ) -> Result<Vec<ProviderTestResult>, AppError> {
        if ids.len() > crate::domain::MAX_PROVIDER_ENDPOINT_CANDIDATES {
            return Err(
                AppError::new(ErrorCode::ProviderUnreachable, "error.provider.testFailed")
                    .with_technical("saved failover candidate limit exceeded")
                    .with_remediation("error.remediation.retryOrViewDetails"),
            );
        }
        let config = StreamCheckConfig {
            timeout_secs: 4,
            max_retries: 0,
            ..StreamCheckConfig::default()
        };
        let probes = ids
            .iter()
            .filter_map(|id| match self.find_raw(tool, id) {
                Ok(raw) => Some(Self::probe_failover_candidate(tool, raw, config.clone())),
                Err(error) => {
                    log::warn!(
                        "skipping failover candidate {}/{id}: {}",
                        tool.as_str(),
                        error.message_key
                    );
                    None
                }
            })
            .collect::<Vec<_>>();
        Ok(join_all(probes).await)
    }

    async fn probe_failover_candidate(
        tool: ToolId,
        raw: UpstreamProvider,
        config: StreamCheckConfig,
    ) -> ProviderTestResult {
        let provider_id = raw.id.clone();
        Self::test_raw(tool, raw, config)
            .await
            .unwrap_or_else(|error| {
                log::warn!(
                    "failover probe for {}/{provider_id} could not run: {}",
                    tool.as_str(),
                    error.message_key
                );
                ProviderTestResult {
                    provider_id,
                    reachability: ProviderReachability::Failed,
                    response_time_ms: None,
                    http_status: None,
                }
            })
    }

    async fn test_raw(
        tool: ToolId,
        raw: UpstreamProvider,
        config: StreamCheckConfig,
    ) -> Result<ProviderTestResult, AppError> {
        let app_type = app_type_for(tool);

        // Uses the same target resolution as the list. Official addresses are read only
        // from the audited preset catalog, never guessed and never written into this
        // official record; a custom service still only uses the safe address it stored.
        let Some(probe_target) = probe_target_for(tool, &raw)? else {
            return Err(AppError::new(
                ErrorCode::ProviderUnreachable,
                "error.provider.notTestable",
            )
            .with_remediation("error.remediation.checkServiceSettings"));
        };

        let outcome =
            StreamCheckService::check_with_retry(&app_type, &raw, &config, Some(probe_target))
                .await
                .map_err(|error| {
                    AppError::new(ErrorCode::NetworkError, "error.provider.testFailed")
                        .with_technical(upstream_detail(&error))
                        .with_remediation("error.remediation.checkServiceSettings")
                })?;

        Ok(ProviderTestResult {
            provider_id: raw.id.clone(),
            reachability: reachability_from(&outcome.status),
            response_time_ms: outcome.response_time_ms,
            http_status: outcome.http_status,
        })
    }

    /// Fetch a single upstream record. It returns an upstream type, so it is visible to this module only.
    pub(super) fn find_raw(&self, tool: ToolId, id: &str) -> Result<UpstreamProvider, AppError> {
        let app_type = app_type_for(tool);
        let raw = ProviderService::list(&self.state, app_type).map_err(|error| {
            AppError::new(ErrorCode::UpstreamError, "error.provider.listFailed")
                .with_technical(upstream_detail(&error))
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        raw.get(id).cloned().ok_or_else(|| {
            AppError::new(ErrorCode::ProviderNotFound, "error.provider.notFound")
                .with_technical(format!("{}/{id}", tool.as_str()))
        })
    }
}

// The production-code scanner in message_keys.rs stops at the first `#[cfg(test)]` in a
// file — both mod declarations must therefore come after all production code that emits a
// message_key, otherwise the keys in `ProviderStore` would be misreported as "unused" (see
// the scanner comment in that file, and the same trap hit in Phase 4 Task 2). The tests
// themselves are split into two files along the read/write paths, each far below the
// 500-line limit.
#[cfg(test)]
#[path = "provider/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "provider/tests_write.rs"]
mod tests_write;

#[cfg(test)]
#[path = "provider/tests_remove.rs"]
mod tests_remove;

#[cfg(test)]
#[path = "provider/tests_create.rs"]
mod tests_create;

#[cfg(test)]
#[path = "provider/tests_advanced.rs"]
mod tests_advanced;

#[cfg(test)]
#[path = "provider/tests_save.rs"]
mod tests_save;

#[cfg(test)]
#[path = "provider/tests_switch.rs"]
mod tests_switch;

#[cfg(test)]
#[path = "provider/tests_live_preservation.rs"]
mod tests_live_preservation;
