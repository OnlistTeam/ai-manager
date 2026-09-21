//! Request handlers
//!
//! Handles HTTP requests for the various API endpoints
//!
//! Structure after the refactor:
//! - shared logic lives in the `handler_context` and `response_processor` modules
//! - each handler keeps only its own business logic
//! - Claude's format conversion stays in this file (for the legacy OpenRouter fallback)

use super::{
    content_encoding::{decompress_body, get_content_encoding, is_supported_content_encoding},
    error_mapper::{get_error_message, map_proxy_error_to_status},
    forwarder::ActiveConnectionGuard,
    handler_config::{
        claude_stream_usage_event_filter, codex_stream_usage_event_filter, CLAUDE_PARSER_CONFIG,
        CODEX_PARSER_CONFIG, GEMINI_PARSER_CONFIG, OPENAI_PARSER_CONFIG,
    },
    handler_context::RequestContext,
    providers::{
        codex_chat_common::extract_reasoning_field_text,
        codex_chat_history::record_responses_sse_stream,
        get_adapter, get_claude_api_format,
        streaming::create_anthropic_sse_stream,
        streaming_codex_anthropic::{
            create_responses_sse_stream_from_anthropic_with_context,
            responses_sse_events_from_anthropic_message,
        },
        streaming_codex_chat::create_responses_sse_stream_from_chat_with_context,
        streaming_gemini::create_anthropic_sse_stream_from_gemini,
        streaming_responses::{
            create_anthropic_sse_stream_from_responses,
            create_anthropic_sse_stream_from_responses_with_web_search_options,
        },
        transform, transform_codex_anthropic, transform_codex_chat,
        transform_codex_responses_namespace, transform_codex_responses_xai_sanitize,
        transform_gemini, transform_responses,
    },
    response_processor::{
        create_logged_passthrough_stream, create_usage_collector, process_response,
        read_decoded_body, strip_entity_headers_for_rebuilt_body,
        strip_hop_by_hop_response_headers, usage_logging_enabled, SseUsageCollector,
    },
    server::ProxyState,
    sse::{strip_sse_field, take_sse_block},
    types::*,
    usage::parser::TokenUsage,
    ProxyError,
};
use crate::app_config::AppType;
use crate::database::PRICING_SOURCE_REQUEST;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use bytes::Bytes;
use futures::StreamExt;
use http_body_util::BodyExt;
use serde_json::{json, Value};

// ============================================================================
// Health check and status queries (simple endpoints)
// ============================================================================

/// Health check
pub async fn health_check() -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
}

/// Returns the service status
pub async fn get_status(State(state): State<ProxyState>) -> Result<Json<ProxyStatus>, ProxyError> {
    let status = state.status.read().await.clone();
    Ok(Json(status))
}

/// GET /v1/models — Codex model list (reachability check)
///
/// Codex CLI probes this endpoint at startup and deserializes the response as a
/// catalog with a top-level `models` field.  Return the cc-switch–managed model
/// catalog file directly so the format always matches what the current version
/// of Codex expects.
///
/// Only serves the catalog when the live config.toml still references the
/// cc-switch–owned `model_catalog_json`, using the same path ownership rules as
/// Codex live-setting import.
pub async fn handle_models() -> Result<Json<Value>, ProxyError> {
    let config_dir = crate::codex_config::get_codex_config_dir();
    let active_catalog_path = match crate::codex_config::read_codex_config_text() {
        Ok(config_text) => {
            crate::codex_config::resolve_cc_switch_catalog_path(&config_text, &config_dir)
        }
        Err(_) => None,
    };

    let catalog = if let Some(catalog_path) =
        active_catalog_path.as_ref().filter(|path| path.exists())
    {
        match crate::codex_config::read_codex_model_catalog_text(catalog_path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or(json!({"models": []})),
            Err(error) => {
                log::warn!("[models] refused an out-of-bounds or oversized catalog file: {error}");
                json!({"models": []})
            }
        }
    } else {
        if active_catalog_path.is_none() {
            log::debug!(
                "[models] stale guard: catalog not served (model_catalog_json not set to cc-switch catalog)"
            );
        }
        json!({"models": []})
    };
    Ok(Json(catalog))
}

// ============================================================================
// Claude API handlers (including format conversion)
// ============================================================================

/// Handles /v1/messages requests (Claude API)
///
/// The Claude handler carries its own format conversion:
/// - it used to serve OpenRouter's OpenAI Chat Completions compatible API (Anthropic/OpenAI conversion)
/// - OpenRouter now ships a Claude Code compatible API, so the conversion is off by default (kept for fallback)
pub async fn handle_messages(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    handle_messages_for_app(state, request, AppType::Claude, "Claude", "claude", None).await
}

pub async fn handle_claude_desktop_messages(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    validate_claude_desktop_gateway_auth(&state, request.headers())?;
    handle_messages_for_app(
        state,
        request,
        AppType::ClaudeDesktop,
        "Claude Desktop",
        "claude-desktop",
        Some("/claude-desktop"),
    )
    .await
}

pub async fn handle_claude_desktop_models(
    State(state): State<ProxyState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>, ProxyError> {
    validate_claude_desktop_gateway_auth(&state, &headers)?;
    let providers = state
        .provider_router
        .select_providers("claude-desktop")
        .await
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
    let provider = providers.first().ok_or(ProxyError::NoAvailableProvider)?;
    let response = crate::claude_desktop_config::model_list_response(provider)
        .map_err(|e| ProxyError::ConfigError(e.to_string()))?;
    Ok(Json(response))
}

async fn handle_messages_for_app(
    state: ProxyState,
    request: axum::extract::Request,
    app_type: AppType,
    tag: &'static str,
    app_type_str: &'static str,
    strip_prefix: Option<&'static str>,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, body) = request.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri;
    let headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Internal(format!("Failed to parse request body: {e}")))?;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, app_type.clone(), tag, app_type_str).await?;

    let raw_endpoint = uri
        .path_and_query()
        .map(|path_and_query| path_and_query.as_str())
        .unwrap_or(uri.path());
    let endpoint = strip_prefix
        .and_then(|prefix| raw_endpoint.strip_prefix(prefix))
        .unwrap_or(raw_endpoint);

    let is_stream = body
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    // Forward the request
    let forwarder = ctx.create_forwarder(&state);
    let mut result = match forwarder
        .forward_with_retry(
            &app_type,
            method,
            endpoint,
            body.clone(),
            headers,
            extensions,
            ctx.get_providers(),
        )
        .await
    {
        Ok(result) => result,
        Err(mut err) => {
            if let Some(provider) = err.provider.take() {
                ctx.provider = provider;
            }
            log_forward_error(&state, &ctx, is_stream, &err.error);
            return Err(err.error);
        }
    };

    let connection_guard = result.connection_guard.take();
    ctx.outbound_model = result.outbound_model.take();
    ctx.provider = result.provider;
    let api_format = result
        .claude_api_format
        .as_deref()
        .unwrap_or_else(|| get_claude_api_format(&ctx.provider))
        .to_string();
    let response = result.response;

    // Check whether format conversion is needed (relays such as OpenRouter)
    let adapter = get_adapter(&app_type).ok_or_else(|| {
        ProxyError::ConfigError(format!(
            "{} does not support proxy routing",
            app_type.as_str()
        ))
    })?;
    let needs_transform = adapter.needs_transform(&ctx.provider);

    // Claude only: format conversion handling
    if needs_transform {
        return handle_claude_transform(
            response,
            &ctx,
            &state,
            &body,
            is_stream,
            &api_format,
            connection_guard,
        )
        .await;
    }

    // Generic response handling (pass-through mode)
    process_response(
        response,
        &ctx,
        &state,
        &CLAUDE_PARSER_CONFIG,
        connection_guard,
    )
    .await
}

fn validate_claude_desktop_gateway_auth(
    state: &ProxyState,
    headers: &axum::http::HeaderMap,
) -> Result<(), ProxyError> {
    let expected = crate::claude_desktop_config::get_or_create_gateway_token(state.db.as_ref())
        .map_err(|e| ProxyError::AuthError(e.to_string()))?;
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Err(ProxyError::AuthError(
            "Claude Desktop gateway request is missing the Authorization header".to_string(),
        ));
    };
    let value = value
        .to_str()
        .map_err(|_| ProxyError::AuthError("Invalid Authorization header format".to_string()))?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .unwrap_or("")
        .trim();
    if token != expected {
        return Err(ProxyError::AuthError(
            "Invalid Claude Desktop gateway token".to_string(),
        ));
    }
    Ok(())
}

/// Claude format conversion handling (specific to this handler)
///
/// Supports conversion for both the OpenAI Chat Completions and Responses API formats
struct ClaudeUsageLog {
    model: String,
    request_model: String,
    outbound_model: String,
    app_type: &'static str,
    provider_id: String,
    session_id: String,
    usage: TokenUsage,
    latency_ms: u64,
    status_code: u16,
    is_streaming: bool,
}

fn prepare_claude_usage_log(
    ctx: &RequestContext,
    response: &Value,
    status_code: u16,
    is_streaming: bool,
) -> Option<ClaudeUsageLog> {
    let usage =
        TokenUsage::from_claude_response(response).filter(TokenUsage::has_billable_tokens)?;

    let model = response
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .or_else(|| ctx.outbound_model.clone())
        .unwrap_or_else(|| ctx.request_model.clone());

    Some(ClaudeUsageLog {
        model,
        request_model: ctx.request_model.clone(),
        outbound_model: ctx
            .outbound_model
            .clone()
            .unwrap_or_else(|| ctx.request_model.clone()),
        app_type: ctx.app_type_str,
        provider_id: ctx.provider.id.clone(),
        session_id: ctx.session_id.clone(),
        usage,
        latency_ms: ctx.latency_ms(),
        status_code,
        is_streaming,
    })
}

async fn write_claude_usage_log(state: &ProxyState, log: ClaudeUsageLog) {
    log_usage(
        state,
        &log.provider_id,
        log.app_type,
        &log.model,
        &log.request_model,
        &log.outbound_model,
        log.usage,
        log.latency_ms,
        None,
        log.is_streaming,
        log.status_code,
        Some(log.session_id),
    )
    .await;
}

fn spawn_claude_usage_log(
    state: &ProxyState,
    ctx: &RequestContext,
    response: &Value,
    status_code: u16,
    is_streaming: bool,
) {
    if !usage_logging_enabled(state) {
        return;
    }
    let Some(log) = prepare_claude_usage_log(ctx, response, status_code, is_streaming) else {
        return;
    };
    let state = state.clone();
    tokio::spawn(async move {
        write_claude_usage_log(&state, log).await;
    });
}

