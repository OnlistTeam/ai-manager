//! Response processor module
//!
//! Handles streaming and non-streaming API responses uniformly

use super::{
    content_encoding::{decompress_body_with_limit, get_content_encoding, DecompressError},
    forwarder::ActiveConnectionGuard,
    handler_config::{StreamUsageEventFilter, UsageParserConfig},
    handler_context::{RequestContext, StreamingTimeoutConfig},
    hyper_client::{ProxyResponse, MAX_RESPONSE_BODY_BYTES},
    server::ProxyState,
    sse::{strip_sse_field, take_sse_block},
    usage::parser::TokenUsage,
    ProxyError,
};
use crate::database::PRICING_SOURCE_REQUEST;
use axum::http::{header::HeaderMap, HeaderName};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use serde_json::Value;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Mutex;

// ============================================================================
// Response header handling
// ============================================================================

/// Response headers defined by RFC 2616 / RFC 7230 that a proxy must not forward further.
const HOP_BY_HOP_RESPONSE_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "trailers",
    "transfer-encoding",
    "upgrade",
];

/// Strip hop-by-hop response headers, plus any extension headers named in `Connection`.
pub(crate) fn strip_hop_by_hop_response_headers(headers: &mut HeaderMap) {
    let connection_listed_headers: Vec<HeaderName> = headers
        .get_all(axum::http::header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .filter_map(|name| HeaderName::from_bytes(name.as_bytes()).ok())
        .collect();

    for name in HOP_BY_HOP_RESPONSE_HEADERS {
        headers.remove(*name);
    }

    for name in connection_listed_headers {
        headers.remove(name);
    }
}

/// Strip entity headers that would become stale after the response body is rebuilt.
pub(crate) fn strip_entity_headers_for_rebuilt_body(headers: &mut HeaderMap) {
    headers.remove(axum::http::header::CONTENT_ENCODING);
    headers.remove(axum::http::header::CONTENT_LENGTH);
    headers.remove(axum::http::header::TRANSFER_ENCODING);
}

/// Read the response body and decompress it if needed, keeping headers consistent with the returned body.
///
/// `body_timeout`: whole-body timeout. When nonzero, wraps the `.bytes()` call with
/// `tokio::time::timeout` to stop the upstream hanging the request forever by
/// sending headers and then stalling on the body.
/// Pass `Duration::ZERO` to disable the timeout (used when failover is off).
pub(crate) async fn read_decoded_body(
    response: ProxyResponse,
    tag: &str,
    body_timeout: Duration,
) -> Result<(HeaderMap, http::StatusCode, Bytes), ProxyError> {
    let mut headers = response.headers().clone();
    let status = response.status();
    let bytes_future = response.bytes_with_limit(MAX_RESPONSE_BODY_BYTES);
    let raw_bytes = if body_timeout.is_zero() {
        bytes_future.await?
    } else {
        tokio::time::timeout(body_timeout, bytes_future)
            .await
            .map_err(|_| {
                ProxyError::Timeout(format!(
                    "Response body read timed out: {}s (body did not arrive after upstream sent headers)",
                    body_timeout.as_secs()
                ))
            })??
    };

    log::debug!(
        "[{tag}] Received upstream response body: status={}, bytes={}, headers={}",
        status.as_u16(),
        raw_bytes.len(),
        format_headers(&headers)
    );

    let mut body_bytes = raw_bytes.clone();
    let mut decoded = false;

    if let Some(encoding) = get_content_encoding(&headers) {
        log::debug!("[{tag}] Decompressing non-streaming response: content-encoding={encoding}");
        match decompress_body_with_limit(&encoding, &raw_bytes, MAX_RESPONSE_BODY_BYTES) {
            Ok(Some(decompressed)) => {
                // The decoder stops as soon as the budget is exhausted, so this is always <= MAX_RESPONSE_BODY_BYTES
                body_bytes = Bytes::from(decompressed);
                decoded = true;
            }
            // Unsupported encoding: pass through as-is and keep the content-encoding header
            // so downstream diagnostics/clients know these bytes are still compressed
            Ok(None) => {}
            Err(DecompressError::TooLarge { .. }) => {
                return Err(ProxyError::ResponseBodyTooLarge(MAX_RESPONSE_BODY_BYTES));
            }
            Err(DecompressError::Io(e)) => {
                log::warn!("[{tag}] Decompression failed ({encoding}): {e}, using raw data");
            }
        }
    }

    if decoded {
        strip_entity_headers_for_rebuilt_body(&mut headers);
    }

    Ok((headers, status, body_bytes))
}

// ============================================================================
// Public interface
// ============================================================================

/// Detect whether the response is an SSE streaming response
#[inline]
pub fn is_sse_response(response: &ProxyResponse) -> bool {
    response.is_sse()
}

/// Handle a streaming response
pub async fn handle_streaming(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    parser_config: &UsageParserConfig,
    connection_guard: Option<ActiveConnectionGuard>,
) -> Response {
    let status = response.status();
    log::debug!(
        "[{}] Received upstream streaming response: status={}, headers={}",
        ctx.tag,
        status.as_u16(),
        format_headers(response.headers())
    );
    // Check whether the streaming response is compressed (SSE is normally uncompressed; compression breaks SSE parsing)
    if let Some(encoding) = get_content_encoding(response.headers()) {
        log::warn!(
            "[{}] Streaming response has content-encoding={encoding}, SSE parsing may fail. \
             Upstream compressed the SSE stream after passing through accept-encoding.",
            ctx.tag
        );
    }

    let mut response_headers = response.headers().clone();
    strip_hop_by_hop_response_headers(&mut response_headers);

    let mut builder = axum::response::Response::builder().status(status);

    // Copy response headers
    for (key, value) in &response_headers {
        builder = builder.header(key, value);
    }

    // Create byte stream
    let stream = response.bytes_stream();

    // Create the usage collector; skip parsing every SSE event on the streaming hot path when usage logging is off.
    let usage_collector = create_usage_collector(ctx, state, status.as_u16(), parser_config);

    // Get streaming timeout config
    let timeout_config = ctx.streaming_timeout_config();

    // Create a passthrough stream with logging and timeout
    let logged_stream = create_logged_passthrough_stream(
        stream,
        ctx.tag,
        usage_collector,
        timeout_config,
        connection_guard,
    );

    let body = axum::body::Body::from_stream(logged_stream);
    match builder.body(body) {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("[{}] Failed to build streaming response: {e}", ctx.tag);
            ProxyError::Internal(format!("Failed to build streaming response: {e}")).into_response()
        }
    }
}

