//! Provider reachability check service
//!
//! Only probes whether the provider `base_url` is reachable, **it never sends a real model request**:
//! - any HTTP response (200/4xx/5xx) counts as "reachable" (port open, gateway alive);
//! - only network-level errors such as DNS / connection refused / TLS / timeout count as "unreachable";
//! - latency = time until the response headers arrive (TTFB, a real round trip).
//!
//! ## Design trade-off: reachable != correctly configured
//!
//! This check deliberately validates neither auth nor models, so third-party auth rejection / model
//! validation cannot mark it "unavailable". The cost: it cannot tell you if auth or the model is right.
//!
//! ## Relationship with failover (important invariant)
//!
//! The reachability check **never** touches the failover circuit breaker: a provider returning 403/401
//! counts as "reachable" here but is broken for real traffic. The breaker is driven only by
//! real traffic in `proxy/forwarder.rs` (passively). Reachability answers "can we get there", real
//! traffic answers "does it actually work".

use reqwest::header::HeaderValue;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::app_config::AppType;
use crate::error::AppError;
use crate::provider::Provider;
use crate::proxy::providers::{get_adapter, ClaudeAdapter, ProviderAdapter};

/// Health status enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Operational,
    Degraded,
    Failed,
}

/// Reachability check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamCheckConfig {
    /// Timeout of a single probe (seconds)
    pub timeout_secs: u64,
    /// Maximum retries for timeout-like failures
    pub max_retries: u32,
    /// Degradation threshold (ms): reachable but a TTFB above this counts as "slow"
    pub degraded_threshold_ms: u64,
}

impl Default for StreamCheckConfig {
    fn default() -> Self {
        // A reachability probe is a small request against base_url (headers only) and does not wait for
        // model generation, so the timeout is far below the old real-request check (45s -> 8s); the
        // degradation threshold keeps the old 6000ms scale - probe TTFB is normally far below it, so
        // only a genuinely slow probe is marked "slow" instead of a normal one-second latency.
        Self {
            timeout_secs: 8,
            max_retries: 1,
            degraded_threshold_ms: 6000,
        }
    }
}

/// Reachability check result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamCheckResult {
    pub status: HealthStatus,
    pub success: bool,
    pub message: String,
    pub response_time_ms: Option<u64>,
    pub http_status: Option<u16>,
    /// Kept for compatibility with the `stream_check_logs` table layout; always an empty string for the reachability check.
    pub model_used: String,
    pub tested_at: i64,
    pub retry_count: u32,
    /// Fine-grained error classification; the reachability check no longer subdivides, so always None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_category: Option<String>,
}

/// Reachability check service
pub struct StreamCheckService;

impl StreamCheckService {
    /// Run the reachability check (retries only on timeout-like failures).
    ///
    /// `base_url_override`: used for endpoints resolved dynamically by the OAuth manager, and for the
    /// official endpoint the product compatibility layer picks from the audited preset catalog. The caller
    /// is responsible for validation; this service never attaches credentials. Other providers pass
    /// `None` and this service extracts the value from `settings_config`.
    pub async fn check_with_retry(
        app_type: &AppType,
        provider: &Provider,
        config: &StreamCheckConfig,
        base_url_override: Option<String>,
    ) -> Result<StreamCheckResult, AppError> {
        let mut last_result: Option<StreamCheckResult> = None;
        for attempt in 0..=config.max_retries {
            let start = Instant::now();
            let result =
                Self::check_once(app_type, provider, config, base_url_override.clone(), start)
                    .await?;

            if result.success {
                return Ok(StreamCheckResult {
                    retry_count: attempt,
                    ..result
                });
            }

            // Only timeout / abort style network jitter is worth retrying; connection refused, DNS failures and so on return immediately.
            if Self::should_retry(&result.message) && attempt < config.max_retries {
                last_result = Some(result);
                continue;
            }
            return Ok(StreamCheckResult {
                retry_count: attempt,
                ..result
            });
        }

        Ok(last_result.unwrap_or_else(|| StreamCheckResult {
            status: HealthStatus::Failed,
            success: false,
            message: "Check failed".to_string(),
            response_time_ms: None,
            http_status: None,
            model_used: String::new(),
            tested_at: chrono::Utc::now().timestamp(),
            retry_count: config.max_retries,
            error_category: None,
        }))
    }

    /// A single reachability probe.
    async fn check_once(
        app_type: &AppType,
        provider: &Provider,
        config: &StreamCheckConfig,
        base_url_override: Option<String>,
        start: Instant,
    ) -> Result<StreamCheckResult, AppError> {
        let base_url = match base_url_override {
            Some(b) => b,
            None => Self::resolve_base_url(app_type, provider)?,
        };

        let client = crate::proxy::http_client::get_for_url(&base_url);
        let timeout = std::time::Duration::from_secs(config.timeout_secs);
        let ua = Self::custom_user_agent(provider);

        let result = Self::probe_reachability(&client, &base_url, timeout, ua).await;
        let response_time = start.elapsed().as_millis() as u64;
        Ok(Self::build_result(
            result,
            response_time,
            config.degraded_threshold_ms,
        ))
    }

