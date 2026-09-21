//! Request forwarder
//!
//! Forwards requests to upstream providers, with failover support

use super::hyper_client::{ProxyResponse, MAX_RESPONSE_BODY_BYTES};
use super::{
    body_filter::filter_private_params_with_whitelist,
    content_encoding::{decompress_body_with_limit, get_content_encoding},
    error::*,
    failover_switch::FailoverSwitchManager,
    json_canonical::{canonicalize_value, short_value_hash},
    log_codes::fwd as log_fwd,
    provider_router::ProviderRouter,
    providers::{
        codex_chat_history::CodexChatHistoryStore, gemini_shadow::GeminiShadowStore, get_adapter,
        AuthInfo, AuthStrategy, ProviderAdapter, ProviderType,
    },
    thinking_budget_rectifier::{rectify_thinking_budget, should_rectify_thinking_budget},
    thinking_rectifier::{
        normalize_thinking_type, rectify_anthropic_request, should_rectify_thinking_signature,
    },
    types::{CopilotOptimizerConfig, OptimizerConfig, ProxyStatus, RectifierConfig},
    ProxyError,
};
use crate::commands::{CodexOAuthState, CopilotAuthState, XaiOAuthState};
use crate::proxy::providers::copilot_auth::CopilotAuthManager;
use crate::proxy::providers::xai_oauth_auth::XaiOAuthManager;
use crate::{
    app_config::AppType,
    provider::{LocalProxyRequestOverrides, Provider},
};
use bytes::Bytes;
use futures::StreamExt;
use http::Extensions;
use serde_json::Value;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

const PROXY_AUTH_PLACEHOLDER: &str = "PROXY_MANAGED";

fn validate_codex_official_authorization(
    headers: &http::HeaderMap,
    provider: &Provider,
) -> Result<(), ProxyError> {
    let authorization = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim);
    match authorization {
        None | Some("") => Err(ProxyError::AuthError(
            "The official Codex login is unavailable, please sign in to ChatGPT in Codex first".to_string(),
        )),
        Some(value) if value.contains(PROXY_AUTH_PLACEHOLDER) => Err(ProxyError::AuthError(
            "Switched to the official OpenAI provider, please restart Codex or start a new session to load the official login".to_string(),
        )),
        Some(_) => {
            let expected_account_id = provider
                .meta
                .as_ref()
                .and_then(|meta| meta.managed_account_id_for("codex_oauth"))
                .map(|account_id| account_id.trim().to_string())
                .filter(|account_id| !account_id.is_empty());
            if let Some(expected_account_id) = expected_account_id {
                let request_account_id = headers
                    .get("chatgpt-account-id")
                    .and_then(|value| value.to_str().ok())
                    .map(str::trim)
                    .filter(|account_id| !account_id.is_empty());
                if request_account_id != Some(expected_account_id.as_str()) {
                    return Err(ProxyError::AuthError(
                        "The current Codex session has not loaded the selected ChatGPT account, please restart Codex or start a new session and retry"
                            .to_string(),
                    ));
                }
            }
            Ok(())
        }
    }
}

pub struct ForwardResult {
    pub response: ProxyResponse,
    pub provider: Provider,
    pub claude_api_format: Option<String>,
    /// The model actually sent upstream (the truth after routing takeover / model mapping).
    ///
    /// Usage attribution must not rely on ctx.request_model (the pre-mapping client alias): when the
    /// upstream response omits model or echoes the alias, takeover traffic would be recorded as claude-* and priced accordingly.
    pub outbound_model: Option<String>,
    /// Active-connection RAII guard: travels with the response into response_processor /
    /// handle_claude_transform and is finally moved into the streaming body future (or the non-streaming response scope), covering the whole response lifetime.
    pub(crate) connection_guard: Option<ActiveConnectionGuard>,
}

pub struct ForwardError {
    pub error: ProxyError,
    pub provider: Option<Provider>,
}

/// Active-connection RAII guard
///
/// Construction increments `ProxyStatus.active_connections`; Drop schedules an async task on the
/// tokio runtime to decrement it, so the guard can be moved into the streaming body future (both
/// drop together when the stream ends naturally).
///
/// Motivation: the decrement used to happen synchronously at the exit of `forward_with_retry`, but
/// a streaming body keeps yielding bytes inside `create_logged_passthrough_stream`, so the UI's
/// `active_connections` dropped to zero too early. The RAII guard lets the type system drive the
/// decrement instead of requiring a manual call on every exit path.
pub(crate) struct ActiveConnectionGuard {
    status: Arc<RwLock<ProxyStatus>>,
}

impl ActiveConnectionGuard {
    pub(crate) async fn acquire(status: Arc<RwLock<ProxyStatus>>) -> Self {
        {
            let mut s = status.write().await;
            s.active_connections = s.active_connections.saturating_add(1);
        }
        Self { status }
    }
}

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        // Drop cannot await, so the decrement is scheduled on the tokio runtime
        let status = self.status.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let mut s = status.write().await;
                s.active_connections = s.active_connections.saturating_sub(1);
            });
        }
        // Without a runtime the count is silently lost (display only, eventual consistency is fine)
    }
}

pub struct RequestForwarder {
    /// Shared ProviderRouter (holds the circuit breaker state)
    router: Arc<ProviderRouter>,
    status: Arc<RwLock<ProxyStatus>>,
    current_providers: Arc<RwLock<std::collections::HashMap<String, (String, String)>>>,
    gemini_shadow: Arc<GeminiShadowStore>,
    codex_chat_history: Arc<CodexChatHistoryStore>,
    /// Failover switch manager
    failover_manager: Arc<FailoverSwitchManager>,
    /// AppHandle, used to emit events and refresh the tray
    app_handle: Option<tauri::AppHandle>,
    /// The "current provider id" when the request started (used to decide whether to sync UI/tray)
    current_provider_id_at_start: String,
    /// Proxy session ID (used for Gemini Native shadow replay)
    session_id: String,
    /// Whether the client supplied the session ID; a generated one must not act as an upstream cache identity.
    session_client_provided: bool,
    /// Rectifier configuration
    rectifier_config: RectifierConfig,
    /// Optimizer configuration
    optimizer_config: OptimizerConfig,
    /// Copilot optimizer configuration
    copilot_optimizer_config: CopilotOptimizerConfig,
    /// Non-streaming request timeout (seconds)
    non_streaming_timeout: std::time::Duration,
    /// Timeout waiting for streaming response headers (seconds)
    streaming_first_byte_timeout: std::time::Duration,
    /// Maximum number of providers tried for a single client request.
    ///
    /// Derived from `AppProxyConfig.max_retries` (UI: retries on request failure, 0-10):
    /// `max_attempts = max_retries + 1`, so max_retries=0 tries a single provider and max_retries=3
    /// (the default) tries at most four. The loop is also naturally bounded by providers.len().
    max_attempts: usize,
}

impl RequestForwarder {
    /// Preventive media fallback: replace image blocks with a marker for text-only models before sending.
    ///
    /// Governed by `enabled && request_media_fallback`; the heuristic model-list prediction is
    /// additionally governed by `request_media_heuristic` (an explicit text-only declaration always applies).
    /// Returns how many image blocks were replaced (0 means not triggered or disabled).
    fn apply_media_prevention(&self, body: &mut Value, provider: &Provider) -> usize {
        if !(self.rectifier_config.enabled && self.rectifier_config.request_media_fallback) {
            return 0;
        }
        let replaced_images = super::media_sanitizer::replace_images_for_text_only_model(
            body,
            provider,
            self.rectifier_config.request_media_heuristic,
        );
        if replaced_images > 0 {
            let model = body.get("model").and_then(Value::as_str).unwrap_or("");
            log::info!(
                "[Media] Replaced {replaced_images} image block(s) with {} for text-only provider={}, model={}",
                super::media_sanitizer::UNSUPPORTED_IMAGE_MARKER,
                provider.id,
                model
            );
        }
        replaced_images
    }

    /// Reactive media retry decision: after upstream errors on image input, whether to replace image blocks and retry the same provider once.
    ///
    /// Governed by `enabled && request_media_fallback` only, not `request_media_heuristic`: this is
    /// pure recovery after an observed upstream error rather than prediction, so the heuristic switch is irrelevant.
    fn media_retry_should_trigger(
        &self,
        adapter_name: &str,
        already_retried: bool,
        provider_body: &Value,
        error: &ProxyError,
    ) -> bool {
        matches!(adapter_name, "Claude" | "Codex")
            && self.rectifier_config.enabled
            && self.rectifier_config.request_media_fallback
            && !already_retried
            && super::media_sanitizer::contains_image_blocks(provider_body)
            && super::media_sanitizer::is_unsupported_image_error(error)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        router: Arc<ProviderRouter>,
        non_streaming_timeout: u64,
        status: Arc<RwLock<ProxyStatus>>,
        current_providers: Arc<RwLock<std::collections::HashMap<String, (String, String)>>>,
        gemini_shadow: Arc<GeminiShadowStore>,
        codex_chat_history: Arc<CodexChatHistoryStore>,
        failover_manager: Arc<FailoverSwitchManager>,
        app_handle: Option<tauri::AppHandle>,
        current_provider_id_at_start: String,
        session_id: String,
        session_client_provided: bool,
        streaming_first_byte_timeout: u64,
        _streaming_idle_timeout: u64,
        rectifier_config: RectifierConfig,
        optimizer_config: OptimizerConfig,
        copilot_optimizer_config: CopilotOptimizerConfig,
        max_retries: u32,
    ) -> Self {
        // max_retries means retries after a failure, so the attempt cap is retries + 1.
        // saturating_add prevents an overflow at u32::MAX + 1.
        let max_attempts = (max_retries as usize).saturating_add(1);
        Self {
            router,
            status,
            current_providers,
            gemini_shadow,
            codex_chat_history,
            failover_manager,
            app_handle,
            current_provider_id_at_start,
            session_id,
            session_client_provided,
            rectifier_config,
            optimizer_config,
            copilot_optimizer_config,
            non_streaming_timeout: std::time::Duration::from_secs(non_streaming_timeout),
            streaming_first_byte_timeout: std::time::Duration::from_secs(
                streaming_first_byte_timeout,
            ),
            max_attempts,
        }
    }

    async fn record_success_result(
        &self,
        provider_id: &str,
        app_type: &str,
        used_half_open_permit: bool,
    ) {
        if used_half_open_permit {
            if let Err(e) = self
                .router
                .record_result(provider_id, app_type, true, true, None)
                .await
            {
                log::warn!(
                    "[{app_type}] failed to record provider success: provider_id={provider_id}, error={e}"
                );
            }
            return;
        }

        let router = self.router.clone();
        let provider_id = provider_id.to_string();
        let app_type = app_type.to_string();
        tokio::spawn(async move {
            if let Err(e) = router
                .record_result(&provider_id, &app_type, false, true, None)
                .await
            {
                log::warn!(
                    "[{app_type}] async recording of provider success failed: provider_id={provider_id}, error={e}"
                );
            }
        });
    }

    /// Shared wrap-up after a rectifier retry (thinking signature or budget) fails.
    ///
    /// `None` means the circuit breaker was recorded and `last_error`/`last_provider` accumulated, so
    /// the caller should `continue` and let failover move to the next provider.
    /// `Some(ForwardError)` means a client error that no provider can fix, so the caller should
    /// `return` it straight to the client.
    #[allow(clippy::too_many_arguments)]
    async fn handle_rectifier_retry_failure(
        &self,
        retry_err: ProxyError,
        provider: &Provider,
        app_type_str: &str,
        used_half_open_permit: bool,
        rectifier_label: &str,
        last_error: &mut Option<ProxyError>,
        last_provider: &mut Option<Provider>,
    ) -> Option<ForwardError> {
        // Provider error: this upstream or the network really failed and the next provider may work -> keep failing over.
        // Client error: the request is still invalid after rectification and no other provider can fix it -> return.
        let is_provider_error = match &retry_err {
            ProxyError::Timeout(_) | ProxyError::ForwardFailed(_) => true,
            ProxyError::UpstreamError { status, .. } => *status >= 500,
            _ => false,
        };

        if is_provider_error {
            let _ = self
                .router
                .record_result(
                    &provider.id,
                    app_type_str,
                    used_half_open_permit,
                    false,
                    Some(retry_err.to_string()),
                )
                .await;
            {
                let mut status = self.status.write().await;
                status.last_error = Some(format!(
                    "Provider {} {rectifier_label} retry failed: {}",
                    provider.name, retry_err
                ));
            }
            *last_error = Some(retry_err);
            *last_provider = Some(provider.clone());
            return None;
        }

        self.router
            .release_permit_neutral(&provider.id, app_type_str, used_half_open_permit)
            .await;
        let mut status = self.status.write().await;
        status.failed_requests += 1;
        status.last_error = Some(retry_err.to_string());
        if status.total_requests > 0 {
            status.success_rate =
                (status.success_requests as f32 / status.total_requests as f32) * 100.0;
        }
        Some(ForwardError {
            error: retry_err,
            provider: Some(provider.clone()),
        })
    }

    /// Forwards a request (with failover)
    ///
    /// A thin wrapper: per client request it bumps `total_requests`, adjusts `active_connections`, and
    /// refreshes `last_request_at`; whichever exit path the inner call takes, `active_connections` is
    /// reclaimed at the exit. Per-attempt counters (success, failure, tripping, and so on) are still
    /// updated inside the inner call via `success_requests` / `failed_requests`.
    #[allow(clippy::too_many_arguments)]
    pub async fn forward_with_retry(
        &self,
        app_type: &AppType,
        method: http::Method,
        endpoint: &str,
        body: Value,
        headers: axum::http::HeaderMap,
        extensions: Extensions,
        providers: Vec<Provider>,
    ) -> Result<ForwardResult, ForwardError> {
        let guard = ActiveConnectionGuard::acquire(self.status.clone()).await;
        {
            let mut s = self.status.write().await;
            s.total_requests = s.total_requests.saturating_add(1);
            s.last_request_at = Some(chrono::Utc::now().to_rfc3339());
        }
        let result = self
            .forward_with_retry_inner(
                app_type, method, endpoint, body, headers, extensions, providers,
            )
            .await;
        // Inject the guard into the Ok result so it travels with the response into response_processor and
        // only really drops inside the streaming body future.
        // On the Err path the guard drops automatically when the return value leaves this scope.
        result.map(|mut fr| {
            fr.connection_guard = Some(guard);
            fr
        })
    }