/// Handle a non-streaming response
pub async fn handle_non_streaming(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    parser_config: &UsageParserConfig,
    // The guard is held within this function's scope and dropped along with the return once the full body has been read
    _connection_guard: Option<ActiveConnectionGuard>,
) -> Result<Response, ProxyError> {
    // Whole-body timeout: only active when failover is enabled and the configured value is nonzero
    let body_timeout =
        if ctx.app_config.auto_failover_enabled && ctx.app_config.non_streaming_timeout > 0 {
            Duration::from_secs(ctx.app_config.non_streaming_timeout as u64)
        } else {
            Duration::ZERO
        };
    let (mut response_headers, status, body_bytes) =
        read_decoded_body(response, ctx.tag, body_timeout).await?;
    strip_hop_by_hop_response_headers(&mut response_headers);

    log::debug!(
        "[{}] Upstream response body received: bytes={} (content omitted)",
        ctx.tag,
        body_bytes.len()
    );

    // Parse and log usage. Skip entirely when usage logging is off, to avoid a full JSON parse of the non-streaming response.
    if usage_logging_enabled(state) {
        if let Ok(json_value) = serde_json::from_slice::<Value>(&body_bytes) {
            // Parse usage
            if let Some(usage) = (parser_config.response_parser)(&json_value) {
                // Attribution priority: model parsed from usage -> response `model` field -> mapped
                // outbound model (routing takeover ground truth) -> client request model. Empty strings count as missing.
                let model = usage
                    .model
                    .clone()
                    .filter(|m| !m.is_empty())
                    .or_else(|| {
                        json_value
                            .get("model")
                            .and_then(|m| m.as_str())
                            .filter(|m| !m.is_empty())
                            .map(str::to_string)
                    })
                    .or_else(|| ctx.outbound_model.clone())
                    .unwrap_or_else(|| ctx.request_model.clone());

                spawn_log_usage(
                    state,
                    ctx,
                    usage,
                    &model,
                    &ctx.request_model,
                    status.as_u16(),
                    false,
                );
            } else {
                let model = json_value
                    .get("model")
                    .and_then(|m| m.as_str())
                    .filter(|m| !m.is_empty())
                    .map(str::to_string)
                    .or_else(|| ctx.outbound_model.clone())
                    .unwrap_or_else(|| ctx.request_model.clone());
                spawn_log_usage(
                    state,
                    ctx,
                    TokenUsage::default(),
                    &model,
                    &ctx.request_model,
                    status.as_u16(),
                    false,
                );
                log::debug!(
                    "[{}] Failed to parse usage info, skipping log",
                    parser_config.app_type_str
                );
            }
        } else {
            log::debug!(
                "[{}] <<< Response (non-JSON): {} bytes",
                ctx.tag,
                body_bytes.len()
            );
            spawn_log_usage(
                state,
                ctx,
                TokenUsage::default(),
                ctx.outbound_model.as_deref().unwrap_or(&ctx.request_model),
                &ctx.request_model,
                status.as_u16(),
                false,
            );
        }
    } else {
        log::debug!(
            "[{}] usage logging is disabled, skipping non-streaming usage parse",
            ctx.tag
        );
    }

    // Build response
    let mut builder = axum::response::Response::builder().status(status);
    for (key, value) in response_headers.iter() {
        builder = builder.header(key, value);
    }

    let body = axum::body::Body::from(body_bytes);
    builder.body(body).map_err(|e| {
        log::error!("[{}] Failed to build response: {e}", ctx.tag);
        ProxyError::Internal(format!("Failed to build response: {e}"))
    })
}