    /// Resolve the provider `base_url`.
    ///
    /// A reachability probe only has to hit the base (origin, or the base path the user configured) -
    /// any HTTP response proves the port is reachable, so unlike the old real-request check there is no
    /// need to resolve a concrete API path
    /// (`/v1/messages` vs `/chat/completions` vs `:streamGenerateContent`).
    ///
    /// Official providers (`category == "official"`) deliberately leave base_url empty (they use the
    /// client default / OAuth endpoint), so the generic resolution stays fail-closed. The product
    /// compatibility layer may resolve a target from the audited preset catalog and pass it through
    /// `base_url_override`, but this upstream service never guesses an official endpoint itself.
    fn resolve_base_url(app_type: &AppType, provider: &Provider) -> Result<String, AppError> {
        if provider.category.as_deref() == Some("official") {
            return Err(AppError::Message(
                "Official providers do not expose a reachability-check target".to_string(),
            ));
        }

        match app_type {
            // Additive-mode apps have a settings_config layout different from Claude/Codex/Gemini;
            // they bypass the adapter and extract base_url according to their own conventions.
            AppType::OpenCode => {
                let npm = Self::extract_opencode_npm(provider);
                Self::resolve_opencode_base_url(provider, npm.as_deref())
            }
            AppType::OpenClaw => Self::extract_openclaw_base_url(provider),
            AppType::Hermes => Self::extract_hermes_base_url(provider),
            AppType::Pi => crate::pi_config::provider_base_url(&provider.settings_config),
            AppType::ClaudeDesktop => ClaudeAdapter::new()
                .extract_base_url(provider)
                .map_err(|e| AppError::Message(format!("Failed to extract base_url: {e}"))),
            _ => get_adapter(app_type)
                .ok_or_else(|| {
                    AppError::InvalidInput(format!(
                        "{} does not support proxy adapters",
                        app_type.as_str()
                    ))
                })?
                .extract_base_url(provider)
                .map_err(|e| AppError::Message(format!("Failed to extract base_url: {e}"))),
        }
    }

    /// Lightweight reachability probe: GET `base_url`; any HTTP response means reachable.
    ///
    /// - `send()` returns as soon as the response headers arrive, so the timing is naturally TTFB; the body is not read.
    /// - reqwest returns `Ok` for every HTTP status code and only network-level errors land in `Err` -
    ///   exactly the "any response counts as reachable, only a failed connection counts as failure" semantics.
    async fn probe_reachability(
        client: &Client,
        base_url: &str,
        timeout: std::time::Duration,
        custom_ua: Option<HeaderValue>,
    ) -> Result<u16, AppError> {
        let url = base_url.trim();
        if url.is_empty() {
            return Err(AppError::Message("base_url is empty".to_string()));
        }

        let mut req = client
            .get(url)
            .timeout(timeout)
            .header("accept", "*/*")
            .header("accept-encoding", "identity");
        // Reuse the provider custom UA (some gateways allow traffic by UA allowlist), consistent with the forwarding path.
        if let Some(ua) = custom_ua {
            req = req.header("user-agent", ua);
        }

        match req.send().await {
            Ok(resp) => Ok(resp.status().as_u16()),
            Err(e) => Err(Self::map_request_error(e)),
        }
    }

    /// Wrap the raw probe outcome into a `StreamCheckResult`.
    fn build_result(
        result: Result<u16, AppError>,
        response_time: u64,
        degraded_threshold_ms: u64,
    ) -> StreamCheckResult {
        let tested_at = chrono::Utc::now().timestamp();
        match result {
            Ok(status) => StreamCheckResult {
                status: Self::determine_status(response_time, degraded_threshold_ms),
                success: true,
                message: "Reachable".to_string(),
                response_time_ms: Some(response_time),
                http_status: Some(status),
                model_used: String::new(),
                tested_at,
                retry_count: 0,
                error_category: None,
            },
            Err(e) => StreamCheckResult {
                status: HealthStatus::Failed,
                success: false,
                message: e.to_string(),
                response_time_ms: Some(response_time),
                http_status: None,
                model_used: String::new(),
                tested_at,
                retry_count: 0,
                error_category: None,
            },
        }
    }

    fn determine_status(latency_ms: u64, threshold: u64) -> HealthStatus {
        if latency_ms <= threshold {
            HealthStatus::Operational
        } else {
            HealthStatus::Degraded
        }
    }