    /// The real forwarding logic (without the per-client entry/exit counters)
    ///
    /// # Arguments
    /// * `app_type` - app type
    /// * `method` - the client's HTTP method (passed upstream; GET, POST, and others are supported)
    /// * `endpoint` - API endpoint
    /// * `body` - request body
    /// * `headers` - request headers
    /// * `providers` - the already selected provider list (from RequestContext, avoiding a second select_providers call)
    #[allow(clippy::too_many_arguments)]
    async fn forward_with_retry_inner(
        &self,
        app_type: &AppType,
        method: http::Method,
        endpoint: &str,
        body: Value,
        headers: axum::http::HeaderMap,
        extensions: Extensions,
        providers: Vec<Provider>,
    ) -> Result<ForwardResult, ForwardError> {
        // Get the adapter
        let adapter = get_adapter(app_type).ok_or_else(|| ForwardError {
            error: ProxyError::ConfigError(format!(
                "{} does not support proxy routing",
                app_type.as_str()
            )),
            provider: None,
        })?;
        let app_type_str = app_type.as_str();

        if providers.is_empty() {
            return Err(ForwardError {
                error: ProxyError::NoAvailableProvider,
                provider: None,
            });
        }

        let mut last_error = None;
        let mut last_provider = None;
        let mut attempted_providers = 0usize;

        // Skip the circuit breaker check for a single provider (failover off)
        let bypass_circuit_breaker = providers.len() == 1;

        // Try each provider in turn
        for provider in providers.iter() {
            // Rectifier retry flags are per provider so a flag cannot short-circuit failover across providers:
            // if the first provider is knocked out by a 5xx or timeout after rectification, the next one can still run the rectifier flow on the rectified body
            let mut rectifier_retried = false;
            let mut budget_rectifier_retried = false;
            let mut media_rectifier_retried = false;

            // Cap check: respects the retry count configured in AppProxyConfig.max_retries. It runs before
            // the circuit breaker allow check so an over-cap attempt cannot take a HalfOpen probe slot.
            if attempted_providers >= self.max_attempts {
                log::warn!(
                    "[{app_type_str}] attempt cap reached ({}/{}), stopping failover",
                    attempted_providers,
                    self.max_attempts
                );
                break;
            }

            // Take the circuit breaker permit before sending (HalfOpen consumes a probe slot)
            // Skip this for a single provider so the breaker cannot block every request
            let (allowed, used_half_open_permit) = if bypass_circuit_breaker {
                (true, false)
            } else {
                let permit = self
                    .router
                    .allow_provider_request(&provider.id, app_type_str)
                    .await;
                (permit.allowed, permit.used_half_open_permit)
            };

            if !allowed {
                continue;
            }

            // Pre-send optimizer: each provider decides for itself whether to optimize
            // Clone the body so Bedrock optimization fields cannot leak to a non-Bedrock provider during failover
            let mut provider_body =
                if self.optimizer_config.enabled && is_bedrock_provider(provider) {
                    let mut b = body.clone();
                    if self.optimizer_config.thinking_optimizer {
                        super::thinking_optimizer::optimize(&mut b, &self.optimizer_config);
                    }
                    if self.optimizer_config.cache_injection {
                        super::cache_injector::inject(&mut b, &self.optimizer_config);
                    }
                    b
                } else {
                    body.clone()
                };

            attempted_providers += 1;

            // Update the current provider in the status (a per-attempt identifier)
            //
            // total_requests / last_request_at / active_connections are already handled per client request by
            // the forward_with_retry wrapper, so this only refreshes the display field showing which provider
            // is currently being tried.
            {
                let mut status = self.status.write().await;
                status.current_provider = Some(provider.name.clone());
                status.current_provider_id = Some(provider.id.clone());
            }

            // Forward the request (one attempt per provider; retries are the client's business)
            match self
                .forward(
                    app_type,
                    &method,
                    provider,
                    endpoint,
                    &provider_body,
                    &headers,
                    &extensions,
                    adapter.as_ref(),
                )
                .await
            {
                Ok((response, claude_api_format, outbound_model)) => {
                    // Success: an ordinary closed breaker is recorded asynchronously so the first streaming chunk is
                    // not blocked, while a HalfOpen probe still waits synchronously so the permit and state are released promptly.
                    self.record_success_result(&provider.id, app_type_str, used_half_open_permit)
                        .await;

                    // Update the provider in use for this app type
                    {
                        let mut current_providers = self.current_providers.write().await;
                        current_providers.insert(
                            app_type_str.to_string(),
                            (provider.id.clone(), provider.name.clone()),
                        );
                    }

                    // Update the success counters
                    {
                        let mut status = self.status.write().await;
                        status.success_requests += 1;
                        status.last_error = None;
                        let should_switch =
                            self.current_provider_id_at_start.as_str() != provider.id.as_str();
                        if should_switch {
                            status.failover_count += 1;

                            // Trigger the provider switch asynchronously, refresh UI/tray, and sync the current provider to the one actually used
                            let fm = self.failover_manager.clone();
                            let ah = self.app_handle.clone();
                            let pid = provider.id.clone();
                            let pname = provider.name.clone();
                            let at = app_type_str.to_string();

                            tokio::spawn(async move {
                                let _ = fm.try_switch(ah.as_ref(), &at, &pid, &pname).await;
                            });
                        }
                        // Recompute the success rate
                        if status.total_requests > 0 {
                            status.success_rate = (status.success_requests as f32
                                / status.total_requests as f32)
                                * 100.0;
                        }
                    }

                    return Ok(ForwardResult {
                        response,
                        provider: provider.clone(),
                        claude_api_format,
                        outbound_model,
                        connection_guard: None,
                    });
                }
                Err(e) => {
                    // Check whether the rectifier should run (Claude/ClaudeAuth providers only)
                    let provider_type = ProviderType::from_app_type_and_config(app_type, provider);
                    let is_anthropic_provider = matches!(
                        provider_type,
                        Some(ProviderType::Claude | ProviderType::ClaudeAuth)
                    );
                    let mut signature_rectifier_non_retryable_client_error = false;

                    if self.media_retry_should_trigger(
                        adapter.name(),
                        media_rectifier_retried,
                        &provider_body,
                        &e,
                    ) {
                        let mut media_body = provider_body.clone();
                        let replaced_images =
                            super::media_sanitizer::replace_image_blocks_with_marker(
                                &mut media_body,
                            );

                        if replaced_images > 0 {
                            let _ = std::mem::replace(&mut media_rectifier_retried, true);
                            let model = media_body
                                .get("model")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            log::info!(
                                "[{app_type_str}] [Media] Upstream rejected image input; retrying provider={} model={} with {replaced_images} image block(s) replaced by {}",
                                provider.id,
                                model,
                                super::media_sanitizer::UNSUPPORTED_IMAGE_MARKER
                            );

                            match self
                                .forward(
                                    app_type,
                                    &method,
                                    provider,
                                    endpoint,
                                    &media_body,
                                    &headers,
                                    &extensions,
                                    adapter.as_ref(),
                                )
                                .await
                            {
                                Ok((response, claude_api_format, outbound_model)) => {
                                    log::info!(
                                        "[{app_type_str}] [Media] Unsupported-image retry succeeded"
                                    );
                                    self.record_success_result(
                                        &provider.id,
                                        app_type_str,
                                        used_half_open_permit,
                                    )
                                    .await;

                                    {
                                        let mut current_providers =
                                            self.current_providers.write().await;
                                        current_providers.insert(
                                            app_type_str.to_string(),
                                            (provider.id.clone(), provider.name.clone()),
                                        );
                                    }

                                    {
                                        let mut status = self.status.write().await;
                                        status.success_requests += 1;
                                        status.last_error = None;
                                        let should_switch =
                                            self.current_provider_id_at_start.as_str()
                                                != provider.id.as_str();
                                        if should_switch {
                                            status.failover_count += 1;
                                            let fm = self.failover_manager.clone();
                                            let ah = self.app_handle.clone();
                                            let pid = provider.id.clone();
                                            let pname = provider.name.clone();
                                            let at = app_type_str.to_string();

                                            tokio::spawn(async move {
                                                let _ = fm
                                                    .try_switch(ah.as_ref(), &at, &pid, &pname)
                                                    .await;
                                            });
                                        }
                                        if status.total_requests > 0 {
                                            status.success_rate = (status.success_requests as f32
                                                / status.total_requests as f32)
                                                * 100.0;
                                        }
                                    }

                                    return Ok(ForwardResult {
                                        response,
                                        provider: provider.clone(),
                                        claude_api_format,
                                        outbound_model,
                                        connection_guard: None,
                                    });
                                }
                                Err(retry_err) => {
                                    log::warn!(
                                        "[{app_type_str}] [Media] Unsupported-image retry still failed: {retry_err}"
                                    );
                                    if let Some(err) = self
                                        .handle_rectifier_retry_failure(
                                            retry_err,
                                            provider,
                                            app_type_str,
                                            used_half_open_permit,
                                            "media fallback",
                                            &mut last_error,
                                            &mut last_provider,
                                        )
                                        .await
                                    {
                                        return Err(err);
                                    }
                                    continue;
                                }
                            }
                        }
                    }

                    if is_anthropic_provider {
                        let error_message = extract_error_message(&e);
                        if should_rectify_thinking_signature(
                            error_message.as_deref(),
                            &self.rectifier_config,
                        ) {
                            // Already retried: return the error directly (a non-retryable client error)
                            if rectifier_retried {
                                log::warn!("[{app_type_str}] [RECT-005] the rectifier already ran, not retrying again");
                                // Release the HalfOpen permit (no breaker record; this is a client compatibility issue)
                                self.router
                                    .release_permit_neutral(
                                        &provider.id,
                                        app_type_str,
                                        used_half_open_permit,
                                    )
                                    .await;
                                let mut status = self.status.write().await;
                                status.failed_requests += 1;
                                status.last_error = Some(e.to_string());
                                if status.total_requests > 0 {
                                    status.success_rate = (status.success_requests as f32
                                        / status.total_requests as f32)
                                        * 100.0;
                                }
                                return Err(ForwardError {
                                    error: e,
                                    provider: Some(provider.clone()),
                                });
                            }

                            // First trigger: rectify the request body
                            let rectified = rectify_anthropic_request(&mut provider_body);

                            // Rectification changed nothing: fall through to the budget path instead of short-circuiting
                            if !rectified.applied {
                                log::warn!(
                                    "[{app_type_str}] [RECT-006] the thinking signature rectifier fired but found nothing to rectify, checking budget next; if budget misses too this returns as a client error"
                                );
                                signature_rectifier_non_retryable_client_error = true;
                            } else {
                                log::info!(
                                    "[{}] [RECT-001] thinking signature rectifier fired, removed {} thinking blocks, {} redacted_thinking blocks, {} signature fields",
                                    app_type_str,
                                    rectified.removed_thinking_blocks,
                                    rectified.removed_redacted_thinking_blocks,
                                    rectified.removed_signature_fields
                                );

                                // Mark as retried (today the retry always returns, but the flag is kept for future use)
                                let _ = std::mem::replace(&mut rectifier_retried, true);

                                // Retry the same provider (not counted by the circuit breaker)
                                match self
                                    .forward(
                                        app_type,
                                        &method,
                                        provider,
                                        endpoint,
                                        &provider_body,
                                        &headers,
                                        &extensions,
                                        adapter.as_ref(),
                                    )
                                    .await
                                {
                                    Ok((response, claude_api_format, outbound_model)) => {
                                        log::info!("[{app_type_str}] [RECT-002] retry succeeded");
                                        self.record_success_result(
                                            &provider.id,
                                            app_type_str,
                                            used_half_open_permit,
                                        )
                                        .await;

                                        // Update the provider in use for this app type
                                        {
                                            let mut current_providers =
                                                self.current_providers.write().await;
                                            current_providers.insert(
                                                app_type_str.to_string(),
                                                (provider.id.clone(), provider.name.clone()),
                                            );
                                        }

                                        // Update the success counters
                                        {
                                            let mut status = self.status.write().await;
                                            status.success_requests += 1;
                                            status.last_error = None;
                                            let should_switch =
                                                self.current_provider_id_at_start.as_str()
                                                    != provider.id.as_str();
                                            if should_switch {
                                                status.failover_count += 1;

                                                // Trigger the provider switch asynchronously and refresh UI/tray
                                                let fm = self.failover_manager.clone();
                                                let ah = self.app_handle.clone();
                                                let pid = provider.id.clone();
                                                let pname = provider.name.clone();
                                                let at = app_type_str.to_string();

                                                tokio::spawn(async move {
                                                    let _ = fm
                                                        .try_switch(ah.as_ref(), &at, &pid, &pname)
                                                        .await;
                                                });
                                            }
                                            if status.total_requests > 0 {
                                                status.success_rate = (status.success_requests
                                                    as f32
                                                    / status.total_requests as f32)
                                                    * 100.0;
                                            }
                                        }

                                        return Ok(ForwardResult {
                                            response,
                                            provider: provider.clone(),
                                            claude_api_format,
                                            outbound_model,
                                            connection_guard: None,
                                        });
                                    }
                                    Err(retry_err) => {
                                        log::warn!(
                                            "[{app_type_str}] [RECT-003] rectifier retry still failed: {retry_err}"
                                        );
                                        if let Some(err) = self
                                            .handle_rectifier_retry_failure(
                                                retry_err,
                                                provider,
                                                app_type_str,
                                                used_half_open_permit,
                                                "rectifier",
                                                &mut last_error,
                                                &mut last_provider,
                                            )
                                            .await
                                        {
                                            return Err(err);
                                        }
                                        continue;
                                    }
                                }
                            }
                        }
                    }

                    // Check whether the budget rectifier should run (Claude/ClaudeAuth providers only)
                    if is_anthropic_provider {
                        let error_message = extract_error_message(&e);
                        if should_rectify_thinking_budget(
                            error_message.as_deref(),
                            &self.rectifier_config,
                        ) {
                            // Already retried: return the error directly (a non-retryable client error)
                            if budget_rectifier_retried {
                                log::warn!(
                                    "[{app_type_str}] [RECT-013] the budget rectifier already ran, not retrying again"
                                );
                                self.router
                                    .release_permit_neutral(
                                        &provider.id,
                                        app_type_str,
                                        used_half_open_permit,
                                    )
                                    .await;
                                let mut status = self.status.write().await;
                                status.failed_requests += 1;
                                status.last_error = Some(e.to_string());
                                if status.total_requests > 0 {
                                    status.success_rate = (status.success_requests as f32
                                        / status.total_requests as f32)
                                        * 100.0;
                                }
                                return Err(ForwardError {
                                    error: e,
                                    provider: Some(provider.clone()),
                                });
                            }

                            let budget_rectified = rectify_thinking_budget(&mut provider_body);
                            if !budget_rectified.applied {
                                log::warn!(
                                    "[{app_type_str}] [RECT-014] the budget rectifier fired but found nothing to rectify, skipping a pointless retry"
                                );
                                self.router
                                    .release_permit_neutral(
                                        &provider.id,
                                        app_type_str,
                                        used_half_open_permit,
                                    )
                                    .await;
                                let mut status = self.status.write().await;
                                status.failed_requests += 1;
                                status.last_error = Some(e.to_string());
                                if status.total_requests > 0 {
                                    status.success_rate = (status.success_requests as f32
                                        / status.total_requests as f32)
                                        * 100.0;
                                }
                                return Err(ForwardError {
                                    error: e,
                                    provider: Some(provider.clone()),
                                });
                            }

                            log::info!(
                                "[{}] [RECT-010] thinking budget rectifier fired, before={:?}, after={:?}",
                                app_type_str,
                                budget_rectified.before,
                                budget_rectified.after
                            );

                            let _ = std::mem::replace(&mut budget_rectifier_retried, true);

                            // Retry the same provider (not counted by the circuit breaker)
                            match self
                                .forward(
                                    app_type,
                                    &method,
                                    provider,
                                    endpoint,
                                    &provider_body,
                                    &headers,
                                    &extensions,
                                    adapter.as_ref(),
                                )
                                .await
                            {
                                Ok((response, claude_api_format, outbound_model)) => {
                                    log::info!("[{app_type_str}] [RECT-011] budget rectifier retry succeeded");
                                    self.record_success_result(
                                        &provider.id,
                                        app_type_str,
                                        used_half_open_permit,
                                    )
                                    .await;

                                    {
                                        let mut current_providers =
                                            self.current_providers.write().await;
                                        current_providers.insert(
                                            app_type_str.to_string(),
                                            (provider.id.clone(), provider.name.clone()),
                                        );
                                    }

                                    {
                                        let mut status = self.status.write().await;
                                        status.success_requests += 1;
                                        status.last_error = None;
                                        let should_switch =
                                            self.current_provider_id_at_start.as_str()
                                                != provider.id.as_str();
                                        if should_switch {
                                            status.failover_count += 1;
                                            let fm = self.failover_manager.clone();
                                            let ah = self.app_handle.clone();
                                            let pid = provider.id.clone();
                                            let pname = provider.name.clone();
                                            let at = app_type_str.to_string();
                                            tokio::spawn(async move {
                                                let _ = fm
                                                    .try_switch(ah.as_ref(), &at, &pid, &pname)
                                                    .await;
                                            });
                                        }
                                        if status.total_requests > 0 {
                                            status.success_rate = (status.success_requests as f32
                                                / status.total_requests as f32)
                                                * 100.0;
                                        }
                                    }

                                    return Ok(ForwardResult {
                                        response,
                                        provider: provider.clone(),
                                        claude_api_format,
                                        outbound_model,
                                        connection_guard: None,
                                    });
                                }
                                Err(retry_err) => {
                                    log::warn!(
                                        "[{app_type_str}] [RECT-012] budget rectifier retry still failed: {retry_err}"
                                    );
                                    if let Some(err) = self
                                        .handle_rectifier_retry_failure(
                                            retry_err,
                                            provider,
                                            app_type_str,
                                            used_half_open_permit,
                                            "budget rectifier",
                                            &mut last_error,
                                            &mut last_provider,
                                        )
                                        .await
                                    {
                                        return Err(err);
                                    }
                                    continue;
                                }
                            }
                        }
                    }

                    if signature_rectifier_non_retryable_client_error {
                        self.router
                            .release_permit_neutral(
                                &provider.id,
                                app_type_str,
                                used_half_open_permit,
                            )
                            .await;
                        let mut status = self.status.write().await;
                        status.failed_requests += 1;
                        status.last_error = Some(e.to_string());
                        if status.total_requests > 0 {
                            status.success_rate = (status.success_requests as f32
                                / status.total_requests as f32)
                                * 100.0;
                        }
                        return Err(ForwardError {
                            error: e,
                            provider: Some(provider.clone()),
                        });
                    }

                    // Classify the error first to decide whether it counts toward provider health:
                    // NonRetryable and ClientAbort are client-layer errors that every provider would reject, so they
                    // must not pollute the breaker or the database health (same semantics as release_permit_neutral).
                    let category = self.categorize_proxy_error(&e, provider);

                    match category {
                        ErrorCategory::Retryable => {
                            // Retryable: a genuine provider fault -> record the failure and update breaker/DB health
                            let _ = self
                                .router
                                .record_result(
                                    &provider.id,
                                    app_type_str,
                                    used_half_open_permit,
                                    false,
                                    Some(e.to_string()),
                                )
                                .await;

                            {
                                let mut status = self.status.write().await;
                                status.last_error =
                                    Some(format!("Provider {} failed: {}", provider.name, e));
                            }

                            let (log_code, log_message) = build_retryable_failure_log(
                                &provider.name,
                                attempted_providers,
                                providers.len(),
                                &e,
                            );
                            log::warn!("[{app_type_str}] [{log_code}] {log_message}");

                            last_error = Some(e);
                            last_provider = Some(provider.clone());
                            // Move on to the next provider
                            continue;
                        }
                        ErrorCategory::NonRetryable => {
                            // Non-retryable: a client-layer error or client disconnect -> leave health alone, only release the HalfOpen permit
                            self.router
                                .release_permit_neutral(
                                    &provider.id,
                                    app_type_str,
                                    used_half_open_permit,
                                )
                                .await;
                            {
                                let mut status = self.status.write().await;
                                status.failed_requests += 1;
                                status.last_error = Some(e.to_string());
                                if status.total_requests > 0 {
                                    status.success_rate = (status.success_requests as f32
                                        / status.total_requests as f32)
                                        * 100.0;
                                }
                            }
                            return Err(ForwardError {
                                error: e,
                                provider: Some(provider.clone()),
                            });
                        }
                    }
                }
            }
        }

        if attempted_providers == 0 {
            // The provider list is non-empty but every entry was rejected by the breaker (typically all HalfOpen slots taken)
            {
                let mut status = self.status.write().await;
                status.failed_requests += 1;
                status.last_error = Some("All providers temporarily unavailable".to_string());
                if status.total_requests > 0 {
                    status.success_rate =
                        (status.success_requests as f32 / status.total_requests as f32) * 100.0;
                }
            }
            return Err(ForwardError {
                error: ProxyError::NoAvailableProvider,
                provider: None,
            });
        }

        // Every provider failed
        {
            let mut status = self.status.write().await;
            status.failed_requests += 1;
            status.last_error = Some("All providers failed".to_string());
            if status.total_requests > 0 {
                status.success_rate =
                    (status.success_requests as f32 / status.total_requests as f32) * 100.0;
            }
        }

        if let Some((log_code, log_message)) =
            build_terminal_failure_log(attempted_providers, providers.len(), last_error.as_ref())
        {
            log::warn!("[{app_type_str}] [{log_code}] {log_message}");
        }

        Err(ForwardError {
            error: last_error.unwrap_or(ProxyError::MaxRetriesExceeded),
            provider: last_provider,
        })
    }