/// General response processing entry point
///
/// Automatically picks streaming or non-streaming handling based on the response type
pub async fn process_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    parser_config: &UsageParserConfig,
    connection_guard: Option<ActiveConnectionGuard>,
) -> Result<Response, ProxyError> {
    if is_sse_response(&response) {
        Ok(handle_streaming(response, ctx, state, parser_config, connection_guard).await)
    } else {
        handle_non_streaming(response, ctx, state, parser_config, connection_guard).await
    }
}

// ============================================================================
// SSE usage collector
// ============================================================================

type UsageCallbackWithTiming = Arc<dyn Fn(Vec<Value>, Option<u64>) + Send + Sync + 'static>;

/// SSE usage collector
#[derive(Clone)]
pub struct SseUsageCollector {
    inner: Arc<SseUsageCollectorInner>,
}

struct SseUsageCollectorInner {
    events: Mutex<Vec<Value>>,
    first_event_time: Mutex<Option<std::time::Instant>>,
    first_event_set: AtomicBool,
    start_time: std::time::Instant,
    on_complete: UsageCallbackWithTiming,
    should_collect: Option<StreamUsageEventFilter>,
    finished: AtomicBool,
}

impl SseUsageCollector {
    /// Create a usage collector; `should_collect` is used to skip usage-irrelevant events on the hot path.
    pub fn new(
        start_time: std::time::Instant,
        should_collect: Option<StreamUsageEventFilter>,
        callback: impl Fn(Vec<Value>, Option<u64>) + Send + Sync + 'static,
    ) -> Self {
        let on_complete: UsageCallbackWithTiming = Arc::new(callback);
        Self {
            inner: Arc::new(SseUsageCollectorInner {
                events: Mutex::new(Vec::new()),
                first_event_time: Mutex::new(None),
                first_event_set: AtomicBool::new(false),
                start_time,
                on_complete,
                should_collect,
                finished: AtomicBool::new(false),
            }),
        }
    }

    pub fn should_collect(&self, data: &str) -> bool {
        self.inner
            .should_collect
            .map(|filter| filter(data))
            .unwrap_or(true)
    }

    /// Mark the timestamp of the first collected SSE event, following `first_token_ms`'s existing approximate semantics.
    async fn mark_first_collected_event_time(&self) {
        if self.inner.first_event_set.load(Ordering::Acquire) {
            return;
        }
        let mut first_time = self.inner.first_event_time.lock().await;
        if first_time.is_none() {
            *first_time = Some(std::time::Instant::now());
            self.inner.first_event_set.store(true, Ordering::Release);
        }
    }

    /// Push an SSE event
    pub async fn push(&self, event: Value) {
        self.mark_first_collected_event_time().await;
        let mut events = self.inner.events.lock().await;
        events.push(event);
    }

    /// Finish collecting and trigger the callback
    pub async fn finish(&self) {
        if self.inner.finished.swap(true, Ordering::SeqCst) {
            return;
        }

        let events = {
            let mut guard = self.inner.events.lock().await;
            std::mem::take(&mut *guard)
        };

        let first_token_ms = {
            let first_time = self.inner.first_event_time.lock().await;
            first_time.map(|t| (t - self.inner.start_time).as_millis() as u64)
        };

        (self.inner.on_complete)(events, first_token_ms);
    }
}

struct SseUsageFinishGuard {
    collector: Option<SseUsageCollector>,
}

impl SseUsageFinishGuard {
    fn new(collector: SseUsageCollector) -> Self {
        Self {
            collector: Some(collector),
        }
    }

    fn disarm(&mut self) {
        self.collector = None;
    }
}

impl Drop for SseUsageFinishGuard {
    fn drop(&mut self) {
        if let Some(collector) = self.collector.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    collector.finish().await;
                });
            } else {
                log::warn!("Tokio runtime unavailable when the SSE usage finish guard fired, skipping async finish");
            }
        }
    }
}

// ============================================================================
// Internal helper functions
// ============================================================================