async fn handle_claude_transform(
    response: super::hyper_client::ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    original_body: &Value,
    is_stream: bool,
    api_format: &str,
    connection_guard: Option<ActiveConnectionGuard>,
) -> Result<axum::response::Response, ProxyError> {
    let status = response.status();
    let is_codex_oauth = ctx
        .provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        == Some("codex_oauth");
    // Codex OAuth upgrades openai_responses replies to SSE even when the client sent stream:false.
    // By default should_use_claude_transform_streaming routes that combination to the streaming
    // converter, which avoids a 422 from JSON parsing but hands a non-streaming client
    // text/event-stream, violating Anthropic's non-streaming semantics. This override aggregates the
    // upstream SSE into Anthropic JSON for exactly that combination; every other case (any upstream is_sse, non-Codex OAuth, etc.) keeps the old streaming fallback.
    let aggregate_codex_oauth_responses_sse =
        !is_stream && is_codex_oauth && api_format == "openai_responses";
    let use_streaming = if aggregate_codex_oauth_responses_sse {
        false
    } else {
        should_use_claude_transform_streaming(
            is_stream,
            response.is_sse(),
            api_format,
            is_codex_oauth,
        )
    };
    let tool_schema_hints = transform_gemini::extract_anthropic_tool_schema_hints(original_body);
    let tool_schema_hints = (!tool_schema_hints.is_empty()).then_some(tool_schema_hints);
    let hosted_web_search_name =
        transform_responses::anthropic_web_search_tool_name(original_body).map(ToString::to_string);
    let hosted_web_search_max_uses =
        transform_responses::anthropic_web_search_max_uses(original_body);

    if use_streaming {
        // Pick the streaming converter based on api_format
        let stream = response.bytes_stream();
        let sse_stream: Box<
            dyn futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + Unpin,
        > = if api_format == "openai_responses" {
            if hosted_web_search_name.is_none() && hosted_web_search_max_uses.is_none() {
                Box::new(Box::pin(create_anthropic_sse_stream_from_responses(stream)))
            } else {
                Box::new(Box::pin(
                    create_anthropic_sse_stream_from_responses_with_web_search_options(
                        stream,
                        hosted_web_search_name.clone(),
                        hosted_web_search_max_uses,
                    ),
                ))
            }
        } else if api_format == "gemini_native" {
            Box::new(Box::pin(create_anthropic_sse_stream_from_gemini(
                stream,
                Some(state.gemini_shadow.clone()),
                Some(ctx.provider.id.clone()),
                Some(ctx.session_id.clone()),
                tool_schema_hints.clone(),
            )))
        } else {
            Box::new(Box::pin(create_anthropic_sse_stream(stream)))
        };

        // Create the usage collector; with usage logging off, do not parse the converted SSE.
        let usage_collector = if usage_logging_enabled(state) {
            let state = state.clone();
            let provider_id = ctx.provider.id.clone();
            let request_model = ctx.request_model.clone();
            // When neither upstream nor the transform layer echoes a model, prefer the mapped outbound model
            // (the truth under routing takeover), then the client's request alias. An empty string counts as missing (the converter synthesizes "" for upstreams that do not echo).
            let fallback_model = ctx
                .outbound_model
                .clone()
                .unwrap_or_else(|| ctx.request_model.clone());
            let status_code = status.as_u16();
            let start_time = ctx.start_time;
            let session_id = ctx.session_id.clone();
            // Use the app_type from ctx: the Claude Desktop gateway also takes this conversion path, and
            // hardcoding "claude" would file claude-desktop rows under claude
            let app_type_str = ctx.app_type_str;

            Some(SseUsageCollector::new(
                start_time,
                Some(claude_stream_usage_event_filter),
                move |events, first_token_ms| {
                    if let Some(usage) = TokenUsage::from_claude_stream_events(&events) {
                        let model = usage
                            .model
                            .clone()
                            .filter(|m| !m.is_empty())
                            .unwrap_or_else(|| fallback_model.clone());
                        let latency_ms = start_time.elapsed().as_millis() as u64;
                        let state = state.clone();
                        let provider_id = provider_id.clone();
                        let session_id = session_id.clone();
                        let request_model = request_model.clone();
                        let outbound_model = fallback_model.clone();

                        tokio::spawn(async move {
                            log_usage(
                                &state,
                                &provider_id,
                                app_type_str,
                                &model,
                                &request_model,
                                &outbound_model,
                                usage,
                                latency_ms,
                                first_token_ms,
                                true,
                                status_code,
                                Some(session_id),
                            )
                            .await;
                        });
                    } else {
                        log::debug!("[Claude] the OpenRouter streaming response has no usage stats, skipping the consumption record");
                    }
                },
            ))
        } else {
            None
        };

        // Read the streaming timeout configuration
        let timeout_config = ctx.streaming_timeout_config();

        let logged_stream = create_logged_passthrough_stream(
            sse_stream,
            "Claude/OpenRouter",
            usage_collector,
            timeout_config,
            connection_guard,
        );

        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "Content-Type",
            axum::http::HeaderValue::from_static("text/event-stream"),
        );
        headers.insert(
            "Cache-Control",
            axum::http::HeaderValue::from_static("no-cache"),
        );

        let body = axum::body::Body::from_stream(logged_stream);
        return Ok((headers, body).into_response());
    }

    // Non-streaming response conversion (OpenAI/Responses -> Anthropic)
    let body_timeout =
        if ctx.app_config.auto_failover_enabled && ctx.app_config.non_streaming_timeout > 0 {
            std::time::Duration::from_secs(ctx.app_config.non_streaming_timeout as u64)
        } else {
            std::time::Duration::ZERO
        };
    let enforce_codex_web_search_limit_while_aggregating =
        aggregate_codex_oauth_responses_sse && hosted_web_search_max_uses.is_some();
    let (mut response_headers, direct_anthropic_response, upstream_response) =
        if enforce_codex_web_search_limit_while_aggregating {
            if let Some(encoding) = get_content_encoding(response.headers()) {
                // Transformed requests advertise `accept-encoding: identity`.
                // If an upstream ignores that contract, fail closed rather than
                // buffering a compressed stream and losing early cancellation.
                return Err(ProxyError::TransformError(format!(
                    "Cannot enforce Anthropic WebSearch max_uses on a compressed Codex SSE response ({encoding})"
                )));
            }
            let response_headers = response.headers().clone();
            let message = responses_sse_stream_to_anthropic_message(
                response.bytes_stream(),
                hosted_web_search_name.clone(),
                hosted_web_search_max_uses,
                body_timeout,
            )
            .await?;
            (response_headers, Some(message), None)
        } else {
            let (response_headers, _status, body_bytes) =
                read_decoded_body(response, ctx.tag, body_timeout).await?;
            let body_str = String::from_utf8_lossy(&body_bytes);
            let upstream_response = if aggregate_codex_oauth_responses_sse {
                responses_sse_to_response_value(&body_str)?
            } else {
                match serde_json::from_slice(&body_bytes) {
                    Ok(value) => value,
                    // Fallback sniffing (#2234): some gateways return an SSE body for stream:false while labelling
                    // Content-Type as application/json or similar, defeating the header check in is_sse().
                    // In that case aggregate the SSE into a single JSON and run the existing non-streaming converter,
                    // so the client still gets Anthropic JSON with unchanged semantics. gemini_native has no aggregator yet and falls through to a diagnostic error.
                    Err(_) if body_looks_like_sse(&body_str) && api_format != "gemini_native" => {
                        log::warn!(
                            "[Claude] upstream returned an unlabelled SSE body for a non-streaming request (api_format={api_format}), falling back to SSE aggregation"
                        );
                        let aggregated = if api_format == "openai_responses" {
                            responses_sse_to_response_value(&body_str)
                        } else {
                            chat_sse_to_response_value(&body_str)
                        };
                        // If aggregation also fails, the server log records only the length while the client error carries
                        // the same field diagnostics (content-type and body classification); otherwise users hitting the
                        // sniffing arm get only a bare aggregation error and lose the diagnostics the other arm already has (C7)
                        aggregated.map_err(|e| {
                            log::error!(
                                "[Claude] SSE aggregation fallback failed: {e}, body_bytes={}",
                                body_bytes.len()
                            );
                            aggregate_fallback_error(e, &response_headers, &body_str)
                        })?
                    }
                    Err(e) => {
                        log::error!(
                            "[Claude] failed to parse the upstream response: {e}, body_bytes={}",
                            body_bytes.len()
                        );
                        return Err(upstream_body_parse_error(
                            "Failed to parse upstream response",
                            &e,
                            &response_headers,
                            &body_str,
                        ));
                    }
                }
            };
            (response_headers, None, Some(upstream_response))
        };

    // Preserve usage so a post-upstream conversion failure still records tokens.
    // The direct Anthropic branch below is already fully transformed and cannot
    // enter the conversion-error path. Snapshot usage only for raw upstream
    // responses that still need conversion; cloning the direct message would
    // duplicate potentially large text and search-result content.
    let raw_usage_response = upstream_response.as_ref().map(|response| {
        json!({
            "id": response.get("id").cloned().unwrap_or(Value::Null),
            "model": response.get("model").cloned().unwrap_or(Value::Null),
            "usage": transform_responses::build_anthropic_usage_from_responses(
                response.get("usage")
            )
        })
    });

    // Pick the non-streaming converter based on api_format
    let transform_result = match (direct_anthropic_response, upstream_response) {
        (Some(response), _) => Ok(response),
        (None, Some(response)) if api_format == "openai_responses" => {
            transform_responses::responses_to_anthropic_with_web_search_options(
                response,
                hosted_web_search_name.as_deref(),
                hosted_web_search_max_uses,
            )
        }
        (None, Some(response)) if api_format == "gemini_native" => {
            transform_gemini::gemini_to_anthropic_with_shadow_and_hints(
                response,
                Some(state.gemini_shadow.as_ref()),
                Some(&ctx.provider.id),
                Some(&ctx.session_id),
                tool_schema_hints.as_ref(),
            )
        }
        (None, Some(response)) => transform::openai_to_anthropic(response),
        (None, None) => Err(ProxyError::Internal(
            "Missing upstream response after Claude format conversion".to_string(),
        )),
    };
    let anthropic_response = match transform_result {
        Ok(response) => response,
        Err(error) => {
            log::error!("[Claude] response conversion failed: {error}");
            if usage_logging_enabled(state) {
                if let Some(log) = raw_usage_response.as_ref().and_then(|response| {
                    prepare_claude_usage_log(ctx, response, status.as_u16(), false)
                }) {
                    // The upstream request already succeeded and consumed tokens. Persist
                    // usage before returning the terminal transform error to the client.
                    write_claude_usage_log(state, log).await;
                }
            }
            return Err(error);
        }
    };

    // Record usage
    // All-zero usage is not recorded (matching the skip in the Codex streaming collector): a stream
    // rescued by SSE aggregation has no usage when upstream lacks stream_options.include_usage, so writing would only add a meaningless empty row
    spawn_claude_usage_log(state, ctx, &anthropic_response, status.as_u16(), false);

    // Build the response
    let mut builder = axum::response::Response::builder().status(status);
    strip_entity_headers_for_rebuilt_body(&mut response_headers);
    strip_hop_by_hop_response_headers(&mut response_headers);
    // Builder::header appends, so without a remove first the upstream Content-Type would be sent twice.
    response_headers.remove(axum::http::header::CONTENT_TYPE);

    for (key, value) in response_headers.iter() {
        builder = builder.header(key, value);
    }

    builder = builder.header(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );

    let response_body = serde_json::to_vec(&anthropic_response).map_err(|e| {
        log::error!("[Claude] failed to serialize the response: {e}");
        ProxyError::TransformError(format!("Failed to serialize response: {e}"))
    })?;

    let body = axum::body::Body::from(response_body);
    builder.body(body).map_err(|e| {
        log::error!("[Claude] failed to build the response: {e}");
        ProxyError::Internal(format!("Failed to build response: {e}"))
    })
}

fn endpoint_with_query(uri: &axum::http::Uri, endpoint: &str) -> String {
    match uri.query() {
        Some(query) => format!("{endpoint}?{query}"),
        None => endpoint.to_string(),
    }
}

/// Codex clients (especially a signed-in Desktop) may zstd-compress the request body, which makes
/// a later `serde_json::from_slice` fail outright. Decompress before parsing and strip the now
/// stale entity headers (content-encoding / content-length / transfer-encoding); the forwarding
/// layer regenerates correct headers from the decompressed plaintext JSON.
fn decode_codex_request_body(
    headers: &mut axum::http::HeaderMap,
    body_bytes: Bytes,
) -> Result<Bytes, ProxyError> {
    let Some(encoding) = get_content_encoding(headers) else {
        return Ok(body_bytes);
    };

    if !is_supported_content_encoding(&encoding) {
        return Err(ProxyError::InvalidRequest(format!(
            "Unsupported request content-encoding: {encoding}"
        )));
    }

    log::debug!("[Codex] decompressing the request body: content-encoding={encoding}");
    let decompressed = match decompress_body(&encoding, &body_bytes) {
        Ok(Some(decompressed)) => decompressed,
        // is_supported_content_encoding already guaranteed the encoding is supported, so None should be
        // impossible; defensively, better to error than to pass compressed bytes on as JSON.
        Ok(None) => {
            return Err(ProxyError::InvalidRequest(format!(
                "Unsupported request content-encoding: {encoding}"
            )));
        }
        Err(e) => {
            log::warn!("[Codex] request body decompression failed ({encoding}): {e}");
            return Err(ProxyError::InvalidRequest(format!(
                "Failed to decompress request body ({encoding}): {e}"
            )));
        }
    };

    headers.remove(axum::http::header::CONTENT_ENCODING);
    headers.remove(axum::http::header::CONTENT_LENGTH);
    headers.remove(axum::http::header::TRANSFER_ENCODING);

    Ok(Bytes::from(decompressed))
}

// ============================================================================
// Codex API handlers
// ============================================================================

/// Handles /v1/chat/completions requests (OpenAI Chat Completions API - Codex CLI)
pub async fn handle_chat_completions(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri;
    let mut headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    let body_bytes = decode_codex_request_body(&mut headers, body_bytes)?;
    let body: Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Internal(format!("Failed to parse request body: {e}")))?;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, AppType::Codex, "Codex", "codex").await?;
    let endpoint = endpoint_with_query(&uri, "/chat/completions");

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let forwarder = ctx.create_forwarder(&state);
    let mut result = match forwarder
        .forward_with_retry(
            &AppType::Codex,
            method,
            &endpoint,
            body,
            headers,
            extensions,
            ctx.get_providers(),
        )
        .await
    {
        Ok(result) => result,
        Err(mut err) => {
            if let Some(provider) = err.provider.take() {
                ctx.provider = provider;
            }
            log_forward_error(&state, &ctx, is_stream, &err.error);
            return build_codex_proxy_error_response(&ctx, &endpoint, &err.error);
        }
    };

    let connection_guard = result.connection_guard.take();
    ctx.outbound_model = result.outbound_model.take();
    ctx.provider = result.provider;
    let response = result.response;

    process_response(
        response,
        &ctx,
        &state,
        &OPENAI_PARSER_CONFIG,
        connection_guard,
    )
    .await
}

/// Handles /v1/responses requests (OpenAI Responses API - Codex CLI pass-through)
pub async fn handle_responses(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    handle_responses_for_app(state, request, AppType::Codex, "Codex", "codex").await
}

pub async fn handle_grokbuild_responses(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    handle_responses_for_app(
        state,
        request,
        AppType::GrokBuild,
        "Grok Build",
        "grokbuild",
    )
    .await
}

async fn handle_responses_for_app(
    state: ProxyState,
    request: axum::extract::Request,
    app_type: AppType,
    tag: &'static str,
    app_type_str: &'static str,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri;
    let mut headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    let body_bytes = decode_codex_request_body(&mut headers, body_bytes)?;
    let body: Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Internal(format!("Failed to parse request body: {e}")))?;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, app_type.clone(), tag, app_type_str).await?;
    let endpoint = endpoint_with_query(&uri, "/responses");

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let codex_tool_context = transform_codex_chat::build_codex_tool_context_from_request(&body);
    // Captured before `body` is moved into the forwarder: the flat-name →
    // {namespace, name} map used to restore the native Responses upstream's
    // function-call names (see the namespace-restore dispatch below).
    let namespace_restore_map = transform_codex_responses_namespace::namespace_restore_map(&body);

    let forwarder = ctx.create_forwarder(&state);
    let mut result = match forwarder
        .forward_with_retry(
            &app_type,
            method,
            &endpoint,
            body,
            headers,
            extensions,
            ctx.get_providers(),
        )
        .await
    {
        Ok(result) => result,
        Err(mut err) => {
            if let Some(provider) = err.provider.take() {
                ctx.provider = provider;
            }
            log_forward_error(&state, &ctx, is_stream, &err.error);
            return build_codex_proxy_error_response(&ctx, &endpoint, &err.error);
        }
    };

    let connection_guard = result.connection_guard.take();
    ctx.outbound_model = result.outbound_model.take();
    ctx.provider = result.provider;
    let response = result.response;

    if super::providers::should_convert_codex_responses_to_anthropic(&ctx.provider, &endpoint) {
        return handle_codex_anthropic_to_responses_transform(
            response,
            &ctx,
            &state,
            is_stream,
            connection_guard,
            codex_tool_context,
        )
        .await;
    }

    if super::providers::should_convert_codex_responses_to_chat(&ctx.provider, &endpoint) {
        return handle_codex_chat_to_responses_transform(
            response,
            &ctx,
            &state,
            is_stream,
            connection_guard,
            codex_tool_context,
        )
        .await;
    }

    // Native Responses passthrough to a strict gateway (xAI): restore flattened
    // function-call names *and* rewrite whole-float tool arguments. The integer
    // rewrite must run even when the request had no namespace tools.
    if super::providers::provider_needs_responses_namespace_flatten(&ctx.provider) {
        return handle_codex_xai_native_responses_rewrite(
            response,
            &ctx,
            &state,
            connection_guard,
            namespace_restore_map,
        )
        .await;
    }

    process_response(
        response,
        &ctx,
        &state,
        &CODEX_PARSER_CONFIG,
        connection_guard,
    )
    .await
}