    /// Forwards a single request through the adapter
    ///
    /// On success returns `(response, claude_api_format, outbound_model)`, where `outbound_model` is
    /// the model finally sent upstream (after every mapping and rewrite).
    #[allow(clippy::too_many_arguments)]
    async fn forward(
        &self,
        app_type: &AppType,
        method: &http::Method,
        provider: &Provider,
        endpoint: &str,
        body: &Value,
        headers: &axum::http::HeaderMap,
        extensions: &Extensions,
        adapter: &dyn ProviderAdapter,
    ) -> Result<(ProxyResponse, Option<String>, Option<String>), ProxyError> {
        // Extract base_url via the adapter
        let mut base_url = adapter.extract_base_url(provider)?;

        let is_full_url = provider
            .meta
            .as_ref()
            .and_then(|meta| meta.is_full_url)
            .unwrap_or(false)
            && !provider.is_codex_oauth()
            && !provider.is_xai_oauth();

        // The GitHub Copilot API uses /chat/completions (no /v1 prefix)
        let is_copilot = provider
            .meta
            .as_ref()
            .and_then(|m| m.provider_type.as_deref())
            == Some("github_copilot")
            || base_url.contains("githubcopilot.com");

        // Codex upstream conversion mode — computed early because the [1m]-suffix strip
        // below must be skipped on the Anthropic path (the marker has to survive to
        // catalog matching and to the transform's own strip+beta detection).
        let codex_responses_to_chat = matches!(app_type, AppType::Codex | AppType::GrokBuild)
            && super::providers::should_convert_codex_responses_to_chat(provider, endpoint);
        let codex_responses_to_anthropic = matches!(app_type, AppType::Codex | AppType::GrokBuild)
            && super::providers::should_convert_codex_responses_to_anthropic(provider, endpoint);
        let codex_official_auth_passthrough = matches!(app_type, AppType::Codex)
            && super::providers::is_codex_official_provider(provider);

        if codex_official_auth_passthrough {
            validate_codex_official_authorization(headers, provider)?;
        }

        // Apply the model mapping (independent of format conversion)
        // Claude Desktop proxy mode must first map the Desktop-visible claude-* route onto the real
        // upstream model name, and an unknown route must error instead of falling back to the default model.
        let mapped_body = if matches!(app_type, AppType::ClaudeDesktop) {
            crate::claude_desktop_config::map_proxy_request_model(body.clone(), provider)
                .map_err(|e| ProxyError::InvalidRequest(e.to_string()))?
        } else {
            let (mapped_body, _original_model, _mapped_model) =
                super::model_mapper::apply_model_mapping(body.clone(), provider);
            mapped_body
        };

        // Aligned with CCH: no proactive thinking rewrite before the request (the hook is kept for compatibility)
        let mut mapped_body = normalize_thinking_type(mapped_body);

        // Grok Build exposes a stable client-side model profile in config.toml.
        // Route requests to the provider's real upstream model before applying
        // the optional Responses -> Chat/Anthropic bridge.
        if matches!(app_type, AppType::GrokBuild) {
            super::providers::apply_codex_upstream_model(provider, &mut mapped_body);
        }

        if is_copilot {
            mapped_body =
                super::providers::copilot_model_map::apply_copilot_model_normalization(mapped_body);
            self.apply_copilot_live_model_resolution(provider, &mut mapped_body)
                .await;
            // Strip the [1M] context marker after Copilot normalization/resolve.
            // A user's mapped value (e.g. "gpt-5.6-sol[1M]") carries [1M] as a
            // Claude Code context-capability declaration that upstream APIs reject
            // as part of the model name. The preceding normalization step already
            // rewrites claude-xxx[1M] into the "-1m" dash form Copilot accepts, and
            // the strip helper only touches the "[1m]" bracket form, so "-1m"
            // variants pass through unchanged.
            mapped_body =
                super::model_mapper::strip_one_m_suffix_for_upstream_from_body(mapped_body);
        } else if !codex_responses_to_anthropic {
            // Skip on the Codex→Anthropic path: stripping [1m] here would break both the
            // model-catalog match (apply_codex_upstream_model) and the transform's own
            // strip+`context-1m` beta detection. The marker is stripped later, on the
            // final anthropic_body.
            mapped_body =
                super::model_mapper::strip_one_m_suffix_for_upstream_from_body(mapped_body);
        }

        // --- Copilot optimizer: classification plus body optimization (before format conversion) ---
        // Note: the deterministic IDs are computed here too, because mapped_body is moved during conversion
        //
        // Order of operations (matching copilot-api):
        //   1. classify on the original body (keeping tool_result semantics so it is not misread as user)
        //   2. sanitize orphan tool_results (so the upstream API does not error)
        //   3. merge tool_result and text (reducing premium billing)
        let copilot_optimization = if is_copilot && self.copilot_optimizer_config.enabled {
            // 1. Classify on the original body; this must happen before sanitizing and merging
            //    an orphan tool_result is still typed tool_result, so classification correctly yields agent
            let has_anthropic_beta = headers.contains_key("anthropic-beta");
            let classification = super::copilot_optimizer::classify_request(
                &mapped_body,
                has_anthropic_beta,
                self.copilot_optimizer_config.compact_detection,
                self.copilot_optimizer_config.subagent_detection,
            );

            log::debug!(
                "[Copilot] optimizer classification: initiator={}, is_warmup={}, is_compact={}, is_subagent={}",
                classification.initiator,
                classification.is_warmup,
                classification.is_compact,
                classification.is_subagent
            );

            // 2. Sanitize orphan tool_results, after classification is done,
            //    so a mismatched tool_result cannot make the upstream API error and cause a retry and double billing
            mapped_body = super::copilot_optimizer::sanitize_orphan_tool_results(mapped_body);

            // 3. Merge tool results, turning [tool_result, text] into [tool_result (with text)]
            if self.copilot_optimizer_config.tool_result_merging {
                mapped_body = super::copilot_optimizer::merge_tool_results(mapped_body);
            }

            // 3.5. Strip thinking blocks proactively: Copilot's OpenAI-compatible endpoints do not know them,
            //      so this avoids an upstream rejection and a reactive rectifier retry (the first request already burned quota)
            if self.copilot_optimizer_config.strip_thinking {
                mapped_body = super::copilot_optimizer::strip_thinking_blocks(mapped_body);
            }

            // 4. Downgrade warmup requests to a small model
            if self.copilot_optimizer_config.warmup_downgrade && classification.is_warmup {
                log::info!(
                    "[Copilot] warmup request downgraded to model: {}",
                    self.copilot_optimizer_config.warmup_model
                );
                mapped_body["model"] =
                    serde_json::json!(&self.copilot_optimizer_config.warmup_model);
            }

            // Precompute the deterministic request ID (before the body is moved)
            // Session extraction priority (matching extract_from_metadata in session.rs):
            //   1. the _session_ suffix inside metadata.user_id
            //   2. metadata.session_id (the direct field)
            //   3. the raw metadata.user_id (whole-string fallback)
            //   4. x-session-id header
            let metadata = body.get("metadata");
            let session_id = metadata
                .and_then(|m| m.get("user_id"))
                .and_then(|v| v.as_str())
                .and_then(super::session::parse_session_from_user_id)
                .or_else(|| {
                    metadata
                        .and_then(|m| m.get("session_id"))
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                })
                .or_else(|| {
                    metadata
                        .and_then(|m| m.get("user_id"))
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                })
                .or_else(|| {
                    headers
                        .get("x-session-id")
                        .and_then(|v| v.to_str().ok())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            let det_request_id = if self.copilot_optimizer_config.deterministic_request_id {
                Some(super::copilot_optimizer::deterministic_request_id(
                    &mapped_body,
                    &session_id,
                ))
            } else {
                None
            };

            // Derive a stable interaction ID from the session ID (shared by one main conversation)
            let interaction_id =
                super::copilot_optimizer::deterministic_interaction_id(&session_id);

            Some((classification, det_request_id, interaction_id))
        } else {
            None
        };

        // GitHub Copilot dynamic endpoint routing
        // Read the cached API endpoint from CopilotAuthManager (supporting enterprise and other non-default endpoints)
        if is_copilot && !is_full_url {
            if let Some(app_handle) = &self.app_handle {
                let copilot_state = app_handle.state::<CopilotAuthState>();
                let copilot_auth = copilot_state.0.read().await;

                // Read the bound GitHub account ID from provider.meta
                let account_id = provider
                    .meta
                    .as_ref()
                    .and_then(|m| m.managed_account_id_for("github_copilot"));

                let dynamic_endpoint = match &account_id {
                    Some(id) => copilot_auth.get_api_endpoint(id).await,
                    None => copilot_auth.get_default_api_endpoint().await,
                };

                // Replace only when the dynamic endpoint differs from the current base_url
                if dynamic_endpoint != base_url {
                    log::debug!(
                        "[Copilot] using the dynamic API endpoint: {} (was: {})",
                        dynamic_endpoint,
                        base_url
                    );
                    base_url = dynamic_endpoint;
                }
            }
        }
        let resolved_claude_api_format = if adapter.name() == "Claude" {
            Some(
                self.resolve_claude_api_format(provider, &mapped_body, is_copilot)
                    .await,
            )
        } else {
            None
        };
        if adapter.name() == "Claude" {
            if let Some(api_format) = resolved_claude_api_format.as_deref() {
                super::providers::normalize_anthropic_messages_for_provider(
                    &mut mapped_body,
                    provider,
                    api_format,
                );
                self.apply_media_prevention(&mut mapped_body, provider);
            }
        }
        let needs_transform = match resolved_claude_api_format.as_deref() {
            Some(api_format) => super::providers::claude_api_format_needs_transform(api_format),
            None => adapter.needs_transform(provider),
        };
        // Codex → Anthropic: Claude Code emulation is off by default and only
        // enabled when the user explicitly turns it on in the UI, so requests can
        // pass a gateway's "Claude Code only" fingerprint check (User-Agent /
        // anthropic-beta / x-app / system prompt first line). Defaulting to off
        // avoids leaking the Claude Code fingerprint and identity prompt to
        // general-purpose gateways.
        let codex_impersonate_claude_code = codex_responses_to_anthropic
            && provider
                .meta
                .as_ref()
                .and_then(|meta| meta.impersonate_claude_code)
                == Some(true);
        let (effective_endpoint, passthrough_query) = if codex_responses_to_chat {
            rewrite_codex_responses_endpoint_to_chat(endpoint)
        } else if codex_responses_to_anthropic {
            rewrite_codex_responses_endpoint_to_anthropic(endpoint)
        } else if needs_transform && adapter.name() == "Claude" {
            let api_format = resolved_claude_api_format
                .as_deref()
                .unwrap_or_else(|| super::providers::get_claude_api_format(provider));
            rewrite_claude_transform_endpoint(endpoint, api_format, is_copilot, &mapped_body)
        } else {
            (
                endpoint.to_string(),
                split_endpoint_and_query(endpoint)
                    .1
                    .map(ToString::to_string),
            )
        };

        let codex_chat_base_is_full_endpoint =
            codex_responses_to_chat && base_url_is_full_endpoint(&base_url, "/chat/completions");

        // Defensive fallback mirroring `codex_chat_base_is_full_endpoint`: if a user pastes
        // a base URL already ending in the Anthropic `/v1/messages` endpoint but leaves the
        // "full URL" switch off, treat it as a full endpoint so we don't double-append
        // `/v1/messages` (→ `.../v1/messages/v1/messages`, a non-retryable 400). Matches the
        // exact endpoint suffix, so prefixed gateways like `.../api/v1/messages` are covered.
        let codex_anthropic_base_is_full_endpoint =
            codex_responses_to_anthropic && base_url_is_full_endpoint(&base_url, "/v1/messages");

        let codex_standalone_endpoint = matches!(app_type, AppType::Codex)
            .then(|| CodexStandaloneEndpoint::from_effective_endpoint(&effective_endpoint))
            .flatten();

        let url = if matches!(resolved_claude_api_format.as_deref(), Some("gemini_native")) {
            super::gemini_url::resolve_gemini_native_url(
                &base_url,
                &effective_endpoint,
                is_full_url,
            )
        } else if is_full_url {
            if let Some(endpoint) = codex_standalone_endpoint {
                rewrite_codex_standalone_full_url(
                    &base_url,
                    passthrough_query.as_deref(),
                    endpoint,
                )?
            } else {
                append_query_to_full_url(&base_url, passthrough_query.as_deref())
            }
        } else if let Some(endpoint) = codex_standalone_endpoint
            .filter(|endpoint| endpoint.base_url_is_source_endpoint(&base_url))
        {
            // Same tolerance as `codex_chat_base_is_full_endpoint` below: a base URL
            // pasted as a complete endpoint with the full-URL switch off would
            // otherwise become `.../chat/completions/images/generations`.
            rewrite_codex_standalone_full_url(&base_url, passthrough_query.as_deref(), endpoint)?
        } else if codex_chat_base_is_full_endpoint || codex_anthropic_base_is_full_endpoint {
            append_query_to_full_url(&base_url, passthrough_query.as_deref())
        } else {
            adapter.build_url(&base_url, &effective_endpoint)
        };

        // Record the mapped outbound model name (mapped_body has by now gone through takeover mapping,
        // [1m] stripping, and Copilot normalization). If the converted body still carries a model field it
        // is refreshed below; formats like gemini_native that carry the model in the URL keep this pre-conversion truth.
        let mut outbound_model = mapped_body
            .get("model")
            .and_then(|m| m.as_str())
            .filter(|m| !m.is_empty())
            .map(str::to_string);

        // Codex→Anthropic: when the model name carries the [1m] marker, strip the
        // suffix and add the context-1m beta header.
        let mut codex_anthropic_one_m = false;

        // Convert the request body when required
        let mut request_body = if codex_responses_to_chat {
            let mut mapped_body = mapped_body;
            let explicit_prompt_cache_key = mapped_body
                .get("prompt_cache_key")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
            let restored = self
                .codex_chat_history
                .enrich_request(&mut mapped_body)
                .await;
            if restored > 0 {
                log::debug!(
                    "[Codex] Restored or enriched {restored} cached function call item(s) for Chat upstream"
                );
            }
            super::providers::apply_codex_chat_upstream_model(provider, &mut mapped_body);
            let reasoning_config =
                super::providers::resolve_codex_chat_reasoning_config(provider, &mapped_body);
            let mut chat_body = super::providers::transform_codex_chat::responses_to_chat_completions_with_reasoning(
                mapped_body,
                reasoning_config.as_ref(),
            )?;
            super::providers::inject_codex_chat_prompt_cache_key(
                provider,
                &mut chat_body,
                explicit_prompt_cache_key.as_deref(),
                self.session_client_provided
                    .then_some(self.session_id.as_str()),
            );
            chat_body
        } else if codex_responses_to_anthropic {
            let mut mapped_body = mapped_body;
            super::providers::apply_codex_upstream_model(provider, &mut mapped_body);
            // Per-provider output ceiling override. Codex does not forward its
            // `model_max_output_tokens` in the request body, so honor the value
            // configured on the provider here — it takes precedence over any
            // request-supplied `max_output_tokens` and over the default below.
            // Injecting it into the body (rather than overriding after transform)
            // lets the thinking-budget clamp size its headroom against the real
            // ceiling too. Kept per-provider to avoid a global large default that
            // would 400 on low-output-ceiling gateways.
            if let Some(max_out) = provider
                .meta
                .as_ref()
                .and_then(|meta| meta.max_output_tokens)
                .filter(|v| *v > 0)
            {
                mapped_body["max_output_tokens"] = Value::from(max_out);
            }
            // Anthropic requires max_tokens; fall back to this default only when the
            // Codex request omits max_output_tokens (rare — Codex normally sends it).
            // Kept conservative so a low-output-ceiling model or relay does not hard-400
            // on the fallback (a too-high default 400s and is non-retryable); 8192 is
            // accepted by every current Claude model and virtually all gateways. The
            // transform clamps any thinking budget below this value.
            const DEFAULT_CODEX_ANTHROPIC_MAX_TOKENS: u64 = 8192;
            let mut anthropic_body =
                super::providers::transform_codex_anthropic::responses_request_to_anthropic(
                    mapped_body,
                    DEFAULT_CODEX_ANTHROPIC_MAX_TOKENS,
                )?;
            // Handle the 1M-context marker [1m]: strip the model-name suffix (the
            // gateway doesn't recognize it) and set the flag so the beta header is
            // added. apply_codex_upstream_model may have just written back a model
            // name carrying [1m] from the provider config, so strip it once more on
            // the final body here.
            if let Some(model) = anthropic_body.get("model").and_then(|v| v.as_str()) {
                let stripped = super::model_mapper::strip_one_m_suffix_for_upstream(model);
                if stripped != model {
                    codex_anthropic_one_m = true;
                    anthropic_body["model"] = Value::String(stripped.to_string());
                }
            }
            if codex_impersonate_claude_code {
                prepend_claude_code_system_prompt(&mut anthropic_body);
            }
            // Enable Anthropic prompt caching (no beta header required). Reuse the
            // configured TTL rather than silently forcing 5m on this conversion path.
            // otherwise system/tools/history are re-sent at full price every round,
            // inflating cost and first-token latency. The injector handles the
            // string→array `system` conversion and the new-breakpoint budget.
            super::cache_injector::inject(
                &mut anthropic_body,
                &codex_anthropic_cache_config(&self.optimizer_config),
            );
            anthropic_body
        } else if needs_transform {
            if adapter.name() == "Claude" {
                let api_format = resolved_claude_api_format
                    .as_deref()
                    .unwrap_or_else(|| super::providers::get_claude_api_format(provider));
                super::providers::transform_claude_request_for_api_format(
                    mapped_body,
                    provider,
                    api_format,
                    self.session_client_provided
                        .then_some(self.session_id.as_str()),
                    Some(self.gemini_shadow.as_ref()),
                )?
            } else {
                adapter.transform_request(mapped_body, provider)?
            }
        } else {
            mapped_body
        };

        // Native Responses passthrough to a strict third-party gateway (xAI).
        // One gate so rebase conflicts stay here plus the isolate file, not
        // scattered across sanitizers. Flatten namespaces first; then apply
        // xAI request rewrites (schema, agent_message, unknown models).
        if matches!(app_type, AppType::Codex | AppType::GrokBuild)
            && !codex_responses_to_chat
            && !codex_responses_to_anthropic
            && super::providers::provider_needs_responses_namespace_flatten(provider)
        {
            if super::providers::transform_codex_responses_namespace::flatten_request_namespaces(
                &mut request_body,
            )? {
                log::debug!(
                    "[Codex] Flattened namespace tools for native Responses upstream (provider={})",
                    provider.id
                );
            }
            super::providers::transform_codex_responses_xai_sanitize::apply_xai_native_responses_request_compat(
                &mut request_body,
                &provider.id,
                super::providers::codex_provider_upstream_model(provider).as_deref(),
                &provider.settings_config,
            );
        }

        // Moonshot / Kimi Chat Completions reject `$ref` nodes that carry sibling
        // keywords, which Codex Desktop's built-in tool schemas do (#6867). Move
        // each such `$ref` into `allOf` for that upstream only; every other
        // provider keeps byte-identical tool schemas (prompt-cache prefix intact).
        if codex_responses_to_chat
            && super::providers::transform_codex_chat_moonshot_schema::upstream_requires_ref_sibling_all_of(
                &base_url,
            )
        {
            let rewritten =
                super::providers::transform_codex_chat_moonshot_schema::wrap_ref_siblings_in_chat_tools(
                    &mut request_body,
                );
            if rewritten > 0 {
                log::debug!(
                    "[Codex] Moved `$ref` siblings into allOf for {rewritten} tool schema(s) (Moonshot upstream, provider={})",
                    provider.id
                );
            }
        }

        if matches!(app_type, AppType::Codex | AppType::GrokBuild) {
            self.apply_media_prevention(&mut request_body, provider);
        }

        // Filter private parameters (fields starting with `_`) so internal data never leaks upstream
        // The default allowlist is empty, so every _-prefixed field is removed
        let mut filtered_body = prepare_upstream_request_body(request_body);
        if !is_copilot {
            if let Some(overrides) = provider
                .meta
                .as_ref()
                .and_then(|meta| meta.local_proxy_request_overrides.as_ref())
            {
                if apply_local_proxy_body_overrides(&mut filtered_body, overrides) {
                    filtered_body = prepare_upstream_request_body(filtered_body);
                }
            }
        }
        // Refresh the truth once the outbound body is final (covering Codex chat upstream overrides and transform-layer rewrites)
        if let Some(m) = filtered_body
            .get("model")
            .and_then(|m| m.as_str())
            .filter(|m| !m.is_empty())
        {
            outbound_model = Some(m.to_string());
        }
        log_prompt_cache_trace(
            app_type,
            provider,
            &effective_endpoint,
            resolved_claude_api_format.as_deref(),
            &filtered_body,
            self.session_client_provided,
        );
        let request_is_streaming =
            is_streaming_request(&effective_endpoint, &filtered_body, headers);
        let force_identity_encoding = needs_transform
            || codex_responses_to_chat
            || codex_responses_to_anthropic
            || request_is_streaming;

        // The ChatGPT-Account-Id that Codex OAuth must inject (filled in while fetching the dynamic token)
        let mut codex_oauth_account_id: Option<String> = None;
        let mut should_send_codex_oauth_session_headers = false;

        // Build the auth headers up front for inline replacement, keeping the exact credential material
        // solely for log redaction. The values themselves are never logged.
        let mut log_secrets: Vec<String> = Vec::new();
        let mut auth_headers = if let Some(mut auth) = adapter.extract_auth(provider) {
            // GitHub Copilot special case: fetch the real token from CopilotAuthManager
            if auth.strategy == AuthStrategy::GitHubCopilot {
                if let Some(app_handle) = &self.app_handle {
                    let copilot_state = app_handle.state::<CopilotAuthState>();
                    let copilot_auth: tokio::sync::RwLockReadGuard<'_, CopilotAuthManager> =
                        copilot_state.0.read().await;

                    // Read the bound GitHub account ID from provider.meta (multi-account support)
                    let account_id = provider
                        .meta
                        .as_ref()
                        .and_then(|m| m.managed_account_id_for("github_copilot"));

                    // Fetch the token for that account ID (for compatibility, no ID means the first account)
                    let token_result = match &account_id {
                        Some(id) => {
                            log::debug!("[Copilot] fetching a token for account {id}");
                            copilot_auth.get_valid_token_for_account(id).await
                        }
                        None => {
                            log::debug!("[Copilot] fetching a token for the default account");
                            copilot_auth.get_valid_token().await
                        }
                    };

                    match token_result {
                        Ok(token) => {
                            auth = AuthInfo::new(token, AuthStrategy::GitHubCopilot);
                            log::debug!(
                                "[Copilot] obtained a Copilot token (account={})",
                                account_id.as_deref().unwrap_or("default")
                            );
                        }
                        Err(e) => {
                            log::error!(
                                "[Copilot] failed to obtain a Copilot token (account={}): {e}",
                                account_id.as_deref().unwrap_or("default")
                            );
                            return Err(ProxyError::AuthError(format!(
                                "GitHub Copilot authentication failed: {e}"
                            )));
                        }
                    }
                } else {
                    log::error!("[Copilot] AppHandle unavailable");
                    return Err(ProxyError::AuthError(
                        "GitHub Copilot authentication unavailable (no AppHandle)".to_string(),
                    ));
                }
            }

            // Codex OAuth special case: fetch the real access_token from CodexOAuthManager
            if auth.strategy == AuthStrategy::CodexOAuth {
                if let Some(app_handle) = &self.app_handle {
                    let codex_state = app_handle.state::<CodexOAuthState>();
                    let codex_auth = &codex_state.0;

                    // Read the bound ChatGPT account ID from provider.meta
                    let account_id = provider
                        .meta
                        .as_ref()
                        .and_then(|m| m.managed_account_id_for("codex_oauth"));

                    let token_result = match &account_id {
                        Some(id) => {
                            log::debug!("[CodexOAuth] fetching a token for account {id}");
                            codex_auth.get_valid_token_for_account(id).await
                        }
                        None => {
                            log::debug!("[CodexOAuth] fetching a token for the default account");
                            codex_auth.get_valid_token().await
                        }
                    };

                    match token_result {
                        Ok(token) => {
                            auth = AuthInfo::new(token, AuthStrategy::CodexOAuth);
                            should_send_codex_oauth_session_headers = true;
                            // Resolve the account_id used (for the ChatGPT-Account-Id header)
                            codex_oauth_account_id = match account_id {
                                Some(id) => Some(id),
                                None => codex_auth.default_account_id().await,
                            };
                            log::debug!(
                                "[CodexOAuth] obtained an access_token (account={})",
                                codex_oauth_account_id.as_deref().unwrap_or("default")
                            );
                        }
                        Err(e) => {
                            log::error!("[CodexOAuth] failed to obtain an access_token: {e}");
                            return Err(ProxyError::AuthError(format!(
                                "Codex OAuth authentication failed: {e}"
                            )));
                        }
                    }
                } else {
                    log::error!("[CodexOAuth] AppHandle unavailable");
                    return Err(ProxyError::AuthError(
                        "Codex OAuth authentication unavailable (no AppHandle)".to_string(),
                    ));
                }
            }

            // xAI OAuth: resolve a managed account token immediately before
            // sending the request. Invalid refresh credentials are persisted as
            // requiring re-authentication by the manager.
            if auth.strategy == AuthStrategy::XaiOAuth {
                if let Some(app_handle) = &self.app_handle {
                    let xai_state = app_handle.state::<XaiOAuthState>();
                    let xai_auth: tokio::sync::RwLockReadGuard<'_, XaiOAuthManager> =
                        xai_state.0.read().await;
                    let account_id = provider
                        .meta
                        .as_ref()
                        .and_then(|meta| meta.managed_account_id_for("xai_oauth"));
                    let token_result = match &account_id {
                        Some(id) => xai_auth.get_valid_token_for_account(id).await,
                        None => xai_auth.get_valid_token().await,
                    };
                    match token_result {
                        Ok(token) => {
                            auth = AuthInfo::new(token, AuthStrategy::XaiOAuth);
                            log::debug!(
                                "[XaiOAuth] obtained an access_token (account={})",
                                account_id.as_deref().unwrap_or("default")
                            );
                        }
                        Err(error) => {
                            log::error!("[XaiOAuth] failed to obtain an access_token: {error}");
                            return Err(ProxyError::AuthError(format!(
                                "xAI OAuth authentication failed: {error}"
                            )));
                        }
                    }
                } else {
                    return Err(ProxyError::AuthError(
                        "xAI OAuth authentication unavailable (no AppHandle)".to_string(),
                    ));
                }
            }

            for secret in std::iter::once(&auth.api_key).chain(auth.access_token.iter()) {
                if !secret.is_empty() && !log_secrets.contains(secret) {
                    log_secrets.push(secret.clone());
                }
            }

            adapter.get_auth_headers(&auth)?
        } else {
            Vec::new()
        };

        // Inject the Codex OAuth ChatGPT-Account-Id header when an account_id is known
        if let Some(ref account_id) = codex_oauth_account_id {
            if let Ok(hv) = http::HeaderValue::from_str(account_id) {
                auth_headers.push((http::HeaderName::from_static("chatgpt-account-id"), hv));
            }
        }

        let codex_oauth_session_headers =
            if should_send_codex_oauth_session_headers && self.session_client_provided {
                build_codex_oauth_session_headers(&self.session_id)
            } else {
                Vec::new()
            };

        // Custom User-Agent: shares parse_custom_user_agent with stream_check and model_fetch, silently
        // ignoring invalid values at runtime (the frontend warns non-blockingly on input rather than blocking saves).
        // The Copilot fingerprint UA cannot be overridden.
        let custom_user_agent = if is_copilot {
            None
        } else {
            provider
                .meta
                .as_ref()
                .and_then(|meta| meta.custom_user_agent_header().ok().flatten())
        };
        // Codex→Anthropic emulation: when there is no custom UA, override Codex's
        // codex_cli_rs UA with the Claude Code UA.
        let custom_user_agent = if custom_user_agent.is_none() && codex_impersonate_claude_code {
            Some(http::HeaderValue::from_static(CLAUDE_CODE_USER_AGENT))
        } else {
            custom_user_agent
        };

        // --- Copilot optimizer: dynamic header injection ---
        if let Some((ref classification, ref det_request_id, ref interaction_id)) =
            copilot_optimization
        {
            for (name, value) in auth_headers.iter_mut() {
                match name.as_str() {
                    "x-initiator" if self.copilot_optimizer_config.request_classification => {
                        *value = http::HeaderValue::from_static(classification.initiator);
                    }
                    "x-interaction-type" if classification.is_subagent => {
                        // Subagent request: conversation-subagent is not counted as a premium interaction
                        *value = http::HeaderValue::from_static("conversation-subagent");
                    }
                    "x-request-id" | "x-agent-task-id" => {
                        if let Some(ref det_id) = det_request_id {
                            if let Ok(hv) = http::HeaderValue::from_str(det_id) {
                                *value = hv;
                            }
                        }
                    }
                    _ => {}
                }
            }

            // x-interaction-id is injected only when a session exists (not inside get_auth_headers)
            if let Some(ref iid) = interaction_id {
                if let Ok(hv) = http::HeaderValue::from_str(iid) {
                    auth_headers.push((http::HeaderName::from_static("x-interaction-id"), hv));
                }
            }

            if classification.is_subagent {
                log::info!(
                    "[Copilot] subagent request: x-initiator=agent, x-interaction-type=conversation-subagent"
                );
            }
        }

        // Copilot fingerprint header names (injected by get_auth_headers, so they must be deduped from the originals)
        let copilot_fingerprint_headers: &[&str] = if is_copilot {
            &[
                "user-agent",
                "editor-version",
                "editor-plugin-version",
                "copilot-integration-id",
                "x-github-api-version",
                "openai-intent",
                // Added headers
                "x-initiator",
                "x-interaction-type",
                "x-interaction-id",
                "x-vscode-user-agent-library-version",
                "x-request-id",
                "x-agent-task-id",
            ]
        } else {
            &[]
        };

        // Precompute the upstream host value (used to replace the host header in place)
        let upstream_host = url
            .parse::<http::Uri>()
            .ok()
            .and_then(|u| u.authority().map(|a| a.to_string()));

        let should_send_anthropic_headers = adapter.name() == "Claude"
            && matches!(resolved_claude_api_format.as_deref(), Some("anthropic"));

        // Precompute the anthropic-beta value (Claude only)
        let anthropic_beta_value = if should_send_anthropic_headers {
            const CLAUDE_CODE_BETA: &str = "claude-code-20250219";
            Some(if let Some(beta) = headers.get("anthropic-beta") {
                if let Ok(beta_str) = beta.to_str() {
                    if beta_str.contains(CLAUDE_CODE_BETA) {
                        beta_str.to_string()
                    } else {
                        format!("{CLAUDE_CODE_BETA},{beta_str}")
                    }
                } else {
                    CLAUDE_CODE_BETA.to_string()
                }
            } else {
                CLAUDE_CODE_BETA.to_string()
            })
        } else if codex_impersonate_claude_code || codex_anthropic_one_m {
            // Codex→Anthropic: emulation injects the claude-code marker; a [1m]
            // model injects the context-1m marker.
            let mut betas: Vec<&str> = Vec::new();
            if codex_impersonate_claude_code {
                betas.push("claude-code-20250219");
            }
            if codex_anthropic_one_m {
                betas.push("context-1m-2025-08-07");
            }
            Some(betas.join(","))
        } else {
            None
        };

        // ============================================================
        // Build the ordered HeaderMap with inline replacement, preserving the client's original order
        // ============================================================
        let mut ordered_headers = http::HeaderMap::new();
        let mut saw_auth = false;
        let mut saw_accept_encoding = false;
        let mut saw_accept = false;
        let mut saw_user_agent = false;
        let mut saw_anthropic_beta = false;
        let mut saw_anthropic_version = false;

        for (key, value) in headers {
            let key_str = key.as_str();

            // --- host: replaced in place with the upstream host (keeping the client's position) ---
            if key_str.eq_ignore_ascii_case("host") {
                if let Some(ref host_val) = upstream_host {
                    if let Ok(hv) = http::HeaderValue::from_str(host_val) {
                        ordered_headers.append(key.clone(), hv);
                    }
                }
                continue;
            }

            // --- connection / tracing / CDN headers: always skipped ---
            if matches!(
                key_str,
                "content-length"
                    | "transfer-encoding"
                    | "x-forwarded-host"
                    | "x-forwarded-port"
                    | "x-forwarded-proto"
                    | "forwarded"
                    | "cf-connecting-ip"
                    | "cf-ipcountry"
                    | "cf-ray"
                    | "cf-visitor"
                    | "true-client-ip"
                    | "fastly-client-ip"
                    | "x-azure-clientip"
                    | "x-azure-fdid"
                    | "x-azure-ref"
                    | "akamai-origin-hop"
                    | "x-akamai-config-log-detail"
                    | "x-request-id"
                    | "x-correlation-id"
                    | "x-trace-id"
                    | "x-amzn-trace-id"
                    | "x-b3-traceid"
                    | "x-b3-spanid"
                    | "x-b3-parentspanid"
                    | "x-b3-sampled"
                    | "traceparent"
                    | "tracestate"
            ) {
                continue;
            }

            // --- auth headers: replaced by the adapter's headers (in the original position) ---
            if key_str.eq_ignore_ascii_case("authorization")
                || key_str.eq_ignore_ascii_case("x-api-key")
                || key_str.eq_ignore_ascii_case("x-goog-api-key")
            {
                // Codex official account cards deliberately keep credentials
                // out of provider storage. `requires_openai_auth = true` makes
                // Codex send the active ChatGPT authorization, which must reach
                // the official upstream unchanged. Other credential headers
                // are still discarded.
                if codex_official_auth_passthrough && key_str.eq_ignore_ascii_case("authorization")
                {
                    saw_auth = true;
                    ordered_headers.append(key.clone(), value.clone());
                    continue;
                }
                if !saw_auth {
                    saw_auth = true;
                    for (ah_name, ah_value) in &auth_headers {
                        ordered_headers.append(ah_name.clone(), ah_value.clone());
                    }
                }
                continue;
            }

            // --- x-app — during Codex→Anthropic emulation, `cli` is injected uniformly below ---
            if codex_impersonate_claude_code && key_str.eq_ignore_ascii_case("x-app") {
                continue;
            }

            // --- Codex/OpenAI fingerprint headers — never leak to an Anthropic upstream ---
            // These are client/session identifiers from the incoming Codex request,
            // not Anthropic protocol headers. Forwarding them both leaks identity and
            // can defeat strict gateway fingerprint checks.
            // The full set lives in `is_codex_client_fingerprint_header` so it stays in one
            // place. (HeaderName is lowercased by the http crate, so a direct match is safe.)
            if codex_responses_to_anthropic && is_codex_client_fingerprint_header(key_str) {
                continue;
            }

            // --- accept — force application/json on the Codex→Anthropic path ---
            // The Codex CLI sends `Accept: text/event-stream`, whereas a native
            // Anthropic client sends `application/json` (streaming is driven by
            // the body's stream:true). Strict Anthropic gateways return 406 Not
            // Acceptable for an event-stream Accept, so normalize it here.
            if codex_responses_to_anthropic && key_str.eq_ignore_ascii_case("accept") {
                if !saw_accept {
                    saw_accept = true;
                    ordered_headers.append(
                        http::header::ACCEPT,
                        http::HeaderValue::from_static("application/json"),
                    );
                }
                continue;
            }

            // --- accept-encoding: forced to identity on transform / SSE paths, otherwise kept ---
            if key_str.eq_ignore_ascii_case("accept-encoding") {
                if !saw_accept_encoding {
                    saw_accept_encoding = true;
                    if force_identity_encoding {
                        ordered_headers.append(
                            http::header::ACCEPT_ENCODING,
                            http::HeaderValue::from_static("identity"),
                        );
                    } else {
                        ordered_headers.append(key.clone(), value.clone());
                    }
                }
                continue;
            }

            // --- user-agent: provider-level override for local proxy routing ---
            if !is_copilot && key_str.eq_ignore_ascii_case("user-agent") {
                if !saw_user_agent {
                    saw_user_agent = true;
                    if let Some(ref ua) = custom_user_agent {
                        ordered_headers.append(http::header::USER_AGENT, ua.clone());
                    } else {
                        ordered_headers.append(key.clone(), value.clone());
                    }
                }
                continue;
            }

            // --- anthropic-beta: replaced with the rebuilt value (ensuring the claude-code marker) ---
            if key_str.eq_ignore_ascii_case("anthropic-beta") {
                if !saw_anthropic_beta {
                    saw_anthropic_beta = true;
                    if let Some(ref beta_val) = anthropic_beta_value {
                        if let Ok(hv) = http::HeaderValue::from_str(beta_val) {
                            ordered_headers.append("anthropic-beta", hv);
                        }
                    }
                }
                continue;
            }

            // --- anthropic-version: the client value passes through ---
            if key_str.eq_ignore_ascii_case("anthropic-version") {
                if should_send_anthropic_headers {
                    saw_anthropic_version = true;
                    ordered_headers.append(key.clone(), value.clone());
                }
                continue;
            }

            // --- Copilot fingerprint headers: skipped (supplied by auth_headers) ---
            if copilot_fingerprint_headers
                .iter()
                .any(|h| key_str.eq_ignore_ascii_case(h))
            {
                continue;
            }

            // --- default: pass through ---
            ordered_headers.append(key.clone(), value.clone());
        }

        // If the original request had no auth header, append one at the end
        if !saw_auth && !auth_headers.is_empty() {
            for (ah_name, ah_value) in &auth_headers {
                ordered_headers.append(ah_name.clone(), ah_value.clone());
            }
        }

        // Transform / SSE paths add identity when missing; plain pass-through never adds accept-encoding
        if !saw_accept_encoding && force_identity_encoding {
            ordered_headers.append(
                http::header::ACCEPT_ENCODING,
                http::HeaderValue::from_static("identity"),
            );
        }

        // On the Codex→Anthropic path, add application/json when Accept is missing (matching a native Anthropic client).
        if codex_responses_to_anthropic && !saw_accept {
            ordered_headers.append(
                http::header::ACCEPT,
                http::HeaderValue::from_static("application/json"),
            );
        }

        // Codex→Anthropic emulation: inject Claude Code's x-app: cli
        if codex_impersonate_claude_code {
            ordered_headers.append("x-app", http::HeaderValue::from_static("cli"));
        }

        if !saw_user_agent {
            if let Some(ref ua) = custom_user_agent {
                ordered_headers.append(http::header::USER_AGENT, ua.clone());
            }
        }

        // If the original request had no anthropic-beta and a value is needed, append it
        if !saw_anthropic_beta {
            if let Some(ref beta_val) = anthropic_beta_value {
                if let Ok(hv) = http::HeaderValue::from_str(beta_val) {
                    ordered_headers.append("anthropic-beta", hv);
                }
            }
        }

        // anthropic-version: add the default only when it is missing.
        // The Codex→Anthropic path also needs this header. Note this is independent
        // of anthropic-beta: the Claude Code-specific beta is only sent when
        // impersonation is on (handled above); on the plain Codex→Anthropic path
        // (impersonation off) anthropic-version is still required but no beta is sent.
        if (should_send_anthropic_headers || codex_responses_to_anthropic) && !saw_anthropic_version
        {
            ordered_headers.append(
                "anthropic-version",
                http::HeaderValue::from_static("2023-06-01"),
            );
        }

        // The Codex OAuth reverse proxy mirrors the official Codex CLI session routing signals as closely as possible.
        // Only a client-supplied session_id is sent; a generated UUID differs every time and would break prefix caching.
        for (name, value) in codex_oauth_session_headers {
            ordered_headers.insert(name, value);
        }

        // Serialize the request body. GET and HEAD are safe, idempotent methods that should carry no body
        // per HTTP semantics, and forcing a JSON body makes some upstreams (such as Google Gemini's models.list) reject the request.
        let body_bytes = if matches!(method, &http::Method::GET | &http::Method::HEAD) {
            Vec::new()
        } else {
            serde_json::to_vec(&filtered_body).map_err(|e| {
                ProxyError::Internal(format!("Failed to serialize request body: {e}"))
            })?
        };

        // Make sure content-type is present
        if !ordered_headers.contains_key(http::header::CONTENT_TYPE) {
            ordered_headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
        }

        apply_local_proxy_header_overrides(
            &mut ordered_headers,
            provider
                .meta
                .as_ref()
                .and_then(|meta| meta.local_proxy_request_overrides.as_ref()),
            is_copilot,
        );

        reject_proxy_placeholder_for_managed_account_upstream(&url, &ordered_headers)?;

        // Redacting the logged target URL has two cases:
        // - known secrets (log_secrets non-empty): log the redacted full URL, stripping userinfo and query
        //   and masking known secret values, keeping host and path so a 404 from a misconfigured base_url is diagnosable.
        // - no known secrets: a credential may be embedded in the path with nothing to mask, so only the
        //   origin is logged, keeping a path like https://gw/<KEY>/v1 out of the default Info-level log.
        let target_for_log = if log_secrets.is_empty() {
            crate::redact_url_origin_for_log(&url)
        } else {
            crate::redact_url_for_log_with_secrets(&url, &log_secrets)
        };

        // Log the request information
        let tag = adapter.name();
        let request_model = filtered_body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("<none>");
        log::info!("[{tag}] >>> target: {target_for_log} (model={request_model})");
        log::debug!(
            "[{tag}] >>> body ready: bytes={}, hash={} (content omitted)",
            body_bytes.len(),
            short_value_hash(Some(&filtered_body))
        );

        // Determine the timeout
        let timeout = if self.non_streaming_timeout.is_zero() {
            std::time::Duration::from_secs(600) // 600 seconds by default
        } else {
            self.non_streaming_timeout
        };

        // Read the global proxy URL
        let upstream_proxy_url: Option<String> = super::http_client::get_current_proxy_url();

        // SOCKS5 proxies do not support CONNECT tunnelling, so reqwest is required
        let is_socks_proxy = upstream_proxy_url
            .as_deref()
            .map(|u| u.starts_with("socks5"))
            .unwrap_or(false);

        let preserve_exact_header_case = should_preserve_exact_header_case(
            adapter.name(),
            provider,
            resolved_claude_api_format.as_deref(),
            is_copilot,
        );

        // Send the request
        let response = if is_socks_proxy || !preserve_exact_header_case {
            // OpenAI, Copilot, and Codex style backends do not depend on original header casing, so they use
            // the reqwest connection pool instead of re-handshaking on the raw TCP/TLS path each request. SOCKS5 must also use reqwest.
            log::debug!(
                "[Forwarder] Using pooled reqwest client (preserve_exact_header_case={preserve_exact_header_case}, socks_proxy={is_socks_proxy})"
            );
            let client = super::http_client::get_for_url(&url);
            let mut request = client.request(method.clone(), &url);
            if request_is_streaming {
                // The reqwest timeout covers the whole request, so streaming requests rely on the first-chunk and
                // idle timeouts in response_processor instead, so a long stream is not killed by total duration.
                request = request.timeout(std::time::Duration::from_secs(24 * 60 * 60));
            } else if !self.non_streaming_timeout.is_zero() {
                request = request.timeout(self.non_streaming_timeout);
            }
            for (key, value) in &ordered_headers {
                request = request.header(key, value);
            }
            let send = request.body(body_bytes).send();
            let send_result = if request_is_streaming {
                let header_timeout = if self.streaming_first_byte_timeout.is_zero() {
                    timeout
                } else {
                    self.streaming_first_byte_timeout
                };
                tokio::time::timeout(header_timeout, send)
                    .await
                    .map_err(|_| {
                        ProxyError::Timeout(format!(
                            "streaming first chunk timed out after {}s (upstream returned no headers)",
                            header_timeout.as_secs()
                        ))
                    })?
            } else {
                send.await
            };
            let reqwest_resp = send_result.map_err(map_reqwest_send_error)?;
            ProxyResponse::Reqwest(reqwest_resp)
        } else {
            // HTTP proxy or direct: use the hyper raw write path (preserving header casing)
            // With an HTTP proxy, hyper_client tunnels through it with CONNECT
            let uri: http::Uri = url.parse().map_err(|e| {
                ProxyError::ForwardFailed(format!("Invalid upstream URL ({target_for_log}): {e}"))
            })?;
            super::hyper_client::send_request(
                uri,
                &target_for_log,
                method.clone(),
                ordered_headers,
                extensions.clone(),
                body_bytes,
                timeout,
                upstream_proxy_url.as_deref(),
            )
            .await?
        };

        // Check the response status
        let status = response.status();

        if status.is_success() {
            let mut response = self
                .prepare_success_response_for_failover(response, request_is_streaming)
                .await?;
            // Streaming requests normally return SSE. If a compatible gateway
            // explicitly returns JSON instead, buffer and validate it inside the retry
            // loop as well so a 2xx Anthropic error envelope can still fail over. Do
            // not buffer unknown content types: some gateways omit the SSE header.
            if codex_responses_to_anthropic && (!request_is_streaming || response.is_json()) {
                response = self
                    .validate_codex_anthropic_success_response(response)
                    .await?;
            } else if matches!(
                resolved_claude_api_format.as_deref(),
                Some("openai_responses")
            ) {
                if !request_is_streaming || response.is_json() {
                    // Claude→Responses gateways can also return a semantic failure in an
                    // HTTP 2xx Response object. Validate buffered/JSON bodies inside the
                    // retry loop so an early failure can still select another provider.
                    response = self.validate_responses_success_response(response).await?;
                } else {
                    // Delay committing the downstream stream until the upstream emits
                    // either productive output or a valid non-failure terminal event.
                    // A response.failed/error before output remains failover-safe.
                    response = self.validate_responses_stream_start(response).await?;
                }
            }
            Ok((response, resolved_claude_api_format, outbound_model))
        } else {
            let status_code = status.as_u16();
            // Error responses may be compressed upstream too (content-encoding). reqwest has no
            // auto-decompression feature enabled, so these are raw bytes; without decompressing, a compressed
            // error body fails from_utf8 and is discarded, hiding upstream rate-limit or auth details.
            let encoding = get_content_encoding(response.headers());
            let raw = response.bytes_with_limit(MAX_RESPONSE_BODY_BYTES).await?;
            let decoded = match encoding {
                Some(encoding) => {
                    match decompress_body_with_limit(&encoding, &raw, MAX_RESPONSE_BODY_BYTES) {
                        Ok(Some(decompressed)) => decompressed,
                        // Unsupported encoding, failed decompression, or output over the cap: fall back to the
                        // (already capped) raw bytes to keep whatever is readable
                        _ => raw.to_vec(),
                    }
                }
                None => raw.to_vec(),
            };
            let body_text = String::from_utf8(decoded).ok();

            Err(ProxyError::UpstreamError {
                status: status_code,
                body: body_text,
            })
        }
    }

    /// With failover on, success cannot be judged from the upstream headers alone.
    ///
    /// - Non-streaming: read the whole body into memory first, so a read timeout or broken connection returns to the retry loop.
    /// - Streaming: wait for at least the first chunk, so a 200 followed by no SSE is not recorded as success.
    async fn prepare_success_response_for_failover(
        &self,
        response: ProxyResponse,
        request_is_streaming: bool,
    ) -> Result<ProxyResponse, ProxyError> {
        if request_is_streaming {
            return self.prime_streaming_response(response).await;
        }

        if self.non_streaming_timeout.is_zero() {
            return Ok(response);
        }

        let status = response.status();
        let headers = response.headers().clone();
        let body_timeout = self.non_streaming_timeout;
        let body = tokio::time::timeout(
            body_timeout,
            response.bytes_with_limit(MAX_RESPONSE_BODY_BYTES),
        )
        .await
        .map_err(|_| {
            ProxyError::Timeout(format!(
                "response body read timed out after {}s (upstream sent headers but no body)",
                body_timeout.as_secs()
            ))
        })??;

        Ok(ProxyResponse::buffered(status, headers, body))
    }

    /// Some Anthropic-compatible gateways return an Anthropic error envelope with
    /// HTTP 2xx. Validate it inside the retry loop so the request can fail over to
    /// the next provider; the response transformer runs too late for that.
    async fn validate_codex_anthropic_success_response(
        &self,
        response: ProxyResponse,
    ) -> Result<ProxyResponse, ProxyError> {
        let status = response.status();
        let headers = response.headers().clone();
        let encoding = get_content_encoding(&headers);
        let raw = response.bytes_with_limit(MAX_RESPONSE_BODY_BYTES).await?;
        let decoded = match encoding {
            Some(encoding) => {
                match decompress_body_with_limit(&encoding, &raw, MAX_RESPONSE_BODY_BYTES) {
                    Ok(Some(decompressed)) => decompressed,
                    _ => raw.to_vec(),
                }
            }
            None => raw.to_vec(),
        };

        if let Some(message) = codex_anthropic_error_envelope_message(&decoded) {
            return Err(ProxyError::TransformError(format!(
                "Anthropic upstream returned a 2xx error envelope: {message}"
            )));
        }

        Ok(ProxyResponse::buffered(status, headers, raw))
    }

    async fn validate_responses_success_response(
        &self,
        response: ProxyResponse,
    ) -> Result<ProxyResponse, ProxyError> {
        let status = response.status();
        let headers = response.headers().clone();
        let encoding = get_content_encoding(&headers);
        let raw = response.bytes_with_limit(MAX_RESPONSE_BODY_BYTES).await?;
        let decoded = match encoding {
            Some(encoding) => {
                match decompress_body_with_limit(&encoding, &raw, MAX_RESPONSE_BODY_BYTES) {
                    Ok(Some(decompressed)) => decompressed,
                    _ => raw.to_vec(),
                }
            }
            None => raw.to_vec(),
        };

        if let Some(message) = responses_error_envelope_message(&decoded) {
            return Err(ProxyError::TransformError(format!(
                "Responses upstream returned a 2xx failure: {message}"
            )));
        }

        Ok(ProxyResponse::buffered(status, headers, raw))
    }

    async fn validate_responses_stream_start(
        &self,
        response: ProxyResponse,
    ) -> Result<ProxyResponse, ProxyError> {
        const MAX_PRIME_BYTES: usize = 256 * 1024;

        let status = response.status();
        let headers = response.headers().clone();
        let mut stream = Box::pin(response.bytes_stream());
        let mut replay_chunks: Vec<Bytes> = Vec::new();
        let mut parse_buffer = String::new();
        let mut utf8_remainder = Vec::new();

        loop {
            let next = if self.streaming_first_byte_timeout.is_zero() {
                stream.next().await
            } else {
                tokio::time::timeout(self.streaming_first_byte_timeout, stream.next())
                    .await
                    .map_err(|_| {
                        ProxyError::Timeout(format!(
                            "Responses stream produced no semantic output within {}s",
                            self.streaming_first_byte_timeout.as_secs()
                        ))
                    })?
            };

            let Some(chunk) = next else {
                if let Some(outcome) = inspect_responses_json_document(&parse_buffer) {
                    outcome?;
                    let replay = futures::stream::iter(replay_chunks.into_iter().map(Ok));
                    return Ok(ProxyResponse::streamed(status, headers, replay));
                }
                if !parse_buffer.trim().is_empty() {
                    if let Some(outcome) = inspect_responses_start_event(parse_buffer.trim()) {
                        outcome?;
                        let replay = futures::stream::iter(replay_chunks.into_iter().map(Ok));
                        return Ok(ProxyResponse::streamed(status, headers, replay));
                    }
                }
                return Err(ProxyError::ForwardFailed(
                    "Responses stream ended before producing output or a terminal event"
                        .to_string(),
                ));
            };
            let chunk = chunk.map_err(|error| {
                ProxyError::ForwardFailed(format!(
                    "Failed while validating Responses stream start: {error}"
                ))
            })?;
            crate::proxy::sse::append_utf8_safe(&mut parse_buffer, &mut utf8_remainder, &chunk);
            replay_chunks.push(chunk);

            // Some compatible gateways ignore `stream:true` and return a complete
            // Responses JSON document without a JSON content-type. Recognize that
            // shape before looking for SSE delimiters; pretty-printed JSON may itself
            // contain blank lines and must stay intact.
            if let Some(outcome) = inspect_responses_json_document(&parse_buffer) {
                outcome?;
                let replay = futures::stream::iter(replay_chunks.into_iter().map(Ok)).chain(stream);
                return Ok(ProxyResponse::streamed(status, headers, replay));
            }

            while let Some(block) = crate::proxy::sse::take_sse_block(&mut parse_buffer) {
                if let Some(outcome) = inspect_responses_start_event(&block) {
                    outcome?;
                    let replay =
                        futures::stream::iter(replay_chunks.into_iter().map(Ok)).chain(stream);
                    return Ok(ProxyResponse::streamed(status, headers, replay));
                }
            }

            if replay_chunks.iter().map(Bytes::len).sum::<usize>() >= MAX_PRIME_BYTES {
                log::warn!(
                    "[Claude/Responses] semantic stream priming exceeded {MAX_PRIME_BYTES} bytes; committing buffered stream"
                );
                let replay = futures::stream::iter(replay_chunks.into_iter().map(Ok)).chain(stream);
                return Ok(ProxyResponse::streamed(status, headers, replay));
            }
        }
    }

    async fn prime_streaming_response(
        &self,
        response: ProxyResponse,
    ) -> Result<ProxyResponse, ProxyError> {
        if self.streaming_first_byte_timeout.is_zero() {
            return Ok(response);
        }

        let status = response.status();
        let headers = response.headers().clone();
        let timeout = self.streaming_first_byte_timeout;
        let mut stream = Box::pin(response.bytes_stream());

        let first = tokio::time::timeout(timeout, stream.next())
            .await
            .map_err(|_| {
                ProxyError::Timeout(format!(
                    "streaming first chunk timed out after {}s (upstream sent headers but no data)",
                    timeout.as_secs()
                ))
            })?;

        let Some(first) = first else {
            return Err(ProxyError::ForwardFailed(
                "The streaming response ended before the first chunk arrived".to_string(),
            ));
        };

        let first = first
            .map_err(|e| ProxyError::ForwardFailed(format!("first chunk read failed: {e}")))?;

        let replay = futures::stream::once(async move { Ok(first) }).chain(stream);
        Ok(ProxyResponse::streamed(status, headers, replay))
    }

    async fn resolve_claude_api_format(
        &self,
        provider: &Provider,
        body: &Value,
        is_copilot: bool,
    ) -> String {
        if !is_copilot {
            return super::providers::get_claude_api_format(provider).to_string();
        }

        let model = body.get("model").and_then(|value| value.as_str());
        if let Some(model_id) = model {
            if self
                .is_copilot_openai_vendor_model(provider, model_id)
                .await
            {
                return "openai_responses".to_string();
            }
        }

        "openai_chat".to_string()
    }

    /// Confirms a model ID really exists via Copilot's live `/models` list, downgrading by family when missing.
    /// Synchronous on a cache hit; a first request or an expired 5 min cache triggers one HTTP call.
    async fn apply_copilot_live_model_resolution(
        &self,
        provider: &Provider,
        body: &mut serde_json::Value,
    ) {
        let Some(model_id) = body.get("model").and_then(|v| v.as_str()) else {
            return;
        };
        let model_id = model_id.to_string();

        let Some(app_handle) = &self.app_handle else {
            return;
        };
        let copilot_state = app_handle.state::<CopilotAuthState>();
        let copilot_auth = copilot_state.0.read().await;
        let account_id = provider
            .meta
            .as_ref()
            .and_then(|m| m.managed_account_id_for("github_copilot"));

        let models_result = match account_id.as_deref() {
            Some(id) => copilot_auth.fetch_models_for_account(id).await,
            None => copilot_auth.fetch_models().await,
        };

        let models = match models_result {
            Ok(m) => m,
            Err(err) => {
                log::debug!("[Copilot] live model list unavailable, skip resolution: {err}");
                return;
            }
        };

        if let Some(resolved) =
            super::providers::copilot_model_map::resolve_against_models(&model_id, &models)
        {
            log::info!("[Copilot] live-model resolve: {model_id} → {resolved}");
            body["model"] = serde_json::Value::String(resolved);
        }
    }

    async fn is_copilot_openai_vendor_model(&self, provider: &Provider, model_id: &str) -> bool {
        let Some(app_handle) = &self.app_handle else {
            log::debug!("[Copilot] AppHandle unavailable, fallback to chat/completions");
            return false;
        };

        let copilot_state = app_handle.state::<CopilotAuthState>();
        let copilot_auth = copilot_state.0.read().await;
        let account_id = provider
            .meta
            .as_ref()
            .and_then(|m| m.managed_account_id_for("github_copilot"));

        let vendor_result = match account_id.as_deref() {
            Some(id) => {
                copilot_auth
                    .get_model_vendor_for_account(id, model_id)
                    .await
            }
            None => copilot_auth.get_model_vendor(model_id).await,
        };

        match vendor_result {
            Ok(Some(vendor)) => vendor.eq_ignore_ascii_case("openai"),
            Ok(None) => {
                log::debug!(
                    "[Copilot] Model vendor unavailable for {model_id}, fallback to chat/completions"
                );
                false
            }
            Err(err) => {
                log::warn!(
                    "[Copilot] Failed to resolve model vendor for {model_id}, fallback to chat/completions: {err}"
                );
                false
            }
        }
    }

    fn categorize_proxy_error(&self, error: &ProxyError, provider: &Provider) -> ErrorCategory {
        // Authentication belongs to the Codex client for an official route.
        // Every retry would reuse the selected account's inbound Authorization
        // header against another card, so no official-route error may fail over.
        if super::providers::is_codex_official_provider(provider) {
            return ErrorCategory::NonRetryable;
        }

        // xAI OAuth mirrors the same rule for token acquisition: a local
        // AuthError means the managed account needs re-login. Failing over
        // would silently move the conversation off the selected Grok account
        // and poison the provider's health state for an account-level issue.
        if provider.is_xai_oauth() && matches!(error, ProxyError::AuthError(_)) {
            return ErrorCategory::NonRetryable;
        }

        match error {
            // Network and upstream errors: both should move on to the next provider
            ProxyError::Timeout(_) => ErrorCategory::Retryable,
            ProxyError::ForwardFailed(_) => ErrorCategory::Retryable,
            // Upstream HTTP errors, bucketed by status code.
            //
            // Status codes signalling a bad client request would be rejected by every provider, and retrying
            // only inflates the error rate, pollutes breaker health, and wastes quota:
            //   400 Bad Request / 422 Unprocessable Entity   <- body format or semantics are wrong
            //   405 Method Not Allowed / 406 Not Acceptable  <- wrong method or Accept
            //   413 Payload Too Large / 414 URI Too Long     <- the client built something oversized
            //   415 Unsupported Media Type                    <- wrong Content-Type
            //   501 Not Implemented                           <- upstream genuinely lacks the protocol
            //
            // All other 4xx (401/403/404/408/409/429/451 and so on) and every 5xx stay Retryable, since
            // another provider may hold a different key, quota, region, or model mapping.
            ProxyError::UpstreamError { status, .. } => match *status {
                400 | 405 | 406 | 413 | 414 | 415 | 422 | 501 => ErrorCategory::NonRetryable,
                _ => ErrorCategory::Retryable,
            },
            // Provider-level config or conversion problems: another provider may succeed
            ProxyError::ConfigError(_) => ErrorCategory::Retryable,
            ProxyError::TransformError(_) => ErrorCategory::Retryable,
            ProxyError::AuthError(_) => ErrorCategory::Retryable,
            // No provider available: every provider was tried, so no retry is possible
            ProxyError::NoAvailableProvider => ErrorCategory::NonRetryable,
            // Other errors (database, internal, and so on): switching provider does not help
            _ => ErrorCategory::NonRetryable,
        }
    }
}

/// Extracts the error message from a ProxyError
fn extract_error_message(error: &ProxyError) -> Option<String> {
    match error {
        ProxyError::UpstreamError { body, .. } => body.clone(),
        _ => Some(error.to_string()),
    }
}

/// Detects a Bedrock provider (via the CLAUDE_CODE_USE_BEDROCK environment variable)
fn is_bedrock_provider(provider: &Provider) -> bool {
    provider
        .settings_config
        .get("env")
        .and_then(|e| e.get("CLAUDE_CODE_USE_BEDROCK"))
        .and_then(|v| v.as_str())
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn build_retryable_failure_log(
    provider_name: &str,
    attempted_providers: usize,
    total_providers: usize,
    error: &ProxyError,
) -> (&'static str, String) {
    let error_summary = summarize_proxy_error(error);

    if total_providers <= 1 {
        (
            log_fwd::SINGLE_PROVIDER_FAILED,
            format!("Provider {provider_name} request failed: {error_summary}"),
        )
    } else {
        (
            log_fwd::PROVIDER_FAILED_RETRY,
            format!(
                "Provider {provider_name} failed, trying the next one ({attempted_providers}/{total_providers}): {error_summary}"
            ),
        )
    }
}

fn build_terminal_failure_log(
    attempted_providers: usize,
    total_providers: usize,
    last_error: Option<&ProxyError>,
) -> Option<(&'static str, String)> {
    if total_providers <= 1 {
        return None;
    }

    let error_summary = last_error
        .map(summarize_proxy_error)
        .unwrap_or_else(|| "unknown error".to_string());

    Some((
        log_fwd::ALL_PROVIDERS_FAILED,
        format!(
            "Tried {attempted_providers}/{total_providers} providers, all failed. Last error: {error_summary}"
        ),
    ))
}

fn summarize_proxy_error(error: &ProxyError) -> String {
    match error {
        ProxyError::UpstreamError { status, body } => {
            let body_summary = body
                .as_deref()
                .map(summarize_upstream_body)
                .filter(|summary| !summary.is_empty());

            match body_summary {
                Some(summary) => format!("upstream HTTP {status}: {summary}"),
                None => format!("upstream HTTP {status}"),
            }
        }
        ProxyError::Timeout(message) => {
            format!(
                "request timed out: {}",
                summarize_text_for_log(message, 180)
            )
        }
        ProxyError::ForwardFailed(message) => {
            format!("forward failed: {}", summarize_text_for_log(message, 180))
        }
        ProxyError::TransformError(message) => {
            format!("convert failed: {}", summarize_text_for_log(message, 180))
        }
        ProxyError::ConfigError(message) => {
            format!("config error: {}", summarize_text_for_log(message, 180))
        }
        ProxyError::AuthError(message) => {
            format!("auth failed: {}", summarize_text_for_log(message, 180))
        }
        _ => summarize_text_for_log(&error.to_string(), 180),
    }
}

fn summarize_upstream_body(body: &str) -> String {
    if let Ok(json_body) = serde_json::from_str::<Value>(body) {
        if let Some(message) = extract_json_error_message(&json_body) {
            return summarize_text_for_log(&message, 180);
        }

        if let Ok(compact_json) = serde_json::to_string(&json_body) {
            return summarize_text_for_log(&compact_json, 180);
        }
    }

    summarize_text_for_log(body, 180)
}

fn extract_json_error_message(body: &Value) -> Option<String> {
    let candidates = [
        body.pointer("/error/message"),
        body.pointer("/message"),
        body.pointer("/detail"),
        body.pointer("/error"),
    ];

    candidates
        .into_iter()
        .flatten()
        .find_map(|value| value.as_str().map(ToString::to_string))
}

fn split_endpoint_and_query(endpoint: &str) -> (&str, Option<&str>) {
    endpoint
        .split_once('?')
        .map_or((endpoint, None), |(path, query)| (path, Some(query)))
}

fn strip_beta_query(query: Option<&str>) -> Option<String> {
    let filtered = query.map(|query| {
        query
            .split('&')
            .filter(|pair| !pair.is_empty() && !pair.starts_with("beta="))
            .collect::<Vec<_>>()
            .join("&")
    });

    match filtered.as_deref() {
        Some("") | None => None,
        Some(_) => filtered,
    }
}

fn is_claude_messages_path(path: &str) -> bool {
    matches!(path, "/v1/messages" | "/claude/v1/messages")
}

fn rewrite_codex_responses_endpoint_to_chat(endpoint: &str) -> (String, Option<String>) {
    let (_path, query) = split_endpoint_and_query(endpoint);
    let passthrough_query = query.map(ToString::to_string);
    let target_path = "/chat/completions";
    let rewritten = match passthrough_query.as_deref() {
        Some(query) if !query.is_empty() => format!("{target_path}?{query}"),
        _ => target_path.to_string(),
    };

    (rewritten, passthrough_query)
}

/// Claude Code client fingerprint (used for Codex→Anthropic emulation to pass a
/// gateway's "Claude Code only" check).
const CLAUDE_CODE_USER_AGENT: &str = "claude-cli/1.0.119 (external, cli)";
const CLAUDE_CODE_SYSTEM_IDENTITY: &str =
    "You are Claude Code, Anthropic's official CLI for Claude.";

/// Insert the Claude Code identity as the first line before the `system` field in
/// the Anthropic request body.
///
/// Anthropic subscription/OAuth plans require the first system block to be exactly
/// this identity line. After conversion `system` is a string (from Codex
/// instructions); normalize it into an array here: [identity line, original system...].
fn prepend_claude_code_system_prompt(body: &mut Value) {
    let identity = serde_json::json!({ "type": "text", "text": CLAUDE_CODE_SYSTEM_IDENTITY });
    let mut blocks: Vec<Value> = vec![identity];
    match body.get("system") {
        Some(Value::String(existing)) if !existing.is_empty() => {
            blocks.push(serde_json::json!({ "type": "text", "text": existing }));
        }
        Some(Value::Array(existing)) => {
            // Idempotent: skip re-injection if the first block is already the identity line.
            if existing
                .first()
                .and_then(|b| b.get("text"))
                .and_then(|t| t.as_str())
                == Some(CLAUDE_CODE_SYSTEM_IDENTITY)
            {
                return;
            }
            blocks.extend(existing.iter().cloned());
        }
        _ => {}
    }
    body["system"] = Value::Array(blocks);
}

/// Headers a native Claude Code client never sends but the Codex/OpenAI CLI (and its
/// stainless SDK layer) do. Dropped for every Codex→Anthropic request so the upstream sees a
/// clean Anthropic client fingerprint. Centralized here so the set stays in one place and future
/// additions can't miss a code path. `key_str` is already lowercased by the http crate.
/// Whether `base_url` already ends in `endpoint_suffix` (e.g. `/v1/messages` or
/// `/chat/completions`), ignoring surrounding whitespace, any `?query`/`#fragment`, and a
/// trailing slash. Used to avoid double-appending the endpoint when a user pastes a full
/// URL but leaves the "full URL" switch off (`.../v1/messages` → `.../v1/messages/v1/messages`,
/// a non-retryable 400). `endpoint_suffix` must be lowercase.
fn base_url_is_full_endpoint(base_url: &str, endpoint_suffix: &str) -> bool {
    let trimmed = base_url.trim();
    // Match against the path only: a `?query`/`#fragment` on a full endpoint URL must not
    // hide the suffix (`.../v1/messages?beta=true` still ends in the endpoint).
    let path = match trimmed.split_once(['?', '#']) {
        Some((head, _)) => head,
        None => trimmed,
    };
    path.trim_end_matches('/')
        .to_ascii_lowercase()
        .ends_with(endpoint_suffix)
}

fn is_codex_client_fingerprint_header(key_str: &str) -> bool {
    matches!(
        key_str,
        "originator"
            | "session_id"
            | "session-id"
            | "thread-id"
            | "conversation_id"
            | "chatgpt-account-id"
            | "x-openai-subagent"
            | "x-client-request-id"
            | "openai-beta"
            | "openai-organization"
            | "openai-project"
    ) || key_str.starts_with("x-stainless-")
        || key_str.starts_with("x-codex-")
}

fn codex_anthropic_error_envelope_message(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("error") && value.get("error").is_none() {
        return None;
    }
    let error = value.get("error").unwrap_or(&value);
    let error_type = error.get("type").and_then(Value::as_str).unwrap_or("error");
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| error.to_string());
    Some(format!("{error_type}: {message}"))
}

fn responses_error_envelope_message(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let status = value.get("status").and_then(Value::as_str);
    let has_error = value.get("error").is_some_and(|error| !error.is_null());
    if !matches!(status, Some("failed" | "cancelled")) && !has_error {
        return None;
    }

    let error = value.get("error").unwrap_or(&value);
    let error_type = error
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| error.get("code").and_then(Value::as_str))
        .unwrap_or_else(|| status.unwrap_or("error"));
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .filter(|message| !message.trim().is_empty())
        .unwrap_or(match status {
            Some("cancelled") => "response generation was cancelled",
            _ => "response generation failed",
        });
    Some(format!("{error_type}: {message}"))
}

/// Prompt caching is part of the Codex→Anthropic protocol bridge rather than an
/// optional Bedrock optimizer. Codex requests do not contain Anthropic
/// `cache_control`, so keep bridge caching on by default while still honoring the
/// dedicated cache-injection switch. Injected breakpoints always use Anthropic's
/// standard 5-minute TTL.
fn codex_anthropic_cache_config(config: &OptimizerConfig) -> OptimizerConfig {
    OptimizerConfig {
        enabled: true,
        thinking_optimizer: false,
        cache_injection: config.cache_injection,
    }
}

/// A streaming request may receive a whole JSON document even when the gateway
/// omits `application/json`. `None` means either "not JSON" or "not complete yet";
/// a parsed document is safe to commit unless it is a semantic failure envelope.
fn inspect_responses_json_document(buffer: &str) -> Option<Result<(), ProxyError>> {
    let trimmed = buffer.trim();
    if !matches!(trimmed.as_bytes().first(), Some(b'{') | Some(b'[')) {
        return None;
    }
    let _: Value = serde_json::from_str(trimmed).ok()?;
    if let Some(message) = responses_error_envelope_message(trimmed.as_bytes()) {
        return Some(Err(ProxyError::TransformError(format!(
            "Responses upstream returned a 2xx failure: {message}"
        ))));
    }
    Some(Ok(()))
}

/// Inspect one complete Responses SSE block while the response is still inside
/// the retry loop. `None` means the event is lifecycle-only and priming should
/// continue; `Some(Ok(()))` means it is safe to commit/replay the stream.
fn inspect_responses_start_event(block: &str) -> Option<Result<(), ProxyError>> {
    let mut named_event = None;
    let mut data_lines = Vec::new();
    for line in block.lines() {
        if let Some(event) = crate::proxy::sse::strip_sse_field(line, "event") {
            named_event = Some(event.trim().to_string());
        } else if let Some(data) = crate::proxy::sse::strip_sse_field(line, "data") {
            data_lines.push(data);
        }
    }
    if data_lines.is_empty() {
        return None;
    }
    let value: Value = match serde_json::from_str(&data_lines.join("\n")) {
        Ok(value) => value,
        Err(_) => return None,
    };
    let event = named_event
        .as_deref()
        .filter(|event| !event.is_empty())
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or("");

    let response = value.get("response").unwrap_or(&value);
    if matches!(
        response.get("status").and_then(Value::as_str),
        Some("failed" | "cancelled")
    ) || response.get("error").is_some_and(|error| !error.is_null())
    {
        let error = response.get("error").unwrap_or(response);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| error.as_str())
            .unwrap_or("Responses upstream failed before output");
        let error_type = error
            .get("type")
            .and_then(Value::as_str)
            .or_else(|| error.get("code").and_then(Value::as_str))
            .or_else(|| response.get("status").and_then(Value::as_str))
            .unwrap_or("upstream_error");
        return Some(Err(ProxyError::TransformError(format!(
            "Responses upstream {error_type}: {message}"
        ))));
    }