/// Create a usage collector
pub(crate) fn create_usage_collector(
    ctx: &RequestContext,
    state: &ProxyState,
    status_code: u16,
    parser_config: &UsageParserConfig,
) -> Option<SseUsageCollector> {
    let logging_enabled = state
        .config
        .try_read()
        .map(|c| c.enable_logging)
        .unwrap_or(true);
    if !logging_enabled {
        return None;
    }

    let state = state.clone();
    let provider_id = ctx.provider.id.clone();
    let request_model = ctx.request_model.clone();
    // Attribution fallback when a streaming event lacks a model name: the mapped
    // outbound model (routing takeover ground truth) takes priority, then the client request alias
    let fallback_model = ctx
        .outbound_model
        .clone()
        .unwrap_or_else(|| ctx.request_model.clone());
    // Use ctx's app_type rather than parser_config's: Claude Desktop streaming passthrough reuses
    // CLAUDE_PARSER_CONFIG (app_type_str="claude"); logging by parser_config would attribute
    // claude-desktop rows to claude, so the provider pricing override would fail to resolve.
    let app_type_str = ctx.app_type_str;
    let tag = ctx.tag;
    let start_time = ctx.start_time;
    let stream_parser = parser_config.stream_parser;
    let model_extractor = parser_config.model_extractor;
    let session_id = ctx.session_id.clone();

    Some(SseUsageCollector::new(
        start_time,
        parser_config.stream_event_filter,
        move |events, first_token_ms| {
            if let Some(usage) = stream_parser(&events) {
                let model = model_extractor(&events, &fallback_model);
                let latency_ms = start_time.elapsed().as_millis() as u64;

                let state = state.clone();
                let provider_id = provider_id.clone();
                let session_id = session_id.clone();
                let request_model = request_model.clone();
                let outbound_model = fallback_model.clone();

                tokio::spawn(async move {
                    log_usage_internal(
                        &state,
                        &provider_id,
                        app_type_str,
                        &model,
                        &request_model,
                        &outbound_model,
                        usage,
                        latency_ms,
                        first_token_ms,
                        true, // is_streaming
                        status_code,
                        Some(session_id),
                    )
                    .await;
                });
            } else {
                let model = model_extractor(&events, &fallback_model);
                let latency_ms = start_time.elapsed().as_millis() as u64;
                let state = state.clone();
                let provider_id = provider_id.clone();
                let session_id = session_id.clone();
                let request_model = request_model.clone();
                let outbound_model = fallback_model.clone();

                tokio::spawn(async move {
                    log_usage_internal(
                        &state,
                        &provider_id,
                        app_type_str,
                        &model,
                        &request_model,
                        &outbound_model,
                        TokenUsage::default(),
                        latency_ms,
                        first_token_ms,
                        true, // is_streaming
                        status_code,
                        Some(session_id),
                    )
                    .await;
                });
                log::debug!(
                    "[{tag}] Streaming response is missing usage stats, skipping consumption log"
                );
            }
        },
    ))
}

/// Log usage asynchronously
fn spawn_log_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    usage: TokenUsage,
    model: &str,
    request_model: &str,
    status_code: u16,
    is_streaming: bool,
) {
    // Check enable_logging before spawning the log task
    if let Ok(config) = state.config.try_read() {
        if !config.enable_logging {
            return;
        }
    }

    let state = state.clone();
    let provider_id = ctx.provider.id.clone();
    let app_type_str = ctx.app_type_str.to_string();
    let model = model.to_string();
    let request_model = request_model.to_string();
    // Anchor for "pricing by request" mode: the mapped outbound model, equal to request_model when unmapped
    let outbound_model = ctx
        .outbound_model
        .clone()
        .unwrap_or_else(|| ctx.request_model.clone());
    let latency_ms = ctx.latency_ms();
    let session_id = ctx.session_id.clone();

    tokio::spawn(async move {
        log_usage_internal(
            &state,
            &provider_id,
            &app_type_str,
            &model,
            &request_model,
            &outbound_model,
            usage,
            latency_ms,
            None,
            is_streaming,
            status_code,
            Some(session_id),
        )
        .await;
    });
}

pub(crate) fn usage_logging_enabled(state: &ProxyState) -> bool {
    state
        .config
        .try_read()
        .map(|config| config.enable_logging)
        .unwrap_or(true)
}