/// Handles /v1/responses/compact requests (OpenAI Responses Compact API - Codex CLI pass-through)
pub async fn handle_responses_compact(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    handle_responses_compact_for_app(state, request, AppType::Codex, "Codex", "codex").await
}

/// Handle Codex's standalone Alpha Search protocol as a semantic passthrough.
///
/// Recent Codex clients send web-search commands to a dedicated endpoint instead
/// of embedding them in a Responses request. Keep this path out of the
/// Responses-to-Chat/Anthropic bridges: those formats cannot represent the Alpha
/// Search protocol.
pub async fn handle_alpha_search(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    handle_codex_standalone_passthrough(state, request, "/alpha/search").await
}

/// Handle Codex's legacy Images API endpoint for built-in ImageGen.
pub async fn handle_images_generations(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    handle_codex_standalone_passthrough(state, request, "/images/generations").await
}

/// Handle Codex's legacy Images API edit endpoint for built-in ImageGen.
///
/// Codex switches from `/images/generations` to `/images/edits` whenever the
/// ImageGen tool references existing images (explicit file paths or the last N
/// generated images). The body is plain JSON with data-URL images, so it takes
/// the same standalone passthrough as generations; only the upstream path differs.
pub async fn handle_images_edits(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    handle_codex_standalone_passthrough(state, request, "/images/edits").await
}

async fn handle_codex_standalone_passthrough(
    state: ProxyState,
    request: axum::extract::Request,
    canonical_endpoint: &'static str,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri;
    let mut headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    let body_bytes = decode_codex_request_body(&mut headers, body_bytes)?;
    let body: Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::InvalidRequest(format!("Failed to parse request body: {e}")))?;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, AppType::Codex, "Codex", "codex").await?;
    let endpoint = endpoint_with_query(&uri, canonical_endpoint);

    let forwarder = ctx.create_forwarder(&state);
    let mut result = match forwarder
        .forward_with_retry(
            &AppType::Codex,
            method,
            &endpoint,
            body,
            headers,
            extensions,
            ctx.get_providers(),
        )
        .await
    {
        Ok(result) => result,
        Err(mut err) => {
            if let Some(provider) = err.provider.take() {
                ctx.provider = provider;
            }
            log_forward_error(&state, &ctx, false, &err.error);
            return build_codex_proxy_error_response(&ctx, &endpoint, &err.error);
        }
    };

    let connection_guard = result.connection_guard.take();
    ctx.outbound_model = result.outbound_model.take();
    ctx.provider = result.provider;

    process_response(
        result.response,
        &ctx,
        &state,
        &CODEX_PARSER_CONFIG,
        connection_guard,
    )
    .await
}

pub async fn handle_grokbuild_responses_compact(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    handle_responses_compact_for_app(
        state,
        request,
        AppType::GrokBuild,
        "Grok Build",
        "grokbuild",
    )
    .await
}

async fn handle_responses_compact_for_app(
    state: ProxyState,
    request: axum::extract::Request,
    app_type: AppType,
    tag: &'static str,
    app_type_str: &'static str,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri;
    let mut headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    let body_bytes = decode_codex_request_body(&mut headers, body_bytes)?;
    let body: Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Internal(format!("Failed to parse request body: {e}")))?;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, app_type.clone(), tag, app_type_str).await?;
    let endpoint = endpoint_with_query(&uri, "/responses/compact");

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let codex_tool_context = transform_codex_chat::build_codex_tool_context_from_request(&body);
    let namespace_restore_map = transform_codex_responses_namespace::namespace_restore_map(&body);

    let forwarder = ctx.create_forwarder(&state);
    let mut result = match forwarder
        .forward_with_retry(
            &app_type,
            method,
            &endpoint,
            body,
            headers,
            extensions,
            ctx.get_providers(),
        )
        .await
    {
        Ok(result) => result,
        Err(mut err) => {
            if let Some(provider) = err.provider.take() {
                ctx.provider = provider;
            }
            log_forward_error(&state, &ctx, is_stream, &err.error);
            return build_codex_proxy_error_response(&ctx, &endpoint, &err.error);
        }
    };

    let connection_guard = result.connection_guard.take();
    ctx.outbound_model = result.outbound_model.take();
    ctx.provider = result.provider;
    let response = result.response;

    if super::providers::should_convert_codex_responses_to_anthropic(&ctx.provider, &endpoint) {
        return handle_codex_anthropic_to_responses_transform(
            response,
            &ctx,
            &state,
            is_stream,
            connection_guard,
            codex_tool_context,
        )
        .await;
    }

    if super::providers::should_convert_codex_responses_to_chat(&ctx.provider, &endpoint) {
        return handle_codex_chat_to_responses_transform(
            response,
            &ctx,
            &state,
            is_stream,
            connection_guard,
            codex_tool_context,
        )
        .await;
    }

    if super::providers::provider_needs_responses_namespace_flatten(&ctx.provider) {
        return handle_codex_xai_native_responses_rewrite(
            response,
            &ctx,
            &state,
            connection_guard,
            namespace_restore_map,
        )
        .await;
    }

    process_response(
        response,
        &ctx,
        &state,
        &CODEX_PARSER_CONFIG,
        connection_guard,
    )
    .await
}

/// Response handler for the native Responses passthrough to xAI: restore
/// flattened `function_call` names and rewrite whole-float tool arguments.
/// Error bodies pass through unchanged. Usage is collected exactly as
/// `process_response` would (same `CODEX_PARSER_CONFIG`).
async fn handle_codex_xai_native_responses_rewrite(
    response: super::hyper_client::ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    connection_guard: Option<ActiveConnectionGuard>,
    restore_map: std::collections::HashMap<
        String,
        transform_codex_responses_namespace::NamespacedName,
    >,
) -> Result<axum::response::Response, ProxyError> {
    let status = response.status();

    // Error bodies (and any non-SSE, non-success response) never contain
    // restorable function calls; hand them to the generic passthrough so error
    // shape and usage handling stay identical to the untransformed path.
    if !status.is_success() {
        return process_response(response, ctx, state, &CODEX_PARSER_CONFIG, connection_guard)
            .await;
    }

    if response.is_sse() {
        let mut response_headers = response.headers().clone();
        strip_hop_by_hop_response_headers(&mut response_headers);

        let mut builder = axum::response::Response::builder().status(status);
        for (key, value) in &response_headers {
            builder = builder.header(key, value);
        }

        let restore_stream =
            transform_codex_responses_xai_sanitize::create_xai_native_responses_sse_stream(
                response.bytes_stream(),
                restore_map,
            );
        let usage_collector =
            create_usage_collector(ctx, state, status.as_u16(), &CODEX_PARSER_CONFIG);
        let logged_stream = create_logged_passthrough_stream(
            restore_stream,
            ctx.tag,
            usage_collector,
            ctx.streaming_timeout_config(),
            connection_guard,
        );

        let body = axum::body::Body::from_stream(logged_stream);
        return builder.body(body).map_err(|e| {
            log::error!("[{}] build restored stream response failed: {e}", ctx.tag);
            ProxyError::Internal(format!("Failed to build streaming response: {e}"))
        });
    }

    // Non-streaming: restore the flattened function-call names in the full body,
    // then account usage from the (restore-neutral) Responses payload.
    let _connection_guard = connection_guard;
    let body_timeout =
        if ctx.app_config.auto_failover_enabled && ctx.app_config.non_streaming_timeout > 0 {
            std::time::Duration::from_secs(ctx.app_config.non_streaming_timeout as u64)
        } else {
            std::time::Duration::ZERO
        };
    let (mut response_headers, status, body_bytes) =
        read_decoded_body(response, ctx.tag, body_timeout).await?;
    strip_hop_by_hop_response_headers(&mut response_headers);

    // Restore names when the body parses as JSON; otherwise pass the bytes
    // through untouched (a native Responses non-stream body is always JSON, so
    // this only guards against a malformed upstream).
    let restored_bytes = match serde_json::from_slice::<Value>(&body_bytes) {
        Ok(mut value) => {
            transform_codex_responses_namespace::restore_response_namespaces(
                &mut value,
                &restore_map,
            );
            transform_codex_responses_xai_sanitize::normalize_xai_function_call_integer_arguments(
                &mut value,
            );
            if let Some(usage) =
                TokenUsage::from_codex_response_auto(&value).filter(TokenUsage::has_billable_tokens)
            {
                let model = value
                    .get("model")
                    .and_then(|m| m.as_str())
                    .filter(|m| !m.is_empty())
                    .map(str::to_string)
                    .or_else(|| ctx.outbound_model.clone())
                    .unwrap_or_else(|| ctx.request_model.clone());
                let request_model = ctx.request_model.clone();
                let outbound_model = ctx
                    .outbound_model
                    .clone()
                    .unwrap_or_else(|| ctx.request_model.clone());
                let app_type_str = ctx.app_type_str;
                tokio::spawn({
                    let state = state.clone();
                    let provider_id = ctx.provider.id.clone();
                    let session_id = ctx.session_id.clone();
                    let latency_ms = ctx.latency_ms();
                    async move {
                        log_usage(
                            &state,
                            &provider_id,
                            app_type_str,
                            &model,
                            &request_model,
                            &outbound_model,
                            usage,
                            latency_ms,
                            None,
                            false,
                            status.as_u16(),
                            Some(session_id),
                        )
                        .await;
                    }
                });
            }
            match serde_json::to_vec(&value) {
                Ok(bytes) => Bytes::from(bytes),
                Err(e) => {
                    log::error!("[{}] serialize restored response failed: {e}", ctx.tag);
                    body_bytes
                }
            }
        }
        Err(_) => body_bytes,
    };

    strip_entity_headers_for_rebuilt_body(&mut response_headers);
    response_headers.remove(axum::http::header::CONTENT_TYPE);

    let mut builder = axum::response::Response::builder().status(status);
    for (key, value) in response_headers.iter() {
        builder = builder.header(key, value);
    }
    builder = builder.header(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    builder
        .body(axum::body::Body::from(restored_bytes))
        .map_err(|e| {
            log::error!("[{}] failed to build the restored response: {e}", ctx.tag);
            ProxyError::Internal(format!("Failed to build response: {e}"))
        })
}

async fn handle_codex_chat_to_responses_transform(
    response: super::hyper_client::ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    is_stream: bool,
    connection_guard: Option<ActiveConnectionGuard>,
    tool_context: transform_codex_chat::CodexToolContext,
) -> Result<axum::response::Response, ProxyError> {
    let status = response.status();

    if !status.is_success() {
        // Upstream Chat error bodies differ in shape from Responses (MiniMax's base_resp, custom detail
        // fields, and so on), and passing them through leaves the Codex client unable to read the error
        // code, so they are normalized to `{"error": {message, type, code, param}}` with the original HTTP status preserved.
        return handle_codex_chat_error_response(response, ctx, status).await;
    }

    if is_stream || response.is_sse() {
        let stream = response.bytes_stream();
        let sse_stream = create_responses_sse_stream_from_chat_with_context(stream, tool_context);
        let sse_stream = record_responses_sse_stream(sse_stream, state.codex_chat_history.clone());

        let usage_collector = if usage_logging_enabled(state) {
            let state = state.clone();
            let provider_id = ctx.provider.id.clone();
            let request_model = ctx.request_model.clone();
            // Attribution fallback for takeover / model override: the outbound truth beats the client's alias
            let fallback_model = ctx
                .outbound_model
                .clone()
                .unwrap_or_else(|| ctx.request_model.clone());
            let app_type_str = ctx.app_type_str;
            let start_time = ctx.start_time;
            let session_id = ctx.session_id.clone();

            Some(SseUsageCollector::new(
                start_time,
                Some(codex_stream_usage_event_filter),
                move |events, first_token_ms| {
                    let usage =
                        TokenUsage::from_codex_stream_events_auto(&events).unwrap_or_default();
                    // When upstream follows OpenAI semantics and omits usage, the Chat -> Responses converter
                    // synthesizes an all-zero response.completed, and from_codex_response returns Some as long as the
                    // input/output fields exist (even at 0). Without a nonzero gate, all-zero usage is written:
                    // message_id=None makes dedup_request_id a random UUID, dedupe fails, and every request adds a
                    // meaningless empty row that inflates the request count. Matches the skip in the Claude transform handler.
                    if !usage.has_billable_tokens() {
                        log::debug!("[Codex] streaming response usage is all zero or missing, skipping the consumption record");
                        return;
                    }
                    let model = usage
                        .model
                        .clone()
                        .filter(|m| !m.is_empty())
                        .unwrap_or_else(|| fallback_model.clone());
                    let latency_ms = start_time.elapsed().as_millis() as u64;

                    let state = state.clone();
                    let provider_id = provider_id.clone();
                    let request_model = request_model.clone();
                    let outbound_model = fallback_model.clone();
                    let session_id = session_id.clone();

                    tokio::spawn(async move {
                        log_usage(
                            &state,
                            &provider_id,
                            app_type_str,
                            &model,
                            &request_model,
                            &outbound_model,
                            usage,
                            latency_ms,
                            first_token_ms,
                            true,
                            status.as_u16(),
                            Some(session_id),
                        )
                        .await;
                    });
                },
            ))
        } else {
            None
        };

        let logged_stream = create_logged_passthrough_stream(
            sse_stream,
            ctx.tag,
            usage_collector,
            ctx.streaming_timeout_config(),
            connection_guard,
        );

        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "Content-Type",
            axum::http::HeaderValue::from_static("text/event-stream"),
        );
        headers.insert(
            "Cache-Control",
            axum::http::HeaderValue::from_static("no-cache"),
        );

        let body = axum::body::Body::from_stream(logged_stream);
        return Ok((headers, body).into_response());
    }

    let _connection_guard = connection_guard;
    let body_timeout =
        if ctx.app_config.auto_failover_enabled && ctx.app_config.non_streaming_timeout > 0 {
            std::time::Duration::from_secs(ctx.app_config.non_streaming_timeout as u64)
        } else {
            std::time::Duration::ZERO
        };
    let (mut response_headers, status, body_bytes) =
        read_decoded_body(response, ctx.tag, body_timeout).await?;
    let body_str = String::from_utf8_lossy(&body_bytes);
    let chat_response: Value = match serde_json::from_slice(&body_bytes) {
        Ok(value) => value,
        // Fallback sniffing mirroring handle_claude_transform on the Claude side (#2234):
        // aggregate as SSE when upstream returns an unlabelled SSE body for stream:false.
        Err(_) if body_looks_like_sse(&body_str) => {
            log::warn!("[Codex] upstream returned an unlabelled SSE body for a non-streaming request, falling back to Chat SSE aggregation");
            // If aggregation also fails, the server log records only the length while the client error carries field diagnostics (C7)
            chat_sse_to_response_value(&body_str).map_err(|e| {
                log::error!(
                    "[Codex] SSE aggregation fallback failed: {e}, body_bytes={}",
                    body_bytes.len()
                );
                aggregate_fallback_error(e, &response_headers, &body_str)
            })?
        }
        Err(e) => {
            log::error!(
                "[Codex] failed to parse the upstream Chat response: {e}, body_bytes={}",
                body_bytes.len()
            );
            return Err(upstream_body_parse_error(
                "Failed to parse upstream chat response",
                &e,
                &response_headers,
                &body_str,
            ));
        }
    };
    let responses_response = transform_codex_chat::chat_completion_to_response_with_context(
        chat_response,
        &tool_context,
    )
    .map_err(|e| {
        log::error!("[Codex] Chat -> Responses conversion failed: {e}");
        e
    })?;
    state
        .codex_chat_history
        .record_response(&responses_response)
        .await;

    // When a non-streaming Chat upstream omits usage, chat_usage_to_responses_usage synthesizes an
    // all-zero usage (transform_codex_chat.rs:1581), and from_codex_response returns Some whenever the
    // input/output fields exist (even at 0). The has_billable_tokens gate skips all-zero rows so empty
    // rows cannot inflate the request count, matching the streaming branch and the Claude transform handler.
    if let Some(usage) = TokenUsage::from_codex_response_auto(&responses_response)
        .filter(TokenUsage::has_billable_tokens)
    {
        let model = responses_response
            .get("model")
            .and_then(|m| m.as_str())
            .filter(|m| !m.is_empty())
            .map(str::to_string)
            .or_else(|| ctx.outbound_model.clone())
            .unwrap_or_else(|| ctx.request_model.clone());
        let request_model = ctx.request_model.clone();
        let outbound_model = ctx
            .outbound_model
            .clone()
            .unwrap_or_else(|| ctx.request_model.clone());
        let app_type_str = ctx.app_type_str;
        tokio::spawn({
            let state = state.clone();
            let provider_id = ctx.provider.id.clone();
            let session_id = ctx.session_id.clone();
            let latency_ms = ctx.latency_ms();
            async move {
                log_usage(
                    &state,
                    &provider_id,
                    app_type_str,
                    &model,
                    &request_model,
                    &outbound_model,
                    usage,
                    latency_ms,
                    None,
                    false,
                    status.as_u16(),
                    Some(session_id),
                )
                .await;
            }
        });
    }

    strip_entity_headers_for_rebuilt_body(&mut response_headers);
    strip_hop_by_hop_response_headers(&mut response_headers);
    // Builder::header appends, so without a remove first the upstream Content-Type would be sent twice.
    response_headers.remove(axum::http::header::CONTENT_TYPE);

    let mut builder = axum::response::Response::builder().status(status);
    for (key, value) in response_headers.iter() {
        builder = builder.header(key, value);
    }
    builder = builder.header(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );

    let response_body = serde_json::to_vec(&responses_response).map_err(|e| {
        log::error!("[Codex] failed to serialize the Responses response: {e}");
        ProxyError::TransformError(format!("Failed to serialize responses response: {e}"))
    })?;

    builder
        .body(axum::body::Body::from(response_body))
        .map_err(|e| {
            log::error!("[Codex] failed to build the Responses response: {e}");
            ProxyError::Internal(format!("Failed to build response: {e}"))
        })
}