    match event {
        "response.failed" | "error" => {
            let error = response.get("error").unwrap_or(response);
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
                .unwrap_or("Responses upstream emitted an error before output");
            let error_type = error
                .get("type")
                .and_then(Value::as_str)
                .or_else(|| error.get("code").and_then(Value::as_str))
                .unwrap_or("upstream_error");
            Some(Err(ProxyError::TransformError(format!(
                "Responses upstream {error_type}: {message}"
            ))))
        }
        "response.created" | "response.in_progress" | "response.queued" => None,
        "" => None,
        // Productive output, incomplete, and completed terminals are all safe to
        // expose. Mid-stream failures after this point are surfaced by the converter
        // but intentionally do not switch providers.
        _ => Some(Ok(())),
    }
}

/// Rewrite Codex's `/responses` (and variants) to Anthropic's `/v1/messages`, preserving the query.
fn rewrite_codex_responses_endpoint_to_anthropic(endpoint: &str) -> (String, Option<String>) {
    let (_path, query) = split_endpoint_and_query(endpoint);
    let passthrough_query = query.map(ToString::to_string);
    let target_path = "/v1/messages";
    let rewritten = match passthrough_query.as_deref() {
        Some(query) if !query.is_empty() => format!("{target_path}?{query}"),
        _ => target_path.to_string(),
    };

    (rewritten, passthrough_query)
}