/// Internal usage logging function
///
/// `outbound_model` is the anchor for "pricing by request" mode: the model actually
/// sent to upstream (the ground truth after routing takeover mapping, equal to
/// request_model when unmapped). This mode's semantics are "price by what the proxy
/// actually sent, don't trust the upstream echo" -- in a takeover scenario the request
/// sent out is the mapped Y, not the client alias X, so pricing by X would hit the wrong pricing row.
#[allow(clippy::too_many_arguments)]
async fn log_usage_internal(
    state: &ProxyState,
    provider_id: &str,
    app_type: &str,
    model: &str,
    request_model: &str,
    outbound_model: &str,
    usage: TokenUsage,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    is_streaming: bool,
    status_code: u16,
    session_id: Option<String>,
) {
    use super::usage::logger::UsageLogger;

    let logger = UsageLogger::new(&state.db);
    let (multiplier, pricing_model_source) =
        logger.resolve_pricing_config(provider_id, app_type).await;
    let pricing_model = if pricing_model_source == PRICING_SOURCE_REQUEST {
        outbound_model
    } else {
        model
    };

    let dedup_scope = super::usage::parser::dedup_scope_for_app(app_type, provider_id);
    let request_id = usage.dedup_request_id(dedup_scope);

    log::debug!(
        "[{app_type}] Logging request: id={request_id}, provider={provider_id}, model={model}, streaming={is_streaming}, status={status_code}, latency_ms={latency_ms}, first_token_ms={first_token_ms:?}, session={}, input={}, output={}, cache_read={}, cache_creation={}",
        session_id.as_deref().unwrap_or("none"),
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_read_tokens,
        usage.cache_creation_tokens
    );

    if let Err(e) = logger.log_with_calculation(
        request_id,
        provider_id.to_string(),
        app_type.to_string(),
        model.to_string(),
        request_model.to_string(),
        pricing_model.to_string(),
        usage,
        multiplier,
        latency_ms,
        first_token_ms,
        status_code,
        session_id,
        None, // provider_type
        is_streaming,
    ) {
        log::warn!("[USG-001] Failed to log usage: {e}");
    }
}