/// Response-transform handler for the Codex (Responses) ↔ Anthropic Messages gateway.
///
/// Parallel to `handle_codex_chat_to_responses_transform`: the upstream speaks
/// Anthropic Messages, and this converts the response back into the Responses form
/// Codex expects (both streaming and non-streaming). Error bodies reuse
/// `handle_codex_chat_error_response` (whose extraction logic also works for
/// Anthropic's `{"error":{type,message}}`). It does not involve codex_chat_history
/// (tool ids round-trip natively through Anthropic).
async fn handle_codex_anthropic_to_responses_transform(
    response: super::hyper_client::ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    is_stream: bool,
    connection_guard: Option<ActiveConnectionGuard>,
    codex_tool_context: transform_codex_chat::CodexToolContext,
) -> Result<axum::response::Response, ProxyError> {
    let status = response.status();

    if !status.is_success() {
        return handle_codex_chat_error_response(response, ctx, status).await;
    }

    // Preserve live streaming when the gateway marks SSE correctly or omits an
    // explicit JSON media type. Explicit JSON is buffered below so 2xx error
    // envelopes and gateways that ignore stream:true can be converted faithfully.
    if response.is_sse() || (is_stream && !response.is_json()) {
        let stream = response.bytes_stream();
        let sse_stream =
            create_responses_sse_stream_from_anthropic_with_context(stream, codex_tool_context);
        return build_codex_anthropic_sse_response(
            sse_stream,
            ctx,
            state,
            status,
            connection_guard,
        );
    }

    let body_timeout =
        if ctx.app_config.auto_failover_enabled && ctx.app_config.non_streaming_timeout > 0 {
            std::time::Duration::from_secs(ctx.app_config.non_streaming_timeout as u64)
        } else {
            std::time::Duration::ZERO
        };
    let (mut response_headers, status, body_bytes) =
        read_decoded_body(response, ctx.tag, body_timeout).await?;
    let body_str = String::from_utf8_lossy(&body_bytes);
    let anthropic_response: Value = match serde_json::from_slice(&body_bytes) {
        Ok(value) => value,
        // Fallback sniffing symmetric to the chat / claude side (#2234): when the
        // upstream returns an Anthropic SSE body with an unmarked Content-Type,
        // aggregate it back into a message before continuing the conversion.
        Err(_) if body_looks_like_sse(&body_str) => {
            log::warn!("[Codex] Upstream returned an unmarked Anthropic SSE body, falling back to aggregation");
            transform_codex_anthropic::anthropic_sse_to_message_value(&body_str).map_err(|e| {
                log::error!("[Codex] Failed to aggregate Anthropic SSE body: {e}");
                e
            })?
        }
        Err(e) => {
            log::error!(
                "[Codex] Failed to parse Anthropic upstream response: {e}, body_bytes={}",
                body_bytes.len()
            );
            return Err(upstream_body_parse_error(
                "Failed to parse upstream anthropic response",
                &e,
                &response_headers,
                &body_str,
            ));
        }
    };

    if is_stream {
        let events =
            responses_sse_events_from_anthropic_message(&anthropic_response, codex_tool_context);
        let sse_stream = futures::stream::iter(events.into_iter().map(Ok::<Bytes, std::io::Error>));
        return build_codex_anthropic_sse_response(
            sse_stream,
            ctx,
            state,
            status,
            connection_guard,
        );
    }

    let _connection_guard = connection_guard;
    let responses_response =
        transform_codex_anthropic::anthropic_response_to_responses_with_context(
            anthropic_response,
            &codex_tool_context,
        )
        .map_err(|e| {
            log::error!("[Codex] Failed to convert Anthropic response to Responses: {e}");
            e
        })?;

    if let Some(usage) = TokenUsage::from_codex_response_auto(&responses_response)
        .filter(TokenUsage::has_billable_tokens)
    {
        let model = responses_response
            .get("model")
            .and_then(|m| m.as_str())
            .filter(|m| !m.is_empty())
            .map(str::to_string)
            .or_else(|| ctx.outbound_model.clone())
            .unwrap_or_else(|| ctx.request_model.clone());
        let request_model = ctx.request_model.clone();
        let outbound_model = ctx
            .outbound_model
            .clone()
            .unwrap_or_else(|| ctx.request_model.clone());
        let app_type_str = ctx.app_type_str;
        tokio::spawn({
            let state = state.clone();
            let provider_id = ctx.provider.id.clone();
            let session_id = ctx.session_id.clone();
            let latency_ms = ctx.latency_ms();
            async move {
                log_usage(
                    &state,
                    &provider_id,
                    app_type_str,
                    &model,
                    &request_model,
                    &outbound_model,
                    usage,
                    latency_ms,
                    None,
                    false,
                    status.as_u16(),
                    Some(session_id),
                )
                .await;
            }
        });
    }

    strip_entity_headers_for_rebuilt_body(&mut response_headers);
    strip_hop_by_hop_response_headers(&mut response_headers);
    response_headers.remove(axum::http::header::CONTENT_TYPE);

    let mut builder = axum::response::Response::builder().status(status);
    for (key, value) in response_headers.iter() {
        builder = builder.header(key, value);
    }
    builder = builder.header(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );

    let response_body = serde_json::to_vec(&responses_response).map_err(|e| {
        log::error!("[Codex] Failed to serialize Responses response: {e}");
        ProxyError::TransformError(format!("Failed to serialize responses response: {e}"))
    })?;

    builder
        .body(axum::body::Body::from(response_body))
        .map_err(|e| {
            log::error!("[Codex] Failed to build Responses response: {e}");
            ProxyError::Internal(format!("Failed to build response: {e}"))
        })
}

fn build_codex_anthropic_sse_response(
    sse_stream: impl futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    ctx: &RequestContext,
    state: &ProxyState,
    status: StatusCode,
    connection_guard: Option<ActiveConnectionGuard>,
) -> Result<axum::response::Response, ProxyError> {
    let usage_collector = if usage_logging_enabled(state) {
        let state = state.clone();
        let provider_id = ctx.provider.id.clone();
        let request_model = ctx.request_model.clone();
        let fallback_model = ctx
            .outbound_model
            .clone()
            .unwrap_or_else(|| ctx.request_model.clone());
        let app_type_str = ctx.app_type_str;
        let start_time = ctx.start_time;
        let session_id = ctx.session_id.clone();

        Some(SseUsageCollector::new(
            start_time,
            Some(codex_stream_usage_event_filter),
            move |events, first_token_ms| {
                let usage = TokenUsage::from_codex_stream_events_auto(&events).unwrap_or_default();
                if !usage.has_billable_tokens() {
                    log::debug!("[Codex] Anthropic streaming response usage is all-zero or missing, skipping usage recording");
                    return;
                }
                let model = usage
                    .model
                    .clone()
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| fallback_model.clone());
                let latency_ms = start_time.elapsed().as_millis() as u64;

                let state = state.clone();
                let provider_id = provider_id.clone();
                let request_model = request_model.clone();
                let outbound_model = fallback_model.clone();
                let session_id = session_id.clone();

                tokio::spawn(async move {
                    log_usage(
                        &state,
                        &provider_id,
                        app_type_str,
                        &model,
                        &request_model,
                        &outbound_model,
                        usage,
                        latency_ms,
                        first_token_ms,
                        true,
                        status.as_u16(),
                        Some(session_id),
                    )
                    .await;
                });
            },
        ))
    } else {
        None
    };

    let logged_stream = create_logged_passthrough_stream(
        sse_stream,
        ctx.tag,
        usage_collector,
        ctx.streaming_timeout_config(),
        connection_guard,
    );

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "Content-Type",
        axum::http::HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(
        "Cache-Control",
        axum::http::HeaderValue::from_static("no-cache"),
    );

    let body = axum::body::Body::from_stream(logged_stream);
    Ok((headers, body).into_response())
}

/// Converts an upstream Chat Completions error response into the Responses API error shape.
///
/// The counterpart of the success branch: successful responses are already rewritten into the
/// Responses shape, so leaving a Chat error body (such as MiniMax's
/// `{"base_resp": {"status_code": 2013}}`) would leave the Codex client unable to map the fields.
/// This reads the upstream body, normalizes it to `{"error": {message, type, code, param}}`, and keeps the original HTTP status.
async fn handle_codex_chat_error_response(
    response: super::hyper_client::ProxyResponse,
    ctx: &RequestContext,
    status: axum::http::StatusCode,
) -> Result<axum::response::Response, ProxyError> {
    let body_timeout =
        if ctx.app_config.auto_failover_enabled && ctx.app_config.non_streaming_timeout > 0 {
            std::time::Duration::from_secs(ctx.app_config.non_streaming_timeout as u64)
        } else {
            std::time::Duration::ZERO
        };
    let (mut response_headers, _status, body_bytes) =
        read_decoded_body(response, ctx.tag, body_timeout).await?;

    // Dropping a non-JSON upstream error body (Cloudflare HTML, a plain "Unauthorized", and so on) to
    // None would hide the original diagnostics, so wrap it in a Value::String and take the converter's string branch.
    let parsed_value: Value = match serde_json::from_slice::<Value>(&body_bytes) {
        Ok(value) => value,
        Err(_) => {
            const MAX_RAW_ERROR_BYTES: usize = 1024;
            let lossy = String::from_utf8_lossy(&body_bytes);
            let truncated = if lossy.len() > MAX_RAW_ERROR_BYTES {
                let mut end = MAX_RAW_ERROR_BYTES;
                while end > 0 && !lossy.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}…(truncated)", &lossy[..end])
            } else {
                lossy.into_owned()
            };
            log::warn!(
                "[Codex] the Chat error response is not valid JSON, passing it through as text: body_bytes={} (content omitted)",
                body_bytes.len()
            );
            Value::String(truncated)
        }
    };

    let responses_error = transform_codex_chat::chat_error_to_response_error(Some(&parsed_value));

    strip_entity_headers_for_rebuilt_body(&mut response_headers);
    strip_hop_by_hop_response_headers(&mut response_headers);
    // Builder::header appends, so without a remove first the upstream Content-Type would be sent twice.
    response_headers.remove(axum::http::header::CONTENT_TYPE);

    let mut builder = axum::response::Response::builder().status(status);
    for (key, value) in response_headers.iter() {
        builder = builder.header(key, value);
    }
    builder = builder.header(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );

    let body = serde_json::to_vec(&responses_error).map_err(|e| {
        log::error!("[Codex] failed to serialize the Responses error body: {e}");
        ProxyError::TransformError(format!("Failed to serialize responses error: {e}"))
    })?;

    builder.body(axum::body::Body::from(body)).map_err(|e| {
        log::error!("[Codex] failed to build the Responses error response: {e}");
        ProxyError::Internal(format!("Failed to build response: {e}"))
    })
}