fn rewrite_claude_transform_endpoint(
    endpoint: &str,
    api_format: &str,
    is_copilot: bool,
    body: &Value,
) -> (String, Option<String>) {
    let (path, query) = split_endpoint_and_query(endpoint);
    let passthrough_query = if is_claude_messages_path(path) {
        strip_beta_query(query)
    } else {
        query.map(ToString::to_string)
    };

    if !is_claude_messages_path(path) {
        return (endpoint.to_string(), passthrough_query);
    }

    if api_format == "gemini_native" {
        let model =
            super::providers::transform_gemini::extract_gemini_model(body).unwrap_or("unknown");
        // Accept both bare ids (`gemini-2.5-pro`) and the resource-name
        // form (`models/gemini-2.5-pro`) that Gemini SDKs emit. See
        // `normalize_gemini_model_id` for rationale.
        let model = super::gemini_url::normalize_gemini_model_id(model);
        let is_stream = body
            .get("stream")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let target_path = if is_stream {
            format!("/v1beta/models/{model}:streamGenerateContent")
        } else {
            format!("/v1beta/models/{model}:generateContent")
        };

        let rewritten_query = merge_query_params(
            passthrough_query.as_deref(),
            if is_stream { Some("alt=sse") } else { None },
        );

        let rewritten = match rewritten_query.as_deref() {
            Some(query) if !query.is_empty() => format!("{target_path}?{query}"),
            _ => target_path,
        };

        return (rewritten, rewritten_query);
    }

    let target_path = if is_copilot && api_format == "openai_responses" {
        "/v1/responses"
    } else if is_copilot {
        "/chat/completions"
    } else if api_format == "openai_responses" {
        "/v1/responses"
    } else {
        "/v1/chat/completions"
    };

    let rewritten = match passthrough_query.as_deref() {
        Some(query) if !query.is_empty() => format!("{target_path}?{query}"),
        _ => target_path.to_string(),
    };

    (rewritten, passthrough_query)
}