    fn should_retry(msg: &str) -> bool {
        let lower = msg.to_lowercase();
        lower.contains("timeout") || lower.contains("abort") || lower.contains("timed out")
    }

    fn map_request_error(e: reqwest::Error) -> AppError {
        if e.is_timeout() {
            AppError::Message("Request timeout".to_string())
        } else if e.is_connect() {
            AppError::Message(format!("Connection failed: {e}"))
        } else {
            AppError::Message(e.to_string())
        }
    }

    /// Provider-level custom User-Agent (`meta.customUserAgent`), sharing a single convention with the
    /// forwarding path: trimmed, an empty string counts as unset, invalid values are silently ignored (returns `None`).
    fn custom_user_agent(provider: &Provider) -> Option<HeaderValue> {
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.custom_user_agent_header().ok().flatten())
    }

    // ===== base_url extraction per app (the settings_config layouts differ) =====

    /// OpenClaw: `{ baseUrl, apiKey, api, ... }` (camelCase)
    fn extract_openclaw_base_url(provider: &Provider) -> Result<String, AppError> {
        provider
            .settings_config
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::localized(
                    "openclaw_base_url_missing",
                    "OpenClaw provider is missing `baseUrl`",
                )
            })
    }

    /// Hermes: `{ base_url, api_key, api_mode }` (snake_case)
    fn extract_hermes_base_url(provider: &Provider) -> Result<String, AppError> {
        provider
            .settings_config
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::localized(
                    "hermes_base_url_missing",
                    "Hermes provider is missing `base_url`",
                )
            })
    }

    /// OpenCode: `{ npm, options: { baseURL, apiKey }, ... }`
    ///
    /// When the user does not set `options.baseURL` explicitly, fall back to the default endpoint that
    /// ships with the `npm` (AI SDK) package. `@ai-sdk/openai-compatible` has no default and must be explicit.
    fn resolve_opencode_base_url(
        provider: &Provider,
        npm: Option<&str>,
    ) -> Result<String, AppError> {
        if let Some(explicit) = Self::extract_opencode_base_url(provider) {
            return Ok(explicit);
        }

        let fallback = match npm {
            Some("@ai-sdk/openai") => Some("https://api.openai.com/v1"),
            Some("@ai-sdk/anthropic") => Some("https://api.anthropic.com"),
            Some("@ai-sdk/google") => Some("https://generativelanguage.googleapis.com"),
            _ => None,
        };

        fallback.map(|s| s.to_string()).ok_or_else(|| {
            AppError::localized(
                "opencode_base_url_missing",
                "OpenCode provider is missing `options.baseURL` and the SDK package has no default endpoint",
            )
        })
    }

    fn extract_opencode_base_url(provider: &Provider) -> Option<String> {
        provider
            .settings_config
            .get("options")
            .and_then(|v| v.get("baseURL"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn extract_opencode_npm(provider: &Provider) -> Option<String> {
        provider
            .settings_config
            .get("npm")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn make_provider(settings_config: serde_json::Value) -> Provider {
        Provider::with_id(
            "test".to_string(),
            "Test".to_string(),
            settings_config,
            None,
        )
    }

    #[test]
    fn test_default_config_uses_reachability_friendly_values() {
        let config = StreamCheckConfig::default();
        assert_eq!(config.timeout_secs, 8);
        assert_eq!(config.max_retries, 1);
        // The degradation threshold keeps the old scale so a normal latency of a bit over a second is not flagged as "slow"
        assert_eq!(config.degraded_threshold_ms, 6000);
    }

    #[test]
    fn test_determine_status() {
        assert_eq!(
            StreamCheckService::determine_status(1000, 1500),
            HealthStatus::Operational
        );
        assert_eq!(
            StreamCheckService::determine_status(1500, 1500),
            HealthStatus::Operational
        );
        assert_eq!(
            StreamCheckService::determine_status(1501, 1500),
            HealthStatus::Degraded
        );
    }

    #[test]
    fn test_should_retry_only_on_timeout_like_errors() {
        assert!(StreamCheckService::should_retry("Request timeout"));
        assert!(StreamCheckService::should_retry("request timed out"));
        assert!(StreamCheckService::should_retry("connection abort"));
        // Connection refused / DNS failures are not retried
        assert!(!StreamCheckService::should_retry(
            "Connection failed: dns error"
        ));
        assert!(!StreamCheckService::should_retry("Reachable"));
    }

    #[test]
    fn test_build_result_any_http_status_is_reachable() {
        // Every HTTP status code counts as reachable (success=true)
        for status in [200u16, 401, 403, 404, 429, 500, 503] {
            let r = StreamCheckService::build_result(Ok(status), 100, 1500);
            assert!(r.success, "status {status} should be reachable");
            assert_eq!(r.status, HealthStatus::Operational);
            assert_eq!(r.http_status, Some(status));
            assert!(r.model_used.is_empty());
            assert!(r.error_category.is_none());
        }
    }

    #[test]
    fn test_build_result_network_error_is_unreachable() {
        let r = StreamCheckService::build_result(
            Err(AppError::Message("Connection failed: refused".to_string())),
            5,
            1500,
        );
        assert!(!r.success);
        assert_eq!(r.status, HealthStatus::Failed);
        assert!(r.http_status.is_none());
    }

    #[test]
    fn test_build_result_slow_response_is_degraded() {
        let r = StreamCheckService::build_result(Ok(200), 3000, 1500);
        assert!(r.success);
        assert_eq!(r.status, HealthStatus::Degraded);
    }

    #[tokio::test]
    async fn reachability_probe_never_sends_a_credential_header() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local probe server");
        let address = listener.local_addr().expect("read local address");
        let (request_tx, request_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept probe request");
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set request timeout");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            while request.len() < 32 * 1024 {
                let read = socket.read(&mut chunk).expect("read probe request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            request_tx
                .send(String::from_utf8_lossy(&request).into_owned())
                .expect("return captured request");
            socket
                .write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .expect("write probe response");
        });
        let client = Client::builder()
            .no_proxy()
            .build()
            .expect("build local client");

        let status = StreamCheckService::probe_reachability(
            &client,
            &format!("http://{address}"),
            Duration::from_secs(2),
            None,
        )
        .await
        .expect("probe local server");

        assert_eq!(status, 204);
        let request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("capture probe request")
            .to_ascii_lowercase();
        assert!(!request.contains("authorization:"));
        assert!(!request.contains("x-api-key:"));
        server.join().expect("join local probe server");
    }

    #[test]
    fn test_resolve_opencode_base_url_explicit_wins() {
        let p = make_provider(serde_json::json!({
            "npm": "@ai-sdk/openai",
            "options": { "baseURL": "https://proxy.local/v1", "apiKey": "k" },
            "models": {},
        }));
        let resolved =
            StreamCheckService::resolve_opencode_base_url(&p, Some("@ai-sdk/openai")).unwrap();
        assert_eq!(resolved, "https://proxy.local/v1");
    }

    #[test]
    fn test_resolve_opencode_base_url_falls_back_for_known_npm() {
        let p = make_provider(serde_json::json!({
            "npm": "@ai-sdk/anthropic",
            "options": { "apiKey": "k" },
            "models": {},
        }));
        let resolved =
            StreamCheckService::resolve_opencode_base_url(&p, Some("@ai-sdk/anthropic")).unwrap();
        assert_eq!(resolved, "https://api.anthropic.com");
    }

    #[test]
    fn test_resolve_opencode_base_url_errors_for_openai_compatible_without_url() {
        let p = make_provider(serde_json::json!({
            "npm": "@ai-sdk/openai-compatible",
            "options": { "apiKey": "k" },
            "models": {},
        }));
        let result =
            StreamCheckService::resolve_opencode_base_url(&p, Some("@ai-sdk/openai-compatible"));
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_openclaw_base_url_missing_errors() {
        let p = make_provider(serde_json::json!({ "apiKey": "k", "api": "openai-completions" }));
        assert!(StreamCheckService::extract_openclaw_base_url(&p).is_err());

        let p2 = make_provider(serde_json::json!({ "baseUrl": "https://api.deepseek.com/v1" }));
        assert_eq!(
            StreamCheckService::extract_openclaw_base_url(&p2).unwrap(),
            "https://api.deepseek.com/v1"
        );
    }

    #[test]
    fn test_resolve_base_url_uses_explicit_url_or_errors_when_missing() {
        // Explicit base_url -> used directly
        let p = make_provider(
            serde_json::json!({ "env": { "ANTHROPIC_BASE_URL": "https://relay.example/v1" } }),
        );
        assert_eq!(
            StreamCheckService::resolve_base_url(&AppType::Claude, &p).unwrap(),
            "https://relay.example/v1"
        );

        // Missing base_url (official left empty / user forgot) -> error. Official endpoints may only be
        // overridden explicitly by the product compatibility layer from the audited catalog; no fallback
        // here, so a third party with a missing address is never shown a false green light.
        let empty = make_provider(serde_json::json!({ "env": {} }));
        assert!(StreamCheckService::resolve_base_url(&AppType::Claude, &empty).is_err());

        let mut official = make_provider(serde_json::json!({ "auth": {}, "config": "" }));
        official.id = crate::database::CODEX_OFFICIAL_PROVIDER_ID.to_string();
        official.category = Some("official".to_string());
        assert!(StreamCheckService::resolve_base_url(&AppType::Codex, &official).is_err());
    }
}
