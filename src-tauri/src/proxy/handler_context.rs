//! Request context module
//!
//! Manages context across the request lifecycle and wraps the shared initialization logic

use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy::{
    extract_session_id,
    forwarder::RequestForwarder,
    server::ProxyState,
    types::{AppProxyConfig, CopilotOptimizerConfig, OptimizerConfig, RectifierConfig},
    ProxyError,
};
use axum::http::HeaderMap;
use std::time::Instant;

/// Streaming timeout configuration
#[derive(Debug, Clone, Copy)]
pub struct StreamingTimeoutConfig {
    /// First-byte timeout in seconds; 0 disables it
    pub first_byte_timeout: u64,
    /// Idle timeout in seconds; 0 disables it
    pub idle_timeout: u64,
}

/// Request context
///
/// Lives for the whole request lifecycle and holds:
/// - Timing information
/// - Per-app proxy configuration
/// - The selected provider list (for failover)
/// - The requested model name
/// - The log tag
/// - The session ID (for log correlation)
pub struct RequestContext {
    /// When the request started
    pub start_time: Instant,
    /// Per-app proxy configuration (retry count and timeouts included)
    pub app_config: AppProxyConfig,
    /// The selected provider (first in the failover chain)
    pub provider: Provider,
    /// The full provider list (for failover)
    providers: Vec<Provider>,
    /// The "current provider" when the request started (used to decide whether to sync UI/tray)
    ///
    /// This uses the device-level current provider from local settings.
    /// In proxy mode, a mismatch with the provider actually used triggers a switch so the UI stays accurate.
    pub current_provider_id: String,
    /// The model name from the request
    pub request_model: String,
    /// The model actually sent upstream (the truth after routing takeover / model mapping, filled in after a successful forward).
    ///
    /// Usage attribution falls back in this order: upstream response echo -> outbound_model -> request_model.
    /// request_model alone is not a valid fallback: under takeover it is the pre-mapping client alias.
    pub outbound_model: Option<String>,
    /// Log tag (e.g. "Claude", "Codex", "Gemini")
    pub tag: &'static str,
    /// App type string (e.g. "claude", "codex", "gemini")
    pub app_type_str: &'static str,
    /// Session ID (extracted from the client request or newly generated)
    pub session_id: String,
    /// Whether the client supplied the session ID. A generated UUID must not be used as the upstream cache key, or every request changes it.
    pub session_client_provided: bool,
    /// Rectifier configuration
    pub rectifier_config: RectifierConfig,
    /// Optimizer configuration
    pub optimizer_config: OptimizerConfig,
    /// Copilot optimizer configuration
    pub copilot_optimizer_config: CopilotOptimizerConfig,
}