/// Create a passthrough stream with logging and timeout control
pub fn create_logged_passthrough_stream(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    tag: &'static str,
    usage_collector: Option<SseUsageCollector>,
    timeout_config: StreamingTimeoutConfig,
    connection_guard: Option<ActiveConnectionGuard>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send {
    async_stream::stream! {
        let _conn_guard = connection_guard;
        let mut buffer = String::new();
        let mut utf8_remainder: Vec<u8> = Vec::new();
        let mut collector = usage_collector;
        let mut finish_guard = collector.clone().map(SseUsageFinishGuard::new);
        let inspect_sse_events =
            collector.is_some() || log::log_enabled!(log::Level::Debug);
        let mut is_first_chunk = true;

        // Timeout config
        let first_byte_timeout = if timeout_config.first_byte_timeout > 0 {
            Some(Duration::from_secs(timeout_config.first_byte_timeout))
        } else {
            None
        };
        let idle_timeout = if timeout_config.idle_timeout > 0 {
            Some(Duration::from_secs(timeout_config.idle_timeout))
        } else {
            None
        };

        tokio::pin!(stream);

        loop {
            // Pick the timeout duration: first-byte timeout or idle timeout
            let timeout_duration = if is_first_chunk {
                first_byte_timeout
            } else {
                idle_timeout
            };

            let chunk_result = match timeout_duration {
                Some(duration) => {
                    match tokio::time::timeout(duration, stream.next()).await {
                        Ok(Some(chunk)) => Some(chunk),
                        Ok(None) => None, // stream ended
                        Err(_) => {
                            // Timed out
                            let timeout_type = if is_first_chunk { "first byte" } else { "idle period" };
                            log::error!("[{tag}] Streaming response {} timed out ({}s)", timeout_type, duration.as_secs());
                            yield Err(std::io::Error::other(format!("Streaming response {timeout_type} timed out")));
                            break;
                        }
                    }
                }
                None => stream.next().await, // no timeout limit
            };

            match chunk_result {
                Some(Ok(bytes)) => {
                    if is_first_chunk {
                        log::debug!(
                            "[{tag}] Received upstream streaming first chunk: bytes={}",
                            bytes.len()
                        );
                    }
                    is_first_chunk = false;
                    if inspect_sse_events {
                        crate::proxy::sse::append_utf8_safe(&mut buffer, &mut utf8_remainder, &bytes);

                        // Try to parse and log complete SSE events
                        while let Some(event_text) = take_sse_block(&mut buffer) {
                            if !event_text.trim().is_empty() {
                                // Extract the data portion; only parse JSON when a usage collector is present.
                                for line in event_text.lines() {
                                    if let Some(data) = strip_sse_field(line, "data") {
                                        if data.trim() != "[DONE]" {
                                            let collected = match &collector {
                                                Some(c) if c.should_collect(data) => {
                                                    match serde_json::from_str::<Value>(data) {
                                                        Ok(json_value) => {
                                                            c.push(json_value).await;
                                                            true
                                                        }
                                                        Err(_) => false,
                                                    }
                                                }
                                                _ => false,
                                            };
                                            log::trace!(
                                                "[{tag}] <<< SSE data: bytes={}, usage_collected={collected} (content omitted)",
                                                data.len()
                                            );
                                        } else {
                                            log::debug!("[{tag}] <<< SSE: [DONE]");
                                        }
                                    }
                                }
                            }
                        }
                    }

                    yield Ok(bytes);
                }
                Some(Err(e)) => {
                    log::error!("[{tag}] Stream error: {e}");
                    yield Err(std::io::Error::other(e.to_string()));
                    break;
                }
                None => {
                    // Stream ended normally
                    break;
                }
            }
        }

        if let Some(c) = collector.take() {
            c.finish().await;
        }
        if let Some(guard) = &mut finish_guard {
            guard.disarm();
        }
    }
}

fn is_safe_diagnostic_header(name: &str) -> bool {
    matches!(
        name,
        "content-type"
            | "content-encoding"
            | "content-length"
            | "retry-after"
            | "cf-ray"
            | "x-request-id"
            | "request-id"
            | "x-correlation-id"
    ) || name.starts_with("x-ratelimit-")
        || name.starts_with("ratelimit-")
}

fn bounded_header_value(value: &axum::http::HeaderValue) -> Option<String> {
    let value = value.to_str().ok()?;
    let mut bounded = value.chars().take(160).collect::<String>();
    if value.chars().count() > 160 {
        bounded.push('…');
    }
    Some(bounded)
}

fn format_headers(headers: &HeaderMap) -> String {
    let mut entries = headers
        .keys()
        .map(|key| {
            let name = key.as_str();
            if !is_safe_diagnostic_header(name) {
                return name.to_string();
            }

            let values = headers
                .get_all(key)
                .iter()
                .filter_map(bounded_header_value)
                .collect::<Vec<_>>();
            if values.is_empty() {
                name.to_string()
            } else {
                format!("{name}={}", values.join("|"))
            }
        })
        .collect::<Vec<_>>();
    entries.sort();
    format!("[{}]", entries.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::error::AppError;
    use crate::provider::ProviderMeta;
    use crate::proxy::failover_switch::FailoverSwitchManager;
    use crate::proxy::provider_router::ProviderRouter;
    use crate::proxy::providers::{
        codex_chat_history::CodexChatHistoryStore, gemini_shadow::GeminiShadowStore,
    };
    use crate::proxy::types::{ProxyConfig, ProxyStatus};
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[test]
    fn format_headers_keeps_only_allowlisted_diagnostic_values() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer super-secret".parse().unwrap());
        headers.insert("set-cookie", "session=cookie-secret".parse().unwrap());
        headers.insert("retry-after", "30".parse().unwrap());
        headers.insert("x-ratelimit-remaining", "2".parse().unwrap());
        headers.insert("cf-ray", "abc123-SJC".parse().unwrap());

        let formatted = format_headers(&headers);
        assert!(formatted.contains("authorization"), "{formatted}");
        assert!(formatted.contains("set-cookie"), "{formatted}");
        assert!(formatted.contains("retry-after=30"), "{formatted}");
        assert!(formatted.contains("x-ratelimit-remaining=2"), "{formatted}");
        assert!(formatted.contains("cf-ray=abc123-SJC"), "{formatted}");
        assert!(!formatted.contains("super-secret"), "{formatted}");
        assert!(!formatted.contains("cookie-secret"), "{formatted}");
    }

    #[tokio::test]
    async fn read_decoded_body_rejects_compressed_bomb_without_full_expansion() {
        // gzip of a 128 MiB+1 all-zero payload is only ~130 KiB: the raw read limit alone
        // can't block it. Only bounded decoding on the decompression side can reject it. If
        // decoding degraded to "expand fully, then compare", a payload whose expanded length
        // exceeds MAX_RESPONSE_BODY_BYTES would be returned successfully (test failure).
        let payload = vec![0u8; MAX_RESPONSE_BODY_BYTES + 1];
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &payload).unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(compressed.len() < MAX_RESPONSE_BODY_BYTES);

        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", "gzip".parse().unwrap());
        let response =
            ProxyResponse::buffered(http::StatusCode::OK, headers, Bytes::from(compressed));

        let result = read_decoded_body(response, "test", Duration::ZERO).await;
        assert!(
            matches!(result, Err(ProxyError::ResponseBodyTooLarge(_))),
            "Compression bomb should be rejected instead of fully expanded: {:?}",
            result.map(|(_, _, body)| body.len())
        );
    }

    #[test]
    fn test_strip_sse_field_accepts_optional_space() {
        assert_eq!(
            super::strip_sse_field("data: {\"ok\":true}", "data"),
            Some("{\"ok\":true}")
        );
        assert_eq!(
            super::strip_sse_field("data:{\"ok\":true}", "data"),
            Some("{\"ok\":true}")
        );
        assert_eq!(
            super::strip_sse_field("event: message_start", "event"),
            Some("message_start")
        );
        assert_eq!(
            super::strip_sse_field("event:message_start", "event"),
            Some("message_start")
        );
        assert_eq!(super::strip_sse_field("id:1", "data"), None);
    }

    #[test]
    fn test_strip_hop_by_hop_response_headers_removes_standard_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONNECTION,
            axum::http::HeaderValue::from_static("keep-alive"),
        );
        headers.insert(
            axum::http::header::HeaderName::from_static("keep-alive"),
            axum::http::HeaderValue::from_static("timeout=5"),
        );
        headers.insert(
            axum::http::header::TRANSFER_ENCODING,
            axum::http::HeaderValue::from_static("chunked"),
        );
        headers.insert(
            axum::http::header::HeaderName::from_static("proxy-connection"),
            axum::http::HeaderValue::from_static("keep-alive"),
        );
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            axum::http::HeaderValue::from_static("12"),
        );

        strip_hop_by_hop_response_headers(&mut headers);

        assert!(!headers.contains_key(axum::http::header::CONNECTION));
        assert!(!headers.contains_key("keep-alive"));
        assert!(!headers.contains_key(axum::http::header::TRANSFER_ENCODING));
        assert!(!headers.contains_key("proxy-connection"));
        assert_eq!(
            headers.get(axum::http::header::CONTENT_TYPE),
            Some(&axum::http::HeaderValue::from_static("application/json"))
        );
        assert_eq!(
            headers.get(axum::http::header::CONTENT_LENGTH),
            Some(&axum::http::HeaderValue::from_static("12"))
        );
    }

    #[test]
    fn test_strip_hop_by_hop_response_headers_removes_connection_listed_extensions() {
        let mut headers = HeaderMap::new();
        headers.append(
            axum::http::header::CONNECTION,
            axum::http::HeaderValue::from_static("x-trace-hop, x-debug-hop"),
        );
        headers.append(
            axum::http::header::CONNECTION,
            axum::http::HeaderValue::from_static("upgrade"),
        );
        headers.insert(
            axum::http::header::HeaderName::from_static("x-trace-hop"),
            axum::http::HeaderValue::from_static("trace"),
        );
        headers.insert(
            axum::http::header::HeaderName::from_static("x-debug-hop"),
            axum::http::HeaderValue::from_static("debug"),
        );
        headers.insert(
            axum::http::header::UPGRADE,
            axum::http::HeaderValue::from_static("websocket"),
        );
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("text/event-stream"),
        );

        strip_hop_by_hop_response_headers(&mut headers);

        assert!(!headers.contains_key(axum::http::header::CONNECTION));
        assert!(!headers.contains_key("x-trace-hop"));
        assert!(!headers.contains_key("x-debug-hop"));
        assert!(!headers.contains_key(axum::http::header::UPGRADE));
        assert_eq!(
            headers.get(axum::http::header::CONTENT_TYPE),
            Some(&axum::http::HeaderValue::from_static("text/event-stream"))
        );
    }

    fn build_state(db: Arc<Database>) -> ProxyState {
        ProxyState {
            db: db.clone(),
            config: Arc::new(RwLock::new(ProxyConfig::default())),
            status: Arc::new(RwLock::new(ProxyStatus::default())),
            start_time: Arc::new(RwLock::new(None)),
            current_providers: Arc::new(RwLock::new(HashMap::new())),
            provider_router: Arc::new(ProviderRouter::new(db.clone())),
            gemini_shadow: Arc::new(GeminiShadowStore::default()),
            codex_chat_history: Arc::new(CodexChatHistoryStore::default()),
            app_handle: None,
            failover_manager: Arc::new(FailoverSwitchManager::new(db)),
        }
    }

    fn seed_pricing(db: &Database) -> Result<(), AppError> {
        let conn = crate::database::lock_conn!(db.conn);
        conn.execute(
            "INSERT OR REPLACE INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["resp-model", "Resp Model", "1.0", "0"],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["req-model", "Req Model", "2.0", "0"],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    fn insert_provider(
        db: &Database,
        id: &str,
        app_type: &str,
        meta: ProviderMeta,
    ) -> Result<(), AppError> {
        let meta_json =
            serde_json::to_string(&meta).map_err(|e| AppError::Database(e.to_string()))?;
        let conn = crate::database::lock_conn!(db.conn);
        conn.execute(
            "INSERT INTO providers (id, app_type, name, settings_config, meta)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, app_type, "Test Provider", "{}", meta_json],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    #[tokio::test]
    async fn test_log_usage_uses_provider_override_config() -> Result<(), AppError> {
        let db = Arc::new(Database::memory()?);
        let app_type = "claude";

        db.set_default_cost_multiplier(app_type, "1.5").await?;
        db.set_pricing_model_source(app_type, "response").await?;
        seed_pricing(&db)?;

        let meta = ProviderMeta {
            cost_multiplier: Some("2".to_string()),
            pricing_model_source: Some("request".to_string()),
            ..ProviderMeta::default()
        };
        insert_provider(&db, "provider-1", app_type, meta)?;

        let state = build_state(db.clone());
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            model: None,
            message_id: None,
        };

        log_usage_internal(
            &state,
            "provider-1",
            app_type,
            "resp-model",
            "req-model",
            "req-model",
            usage,
            10,
            None,
            false,
            200,
            None,
        )
        .await;

        let conn = crate::database::lock_conn!(db.conn);
        let (model, request_model, total_cost, cost_multiplier): (String, String, String, String) =
            conn.query_row(
                "SELECT model, request_model, total_cost_usd, cost_multiplier
                 FROM proxy_request_logs WHERE provider_id = ?1",
                ["provider-1"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        assert_eq!(model, "resp-model");
        assert_eq!(request_model, "req-model");
        assert_eq!(
            Decimal::from_str(&cost_multiplier).unwrap(),
            Decimal::from_str("2").unwrap()
        );
        assert_eq!(
            Decimal::from_str(&total_cost).unwrap(),
            Decimal::from_str("4").unwrap()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_request_pricing_mode_anchors_to_outbound_model() -> Result<(), AppError> {
        let db = Arc::new(Database::memory()?);
        let app_type = "claude";

        db.set_pricing_model_source(app_type, "request").await?;
        seed_pricing(&db)?;
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute(
                "INSERT OR REPLACE INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million)
                 VALUES ('outbound-model', 'Outbound Model', '4.0', '0')",
                [],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        insert_provider(&db, "provider-3", app_type, ProviderMeta::default())?;

        let state = build_state(db.clone());
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            model: None,
            message_id: None,
        };

        // Routing takeover scenario: client requests req-model ($2/M), the proxy actually
        // sends outbound-model ($4/M), upstream echoes resp-model. "Pricing by request" must anchor to the model actually sent.
        log_usage_internal(
            &state,
            "provider-3",
            app_type,
            "resp-model",
            "req-model",
            "outbound-model",
            usage,
            10,
            None,
            false,
            200,
            None,
        )
        .await;

        let conn = crate::database::lock_conn!(db.conn);
        let (model, request_model, total_cost): (String, String, String) = conn
            .query_row(
                "SELECT model, request_model, total_cost_usd
                 FROM proxy_request_logs WHERE provider_id = ?1",
                ["provider-3"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        // The model / request_model columns are unaffected by the pricing anchor
        assert_eq!(model, "resp-model");
        assert_eq!(request_model, "req-model");
        // Priced by outbound-model ($4/M), not req-model ($2/M) or resp-model ($1/M)
        assert_eq!(
            Decimal::from_str(&total_cost).unwrap(),
            Decimal::from_str("4").unwrap()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_claude_desktop_inherits_claude_global_defaults() -> Result<(), AppError> {
        use crate::proxy::usage::logger::UsageLogger;

        let db = Arc::new(Database::memory()?);

        // The global billing config only has three rows: claude/codex/gemini; claude-desktop's
        // global default must inherit from claude, not silently fall back to the factory default (1 / response)
        db.set_default_cost_multiplier("claude", "1.5").await?;
        db.set_pricing_model_source("claude", "request").await?;

        let logger = UsageLogger::new(&db);
        let (multiplier, source) = logger
            .resolve_pricing_config("nonexistent-provider", "claude-desktop")
            .await;

        assert_eq!(multiplier, Decimal::from_str("1.5").unwrap());
        assert_eq!(source, "request");
        Ok(())
    }

    #[tokio::test]
    async fn test_log_usage_falls_back_to_global_defaults() -> Result<(), AppError> {
        let db = Arc::new(Database::memory()?);
        let app_type = "claude";

        db.set_default_cost_multiplier(app_type, "1.5").await?;
        db.set_pricing_model_source(app_type, "response").await?;
        seed_pricing(&db)?;

        let meta = ProviderMeta::default();
        insert_provider(&db, "provider-2", app_type, meta)?;

        let state = build_state(db.clone());
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            model: None,
            message_id: None,
        };

        log_usage_internal(
            &state,
            "provider-2",
            app_type,
            "resp-model",
            "req-model",
            "req-model",
            usage,
            10,
            None,
            false,
            200,
            None,
        )
        .await;

        let conn = crate::database::lock_conn!(db.conn);
        let (total_cost, cost_multiplier): (String, String) = conn
            .query_row(
                "SELECT total_cost_usd, cost_multiplier
                 FROM proxy_request_logs WHERE provider_id = ?1",
                ["provider-2"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        assert_eq!(
            Decimal::from_str(&cost_multiplier).unwrap(),
            Decimal::from_str("1.5").unwrap()
        );
        assert_eq!(
            Decimal::from_str(&total_cost).unwrap(),
            Decimal::from_str("1.5").unwrap()
        );
        Ok(())
    }
}