/// Builds an enriched Codex error response from a forwarding-layer failure (not an upstream response).
///
/// Unlike `handle_codex_chat_error_response` (which handles a real upstream error response and
/// copies upstream headers), there is no upstream response here, so it emits only an
/// `application/json` error body. The status comes from `map_proxy_error_to_status`, which already matches `ProxyError::into_response`.
///
/// Note: after `endpoint_with_query`, `endpoint` may carry a query (such as `?beta=true`) that is
/// written verbatim into the error body. Current Codex endpoints keep no credentials in the query,
/// so this is safe; if it is ever reused for endpoints that do (such as Gemini's `?key=`), redact before echoing.
fn build_codex_proxy_error_response(
    ctx: &RequestContext,
    endpoint: &str,
    error: &ProxyError,
) -> Result<axum::response::Response, ProxyError> {
    let status = axum::http::StatusCode::from_u16(map_proxy_error_to_status(error))
        .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    let body = codex_proxy_error_json(&ctx.provider.name, &ctx.request_model, endpoint, error);
    let body = serde_json::to_vec(&body).map_err(|e| {
        log::error!("[Codex] failed to serialize the proxy error body: {e}");
        ProxyError::Internal(format!("Failed to serialize proxy error: {e}"))
    })?;

    axum::response::Response::builder()
        .status(status)
        .header(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/json"),
        )
        .body(axum::body::Body::from(body))
        .map_err(|e| {
            log::error!("[Codex] failed to build the proxy error response: {e}");
            ProxyError::Internal(format!("Failed to build proxy error response: {e}"))
        })
}

fn codex_proxy_error_json(
    provider_name: &str,
    request_model: &str,
    endpoint: &str,
    error: &ProxyError,
) -> Value {
    let (mut body, upstream_status) = match error {
        ProxyError::UpstreamError { status, body } => {
            let parsed_body = body
                .as_deref()
                .map(|body| serde_json::from_str::<Value>(body).unwrap_or_else(|_| json!(body)));
            (
                transform_codex_chat::chat_error_to_response_error(parsed_body.as_ref()),
                Some(*status),
            )
        }
        _ => (
            json!({
                "error": {
                    "message": get_error_message(error),
                    "type": "proxy_error",
                    "code": codex_proxy_error_code(error),
                    "param": Value::Null,
                }
            }),
            None,
        ),
    };

    let Some(error_obj) = body
        .get_mut("error")
        .and_then(|value| value.as_object_mut())
    else {
        return body;
    };

    let message = if upstream_status == Some(413) {
        // A 413 comes from the provider's gateway (typically nginx's client_max_body_size), not from the
        // CC Switch local proxy (whose DefaultBodyLimit is already 200MB). The upstream body is usually a
        // whole page of nginx HTML that is worthless to users, so it is replaced with guidance that points
        // clearly at upstream and is actionable, avoiding the recurring belief that CC Switch bundles nginx or is itself at fault.
        format!(
            concat!(
                "Upstream provider rejected the request with HTTP 413 (Payload Too Large). ",
                "The request body exceeds the upstream gateway's size limit; this is the ",
                "provider's server-side limit, not a CC Switch limit. ",
                "Provider: {provider}; model: {model}; endpoint: {endpoint}. ",
                "To recover, shrink the request: run /compact, remove large pasted logs or ",
                "inline images, or ask the provider to raise its request body limit ",
                "(e.g. nginx client_max_body_size)."
            ),
            provider = provider_name,
            model = request_model,
            endpoint = endpoint,
        )
    } else {
        let cause = error_obj
            .get("message")
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
            .filter(|message| !message.trim().is_empty())
            .unwrap_or_else(|| get_error_message(error));
        let status_fragment = upstream_status
            .map(|status| format!("; upstream_status: HTTP {status}"))
            .unwrap_or_default();
        format!(
            "CC Switch local proxy failed while handling Codex endpoint {endpoint}. Provider: {provider_name}; model: {request_model}{status_fragment}; cause: {cause}"
        )
    };

    error_obj.insert(
        "message".to_string(),
        Value::String(compact_error_message(&message, 1800)),
    );

    if error_obj
        .get("type")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        error_obj.insert("type".to_string(), Value::String("proxy_error".to_string()));
    }

    if error_obj.get("code").map(Value::is_null).unwrap_or(true) {
        error_obj.insert(
            "code".to_string(),
            Value::String(codex_proxy_error_code(error).to_string()),
        );
    }

    if !error_obj.contains_key("param") {
        error_obj.insert("param".to_string(), Value::Null);
    }

    error_obj.insert(
        "provider".to_string(),
        Value::String(provider_name.to_string()),
    );
    error_obj.insert(
        "model".to_string(),
        Value::String(request_model.to_string()),
    );
    // For Codex local routing only; do not reuse on endpoints whose query may carry credentials.
    error_obj.insert("endpoint".to_string(), Value::String(endpoint.to_string()));
    if let Some(status) = upstream_status {
        error_obj.insert(
            "upstream_status".to_string(),
            Value::Number(serde_json::Number::from(status)),
        );
    }

    body
}

fn codex_proxy_error_code(error: &ProxyError) -> &'static str {
    match error {
        ProxyError::ForwardFailed(_) => "cc_switch_forward_failed",
        ProxyError::Timeout(_) => "cc_switch_timeout",
        ProxyError::NoAvailableProvider => "cc_switch_no_available_provider",
        ProxyError::AllProvidersCircuitOpen => "cc_switch_all_providers_circuit_open",
        ProxyError::NoProvidersConfigured => "cc_switch_no_providers_configured",
        ProxyError::MaxRetriesExceeded => "cc_switch_max_retries_exceeded",
        ProxyError::ConfigError(_) => "cc_switch_config_error",
        ProxyError::TransformError(_) => "cc_switch_transform_error",
        ProxyError::InvalidRequest(_) => "cc_switch_invalid_request",
        ProxyError::AuthError(_) => "cc_switch_auth_error",
        ProxyError::UpstreamError { .. } => "cc_switch_upstream_error",
        ProxyError::DatabaseError(_) => "cc_switch_database_error",
        ProxyError::Internal(_) => "cc_switch_internal_error",
        ProxyError::AlreadyRunning
        | ProxyError::NotRunning
        | ProxyError::BindFailed(_)
        | ProxyError::StopTimeout
        | ProxyError::StopFailed(_)
        | ProxyError::ResponseBodyTooLarge(_) => "cc_switch_proxy_error",
    }
}

fn compact_error_message(message: &str, max_chars: usize) -> String {
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    let truncated = normalized
        .chars()
        .take(max_chars)
        .collect::<String>()
        .trim_end()
        .to_string();
    format!("{truncated}…(truncated)")
}

// ============================================================================
// Gemini API handlers
// ============================================================================

/// Handles Gemini API requests (pass-through, query parameters included)
pub async fn handle_gemini(
    State(state): State<ProxyState>,
    uri: axum::http::Uri,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    let method = parts.method.clone();
    let headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    // Read-only GET endpoints (/v1beta/models, /v1beta/models/<model>, and so on) have no body,
    // so JSON parsing must not be forced or an empty body would be rejected.
    let body: Value = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes)
            .map_err(|e| ProxyError::Internal(format!("Failed to parse request body: {e}")))?
    };

    // Gemini carries the model name in the URI
    let mut ctx = RequestContext::new(&state, &body, &headers, AppType::Gemini, "Gemini", "gemini")
        .await?
        .with_model_from_uri(&uri);

    // Extract the full path and query string
    let endpoint = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(uri.path());

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let forwarder = ctx.create_forwarder(&state);
    let mut result = match forwarder
        .forward_with_retry(
            &AppType::Gemini,
            method,
            endpoint,
            body,
            headers,
            extensions,
            ctx.get_providers(),
        )
        .await
    {
        Ok(result) => result,
        Err(mut err) => {
            if let Some(provider) = err.provider.take() {
                ctx.provider = provider;
            }
            log_forward_error(&state, &ctx, is_stream, &err.error);
            return Err(err.error);
        }
    };

    let connection_guard = result.connection_guard.take();
    ctx.outbound_model = result.outbound_model.take();
    ctx.provider = result.provider;
    let response = result.response;

    process_response(
        response,
        &ctx,
        &state,
        &GEMINI_PARSER_CONFIG,
        connection_guard,
    )
    .await
}

fn should_use_claude_transform_streaming(
    requested_streaming: bool,
    upstream_is_sse: bool,
    api_format: &str,
    is_codex_oauth: bool,
) -> bool {
    requested_streaming || upstream_is_sse || (is_codex_oauth && api_format == "openai_responses")
}

async fn responses_sse_stream_to_anthropic_message(
    stream: impl futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    hosted_web_search_name: Option<String>,
    max_web_search_uses: Option<u64>,
    body_timeout: std::time::Duration,
) -> Result<Value, ProxyError> {
    let collect = async move {
        let converted = create_anthropic_sse_stream_from_responses_with_web_search_options(
            stream,
            hosted_web_search_name,
            max_web_search_uses,
        );
        tokio::pin!(converted);

        let mut body = Vec::new();
        while let Some(chunk) = converted.next().await {
            let chunk = chunk.map_err(|error| {
                ProxyError::ForwardFailed(format!(
                    "Failed to transform upstream Responses SSE: {error}"
                ))
            })?;
            body.extend_from_slice(&chunk);
        }
        String::from_utf8(body).map_err(|error| {
            ProxyError::TransformError(format!(
                "Transformed Anthropic SSE was not valid UTF-8: {error}"
            ))
        })
    };

    let body = if body_timeout.is_zero() {
        collect.await?
    } else {
        tokio::time::timeout(body_timeout, collect)
            .await
            .map_err(|_| {
                ProxyError::Timeout(format!(
                    "response body read timed out after {}s (upstream sent headers but no body)",
                    body_timeout.as_secs()
                ))
            })??
    };

    transform_codex_anthropic::anthropic_sse_to_message_value(&body)
}

/// Aggregates an OpenAI Responses SSE stream into one complete Responses JSON object so downstream
/// can convert it into a non-streaming Anthropic response. Called only when Codex OAuth upgrades `stream:false` to SSE.
///
/// Reuses `take_sse_block` / `strip_sse_field` from `proxy::sse`: `take_sse_block` handles both
/// the `\n\n` and `\r\n\r\n` separators, and `strip_sse_field` accepts fields with or without a space.
fn responses_sse_to_response_value(body: &str) -> Result<Value, ProxyError> {
    let mut buffer = body.trim_start_matches('\u{feff}').to_string();
    let mut completed_response: Option<Value> = None;
    let mut output_items = Vec::new();

    // strict=false is for the trailing remainder: truncated half JSON is ignored rather than raised,
    // so it cannot break an already complete aggregate (the codex_oauth path reuses this function)
    let mut process_block = |block: &str, strict: bool| -> Result<(), ProxyError> {
        // Once completed has arrived the trailing remainder (strict=false) is skipped entirely: the
        // codex_oauth path reuses this function, and processing a full response.failed or stray event from
        // the remainder afterwards would turn a successful response into a 422 (C8).
        if !strict && completed_response.is_some() {
            return Ok(());
        }
        let mut event_name = "";
        let mut data_lines: Vec<&str> = Vec::new();

        for line in block.lines() {
            let line = line.trim_start();
            if let Some(evt) = strip_sse_field(line, "event") {
                event_name = evt.trim();
            } else if let Some(d) = strip_sse_field(line, "data") {
                data_lines.push(d);
            }
        }

        if data_lines.is_empty() {
            return Ok(());
        }

        let data_str = data_lines.join("\n");
        if data_str.trim() == "[DONE]" {
            return Ok(());
        }

        let data: Value = match serde_json::from_str(&data_str) {
            Ok(v) => v,
            Err(_) if !strict => return Ok(()),
            Err(e) => {
                return Err(ProxyError::TransformError(format!(
                    "Failed to parse upstream SSE event: {e}"
                )))
            }
        };

        match event_name {
            "response.output_item.done" => {
                if let Some(item) = data.get("item") {
                    output_items.push(item.clone());
                }
            }
            "response.completed" => {
                completed_response = Some(data.get("response").cloned().unwrap_or(data));
            }
            "response.failed" => {
                let message = data
                    .pointer("/response/error/message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("response.failed event received");
                return Err(ProxyError::TransformError(message.to_string()));
            }
            _ => {}
        }
        Ok(())
    };

    while let Some(block) = take_sse_block(&mut buffer) {
        process_block(&block, true)?;
    }
    // The last event may lack a blank-line separator (common with mislabelled SSE and non-conforming
    // upstreams), so the remaining buffer is processed as a final block or a trailing response.completed
    // would be lost. The already-completed skip check lives inside the closure (C8).
    process_block(&buffer, false)?;

    let mut response = completed_response.ok_or_else(|| {
        ProxyError::TransformError("No response.completed event in upstream SSE".to_string())
    })?;

    if !output_items.is_empty() {
        if let Some(obj) = response.as_object_mut() {
            obj.insert("output".to_string(), Value::Array(output_items));
        } else {
            return Err(ProxyError::TransformError(
                "response.completed payload is not an object".to_string(),
            ));
        }
    }

    Ok(response)
}

/// Checks whether a body "looks like" SSE text (the #2234 fallback sniffer).
///
/// Called only after JSON parsing failed: valid JSON can never start with these prefixes, so there are no false positives.
/// It covers all four SSE field lines, and ":" is included because OpenRouter and others send a
/// `: PROCESSING` comment line before the stream.
fn body_looks_like_sse(body: &str) -> bool {
    let trimmed = body.trim_start_matches('\u{feff}').trim_start();
    ["data:", "event:", "id:", "retry:", ":"]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

/// Builds an upstream parse error with field diagnostics: only structured classification and
/// metadata, so the response body never reaches persistent logs through the error chain.
fn upstream_body_parse_error(
    prefix: &str,
    err: &serde_json::Error,
    headers: &axum::http::HeaderMap,
    body: &str,
) -> ProxyError {
    ProxyError::TransformError(format!(
        "{prefix}: {err} {}",
        body_diagnostics_suffix(headers, body)
    ))
}

/// When the SSE aggregation fallback fails, attach the same field diagnostics to the aggregator's
/// internal error so clients hitting the #2234 sniffing arm also get root-cause clues, rather than
/// a bare message like "No chat completion choices in upstream SSE" with no header or body context.
fn aggregate_fallback_error(
    err: ProxyError,
    headers: &axum::http::HeaderMap,
    body: &str,
) -> ProxyError {
    let base = match &err {
        ProxyError::TransformError(m) => m.clone(),
        other => other.to_string(),
    };
    ProxyError::TransformError(format!("{base} {}", body_diagnostics_suffix(headers, body)))
}

/// Sorts a body into a small set of classes, keeping clues such as HTML, SSE, or binary garbage without logging the body.
fn classify_body_for_diagnostics(body: &str) -> &'static str {
    let trimmed = body.trim_start_matches('\u{feff}').trim_start();
    if trimmed.is_empty() {
        return "empty";
    }
    if body_looks_like_sse(trimmed) {
        return "sse";
    }

    // Classification looks at the first 4 KiB only, so diagnostics never rescan an abnormally huge body.
    let sample = trimmed.chars().take(4096).collect::<String>();
    let prefix = sample
        .chars()
        .take(256)
        .collect::<String>()
        .to_ascii_lowercase();
    if ["<!doctype html", "<html", "<head", "<body"]
        .iter()
        .any(|marker| prefix.starts_with(marker))
    {
        return "html";
    }
    if sample.contains('\u{fffd}')
        || sample
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return "binary-or-encoded";
    }
    if prefix.starts_with('{') || prefix.starts_with('[') {
        return "json-like";
    }
    "text"
}