impl RequestContext {
    /// Creates a request context
    ///
    /// # Arguments
    /// * `state` - proxy server state
    /// * `body` - request body JSON
    /// * `headers` - request headers (used to extract the session ID)
    /// * `app_type` - app type
    /// * `tag` - log tag
    /// * `app_type_str` - app type string
    ///
    /// # Errors
    /// Returns `ProxyError` when provider selection fails
    pub async fn new(
        state: &ProxyState,
        body: &serde_json::Value,
        headers: &HeaderMap,
        app_type: AppType,
        tag: &'static str,
        app_type_str: &'static str,
    ) -> Result<Self, ProxyError> {
        let start_time = Instant::now();

        // Read the per-app proxy configuration from the database
        let app_config = state
            .db
            .get_proxy_config_for_app(app_type_str)
            .await
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;

        // Read the rectifier configuration from the database
        let rectifier_config = state.db.get_rectifier_config().unwrap_or_default();
        let optimizer_config = state.db.get_optimizer_config().unwrap_or_default();
        let copilot_optimizer_config = state.db.get_copilot_optimizer_config().unwrap_or_default();

        let current_provider_id =
            crate::settings::get_current_provider(&app_type).unwrap_or_default();

        // Extract the model name from the request body
        let request_model = body
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Extract the session ID
        let session_result = extract_session_id(headers, body, app_type_str);
        let session_id = session_result.session_id.clone();

        log::debug!(
            "[{}] Session ID: {} (from {:?}, client_provided: {})",
            tag,
            session_id,
            session_result.source,
            session_result.client_provided
        );

        // Select the provider with the shared ProviderRouter (circuit breaker state persists across requests)
        // Note: called exactly once here and passed to the forwarder, so HalfOpen slots are not consumed twice
        let providers = state
            .provider_router
            .select_providers(app_type_str)
            .await
            .map_err(|e| match e {
                crate::error::AppError::AllProvidersCircuitOpen => {
                    ProxyError::AllProvidersCircuitOpen
                }
                crate::error::AppError::NoProvidersConfigured => ProxyError::NoProvidersConfigured,
                _ => ProxyError::DatabaseError(e.to_string()),
            })?;

        let provider = providers
            .first()
            .cloned()
            .ok_or(ProxyError::NoAvailableProvider)?;

        log::debug!(
            "[{}] Provider: {}, model: {}, failover chain: {} providers, session: {}",
            tag,
            provider.name,
            request_model,
            providers.len(),
            session_id
        );

        Ok(Self {
            start_time,
            app_config,
            provider,
            providers,
            current_provider_id,
            request_model,
            outbound_model: None,
            tag,
            app_type_str,
            session_id,
            session_client_provided: session_result.client_provided,
            rectifier_config,
            optimizer_config,
            copilot_optimizer_config,
        })
    }

    /// Extracts the model name from the URI (Gemini only)
    ///
    /// The Gemini API carries the model name in the URI, in the form:
    /// `/v1beta/models/gemini-pro:generateContent`
    pub fn with_model_from_uri(mut self, uri: &axum::http::Uri) -> Self {
        // Use path() rather than path_and_query(): the model name must come from the path segment,
        // otherwise GET /v1beta/models/<id>?key=... appends the query onto request_model.
        let endpoint = uri.path();

        self.request_model =
            extract_gemini_model_from_path(endpoint).unwrap_or_else(|| "unknown".to_string());

        self
    }

    /// Creates a RequestForwarder
    ///
    /// Uses the shared ProviderRouter so circuit breaker state persists across requests
    ///
    /// How the configuration applies:
    /// - Failover on: timeouts apply normally (0 disables a timeout)
    /// - Failover off: timeouts do not apply (all passed as 0)
    pub fn create_forwarder(&self, state: &ProxyState) -> RequestForwarder {
        let (non_streaming_timeout, first_byte_timeout, idle_timeout) =
            if self.app_config.auto_failover_enabled {
                // Failover on: use the configured values (0 = timeout disabled)
                (
                    self.app_config.non_streaming_timeout as u64,
                    self.app_config.streaming_first_byte_timeout as u64,
                    self.app_config.streaming_idle_timeout as u64,
                )
            } else {
                // Failover off: do not enable the timeout configuration
                log::debug!(
                    "[{}] Failover disabled, timeout configs are bypassed",
                    self.tag
                );
                (0, 0, 0)
            };

        // With failover off, force max_retries=0 (only one provider is tried), matching the "no timeout, no switching" semantics.
        let max_retries = if self.app_config.auto_failover_enabled {
            self.app_config.max_retries
        } else {
            0
        };

        RequestForwarder::new(
            state.provider_router.clone(),
            non_streaming_timeout,
            state.status.clone(),
            state.current_providers.clone(),
            state.gemini_shadow.clone(),
            state.codex_chat_history.clone(),
            state.failover_manager.clone(),
            state.app_handle.clone(),
            self.current_provider_id.clone(),
            self.session_id.clone(),
            self.session_client_provided,
            first_byte_timeout,
            idle_timeout,
            self.rectifier_config.clone(),
            self.optimizer_config.clone(),
            self.copilot_optimizer_config.clone(),
            max_retries,
        )
    }