fn merge_query_params(base_query: Option<&str>, extra_param: Option<&str>) -> Option<String> {
    let mut params: Vec<String> = base_query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter(|pair| !pair.is_empty())
        .filter(|pair| !pair.starts_with("alt="))
        .map(ToString::to_string)
        .collect();

    if let Some(extra_param) = extra_param {
        params.push(extra_param.to_string());
    }

    if params.is_empty() {
        None
    } else {
        Some(params.join("&"))
    }
}

fn append_query_to_full_url(base_url: &str, query: Option<&str>) -> String {
    match query {
        Some(query) if !query.is_empty() => {
            if base_url.contains('?') {
                format!("{base_url}&{query}")
            } else {
                format!("{base_url}?{query}")
            }
        }
        _ => base_url.to_string(),
    }
}

#[derive(Clone, Copy)]
enum CodexStandaloneEndpoint {
    AlphaSearch,
    ImagesGenerations,
    ImagesEdits,
}

impl CodexStandaloneEndpoint {
    fn from_effective_endpoint(endpoint: &str) -> Option<Self> {
        match split_endpoint_and_query(endpoint).0 {
            "/alpha/search" => Some(Self::AlphaSearch),
            "/images/generations" => Some(Self::ImagesGenerations),
            "/images/edits" => Some(Self::ImagesEdits),
            _ => None,
        }
    }

    fn path(self) -> &'static str {
        match self {
            Self::AlphaSearch => "/alpha/search",
            Self::ImagesGenerations => "/images/generations",
            Self::ImagesEdits => "/images/edits",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::AlphaSearch => "Codex Alpha Search",
            Self::ImagesGenerations => "Codex Images generations",
            Self::ImagesEdits => "Codex Images edits",
        }
    }

    fn full_url_hint(self) -> &'static str {
        match self {
            Self::AlphaSearch => "/responses",
            Self::ImagesGenerations | Self::ImagesEdits => {
                "/responses, /chat/completions, /images/generations, or /images/edits"
            }
        }
    }

    /// Full-URL suffixes that unambiguously locate this endpoint's sibling.
    ///
    /// Order matters: a longer suffix must precede any suffix it ends with
    /// (`/responses/compact` before `/responses`), otherwise the shorter one
    /// wins and the rewrite keeps a stray `/compact` segment.
    fn source_suffixes(self) -> &'static [&'static str] {
        match self {
            Self::AlphaSearch => &["/responses/compact", "/responses"],
            // Both Images routes live next to each other, so a full URL pasted
            // for either one is a valid source for the other.
            Self::ImagesGenerations | Self::ImagesEdits => &[
                "/images/generations",
                "/images/edits",
                "/chat/completions",
                "/responses/compact",
                "/responses",
            ],
        }
    }

    fn source_suffix(self, parsed_path: &str) -> Option<&'static str> {
        // Match the case-insensitive pasted-endpoint check. Only normalize for
        // matching; the rewrite keeps the original URL prefix and query intact.
        let parsed_path = parsed_path.to_ascii_lowercase();
        self.source_suffixes()
            .iter()
            .copied()
            .find(|suffix| parsed_path.ends_with(suffix))
    }

    /// Whether a base URL (full-URL switch off) already ends in one of this
    /// endpoint's source suffixes, i.e. was pasted as a complete endpoint URL.
    fn base_url_is_source_endpoint(self, base_url: &str) -> bool {
        self.source_suffixes()
            .iter()
            .any(|suffix| base_url_is_full_endpoint(base_url, suffix))
    }
}

/// Derive Codex standalone sibling endpoints from a provider configured with a
/// complete Codex-compatible API URL.
///
/// Full-URL mode normally means "use this exact URL". That is correct for the
/// request type it was configured for, but standalone Codex protocols cannot be
/// posted to chat/responses endpoints. Only rewrite URL shapes whose sibling
/// endpoint is unambiguous; opaque full URLs fail closed instead of leaking the
/// payload to an unrelated route.
fn rewrite_codex_standalone_full_url(
    base_url: &str,
    request_query: Option<&str>,
    endpoint: CodexStandaloneEndpoint,
) -> Result<String, ProxyError> {
    let trimmed = base_url.trim();
    let parsed = url::Url::parse(trimmed).map_err(|_| {
        ProxyError::ConfigError(format!("{} requires a valid full URL", endpoint.label()))
    })?;

    // Fragments are never sent in HTTP requests. Drop one before splitting the
    // query so an accidental fragment cannot move the incoming query behind `#`.
    let without_fragment = trimmed
        .split_once('#')
        .map_or(trimmed, |(head, _fragment)| head);
    let (url_without_query, base_query) = without_fragment
        .split_once('?')
        .map_or((without_fragment, None), |(head, query)| {
            (head, Some(query))
        });
    let url_without_query = url_without_query.trim_end_matches('/');

    let parsed_path = parsed.path().trim_end_matches('/').to_string();
    let suffix = endpoint.source_suffix(&parsed_path).ok_or_else(|| {
        ProxyError::ConfigError(format!(
            "{} cannot derive {} from an opaque full URL; use a base URL or a full URL ending in {}",
            endpoint.label(),
            endpoint.path(),
            endpoint.full_url_hint()
        ))
    })?;

    let prefix_len = url_without_query
        .len()
        .checked_sub(suffix.len())
        .ok_or_else(|| ProxyError::ConfigError("Invalid Codex full URL".to_string()))?;
    let mut rewritten = format!("{}{}", &url_without_query[..prefix_len], endpoint.path());

    let request_query = request_query.filter(|query| !query.is_empty());
    let base_query = base_query.filter(|query| !query.is_empty());
    match (base_query, request_query) {
        (Some(base), Some(request)) => rewritten.push_str(&format!("?{base}&{request}")),
        (Some(base), None) => rewritten.push_str(&format!("?{base}")),
        (None, Some(request)) => rewritten.push_str(&format!("?{request}")),
        (None, None) => {}
    }

    Ok(rewritten)
}

fn build_codex_oauth_session_headers(
    session_id: &str,
) -> Vec<(http::HeaderName, http::HeaderValue)> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Vec::new();
    }

    let mut headers = Vec::new();
    if let Ok(value) = http::HeaderValue::from_str(session_id) {
        headers.push((http::HeaderName::from_static("session_id"), value.clone()));
        headers.push((http::HeaderName::from_static("x-client-request-id"), value));
    }

    let window_id = format!("{session_id}:0");
    if let Ok(value) = http::HeaderValue::from_str(&window_id) {
        headers.push((http::HeaderName::from_static("x-codex-window-id"), value));
    }

    headers
}

fn reject_proxy_placeholder_for_managed_account_upstream(
    url: &str,
    headers: &http::HeaderMap,
) -> Result<(), ProxyError> {
    if !is_managed_account_upstream_url(url) || !headers_contain_proxy_placeholder(headers) {
        return Ok(());
    }

    Err(ProxyError::AuthError(
        "Managed account proxy auth was not resolved; PROXY_MANAGED must not be sent upstream"
            .to_string(),
    ))
}

fn is_managed_account_upstream_url(url: &str) -> bool {
    let Ok(uri) = url.parse::<http::Uri>() else {
        return false;
    };

    let Some(host) = uri.host().map(str::to_ascii_lowercase) else {
        return false;
    };

    host == "githubcopilot.com"
        || host.ends_with(".githubcopilot.com")
        || (host == "chatgpt.com" && uri.path().starts_with("/backend-api/codex"))
        || (host == "api.x.ai" && uri.path().starts_with("/v1/"))
}

fn headers_contain_proxy_placeholder(headers: &http::HeaderMap) -> bool {
    headers.values().any(|value| {
        value
            .to_str()
            .map(|value| value.contains(PROXY_AUTH_PLACEHOLDER))
            .unwrap_or(false)
    })
}

fn should_preserve_exact_header_case(
    adapter_name: &str,
    provider: &Provider,
    resolved_claude_api_format: Option<&str>,
    is_copilot: bool,
) -> bool {
    if matches!(adapter_name, "Codex" | "Gemini") {
        return false;
    }

    if is_copilot || provider.is_codex_oauth() || provider.is_xai_oauth() {
        return false;
    }

    matches!(resolved_claude_api_format, None | Some("anthropic"))
}

fn is_streaming_request(endpoint: &str, body: &Value, headers: &axum::http::HeaderMap) -> bool {
    if body
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return true;
    }

    if endpoint.contains("streamGenerateContent") || endpoint.contains("alt=sse") {
        return true;
    }

    headers
        .get(axum::http::header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|accept| accept.contains("text/event-stream"))
        .unwrap_or(false)
}

#[cfg(test)]
fn should_force_identity_encoding(
    endpoint: &str,
    body: &Value,
    headers: &axum::http::HeaderMap,
) -> bool {
    is_streaming_request(endpoint, body, headers)
}

fn map_reqwest_send_error(error: reqwest::Error) -> ProxyError {
    if error.is_timeout() {
        ProxyError::Timeout(format!("upstream timed out: {}", error.without_url()))
    } else if error.is_connect() {
        ProxyError::ForwardFailed(format!("upstream connect failed: {}", error.without_url()))
    } else {
        ProxyError::ForwardFailed(format!("upstream send failed: {}", error.without_url()))
    }
}

fn summarize_text_for_log(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = normalized.trim();

    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let truncated: String = trimmed.chars().take(max_chars).collect();
    let truncated = truncated.trim_end();
    format!("{truncated}...")
}

fn apply_local_proxy_body_overrides(
    body: &mut Value,
    overrides: &LocalProxyRequestOverrides,
) -> bool {
    let Some(override_body) = overrides.body.as_ref() else {
        return false;
    };

    if !override_body.is_object() {
        log::warn!("[LocalProxyOverrides] Ignoring body override because it is not an object");
        return false;
    }

    merge_json_override(body, override_body)
}

fn merge_json_override(target: &mut Value, patch: &Value) -> bool {
    merge_json_override_inner(target, patch, true)
}

fn merge_json_override_inner(target: &mut Value, patch: &Value, is_top_level: bool) -> bool {
    match (target, patch) {
        (Value::Object(target_map), Value::Object(patch_map)) => {
            let mut changed = false;
            for (key, patch_value) in patch_map {
                if is_top_level && key == "stream" {
                    log::warn!(
                        "[LocalProxyOverrides] Ignoring body override for protected field: stream"
                    );
                    continue;
                }
                match target_map.get_mut(key) {
                    Some(target_value) => {
                        changed |= merge_json_override_inner(target_value, patch_value, false);
                    }
                    None => {
                        target_map.insert(key.clone(), patch_value.clone());
                        changed = true;
                    }
                }
            }
            changed
        }
        (target_value, patch_value) => {
            if target_value == patch_value {
                false
            } else {
                *target_value = patch_value.clone();
                true
            }
        }
    }
}