/// Field diagnostic suffix: content-type, content-encoding, body length, and a safe classification, never the body.
fn body_diagnostics_suffix(headers: &axum::http::HeaderMap, body: &str) -> String {
    let header_str = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<none>")
    };
    format!(
        "(content-type: {}; content-encoding: {}; body-bytes: {}; body-kind: {}; content omitted)",
        header_str("content-type"),
        header_str("content-encoding"),
        body.len(),
        classify_body_for_diagnostics(body),
    )
}

/// Extracts a reportable error message from a chunk's error field. Placeholder shapes (an empty
/// object, an empty message, false, an empty string, all common in the per-chunk error field of
/// OpenAI-compatible gateways) return None, since they must not fail the whole stream and turn a success into a 422 (C12, the #2234 audience).
fn error_event_message(error: &Value) -> Option<String> {
    if let Some(msg) = error.get("message").and_then(|m| m.as_str()) {
        return (!msg.is_empty()).then(|| msg.to_string());
    }
    if let Some(s) = error.as_str() {
        return (!s.is_empty()).then(|| s.to_string());
    }
    None
}

/// Parses one SSE block's event name and data payload (multi-line data joined with \n per spec).
/// Leading whitespace is allowed before the field name, matching the trim tolerance of
/// body_looks_like_sse; otherwise an indented `  data:` line passes sniffing but is silently lost here (C4). None means there was no data line.
fn sse_block_parts(block: &str) -> Option<(String, String)> {
    let mut event_name = String::new();
    let mut data_lines: Vec<&str> = Vec::new();
    for line in block.lines() {
        let line = line.trim_start();
        if let Some(evt) = strip_sse_field(line, "event") {
            event_name = evt.trim().to_string();
        } else if let Some(d) = strip_sse_field(line, "data") {
            data_lines.push(d);
        }
    }
    (!data_lines.is_empty()).then(|| (event_name, data_lines.join("\n")))
}

/// Aggregates a Chat Completions SSE stream into one chat.completion JSON (the #2234 fallback).
///
/// For the non-streaming branch only: upstream returned an SSE body for stream:false without
/// labelling Content-Type as text/event-stream, so the is_sse header check failed. The aggregate is
/// fed to the existing non-streaming converters (openai_to_anthropic on the Claude side,
/// chat_completion_to_response_with_context on the Codex side), so the client still gets valid JSON.
/// Delta merging matches providers/streaming.rs: tool_calls are located by delta.index, id and name
/// overwrite when present, and arguments strings concatenate; every reasoning shape
/// (reasoning_content / reasoning / reasoning_details) goes through the shared codex_chat_common
/// extractor into one accumulator; the first non-null finish_reason wins (kimi-k2.6 sends another
/// finish_reason block after tool_use, see streaming.rs).
fn chat_sse_to_response_value(body: &str) -> Result<Value, ProxyError> {
    // Strip the BOM: the sniffer accepts a leading BOM, but strip_sse_field matches the line start
    // exactly, so leaving it would silently lose the first data line
    let mut buffer = body.trim_start_matches('\u{feff}').to_string();

    let mut id = Value::Null;
    let mut created = Value::Null;
    let mut model = Value::Null;
    let mut content = String::new();
    let mut reasoning_content = String::new();
    // tool_calls aggregate into a BTreeMap keyed by index: an upstream-controlled u64 index cannot
    // densify an array, whereas the old `while len() <= index { push }` would OOM the whole process at
    // index=4e9 (C1). A BTreeMap avoids unbounded allocation and keeps the output ordered by index.
    let mut tool_calls: std::collections::BTreeMap<usize, Value> =
        std::collections::BTreeMap::new();
    let mut finish_reason = Value::Null;
    let mut usage = Value::Null;
    let mut saw_choice = false;
    let mut saw_done = false;

    // strict=false is for the trailing remainder: truncated half JSON is ignored rather than raised,
    // symmetric with the remainder handling in responses_sse_to_response_value (C2); otherwise a
    // clipped trailing block would turn a complete aggregate into a 422.
    let mut process_event =
        |event_name: &str, data_str: &str, strict: bool| -> Result<(), ProxyError> {
            let trimmed = data_str.trim();
            if trimmed == "[DONE]" {
                saw_done = true;
                return Ok(());
            }
            if trimmed.is_empty() {
                return Ok(());
            }
            let chunk: Value = match serde_json::from_str(data_str) {
                Ok(v) => v,
                Err(_) if !strict => return Ok(()),
                Err(e) => {
                    return Err(ProxyError::TransformError(format!(
                        "Failed to parse upstream SSE chunk: {e}"
                    )))
                }
            };

            // An `event: error` event: the error is marked by the event name and the data body may hold the
            // error object directly with no error key. Even after a complete choice was aggregated this must
            // count as failure, or a gateway quota/rate-limit error would masquerade as success (C18).
            if event_name.eq_ignore_ascii_case("error") {
                let message = chunk
                    .get("error")
                    .and_then(error_event_message)
                    .or_else(|| error_event_message(&chunk))
                    .unwrap_or_else(|| "upstream error event in SSE stream".to_string());
                return Err(ProxyError::TransformError(message));
            }
            // A gateway sending the error as an ordinary data chunk ({"error":{...}}) fails only when the
            // error carries a reportable message. Placeholder shapes (empty object, empty message, null,
            // false), which some OpenAI-compatible gateways attach to every chunk, must not kill a success (C12).
            if let Some(message) = chunk
                .get("error")
                .filter(|e| !e.is_null())
                .and_then(error_event_message)
            {
                return Err(ProxyError::TransformError(message));
            }

            // The first meaningful value locks the envelope. Azure's content-filter preamble carries ""/0
            // placeholders (streaming.rs has the same empty-string guard), which must not freeze the fields
            for (slot, key) in [
                (&mut id, "id"),
                (&mut created, "created"),
                (&mut model, "model"),
            ] {
                if slot.is_null() {
                    if let Some(v) = chunk.get(key).filter(|v| envelope_value_meaningful(v)) {
                        *slot = v.clone();
                    }
                }
            }
            // OpenAI semantics: usage is non-null only in the final chunk
            if let Some(u) = chunk.get("usage").filter(|u| !u.is_null()) {
                usage = u.clone();
            }

            // The proxy context only ever has one choice (n=1), so only index==0 is aggregated
            let Some(choice) = chunk
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .find(|ch| ch.get("index").and_then(|i| i.as_u64()).unwrap_or(0) == 0)
                })
            else {
                return Ok(());
            };

            // The evidence of "a response was seen" must be a choice payload: if a stream of metadata or
            // usage-only chunks plus [DONE] (never a choice) counted, it would bypass both guards below and
            // wrap an empty body as a false success
            saw_choice = true;

            // The first non-null finish_reason wins (matching first-wins in streaming.rs: a trailing "stop"
            // from a multi-finish_reason upstream must not overwrite an earlier "tool_calls")
            if finish_reason.is_null() {
                if let Some(fr) = choice.get("finish_reason").filter(|v| !v.is_null()) {
                    finish_reason = fr.clone();
                }
            }
            // Payload choice: normal deltas use delta, but fake-streaming relays wrap a whole chat.completion
            // in a single event (message rather than delta), sometimes with an empty delta:{}. When delta is
            // an empty object and a message exists, the message snapshot is used (overwriting accumulated
            // deltas to avoid double counting); otherwise the content is silently lost while its finish_reason defeats the completeness guard, yielding an empty false success (C3).
            let delta_nonempty = choice
                .get("delta")
                .and_then(|d| d.as_object())
                .is_some_and(|o| !o.is_empty());
            let (payload, is_full_message) = if delta_nonempty {
                (choice.get("delta").unwrap(), false)
            } else if let Some(message) = choice.get("message") {
                (message, true)
            } else if let Some(delta) = choice.get("delta") {
                // Empty delta and no message: the normal finish_reason-only closing block
                (delta, false)
            } else {
                return Ok(());
            };
            if is_full_message {
                content.clear();
                reasoning_content.clear();
                tool_calls.clear();
            }
            match payload.get("content") {
                Some(Value::String(text)) => content.push_str(text),
                Some(Value::Array(parts)) => {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            content.push_str(text);
                        } else if let Some(refusal) = part.get("refusal").and_then(|r| r.as_str()) {
                            content.push_str(refusal);
                        }
                    }
                }
                _ => {}
            }
            // refusal: OpenAI's official refusal shape (the delta.refusal / message.refusal string).
            // Both downstream converters treat refusal as visible content, so missing it turns a refusal into an empty false success (C15).
            if let Some(refusal) = payload.get("refusal").and_then(|r| r.as_str()) {
                content.push_str(refusal);
            }
            // Exhaustive reasoning extraction reuses codex_chat_common (reasoning_content > reasoning as
            // string/object > reasoning_details) so a third hand-written implementation cannot miss a shape:
            // providers such as MiMo and OpenRouter that only send reasoning_details would otherwise lose the thinking
            if let Some(text) = extract_reasoning_field_text(payload) {
                reasoning_content.push_str(&text);
            }
            if let Some(deltas) = payload.get("tool_calls").and_then(|t| t.as_array()) {
                for (pos, tc) in deltas.iter().enumerate() {
                    merge_tool_call_delta(&mut tool_calls, tc, pos);
                }
            } else if let Some(fc) = payload.get("function_call").filter(|v| !v.is_null()) {
                // Legacy function_call (deprecated in 2023 but still echoed by some relays) becomes a single tool_call.
                // Both downstream converters support function_call, so missing it maps finish_reason
                // "function_call" to stop_reason "tool_use" with no tool block and stalls the agent loop (C17).
                let synthetic = json!({
                    "index": 0,
                    "id": fc.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                    "type": "function",
                    "function": fc,
                });
                merge_tool_call_delta(&mut tool_calls, &synthetic, 0);
            }
            Ok(())
        };

    while let Some(block) = take_sse_block(&mut buffer) {
        if let Some((event, data)) = sse_block_parts(&block) {
            process_event(&event, &data, true)?;
        }
    }
    // The last event may lack a blank-line separator (clipped streams, non-conforming upstreams), so
    // the remaining buffer is processed as a final block, with strict=false tolerating a clipped tail (C2).
    if let Some((event, data)) = sse_block_parts(&buffer) {
        process_event(&event, &data, false)?;
    }

    if !saw_choice {
        return Err(ProxyError::TransformError(
            "No chat completion choices in upstream SSE".to_string(),
        ));
    }
    // Completeness guard: mid-stream truncation of a close-delimited response is undetectable at the
    // byte level, so without either completion signal (finish_reason or [DONE]) it is treated as
    // truncated, rather than silently returning half the content as an apparent success (a failure mode harder to diagnose than a 422).
    if finish_reason.is_null() && !saw_done {
        return Err(ProxyError::TransformError(
            "Upstream SSE stream appears truncated (no finish_reason or [DONE] marker)".to_string(),
        ));
    }

    // Finalizing tool_calls: entirely empty shells (index gaps, or no field ever received) are dropped
    // to avoid ghost tool_use, while a missing id or name is backfilled from the original index
    // (matching tool_call_{idx}/unknown_tool in streaming.rs); an empty id would break Claude's
    // tool_use_id to tool_result round trip
    let tool_calls: Vec<Value> = tool_calls
        .into_iter()
        .filter(|(_, tc)| {
            tc["id"].as_str().is_some_and(|s| !s.is_empty())
                || tc["function"]["name"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
                || tc["function"]["arguments"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
        })
        .map(|(index, mut tc)| {
            if tc["id"].as_str().is_none_or(str::is_empty) {
                tc["id"] = json!(format!("tool_call_{index}"));
            }
            if tc["function"]["name"].as_str().is_none_or(str::is_empty) {
                tc["function"]["name"] = json!("unknown_tool");
            }
            tc
        })
        .collect();

    let mut message = serde_json::Map::new();
    message.insert("role".to_string(), json!("assistant"));
    message.insert("content".to_string(), json!(content));
    if !reasoning_content.is_empty() {
        message.insert("reasoning_content".to_string(), json!(reasoning_content));
    }
    if !tool_calls.is_empty() {
        message.insert("tool_calls".to_string(), Value::Array(tool_calls));
    }

    // Synthesize a UUID when upstream returns no valid id: leaving null or "" would degrade downstream
    // dedup_request_id to a constant "session:" that collides globally, so INSERT OR REPLACE silently overwrites earlier usage rows and undercounts cost (C9).
    let id = if envelope_value_meaningful(&id) {
        id
    } else {
        json!(uuid::Uuid::new_v4().to_string())
    };

    let mut response = json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "message": Value::Object(message),
            "finish_reason": finish_reason,
        }],
    });
    if !usage.is_null() {
        response["usage"] = usage;
    }
    Ok(response)
}

/// Whether an envelope field is meaningful: filters null, an empty string, and numeric 0 (including
/// the float 0.0 placeholder in Azure's content-filter preamble), so placeholders cannot freeze id/model/created.
fn envelope_value_meaningful(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
        _ => true,
    }
}