    /// Returns the provider list (for failover)
    ///
    /// Returns the providers chosen when the context was created, avoiding a second select_providers() call
    pub fn get_providers(&self) -> Vec<Provider> {
        self.providers.clone()
    }

    /// Computes the request latency in milliseconds
    #[inline]
    pub fn latency_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Returns the streaming timeout configuration
    ///
    /// How the configuration applies:
    /// - Failover on: return the configured values (0 disables the timeout check)
    /// - Failover off: return 0 (timeout checks disabled)
    #[inline]
    pub fn streaming_timeout_config(&self) -> StreamingTimeoutConfig {
        if self.app_config.auto_failover_enabled {
            // Failover on: use the configured values (0 = timeout disabled)
            StreamingTimeoutConfig {
                first_byte_timeout: self.app_config.streaming_first_byte_timeout as u64,
                idle_timeout: self.app_config.streaming_idle_timeout as u64,
            }
        } else {
            // Failover off: disable streaming timeout checks
            StreamingTimeoutConfig {
                first_byte_timeout: 0,
                idle_timeout: 0,
            }
        }
    }
}

/// Pull the Gemini model name out of an API path.
///
/// Accepts forms like `/v1beta/models/gemini-pro:generateContent`,
/// `/v1/models/gemini-1.5-flash`, `gemini/v1beta/models/<model>:streamGenerateContent`.
/// Returns `None` when no `models/<name>` segment is present.
pub(crate) fn extract_gemini_model_from_path(endpoint: &str) -> Option<String> {
    let segments: Vec<&str> = endpoint.split('/').collect();
    segments
        .iter()
        .position(|s| *s == "models")
        .and_then(|i| segments.get(i + 1).copied())
        // Defensive trim: even if the caller passes a string with ? or :action, keep only the model id
        .map(|s| s.split('?').next().unwrap_or(s))
        .map(|s| s.split(':').next().unwrap_or(s))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::extract_gemini_model_from_path;

    #[test]
    fn extract_model_with_action() {
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-pro:generateContent").as_deref(),
            Some("gemini-pro"),
        );
    }

    #[test]
    fn extract_model_with_dotted_version() {
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-1.5-flash:streamGenerateContent")
                .as_deref(),
            Some("gemini-1.5-flash"),
        );
    }

    #[test]
    fn extract_model_without_action() {
        assert_eq!(
            extract_gemini_model_from_path("/v1/models/gemini-1.5-pro").as_deref(),
            Some("gemini-1.5-pro"),
        );
    }

    #[test]
    fn extract_model_with_proxy_prefix() {
        assert_eq!(
            extract_gemini_model_from_path("/gemini/v1beta/models/gemini-2.0-flash:countTokens")
                .as_deref(),
            Some("gemini-2.0-flash"),
        );
    }

    #[test]
    fn extract_model_with_query_string() {
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-pro:generateContent?key=abc")
                .as_deref(),
            Some("gemini-pro"),
        );
    }

    #[test]
    fn extract_model_missing_segment() {
        assert_eq!(extract_gemini_model_from_path("/v1beta/operations"), None);
    }

    #[test]
    fn extract_model_trailing_models_segment() {
        // `/v1beta/models` (list endpoint) has no following segment → None.
        assert_eq!(extract_gemini_model_from_path("/v1beta/models"), None);
    }

    #[test]
    fn extract_model_get_with_query_only() {
        // GET /v1beta/models/<id>?key=... has no action verb, so splitting on ':' alone drags the query into the model name.
        // After the fix the query must be stripped.
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-pro?key=abc").as_deref(),
            Some("gemini-pro"),
        );
    }

    #[test]
    fn extract_model_get_with_proxy_prefix_and_query() {
        assert_eq!(
            extract_gemini_model_from_path("/gemini/v1beta/models/gemini-2.0-flash?key=abc")
                .as_deref(),
            Some("gemini-2.0-flash"),
        );
    }
}