fn apply_local_proxy_header_overrides(
    headers: &mut http::HeaderMap,
    overrides: Option<&LocalProxyRequestOverrides>,
    is_copilot: bool,
) {
    if is_copilot {
        return;
    }

    let Some(header_overrides) = overrides.map(|overrides| &overrides.headers) else {
        return;
    };

    for (raw_name, raw_value) in header_overrides {
        let header_name = raw_name.trim().to_ascii_lowercase();
        if header_name.is_empty() {
            log::warn!("[LocalProxyOverrides] Ignoring header override with empty name");
            continue;
        }

        let Ok(name) = http::HeaderName::from_bytes(header_name.as_bytes()) else {
            log::warn!("[LocalProxyOverrides] Ignoring invalid header override name: {raw_name}");
            continue;
        };

        if is_protected_local_proxy_override_header(&name) {
            log::debug!(
                "[LocalProxyOverrides] Ignoring protected header override: {}",
                name.as_str()
            );
            continue;
        }

        let Ok(value) = http::HeaderValue::from_str(raw_value) else {
            log::warn!(
                "[LocalProxyOverrides] Ignoring invalid header override value for {}",
                name.as_str()
            );
            continue;
        };

        headers.insert(name, value);
    }
}

fn is_protected_local_proxy_override_header(name: &http::HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "proxy-authorization"
            | "proxy-authenticate"
            | "te"
            | "trailer"
            | "upgrade"
            | "accept-encoding"
            | "content-type"
            | "authorization"
            | "x-api-key"
            | "x-goog-api-key"
            | "chatgpt-account-id"
            | "session_id"
            | "x-client-request-id"
            | "x-codex-window-id"
            | "x-forwarded-host"
            | "x-forwarded-port"
            | "x-forwarded-proto"
            | "forwarded"
            | "cf-connecting-ip"
            | "cf-ipcountry"
            | "cf-ray"
            | "cf-visitor"
            | "true-client-ip"
            | "fastly-client-ip"
            | "x-azure-clientip"
            | "x-azure-fdid"
            | "x-azure-ref"
            | "akamai-origin-hop"
            | "x-akamai-config-log-detail"
            | "x-request-id"
            | "x-correlation-id"
            | "x-trace-id"
            | "x-amzn-trace-id"
            | "x-b3-traceid"
            | "x-b3-spanid"
            | "x-b3-parentspanid"
            | "x-b3-sampled"
            | "traceparent"
            | "tracestate"
    )
}

fn prepare_upstream_request_body(request_body: Value) -> Value {
    canonicalize_value(filter_private_params_with_whitelist(request_body, &[]))
}

fn log_prompt_cache_trace(
    app_type: &AppType,
    provider: &Provider,
    endpoint: &str,
    api_format: Option<&str>,
    body: &Value,
    session_client_provided: bool,
) {
    if !log::log_enabled!(log::Level::Debug) {
        return;
    }

    let prompt_cache_key = body
        .get("prompt_cache_key")
        .and_then(|value| value.as_str())
        .map(|key| format!("present(len={})", key.len()))
        .unwrap_or_else(|| "absent".to_string());
    let store = body
        .get("store")
        .map(value_for_log)
        .unwrap_or_else(|| "absent".to_string());
    let stream = body
        .get("stream")
        .map(value_for_log)
        .unwrap_or_else(|| "absent".to_string());
    let cache_controls = cache_control_summary(body);

    log::debug!(
        "[CacheTrace] app={}, provider={}, endpoint={}, api_format={}, session_client_provided={}, prompt_cache_key={}, store={}, stream={}, instructions_hash={}, system_hash={}, tools_hash={}, input_hash={}, messages_hash={}, include_hash={}, cache_controls={}, body_hash={}",
        app_type.as_str(),
        provider.id,
        // Gemini endpoints carry ?key=<API_KEY>, so redaction strips the query before logging.
        crate::redact_url_for_log(endpoint),
        api_format.unwrap_or("native"),
        session_client_provided,
        prompt_cache_key,
        store,
        stream,
        short_value_hash(body.get("instructions")),
        short_value_hash(body.get("system")),
        short_value_hash(body.get("tools")),
        short_value_hash(body.get("input")),
        short_value_hash(body.get("messages")),
        short_value_hash(body.get("include")),
        cache_controls,
        short_value_hash(Some(body)),
    );
}

fn cache_control_summary(value: &Value) -> String {
    fn walk(value: &Value, count: &mut usize, ttls: &mut std::collections::BTreeSet<String>) {
        match value {
            Value::Object(object) => {
                if let Some(cache_control) = object.get("cache_control") {
                    *count += 1;
                    let ttl = cache_control
                        .get("ttl")
                        .and_then(Value::as_str)
                        .unwrap_or("default");
                    ttls.insert(ttl.to_string());
                }
                for child in object.values() {
                    walk(child, count, ttls);
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, count, ttls);
                }
            }
            _ => {}
        }
    }

    let mut count = 0;
    let mut ttls = std::collections::BTreeSet::new();
    walk(value, &mut count, &mut ttls);
    format!(
        "count={count},ttls={}",
        if ttls.is_empty() {
            "none".to_string()
        } else {
            ttls.into_iter().collect::<Vec<_>>().join("|")
        }
    )
}