/// Merges one tool_calls delta into the index-keyed BTreeMap: OpenAI streaming puts id and name in
/// the first delta and chunks arguments afterwards, located by delta.index; without an index it
/// falls back to the position within the array (complete tool_calls in message form often carry no index, and using 0 would make them overwrite each other).
fn merge_tool_call_delta(
    tool_calls: &mut std::collections::BTreeMap<usize, Value>,
    delta: &Value,
    fallback_index: usize,
) {
    let index = delta
        .get("index")
        .and_then(|i| i.as_u64())
        .map(|i| i as usize)
        .unwrap_or(fallback_index);
    let target = tool_calls.entry(index).or_insert_with(|| {
        json!({
            "id": "",
            "type": "function",
            "function": {"name": "", "arguments": ""}
        })
    });
    if let Some(v) = delta
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        target["id"] = json!(v);
    }
    if let Some(func) = delta.get("function") {
        if let Some(name) = func
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            target["function"]["name"] = json!(name);
        }
        // arguments: strings concatenate directly while objects and arrays are serialized first, since a
        // non-streaming message snapshot often returns arguments as an object (an OpenAI compatibility
        // quirk), and accepting only strings would drop them and run the tool with empty input (C16)
        match func.get("arguments") {
            Some(Value::String(args)) => {
                if let Some(existing) = target["function"]["arguments"].as_str() {
                    target["function"]["arguments"] = json!(format!("{existing}{args}"));
                }
            }
            Some(v @ (Value::Object(_) | Value::Array(_))) => {
                let serialized = serde_json::to_string(v).unwrap_or_default();
                if let Some(existing) = target["function"]["arguments"].as_str() {
                    target["function"]["arguments"] = json!(format!("{existing}{serialized}"));
                }
            }
            _ => {}
        }
    }
}

// ============================================================================
// Usage recording (kept for the Claude conversion path)
// ============================================================================

fn log_forward_error(
    state: &ProxyState,
    ctx: &RequestContext,
    is_streaming: bool,
    error: &ProxyError,
) {
    use super::usage::logger::UsageLogger;

    let logger = UsageLogger::new(&state.db);
    let status_code = map_proxy_error_to_status(error);
    let error_message = get_error_message(error);
    let request_id = uuid::Uuid::new_v4().to_string();

    if let Err(e) = logger.log_error_with_context(
        request_id,
        ctx.provider.id.clone(),
        ctx.app_type_str.to_string(),
        ctx.request_model.clone(),
        status_code,
        error_message,
        ctx.latency_ms(),
        is_streaming,
        Some(ctx.session_id.clone()),
        None,
    ) {
        log::warn!("failed to record the failed-request log: {e}");
    }
}

/// Records the usage of a request
///
/// `outbound_model` anchors the price-by-request mode: the model actually sent upstream
/// (the truth after routing takeover mapping, equal to request_model when no mapping applies).
#[allow(clippy::too_many_arguments)]
async fn log_usage(
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

    if !usage_logging_enabled(state) {
        return;
    }

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
        log::warn!("[USG-001] failed to record usage: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        body_looks_like_sse, chat_sse_to_response_value, classify_body_for_diagnostics,
        codex_proxy_error_json, responses_sse_stream_to_anthropic_message,
        responses_sse_to_response_value, should_use_claude_transform_streaming, transform,
        upstream_body_parse_error,
    };
    use crate::proxy::ProxyError;
    use bytes::Bytes;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn body_looks_like_sse_detects_unlabeled_sse_prefixes() {
        assert!(body_looks_like_sse("data: {\"id\":\"1\"}\n\n"));
        assert!(body_looks_like_sse("event: message\ndata: {}\n\n"));
        // The other two SSE field lines may lead as well
        assert!(body_looks_like_sse("id: 1\ndata: {}\n\n"));
        assert!(body_looks_like_sse("retry: 3000\ndata: {}\n\n"));
        // OpenRouter sends a comment line before the stream
        assert!(body_looks_like_sse(
            ": OPENROUTER PROCESSING\n\ndata: {}\n\n"
        ));
        // BOM plus leading whitespace
        assert!(body_looks_like_sse("\u{feff}\n  data: {}\n\n"));
        // An HTML block page and plain text must not be mistaken for SSE
        assert!(!body_looks_like_sse("<html><body>blocked</body></html>"));
        assert!(!body_looks_like_sse("Bad Gateway"));
        assert!(!body_looks_like_sse(""));
    }

    #[test]
    fn upstream_body_parse_error_carries_field_diagnostics() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("content-type", "text/html".parse().unwrap());
        headers.insert("content-encoding", "gzip".parse().unwrap());
        let parse_err = serde_json::from_str::<serde_json::Value>("<html>").unwrap_err();

        let err = upstream_body_parse_error(
            "Failed to parse upstream response",
            &parse_err,
            &headers,
            "<html>\nblocked</html>",
        );

        match err {
            ProxyError::TransformError(msg) => {
                assert!(msg.contains("content-type: text/html"), "{msg}");
                assert!(msg.contains("content-encoding: gzip"), "{msg}");
                assert!(msg.contains("body-bytes: 21"), "{msg}");
                assert!(msg.contains("body-kind: html"), "{msg}");
                assert!(!msg.contains("blocked"), "{msg}");
            }
            other => panic!("expected TransformError, got {other:?}"),
        }
    }

    #[test]
    fn upstream_body_parse_error_marks_missing_headers() {
        let headers = axum::http::HeaderMap::new();
        let parse_err = serde_json::from_str::<serde_json::Value>("data:").unwrap_err();

        let err = upstream_body_parse_error("x", &parse_err, &headers, "data: oops");

        match err {
            ProxyError::TransformError(msg) => {
                assert!(msg.contains("content-type: <none>"), "{msg}");
                assert!(msg.contains("content-encoding: <none>"), "{msg}");
                assert!(msg.contains("body-kind: sse"), "{msg}");
            }
            other => panic!("expected TransformError, got {other:?}"),
        }
    }

    #[test]
    fn body_diagnostics_classifies_without_exposing_content() {
        assert_eq!(classify_body_for_diagnostics(""), "empty");
        assert_eq!(classify_body_for_diagnostics("  <HTML>blocked"), "html");
        assert_eq!(classify_body_for_diagnostics("data: {}\n\n"), "sse");
        assert_eq!(classify_body_for_diagnostics("{\"ok\":true}"), "json-like");
        assert_eq!(
            classify_body_for_diagnostics("decoded\u{fffd}payload"),
            "binary-or-encoded"
        );
        assert_eq!(classify_body_for_diagnostics("Bad Gateway"), "text");
    }

    #[test]
    fn chat_sse_to_response_value_collects_reasoning_alias() {
        // OpenRouter/Kimi use reasoning as a string while some gateways use the object form
        let sse = "data: {\"id\":\"c1\",\"model\":\"kimi-k2.6\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning\":\"think\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning\":{\"content\":\"ing\"},\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(
            response["choices"][0]["message"]["reasoning_content"],
            "thinking"
        );
        assert_eq!(response["choices"][0]["message"]["content"], "ok");
    }

    #[test]
    fn chat_sse_to_response_value_collects_reasoning_details() {
        // Providers such as MiMo and OpenRouter that send only reasoning_details (array form) rely on the
        // shared extractor and must not lose the thinking content
        let sse = "data: {\"id\":\"c1\",\"model\":\"mimo\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\"think\"}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\"ing\"}],\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(
            response["choices"][0]["message"]["reasoning_content"],
            "thinking"
        );
        assert_eq!(response["choices"][0]["message"]["content"], "ok");
    }

    #[test]
    fn responses_sse_to_response_value_handles_missing_trailing_blank_line() {
        // Mislabelled SSE fallback or non-conforming upstream: no blank line after the final response.completed
        let sse = "event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_tail\",\"status\":\"completed\",\"model\":\"gpt-5.4\",\"output\":[],\"usage\":{\"input_tokens\":3,\"output_tokens\":1}}}\n";

        let response = responses_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "resp_tail");
    }

    #[test]
    fn responses_sse_to_response_value_ignores_truncated_trailing_block() {
        // A truncated trailing block must not break a complete aggregate (the codex_oauth path reuses this function)
        let sse = "event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_ok\",\"status\":\"completed\",\"model\":\"gpt-5.4\",\"output\":[],\"usage\":{\"input_tokens\":3,\"output_tokens\":1}}}\n\
\n\
event: response.extra\n\
data: {\"type\":\"resp";

        let response = responses_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "resp_ok");
    }

    #[test]
    fn chat_sse_to_response_value_skips_azure_placeholder_envelope() {
        // Azure's content-filter preamble carries ""/0 placeholders that must not freeze envelope fields
        let sse = "data: {\"id\":\"\",\"model\":\"\",\"created\":0,\"object\":\"\",\"choices\":[],\"prompt_filter_results\":[]}\n\n\
data: {\"id\":\"chatcmpl-real\",\"model\":\"gpt-5.4\",\"created\":42,\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "chatcmpl-real");
        assert_eq!(response["model"], "gpt-5.4");
        assert_eq!(response["created"], 42);
    }

    #[test]
    fn chat_sse_to_response_value_tolerates_null_error_field() {
        // one-api style gateways attach "error": null to every chunk, which must not read as an upstream error
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"error\":null,\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn chat_sse_to_response_value_first_finish_reason_wins() {
        // kimi-k2.6 and others send another finish_reason block after tool_use, and that trailing "stop"
        // must not overwrite the earlier "tool_calls" (matching first-wins in streaming.rs)
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(response["choices"][0]["finish_reason"], "tool_calls");
    }

    #[test]
    fn chat_sse_to_response_value_unwraps_message_shaped_fake_stream() {
        // A fake-streaming relay wraps a whole chat.completion in one SSE event (message rather than delta)
        let sse = "data: {\"id\":\"c1\",\"object\":\"chat.completion\",\"model\":\"m\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"full answer\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "full answer");
        assert_eq!(response["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn chat_sse_to_response_value_message_snapshot_overrides_deltas() {
        // Mixed form: when deltas are followed by a full message snapshot, the snapshot overwrites them (no double counting)
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"par\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"full\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "full");
    }

    #[test]
    fn chat_sse_to_response_value_backfills_sparse_tool_call_ids() {
        // Empty shells at index gaps are dropped, and a missing id is backfilled as tool_call_{idx} from the original index
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"name\":\"f2\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        let tool_calls = response["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap();
        assert_eq!(tool_calls.len(), 1, "empty shell at index 0 dropped");
        assert_eq!(tool_calls[0]["id"], "tool_call_1");
        assert_eq!(tool_calls[0]["function"]["name"], "f2");
    }

    #[test]
    fn chat_sse_to_response_value_strips_bom_before_parsing() {
        // The sniffer accepts a BOM, so block parsing must strip it or the first data line is silently lost
        let sse = "\u{feff}data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn chat_sse_to_response_value_aggregates_text_finish_reason_and_usage() {
        let sse = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":123,\"model\":\"gpt-5.4\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"total_tokens\":12}}\n\n\
data: [DONE]\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "chatcmpl-1");
        assert_eq!(response["object"], "chat.completion");
        assert_eq!(response["model"], "gpt-5.4");
        assert_eq!(response["choices"][0]["message"]["role"], "assistant");
        assert_eq!(response["choices"][0]["message"]["content"], "Hello");
        assert_eq!(response["choices"][0]["finish_reason"], "stop");
        assert_eq!(response["usage"]["prompt_tokens"], 10);
    }

    #[test]
    fn chat_sse_to_response_value_merges_tool_call_argument_fragments() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"SF\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: [DONE]\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        let tool_call = &response["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tool_call["id"], "call_1");
        assert_eq!(tool_call["function"]["name"], "get_weather");
        assert_eq!(tool_call["function"]["arguments"], "{\"city\":\"SF\"}");
        assert_eq!(response["choices"][0]["finish_reason"], "tool_calls");
    }

    #[test]
    fn chat_sse_to_response_value_collects_reasoning_content() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"deepseek-r2\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"think\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"ing\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(
            response["choices"][0]["message"]["reasoning_content"],
            "thinking"
        );
        assert_eq!(response["choices"][0]["message"]["content"], "ok");
    }

    #[test]
    fn chat_sse_to_response_value_handles_missing_trailing_blank_line() {
        // Non-conforming upstream or clipped stream: no blank line after the last event
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn chat_sse_to_response_value_handles_crlf_delimiters() {
        // Real HTTP SSE separates events with \r\n\r\n per spec
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\r\n\
\r\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\r\n\
\r\n\
data: [DONE]\r\n\
\r\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
        assert_eq!(response["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn chat_sse_to_response_value_propagates_upstream_error_event() {
        let sse = "data: {\"error\":{\"message\":\"rate limited by gateway\",\"code\":429}}\n\n";

        let err = chat_sse_to_response_value(sse).unwrap_err();
        match err {
            ProxyError::TransformError(msg) => assert!(msg.contains("rate limited by gateway")),
            other => panic!("expected TransformError, got {other:?}"),
        }
    }

    #[test]
    fn chat_sse_to_response_value_rejects_truncated_stream() {
        // Content deltas only, with neither finish_reason nor [DONE]: close-delimited truncation cannot be
        // detected at the byte level, so it must error as truncated rather than silently return half the content
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"par\"},\"finish_reason\":null}]}\n\n";

        let err = chat_sse_to_response_value(sse).unwrap_err();
        match err {
            ProxyError::TransformError(msg) => assert!(msg.contains("truncated")),
            other => panic!("expected TransformError, got {other:?}"),
        }
    }

    #[test]
    fn chat_sse_to_response_value_accepts_done_marker_without_finish_reason() {
        // A non-conforming upstream may omit finish_reason yet close properly with [DONE], which counts as complete
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n\
data: [DONE]\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
        assert_eq!(
            response["choices"][0]["finish_reason"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn chat_sse_to_response_value_rejects_stream_without_chunks() {
        let err = chat_sse_to_response_value(": keepalive\n\ndata: [DONE]\n\n").unwrap_err();
        match err {
            ProxyError::TransformError(msg) => {
                assert!(msg.contains("No chat completion choices"))
            }
            other => panic!("expected TransformError, got {other:?}"),
        }
    }

    #[test]
    fn chat_sse_to_response_value_rejects_choiceless_stream_despite_done() {
        // Metadata or usage-only chunks plus [DONE], with no choice payload at any point:
        // [DONE] alone must not wrap an empty body as a false success (saw_choice needs a choice as evidence)
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":0,\"total_tokens\":1}}\n\n\
data: [DONE]\n\n";

        let err = chat_sse_to_response_value(sse).unwrap_err();
        match err {
            ProxyError::TransformError(msg) => {
                assert!(msg.contains("No chat completion choices"), "{msg}")
            }
            other => panic!("expected TransformError, got {other:?}"),
        }
    }

    #[test]
    fn chat_sse_to_response_value_huge_tool_call_index_does_not_oom() {
        // C1: a huge upstream-controlled index must not densify the array (the old implementation OOMed the
        // whole process); the BTreeMap holds one slot and the original index backfills the synthesized id
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":4000000000,\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();
        let tool_calls = response["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "tool_call_4000000000");
        assert_eq!(tool_calls[0]["function"]["name"], "f");
    }

    #[test]
    fn chat_sse_to_response_value_empty_delta_falls_back_to_message_snapshot() {
        // C3: one choice carrying both an empty delta:{} and a full message snapshot must not short-circuit
        // to the empty delta just because the key exists and lose the message content (its finish_reason would also defeat the guard)
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{},\"message\":{\"role\":\"assistant\",\"content\":\"full answer\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();
        assert_eq!(response["choices"][0]["message"]["content"], "full answer");
        assert_eq!(response["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn chat_sse_to_response_value_empty_delta_scaffold_does_not_wipe_real_content() {
        // The reverse C3 trap: when every chunk has a real content delta plus an empty message shell, the
        // empty message must not clear the accumulated content (a non-empty delta wins and no snapshot overwrite happens)
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"message\":{},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" there\"},\"message\":{},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();
        assert_eq!(response["choices"][0]["message"]["content"], "hi there");
    }

    #[test]
    fn chat_sse_to_response_value_object_form_tool_arguments_preserved() {
        // C16: arguments returned as an object in a message snapshot are serialized and kept, never lost as empty input
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"tool_calls\":[{\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":{\"city\":\"SF\"}}}]},\"finish_reason\":\"tool_calls\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();
        let args = response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(args).unwrap();
        assert_eq!(parsed["city"], "SF");
    }

    #[test]
    fn chat_sse_to_response_value_collects_refusal() {
        // C15: the delta.refusal string merges into visible content so a refusal does not become an empty false success
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"refusal\":\"I can't help with that.\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();
        assert_eq!(
            response["choices"][0]["message"]["content"],
            "I can't help with that."
        );
    }

    #[test]
    fn chat_sse_to_response_value_maps_legacy_function_call() {
        // C17: a legacy function_call becomes a single tool_call, so finish_reason function_call does not
        // map to tool_use with no tool block and stall the agent
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":null,\"function_call\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\\\"SF\\\"}\"}},\"finish_reason\":\"function_call\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();
        let tc = &response["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tc["function"]["name"], "get_weather");
        assert_eq!(tc["function"]["arguments"], "{\"city\":\"SF\"}");
    }

    #[test]
    fn chat_sse_to_response_value_event_error_fails_even_after_complete_choice() {
        // C18: an event:error (whose data has no error key) fails even after a complete choice and must
        // never masquerade as success
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"},\"finish_reason\":\"stop\"}]}\n\n\
event: error\n\
data: {\"message\":\"insufficient_user_quota\",\"code\":429}\n\n";

        let err = chat_sse_to_response_value(sse).unwrap_err();
        match err {
            ProxyError::TransformError(msg) => {
                assert!(msg.contains("insufficient_user_quota"), "{msg}")
            }
            other => panic!("expected TransformError, got {other:?}"),
        }
    }

    #[test]
    fn chat_sse_to_response_value_tolerates_empty_error_placeholder() {
        // C12: placeholder error shapes such as an empty object or empty message must not kill a successful stream
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"error\":{},\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();
        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn chat_sse_to_response_value_tolerates_truncated_residual_after_complete() {
        // C2: a clipped trailing block (half JSON) after a complete finish_reason block must not kill the finished aggregate
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n\
data: {\"usage\":{\"prompt_to";

        let response = chat_sse_to_response_value(sse).unwrap();
        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn chat_sse_to_response_value_float_zero_does_not_freeze_envelope() {
        // C14: a created field holding the float 0.0 placeholder must not freeze the envelope; the real value must override it
        let sse = "data: {\"id\":\"\",\"model\":\"\",\"created\":0.0,\"choices\":[]}\n\n\
data: {\"id\":\"chatcmpl-real\",\"model\":\"m\",\"created\":42,\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();
        assert_eq!(response["created"], 42);
        assert_eq!(response["id"], "chatcmpl-real");
    }

    #[test]
    fn chat_sse_to_response_value_synthesizes_id_when_absent() {
        // C9: synthesize a non-empty unique id when upstream has none, so downstream dedupe does not collapse to a colliding constant
        let sse = "data: {\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let r1 = chat_sse_to_response_value(sse).unwrap();
        let r2 = chat_sse_to_response_value(sse).unwrap();
        let id1 = r1["id"].as_str().unwrap();
        let id2 = r2["id"].as_str().unwrap();
        assert!(!id1.is_empty());
        assert_ne!(id1, id2, "id-less aggregations need distinct ids");
    }

    #[test]
    fn chat_sse_to_response_value_accepts_indented_data_lines() {
        // C4: indented data lines (which the sniffer accepts) must also aggregate rather than be silently lost
        let sse = "  data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse).unwrap();
        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn responses_sse_completed_then_trailing_failed_keeps_success() {
        // C8: after response.completed arrives, a full response.failed in the remainder must not flip the result
        // (the codex_oauth path reuses this function, where that trailing block used to be ignored as success)
        let sse = "event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_ok\",\"status\":\"completed\",\"model\":\"gpt-5.4\",\"output\":[]}}\n\n\
event: response.failed\n\
data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"boom\"}}}\n";

        let response = responses_sse_to_response_value(sse).unwrap();
        assert_eq!(response["id"], "resp_ok");
    }

    #[test]
    fn aggregated_chat_sse_round_trips_through_openai_to_anthropic() {
        // End to end: an SSE body with a mislabelled Content-Type -> aggregation -> the existing non-streaming converter -> Anthropic JSON
        let sse = "data: {\"id\":\"chatcmpl-9\",\"created\":1,\"model\":\"gpt-5.4\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"chatcmpl-9\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":1,\"total_tokens\":5}}\n\n\
data: [DONE]\n\n";

        let aggregated = chat_sse_to_response_value(sse).unwrap();
        let anthropic = transform::openai_to_anthropic(aggregated).unwrap();

        assert_eq!(anthropic["model"], "gpt-5.4");
        assert_eq!(anthropic["content"][0]["type"], "text");
        assert_eq!(anthropic["content"][0]["text"], "Hi");
        assert_eq!(anthropic["stop_reason"], "end_turn");
    }

    #[test]
    fn codex_oauth_responses_force_streaming_even_if_client_sent_false() {
        assert!(should_use_claude_transform_streaming(
            false,
            false,
            "openai_responses",
            true,
        ));
    }

    #[tokio::test]
    async fn non_streaming_codex_web_search_limit_stops_polling_upstream() {
        let chunks = vec![
            concat!(
                "event: response.created\n",
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_limit\",\"model\":\"gpt-5.6\"}}\n\n",
                "event: response.output_item.added\n",
                "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"id\":\"ws_allowed\",\"type\":\"web_search_call\",\"status\":\"in_progress\"}}\n\n"
            ),
            concat!(
                "event: response.output_item.added\n",
                "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"id\":\"ws_over_limit\",\"type\":\"web_search_call\",\"status\":\"in_progress\"}}\n\n"
            ),
            concat!(
                "event: response.output_text.delta\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"must never be polled\"}\n\n",
                "event: response.completed\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_limit\",\"status\":\"completed\"}}\n\n"
            ),
        ];
        let polls = Arc::new(AtomicUsize::new(0));
        let upstream_polls = Arc::clone(&polls);
        let upstream = futures::stream::unfold(
            (chunks.into_iter(), upstream_polls),
            |(mut chunks, polls)| async move {
                chunks.next().map(|chunk| {
                    polls.fetch_add(1, Ordering::SeqCst);
                    (
                        Ok::<_, std::io::Error>(Bytes::from_static(chunk.as_bytes())),
                        (chunks, polls),
                    )
                })
            },
        );

        let message = responses_sse_stream_to_anthropic_message(
            upstream,
            Some("web_search".to_string()),
            Some(1),
            std::time::Duration::ZERO,
        )
        .await
        .unwrap();

        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert_eq!(message["stop_reason"], "end_turn");
        assert_eq!(
            message["usage"]["server_tool_use"]["web_search_requests"],
            1
        );
        let content = message["content"].as_array().unwrap();
        assert_eq!(content.len(), 4);
        assert_eq!(content[0]["type"], "server_tool_use");
        assert_eq!(content[1]["content"]["error_code"], "unavailable");
        assert_eq!(content[2]["type"], "server_tool_use");
        assert_eq!(content[3]["content"]["error_code"], "max_uses_exceeded");
    }

    #[test]
    fn upstream_sse_response_always_uses_streaming_path() {
        assert!(should_use_claude_transform_streaming(
            false,
            true,
            "openai_chat",
            false,
        ));
    }

    #[test]
    fn non_streaming_response_stays_non_streaming_for_regular_openai_responses() {
        assert!(!should_use_claude_transform_streaming(
            false,
            false,
            "openai_responses",
            false,
        ));
    }

    #[test]
    fn responses_sse_to_response_value_collects_output_items() {
        let sse = r#"event: response.output_item.done
data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hello"}]}}

event: response.completed
data: {"type":"response.completed","response":{"id":"resp_1","status":"completed","model":"gpt-5.4","output":[],"usage":{"input_tokens":10,"output_tokens":2}}}

"#;

        let response = responses_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "resp_1");
        assert_eq!(response["output"][0]["type"], "message");
        assert_eq!(response["output"][0]["content"][0]["text"], "hello");
    }

    #[test]
    fn responses_sse_to_response_value_handles_crlf_delimiters() {
        // Real HTTP SSE separates events with \r\n\r\n per spec, so take_sse_block must handle both
        // separators or this path would TransformError on any standard upstream (including the Codex OAuth HTTPS backend).
        let sse = "event: response.output_item.done\r\n\
data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hi\"}]}}\r\n\
\r\n\
event: response.completed\r\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_crlf\",\"status\":\"completed\",\"model\":\"gpt-5.4\",\"output\":[],\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\r\n\
\r\n";

        let response = responses_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "resp_crlf");
        assert_eq!(response["output"][0]["type"], "message");
        assert_eq!(response["output"][0]["content"][0]["text"], "hi");
    }

    #[test]
    fn responses_sse_to_response_value_returns_err_on_response_failed() {
        let sse = "event: response.failed\n\
data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"upstream blew up\"}}}\n\n";

        let err = responses_sse_to_response_value(sse).unwrap_err();
        match err {
            ProxyError::TransformError(msg) => assert!(msg.contains("upstream blew up")),
            other => panic!("expected TransformError, got {other:?}"),
        }
    }

    #[test]
    fn responses_sse_to_response_value_errors_when_no_completed_event() {
        let sse = "event: response.output_item.done\n\
data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"message\"}}\n\n";

        assert!(responses_sse_to_response_value(sse).is_err());
    }

    #[test]
    fn codex_proxy_forward_error_includes_context_and_cause() {
        let error = ProxyError::ForwardFailed("connection failed: dns lookup failed".to_string());
        let body = codex_proxy_error_json("DeepSeek", "deepseek-chat", "/responses", &error);

        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("CC Switch local proxy failed"));
        assert!(message.contains("DeepSeek"));
        assert!(message.contains("deepseek-chat"));
        assert!(message.contains("/responses"));
        assert!(message.contains("dns lookup failed"));
        assert_eq!(body["error"]["code"], "cc_switch_forward_failed");
        assert_eq!(body["error"]["provider"], "DeepSeek");
        assert_eq!(body["error"]["model"], "deepseek-chat");
    }

    #[test]
    fn codex_proxy_upstream_error_normalizes_nonstandard_body() {
        let error = ProxyError::UpstreamError {
            status: 502,
            body: Some(
                r#"{"base_resp":{"status_code":2013,"status_msg":"upstream gateway failed"}}"#
                    .to_string(),
            ),
        };
        let body = codex_proxy_error_json("MiniMax", "abab6.5s", "/responses", &error);

        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("upstream_status: HTTP 502"));
        assert!(message.contains("upstream gateway failed"));
        assert_eq!(body["error"]["code"], 2013);
        assert_eq!(body["error"]["upstream_status"], 502);
    }

    #[test]
    fn codex_proxy_413_points_to_upstream_not_local_proxy() {
        // Simulate the 413 HTML page an upstream provider's nginx returns for client_max_body_size
        // (see issue #666: long contexts, large images, or big logs hitting the upstream size limit)
        let error = ProxyError::UpstreamError {
            status: 413,
            body: Some(
                "<html>\r\n<head><title>413 Request Entity Too Large</title></head>\r\n\
                 <body>\r\n<center><h1>413 Request Entity Too Large</h1></center>\r\n\
                 <hr><center>nginx/1.29.6</center>\r\n</body>\r\n</html>"
                    .to_string(),
            ),
        };
        let body = codex_proxy_error_json("HCAI", "gpt-5.5", "/responses", &error);

        let message = body["error"]["message"].as_str().unwrap();
        // No longer misleading users into blaming the local proxy
        assert!(!message.contains("CC Switch local proxy failed"));
        // Clearly points at upstream, the size limit, and actionable guidance
        assert!(message.contains("413"));
        assert!(message.to_lowercase().contains("upstream"));
        assert!(message.contains("/compact"));
        // Key point: never echo the whole nginx HTML back to the user
        assert!(!message.contains("<html>"));
        assert!(!message.contains("nginx/1.29.6"));
        // The structured fields remain, for programmatic use and UI display
        assert_eq!(body["error"]["upstream_status"], 413);
        assert_eq!(body["error"]["provider"], "HCAI");
        assert_eq!(body["error"]["model"], "gpt-5.5");
        assert_eq!(body["error"]["endpoint"], "/responses");
    }
}