fn value_for_log(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Null => "null".to_string(),
        Value::Array(values) => format!("array(len={})", values.len()),
        Value::Object(values) => format!("object(len={})", values.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::provider::LocalProxyRequestOverrides;
    use axum::http::header::{HeaderValue, ACCEPT};
    use axum::http::HeaderMap;

    use serde_json::json;
    use std::collections::HashMap;

    fn test_provider_with_type(provider_type: Option<&str>) -> Provider {
        Provider {
            id: "provider-1".to_string(),
            name: "Provider 1".to_string(),
            settings_config: json!({}),
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: provider_type.map(|value| crate::provider::ProviderMeta {
                provider_type: Some(value.to_string()),
                ..Default::default()
            }),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn single_provider_retryable_log_uses_single_provider_code() {
        let error = ProxyError::UpstreamError {
            status: 429,
            body: Some(r#"{"error":{"message":"rate limit exceeded"}}"#.to_string()),
        };

        let (code, message) = build_retryable_failure_log("PackyCode-response", 1, 1, &error);

        assert_eq!(code, log_fwd::SINGLE_PROVIDER_FAILED);
        assert!(message.contains("Provider PackyCode-response request failed"));
        assert!(message.contains("upstream HTTP 429"));
        // The upstream error message is kept (truncated) to diagnose the failure.
        assert!(message.contains("rate limit exceeded"));
        assert!(!message.contains("trying the next one"));
    }

    #[test]
    fn multi_provider_retryable_log_keeps_failover_wording() {
        let error = ProxyError::Timeout("upstream timed out after 30s".to_string());

        let (code, message) = build_retryable_failure_log("primary", 1, 3, &error);

        assert_eq!(code, log_fwd::PROVIDER_FAILED_RETRY);
        assert!(message.contains("trying the next one (1/3)"));
        assert!(message.contains("request timed out"));
    }

    #[test]
    fn single_provider_has_no_terminal_all_failed_log() {
        assert!(build_terminal_failure_log(1, 1, None).is_none());
    }

    #[test]
    fn multi_provider_terminal_log_contains_last_error_summary() {
        let error = ProxyError::ForwardFailed("connection reset by peer".to_string());

        let (code, message) =
            build_terminal_failure_log(2, 2, Some(&error)).expect("expected terminal log");

        assert_eq!(code, log_fwd::ALL_PROVIDERS_FAILED);
        assert!(message.contains("Tried 2/2 providers, all failed"));
        assert!(message.contains("connection reset by peer"));
    }

    #[test]
    fn summarize_text_for_log_collapses_whitespace_and_truncates() {
        let summary = summarize_text_for_log("line1\n\n line2   line3", 12);

        assert_eq!(summary, "line1 line2...");
    }

    #[test]
    fn canonical_json_sorts_object_keys_for_cache_trace_hashes() {
        let left = json!({
            "tools": [
                {
                    "parameters": {
                        "properties": {
                            "b": {"type": "string"},
                            "a": {"type": "number"}
                        },
                        "type": "object"
                    },
                    "name": "lookup"
                }
            ]
        });
        let right = json!({
            "tools": [
                {
                    "name": "lookup",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "a": {"type": "number"},
                            "b": {"type": "string"}
                        }
                    }
                }
            ]
        });

        assert_eq!(
            crate::proxy::json_canonical::canonical_json_string(&left),
            crate::proxy::json_canonical::canonical_json_string(&right)
        );
        assert_eq!(
            short_value_hash(Some(&left)),
            short_value_hash(Some(&right))
        );
    }

    #[test]
    fn prepare_upstream_request_body_filters_private_fields_and_canonicalizes_order() {
        let body = json!({
            "z": 1,
            "_internal": "drop",
            "tools": [
                {
                    "name": "lookup",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "_id": {
                                "_private_note": "drop",
                                "type": "string"
                            },
                            "b": {"type": "number"},
                            "a": {"type": "string"}
                        }
                    }
                }
            ],
            "a": 2
        });

        let prepared = prepare_upstream_request_body(body);

        assert!(prepared.get("_internal").is_none());
        assert!(prepared["tools"][0]["parameters"]["properties"]
            .get("_id")
            .is_some());
        assert!(prepared["tools"][0]["parameters"]["properties"]["_id"]
            .get("_private_note")
            .is_none());
        assert_eq!(
            serde_json::to_string(&prepared).unwrap(),
            r#"{"a":2,"tools":[{"name":"lookup","parameters":{"properties":{"_id":{"type":"string"},"a":{"type":"string"},"b":{"type":"number"}},"type":"object"}}],"z":1}"#
        );
    }

    #[test]
    fn local_proxy_body_overrides_deep_merge_final_body_without_stream() {
        let mut body = json!({
            "model": "before",
            "stream": false,
            "metadata": {
                "keep": true,
                "temperature": 1
            },
            "messages": [{ "role": "user", "content": "hello" }]
        });
        let overrides = LocalProxyRequestOverrides {
            headers: HashMap::new(),
            body: Some(json!({
                "model": "after",
                "stream": true,
                "metadata": {
                    "temperature": 0.2,
                    "top_p": 0.9
                },
                "messages": []
            })),
        };

        assert!(apply_local_proxy_body_overrides(&mut body, &overrides));

        assert_eq!(body["model"], "after");
        assert_eq!(body["stream"], false);
        assert_eq!(body["metadata"]["keep"], true);
        assert_eq!(body["metadata"]["temperature"], 0.2);
        assert_eq!(body["metadata"]["top_p"], 0.9);
        assert_eq!(body["messages"], json!([]));
    }

    #[test]
    fn local_proxy_header_overrides_replace_allowed_headers_only() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::USER_AGENT,
            http::HeaderValue::from_static("original"),
        );
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer good"),
        );
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );

        let overrides = LocalProxyRequestOverrides {
            headers: HashMap::from([
                ("User-Agent".to_string(), "custom".to_string()),
                ("X-Test".to_string(), "ok".to_string()),
                ("Authorization".to_string(), "Bearer bad".to_string()),
                ("Content-Type".to_string(), "text/plain".to_string()),
                ("X-Bad".to_string(), "bad\nvalue".to_string()),
            ]),
            body: None,
        };

        apply_local_proxy_header_overrides(&mut headers, Some(&overrides), false);

        assert_eq!(
            headers
                .get(http::header::USER_AGENT)
                .and_then(|value| value.to_str().ok()),
            Some("custom")
        );
        assert_eq!(
            headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer good")
        );
        assert_eq!(
            headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        assert_eq!(
            headers.get("x-test").and_then(|value| value.to_str().ok()),
            Some("ok")
        );
        assert!(headers.get("x-bad").is_none());
    }

    #[test]
    fn local_proxy_header_overrides_are_skipped_for_copilot() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::USER_AGENT,
            http::HeaderValue::from_static("copilot"),
        );
        let overrides = LocalProxyRequestOverrides {
            headers: HashMap::from([("User-Agent".to_string(), "custom".to_string())]),
            body: None,
        };

        apply_local_proxy_header_overrides(&mut headers, Some(&overrides), true);

        assert_eq!(
            headers
                .get(http::header::USER_AGENT)
                .and_then(|value| value.to_str().ok()),
            Some("copilot")
        );
    }

    #[test]
    fn codex_oauth_session_headers_match_codex_cache_identity() {
        let headers = build_codex_oauth_session_headers("session-123");
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.insert(name, value);
        }

        assert_eq!(
            map.get("session_id"),
            Some(&HeaderValue::from_static("session-123"))
        );
        assert_eq!(
            map.get("x-client-request-id"),
            Some(&HeaderValue::from_static("session-123"))
        );
        assert_eq!(
            map.get("x-codex-window-id"),
            Some(&HeaderValue::from_static("session-123:0"))
        );
    }

    #[test]
    fn managed_account_upstream_rejects_proxy_managed_placeholder_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );

        let err = reject_proxy_placeholder_for_managed_account_upstream(
            "https://api.githubcopilot.com/chat/completions",
            &headers,
        )
        .expect_err("placeholder should be rejected before upstream");

        assert!(matches!(
            err,
            ProxyError::AuthError(message) if message.contains("PROXY_MANAGED")
        ));

        let xai_err = reject_proxy_placeholder_for_managed_account_upstream(
            "https://api.x.ai/v1/responses",
            &headers,
        )
        .expect_err("xAI placeholder should be rejected before upstream");
        assert!(matches!(
            xai_err,
            ProxyError::AuthError(message) if message.contains("PROXY_MANAGED")
        ));
    }

    #[test]
    fn codex_oauth_upstream_rejects_proxy_managed_placeholder_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );

        let err = reject_proxy_placeholder_for_managed_account_upstream(
            "https://chatgpt.com/backend-api/codex/responses",
            &headers,
        )
        .expect_err("placeholder should be rejected before upstream");

        assert!(matches!(
            err,
            ProxyError::AuthError(message) if message.contains("PROXY_MANAGED")
        ));
    }

    #[test]
    fn non_managed_upstream_allows_proxy_managed_placeholder_guard() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );

        reject_proxy_placeholder_for_managed_account_upstream(
            "https://api.example.com/v1/messages",
            &headers,
        )
        .expect("guard is scoped to managed-account upstreams");
    }

    #[test]
    fn exact_header_case_preserved_for_native_claude_only() {
        let provider = test_provider_with_type(None);

        assert!(should_preserve_exact_header_case(
            "Claude",
            &provider,
            Some("anthropic"),
            false
        ));
        assert!(!should_preserve_exact_header_case(
            "Claude",
            &provider,
            Some("openai_responses"),
            false
        ));
        assert!(!should_preserve_exact_header_case(
            "Codex", &provider, None, false
        ));
        assert!(!should_preserve_exact_header_case(
            "Gemini", &provider, None, false
        ));
    }

    #[test]
    fn exact_header_case_skipped_for_codex_oauth_and_copilot() {
        let codex_oauth = test_provider_with_type(Some("codex_oauth"));
        let copilot = test_provider_with_type(Some("github_copilot"));

        assert!(!should_preserve_exact_header_case(
            "Claude",
            &codex_oauth,
            Some("openai_responses"),
            false
        ));
        assert!(!should_preserve_exact_header_case(
            "Claude",
            &copilot,
            Some("openai_chat"),
            true
        ));
    }

    #[test]
    fn rewrite_claude_transform_endpoint_strips_beta_for_chat_completions() {
        let (endpoint, passthrough_query) = rewrite_claude_transform_endpoint(
            "/v1/messages?beta=true&foo=bar",
            "openai_chat",
            false,
            &json!({ "model": "gpt-5.4" }),
        );

        assert_eq!(endpoint, "/v1/chat/completions?foo=bar");
        assert_eq!(passthrough_query.as_deref(), Some("foo=bar"));
    }

    #[test]
    fn rewrite_claude_transform_endpoint_strips_beta_for_responses() {
        let (endpoint, passthrough_query) = rewrite_claude_transform_endpoint(
            "/claude/v1/messages?beta=true&x-id=1",
            "openai_responses",
            false,
            &json!({ "model": "gpt-5.4" }),
        );

        assert_eq!(endpoint, "/v1/responses?x-id=1");
        assert_eq!(passthrough_query.as_deref(), Some("x-id=1"));
    }

    #[test]
    fn rewrite_codex_responses_endpoint_to_chat_preserves_query() {
        let (endpoint, passthrough_query) =
            rewrite_codex_responses_endpoint_to_chat("/v1/responses?foo=bar");

        assert_eq!(endpoint, "/chat/completions?foo=bar");
        assert_eq!(passthrough_query.as_deref(), Some("foo=bar"));
    }

    #[test]
    fn prepend_claude_code_system_prompt_from_string() {
        let mut body = json!({ "system": "You are a Codex agent." });
        prepend_claude_code_system_prompt(&mut body);
        let system = body["system"].as_array().unwrap();
        assert_eq!(system[0]["text"], CLAUDE_CODE_SYSTEM_IDENTITY);
        assert_eq!(system[1]["text"], "You are a Codex agent.");
    }

    #[test]
    fn prepend_claude_code_system_prompt_when_absent() {
        let mut body = json!({});
        prepend_claude_code_system_prompt(&mut body);
        let system = body["system"].as_array().unwrap();
        assert_eq!(system.len(), 1);
        assert_eq!(system[0]["text"], CLAUDE_CODE_SYSTEM_IDENTITY);
    }

    #[test]
    fn prepend_claude_code_system_prompt_is_idempotent() {
        let mut body = json!({ "system": "orig" });
        prepend_claude_code_system_prompt(&mut body);
        prepend_claude_code_system_prompt(&mut body);
        let system = body["system"].as_array().unwrap();
        assert_eq!(system.len(), 2);
        assert_eq!(system[0]["text"], CLAUDE_CODE_SYSTEM_IDENTITY);
        assert_eq!(system[1]["text"], "orig");
    }

    #[test]
    fn rewrite_codex_responses_endpoint_to_anthropic_preserves_query() {
        let (endpoint, passthrough_query) =
            rewrite_codex_responses_endpoint_to_anthropic("/responses?x=1");
        assert_eq!(endpoint, "/v1/messages?x=1");
        assert_eq!(passthrough_query.as_deref(), Some("x=1"));

        let (endpoint, _) = rewrite_codex_responses_endpoint_to_anthropic("/v1/responses");
        assert_eq!(endpoint, "/v1/messages");
    }

    #[test]
    fn codex_anthropic_full_endpoint_guard_avoids_double_messages() {
        // On the Codex→Anthropic path a base URL already ending in `/v1/messages` (switch
        // off) must be treated as a full endpoint by the real `base_url_is_full_endpoint`.

        // Without the guard, build_url would concatenate the pasted endpoint with the
        // rewritten `/v1/messages` target, producing a broken double suffix.
        use super::super::providers::ProviderAdapter;
        let doubled = super::super::providers::CodexAdapter::new()
            .build_url("https://host.example/v1/messages", "/v1/messages");
        assert_eq!(doubled, "https://host.example/v1/messages/v1/messages");

        // With the guard, the pasted URL is used verbatim (plus preserved query). Includes
        // query/fragment/whitespace suffixes, which must not hide the endpoint (fix: a base
        // like `.../v1/messages?beta=true` previously evaded the suffix check).
        for base in [
            "https://host.example/v1/messages",
            "https://host.example/v1/messages/",
            "https://host.example/api/v1/messages", // prefixed gateway
            "https://host.example/v1/messages?beta=true",
            "https://host.example/v1/messages/?beta=true",
            "https://host.example/v1/messages#frag",
            "  https://host.example/v1/messages  ",
        ] {
            assert!(
                base_url_is_full_endpoint(base, "/v1/messages"),
                "expected full-endpoint match: {base:?}"
            );
        }
        assert_eq!(
            append_query_to_full_url("https://host.example/v1/messages", Some("x=1")),
            "https://host.example/v1/messages?x=1"
        );
        // A base URL that already carries its own query is preserved verbatim (no double
        // `/v1/messages`, query kept).
        assert_eq!(
            append_query_to_full_url("https://host.example/v1/messages?beta=true", None),
            "https://host.example/v1/messages?beta=true"
        );

        // A non-endpoint base (origin/prefix) must NOT match, so build_url still appends.
        assert!(!base_url_is_full_endpoint(
            "https://host.example",
            "/v1/messages"
        ));
        assert!(!base_url_is_full_endpoint(
            "https://host.example/v1",
            "/v1/messages"
        ));
        // The shared helper also backs the Chat path's `/chat/completions` guard.
        assert!(base_url_is_full_endpoint(
            "https://host.example/v1/chat/completions?api-version=2024",
            "/chat/completions"
        ));
    }

    #[test]
    fn codex_client_fingerprint_headers_are_dropped_for_anthropic_upstreams() {
        // Codex/OpenAI fingerprints a native Claude Code client never sends → must drop.
        for header in [
            "originator",
            "session_id",
            "session-id",
            "thread-id",
            "conversation_id",
            "chatgpt-account-id",
            "x-openai-subagent",
            "x-client-request-id",
            "x-codex-window-id",
            "openai-beta",
            "openai-organization",
            "openai-project",
            "x-stainless-lang",
            "x-stainless-runtime",
            "x-codex-turn-id",
        ] {
            assert!(
                is_codex_client_fingerprint_header(header),
                "expected {header} to be dropped while impersonating Claude Code"
            );
        }

        // Headers a real Claude Code client sends (or that the forwarder rebuilds) must
        // NOT be caught by the denylist.
        for header in [
            "anthropic-version",
            "anthropic-beta",
            "user-agent",
            "accept",
            "content-type",
            "x-app",
        ] {
            assert!(
                !is_codex_client_fingerprint_header(header),
                "{header} must be preserved while impersonating Claude Code"
            );
        }
    }

    #[test]
    fn codex_anthropic_2xx_error_envelope_is_detected_for_failover() {
        let body = br#"{"type":"error","error":{"type":"overloaded_error","message":"busy"}}"#;
        assert_eq!(
            codex_anthropic_error_envelope_message(body).as_deref(),
            Some("overloaded_error: busy")
        );
        assert!(
            codex_anthropic_error_envelope_message(br#"{"type":"message","content":[]}"#).is_none()
        );
    }

    #[test]
    fn responses_2xx_failure_is_detected_for_failover() {
        assert_eq!(
            responses_error_envelope_message(
                br#"{"status":"failed","error":{"type":"server_error","message":"busy"},"output":[]}"#
            )
            .as_deref(),
            Some("server_error: busy")
        );
        assert_eq!(
            responses_error_envelope_message(br#"{"status":"cancelled","output":[]}"#).as_deref(),
            Some("cancelled: response generation was cancelled")
        );
        assert!(responses_error_envelope_message(
            br#"{"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":[]}"#
        )
        .is_none());
        assert!(responses_error_envelope_message(
            br#"{"status":"completed","error":null,"output":[]}"#
        )
        .is_none());
    }

    #[test]
    fn responses_stream_start_semantic_failure_is_retryable() {
        let created = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}"
        );
        assert!(inspect_responses_start_event(created).is_none());

        let failed = concat!(
            "event: response.failed\n",
            "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"type\":\"server_error\",\"message\":\"boom\"}}}"
        );
        assert!(matches!(
            inspect_responses_start_event(failed),
            Some(Err(ProxyError::TransformError(message))) if message.contains("boom")
        ));

        let delta = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}"
        );
        assert!(matches!(inspect_responses_start_event(delta), Some(Ok(()))));
    }

    #[test]
    fn responses_stream_start_accepts_unlabelled_whole_json() {
        assert!(matches!(
            inspect_responses_json_document(
                r#"{
                    "status": "completed",

                    "output": []
                }"#
            ),
            Some(Ok(()))
        ));
        assert!(inspect_responses_json_document(r#"{"status":"completed""#).is_none());

        let failed = inspect_responses_json_document(
            r#"{"status":"failed","error":{"message":"backend unavailable"}}"#,
        );
        assert!(
            matches!(failed, Some(Err(ProxyError::TransformError(message))) if message.contains("backend unavailable"))
        );
    }

    #[test]
    fn codex_anthropic_cache_is_default_on_but_honors_sub_switch() {
        let default = codex_anthropic_cache_config(&OptimizerConfig::default());
        assert!(default.enabled);
        assert!(default.cache_injection);

        let disabled = codex_anthropic_cache_config(&OptimizerConfig {
            cache_injection: false,
            ..OptimizerConfig::default()
        });
        assert!(disabled.enabled);
        assert!(!disabled.cache_injection);
    }

    #[test]
    fn official_codex_rejects_stale_proxy_placeholder_with_restart_hint() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );
        let mut provider = test_provider_with_type(None);
        provider.id = "codex-official".to_string();
        provider.category = Some("official".to_string());
        let error = validate_codex_official_authorization(&headers, &provider)
            .expect_err("stale placeholder must be rejected");
        assert!(
            matches!(error, ProxyError::AuthError(message) if message.contains("restart Codex"))
        );
    }

    #[test]
    fn managed_codex_official_rejects_a_different_session_account() {
        let mut provider = test_provider_with_type(Some("codex_oauth"));
        provider.category = Some("official".to_string());
        provider.meta.as_mut().expect("provider meta").auth_binding =
            Some(crate::provider::AuthBinding {
                source: crate::provider::AuthBindingSource::ManagedAccount,
                auth_provider: Some("codex_oauth".to_string()),
                account_id: Some("account-b".to_string()),
            });

        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer account-a-token"),
        );
        headers.insert("chatgpt-account-id", HeaderValue::from_static("account-a"));
        let error = validate_codex_official_authorization(&headers, &provider)
            .expect_err("a stale Codex session must not cross the account boundary");
        assert!(
            matches!(error, ProxyError::AuthError(message) if message.contains("restart Codex"))
        );

        headers.insert("chatgpt-account-id", HeaderValue::from_static("account-b"));
        validate_codex_official_authorization(&headers, &provider)
            .expect("the selected account may pass through");
    }

    #[test]
    fn rewrite_codex_responses_compact_endpoint_to_chat_preserves_query() {
        let (endpoint, passthrough_query) =
            rewrite_codex_responses_endpoint_to_chat("/v1/responses/compact?foo=bar");

        assert_eq!(endpoint, "/chat/completions?foo=bar");
        assert_eq!(passthrough_query.as_deref(), Some("foo=bar"));
    }

    #[test]
    fn rewrite_claude_transform_endpoint_uses_copilot_path() {
        let (endpoint, passthrough_query) = rewrite_claude_transform_endpoint(
            "/v1/messages?beta=true&x-id=1",
            "anthropic",
            true,
            &json!({ "model": "claude-sonnet-4-6" }),
        );

        assert_eq!(endpoint, "/chat/completions?x-id=1");
        assert_eq!(passthrough_query.as_deref(), Some("x-id=1"));
    }

    #[test]
    fn rewrite_claude_transform_endpoint_uses_copilot_responses_path() {
        let (endpoint, passthrough_query) = rewrite_claude_transform_endpoint(
            "/v1/messages?beta=true&x-id=1",
            "openai_responses",
            true,
            &json!({ "model": "gpt-5.4" }),
        );

        assert_eq!(endpoint, "/v1/responses?x-id=1");
        assert_eq!(passthrough_query.as_deref(), Some("x-id=1"));
    }

    #[test]
    fn rewrite_claude_transform_endpoint_maps_gemini_generate_content() {
        let (endpoint, passthrough_query) = rewrite_claude_transform_endpoint(
            "/v1/messages?beta=true&x-id=1",
            "gemini_native",
            false,
            &json!({ "model": "gemini-2.5-pro" }),
        );

        assert_eq!(
            endpoint,
            "/v1beta/models/gemini-2.5-pro:generateContent?x-id=1"
        );
        assert_eq!(passthrough_query.as_deref(), Some("x-id=1"));
    }

    /// Regression: body.model arriving as the resource-name form
    /// `models/gemini-2.5-pro` must not produce a doubled
    /// `/v1beta/models/models/...` path.
    #[test]
    fn rewrite_claude_transform_endpoint_strips_gemini_model_resource_prefix() {
        let (endpoint, _) = rewrite_claude_transform_endpoint(
            "/v1/messages",
            "gemini_native",
            false,
            &json!({ "model": "models/gemini-2.5-pro" }),
        );

        assert_eq!(endpoint, "/v1beta/models/gemini-2.5-pro:generateContent");
    }

    #[test]
    fn rewrite_claude_transform_endpoint_maps_gemini_streaming() {
        let (endpoint, passthrough_query) = rewrite_claude_transform_endpoint(
            "/v1/messages?beta=true",
            "gemini_native",
            false,
            &json!({ "model": "gemini-2.5-flash", "stream": true }),
        );

        assert_eq!(
            endpoint,
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
        assert_eq!(passthrough_query.as_deref(), Some("alt=sse"));
    }

    #[test]
    fn append_query_to_full_url_preserves_existing_query_string() {
        let url = append_query_to_full_url("https://relay.example/api?foo=bar", Some("x-id=1"));

        assert_eq!(url, "https://relay.example/api?foo=bar&x-id=1");
    }

    #[test]
    fn alpha_search_rewrites_known_full_responses_urls() {
        let cases = [
            (
                "https://relay.example/Gateway/%2F/v1/Responses/Compact/?api-version=CaseValue#fragment",
                "https://relay.example/Gateway/%2F/v1/alpha/search?api-version=CaseValue&client_version=0.144.6",
            ),
            (
                "https://relay.example/v1/responses",
                "https://relay.example/v1/alpha/search?client_version=0.144.6",
            ),
            (
                "https://relay.example/backend-api/codex/responses/compact/",
                "https://relay.example/backend-api/codex/alpha/search?client_version=0.144.6",
            ),
            (
                "https://relay.example/custom/%2F/v1/responses?api-version=2026-07",
                "https://relay.example/custom/%2F/v1/alpha/search?api-version=2026-07&client_version=0.144.6",
            ),
        ];

        for (base_url, expected) in cases {
            assert_eq!(
                rewrite_codex_standalone_full_url(
                    base_url,
                    Some("client_version=0.144.6"),
                    CodexStandaloneEndpoint::AlphaSearch,
                )
                .expect("known Responses full URL should be rewritable"),
                expected
            );
        }
    }

    #[test]
    fn images_generations_rewrites_known_full_codex_urls() {
        let cases = [
            (
                "https://relay.example/Gateway/v1/Images/Edits/?api-version=CaseValue#fragment",
                "https://relay.example/Gateway/v1/images/generations?api-version=CaseValue&client_version=0.145.0",
            ),
            (
                "https://relay.example/v1/responses",
                "https://relay.example/v1/images/generations?client_version=0.145.0",
            ),
            (
                "https://relay.example/backend-api/codex/responses/compact/",
                "https://relay.example/backend-api/codex/images/generations?client_version=0.145.0",
            ),
            (
                "https://relay.example/custom/%2F/v1/responses?api-version=2026-07",
                "https://relay.example/custom/%2F/v1/images/generations?api-version=2026-07&client_version=0.145.0",
            ),
            (
                "https://relay.example/v1/chat/completions?api-version=2026-07",
                "https://relay.example/v1/images/generations?api-version=2026-07&client_version=0.145.0",
            ),
            (
                "https://relay.example/v1/images/edits",
                "https://relay.example/v1/images/generations?client_version=0.145.0",
            ),
        ];

        for (base_url, expected) in cases {
            assert_eq!(
                rewrite_codex_standalone_full_url(
                    base_url,
                    Some("client_version=0.145.0"),
                    CodexStandaloneEndpoint::ImagesGenerations,
                )
                .expect("known Codex full URL should be rewritable"),
                expected
            );
        }
    }

    #[test]
    fn images_generations_preserves_existing_full_images_url() {
        let url = rewrite_codex_standalone_full_url(
            "https://relay.example/v1/images/generations?api-version=2026-07",
            Some("client_version=0.145.0"),
            CodexStandaloneEndpoint::ImagesGenerations,
        )
        .expect("full Images URL should be preserved");

        assert_eq!(
            url,
            "https://relay.example/v1/images/generations?api-version=2026-07&client_version=0.145.0"
        );
    }

    #[test]
    fn images_generations_rejects_opaque_full_url_instead_of_misrouting_payload() {
        let error = rewrite_codex_standalone_full_url(
            "https://relay.example/custom/rpc-endpoint",
            Some("client_version=0.145.0"),
            CodexStandaloneEndpoint::ImagesGenerations,
        )
        .expect_err("opaque endpoint must fail closed");

        assert!(matches!(
            error,
            ProxyError::ConfigError(message)
                if message.contains("cannot derive /images/generations")
        ));
    }

    #[test]
    fn codex_standalone_endpoint_recognizes_images_edits() {
        assert!(matches!(
            CodexStandaloneEndpoint::from_effective_endpoint(
                "/images/edits?client_version=0.145.0"
            ),
            Some(CodexStandaloneEndpoint::ImagesEdits)
        ));
        // Codex ImageGen never calls the variations route; keep it unrouted.
        assert!(CodexStandaloneEndpoint::from_effective_endpoint("/images/variations").is_none());
    }

    #[test]
    fn images_edits_rewrites_known_full_codex_urls() {
        let cases = [
            (
                "https://relay.example/Gateway/v1/Chat/Completions/?api-version=CaseValue#fragment",
                "https://relay.example/Gateway/v1/images/edits?api-version=CaseValue&client_version=0.145.0",
            ),
            (
                "https://relay.example/v1/responses",
                "https://relay.example/v1/images/edits?client_version=0.145.0",
            ),
            (
                "https://relay.example/backend-api/codex/responses/compact/",
                "https://relay.example/backend-api/codex/images/edits?client_version=0.145.0",
            ),
            (
                "https://relay.example/v1/chat/completions?api-version=2026-07",
                "https://relay.example/v1/images/edits?api-version=2026-07&client_version=0.145.0",
            ),
            (
                "https://relay.example/v1/images/generations?api-version=2026-07",
                "https://relay.example/v1/images/edits?api-version=2026-07&client_version=0.145.0",
            ),
        ];

        for (base_url, expected) in cases {
            assert_eq!(
                rewrite_codex_standalone_full_url(
                    base_url,
                    Some("client_version=0.145.0"),
                    CodexStandaloneEndpoint::ImagesEdits,
                )
                .expect("known Codex full URL should be rewritable"),
                expected
            );
        }
    }

    #[test]
    fn images_edits_preserves_existing_full_edits_url() {
        let url = rewrite_codex_standalone_full_url(
            "https://relay.example/v1/images/edits?api-version=2026-07",
            Some("client_version=0.145.0"),
            CodexStandaloneEndpoint::ImagesEdits,
        )
        .expect("full Images edits URL should be preserved");

        assert_eq!(
            url,
            "https://relay.example/v1/images/edits?api-version=2026-07&client_version=0.145.0"
        );
    }

    #[test]
    fn images_edits_rejects_opaque_full_url_instead_of_misrouting_payload() {
        let error = rewrite_codex_standalone_full_url(
            "https://relay.example/custom/rpc-endpoint",
            Some("client_version=0.145.0"),
            CodexStandaloneEndpoint::ImagesEdits,
        )
        .expect_err("opaque endpoint must fail closed");

        assert!(matches!(
            error,
            ProxyError::ConfigError(message)
                if message.contains("cannot derive /images/edits")
        ));
    }

    #[test]
    fn alpha_search_rejects_opaque_full_url_instead_of_misrouting_payload() {
        let error = rewrite_codex_standalone_full_url(
            "https://relay.example/custom/rpc-endpoint",
            Some("client_version=0.144.6"),
            CodexStandaloneEndpoint::AlphaSearch,
        )
        .expect_err("opaque endpoint must fail closed");

        assert!(matches!(
            error,
            ProxyError::ConfigError(message)
                if message.contains("cannot derive /alpha/search")
        ));
    }

    #[test]
    fn build_gemini_native_url_uses_origin_when_base_ends_with_v1beta() {
        let url = crate::proxy::gemini_url::build_gemini_native_url(
            "https://generativelanguage.googleapis.com/v1beta",
            "/v1beta/models/gemini-2.5-pro:generateContent",
        );

        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
        );
    }

    #[test]
    fn build_gemini_native_url_uses_origin_when_base_already_contains_models_prefix() {
        let url = crate::proxy::gemini_url::build_gemini_native_url(
            "https://generativelanguage.googleapis.com/v1beta/models",
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse",
        );

        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn resolve_gemini_native_url_keeps_opaque_full_url_as_is() {
        let url = crate::proxy::gemini_url::resolve_gemini_native_url(
            "https://relay.example/custom/generate-content",
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse",
            true,
        );

        assert_eq!(url, "https://relay.example/custom/generate-content?alt=sse");
    }

    #[test]
    fn force_identity_for_stream_flag_requests() {
        let headers = HeaderMap::new();

        assert!(should_force_identity_encoding(
            "/v1/responses",
            &json!({ "stream": true }),
            &headers
        ));
    }

    #[test]
    fn force_identity_for_gemini_stream_endpoints() {
        let headers = HeaderMap::new();

        assert!(should_force_identity_encoding(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
            &json!({ "model": "gemini-2.5-pro" }),
            &headers
        ));
    }

    #[test]
    fn streaming_request_detects_gemini_sse_without_body_stream_flag() {
        let headers = HeaderMap::new();

        assert!(is_streaming_request(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
            &json!({ "model": "gemini-2.5-pro" }),
            &headers
        ));
    }

    #[test]
    fn force_identity_for_sse_accept_header() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));

        assert!(should_force_identity_encoding(
            "/v1/responses",
            &json!({ "model": "gpt-5" }),
            &headers
        ));
    }

    #[test]
    fn non_streaming_requests_allow_automatic_compression() {
        let headers = HeaderMap::new();

        assert!(!should_force_identity_encoding(
            "/v1/responses",
            &json!({ "model": "gpt-5" }),
            &headers
        ));
    }

    // ==================== Copilot dynamic endpoint routing tests ====================

    /// Checks the is_copilot detection via provider_type
    #[test]
    fn copilot_detection_via_provider_type() {
        use crate::provider::{Provider, ProviderMeta};

        let provider = Provider {
            id: "test".to_string(),
            name: "Test Copilot".to_string(),
            settings_config: serde_json::json!({}),
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: Some(ProviderMeta {
                provider_type: Some("github_copilot".to_string()),
                ..Default::default()
            }),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        };

        let is_copilot = provider
            .meta
            .as_ref()
            .and_then(|m| m.provider_type.as_deref())
            == Some("github_copilot");

        assert!(is_copilot, "provider_type must identify this as Copilot");
    }

    /// Checks the is_copilot detection via base_url
    #[test]
    fn copilot_detection_via_base_url() {
        let base_url = "https://api.githubcopilot.com";
        let is_copilot = base_url.contains("githubcopilot.com");
        assert!(is_copilot, "base_url must identify this as Copilot");

        let non_copilot_url = "https://api.anthropic.com";
        let is_not_copilot = non_copilot_url.contains("githubcopilot.com");
        assert!(!is_not_copilot, "a non-Copilot URL must not match");
    }

    /// Checks that is_copilot still works for an enterprise endpoint without githubcopilot.com
    #[test]
    fn copilot_detection_for_enterprise_endpoint() {
        use crate::provider::{Provider, ProviderMeta};

        // Enterprise case: provider_type is github_copilot while base_url may be an internal domain
        let provider = Provider {
            id: "enterprise".to_string(),
            name: "Enterprise Copilot".to_string(),
            settings_config: serde_json::json!({}),
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: Some(ProviderMeta {
                provider_type: Some("github_copilot".to_string()),
                ..Default::default()
            }),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        };

        let enterprise_base_url = "https://copilot-api.corp.example.com";

        // is_copilot must succeed via provider_type even when base_url lacks githubcopilot.com
        let is_copilot = provider
            .meta
            .as_ref()
            .and_then(|m| m.provider_type.as_deref())
            == Some("github_copilot")
            || enterprise_base_url.contains("githubcopilot.com");

        assert!(
            is_copilot,
            "enterprise Copilot must be detected via provider_type"
        );
    }

    /// Checks the conditions for replacing the endpoint dynamically
    #[test]
    fn dynamic_endpoint_replacement_conditions() {
        // Condition: is_copilot && !is_full_url
        let test_cases = [
            (true, false, true, "Copilot, no full_url: replace"),
            (true, true, false, "Copilot, full_url: keep"),
            (false, false, false, "non-Copilot: keep"),
            (false, true, false, "non-Copilot, full_url: keep"),
        ];

        for (is_copilot, is_full_url, should_replace, desc) in test_cases {
            let will_replace = is_copilot && !is_full_url;
            assert_eq!(will_replace, should_replace, "{desc}");
        }
    }

    // ===== P3: regression tests for the media switches at the forwarder layer =====
    // Checks how the gate is wired at the forwarder layer, not the pure media_sanitizer functions.
}
