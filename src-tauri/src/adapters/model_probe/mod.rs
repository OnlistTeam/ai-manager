//! The outbound model probe (ADR-0041).
//!
//! Unlike every other probe in this product, this one sends the saved
//! credential and reads the response body, because a rejected key and a dead
//! host are otherwise indistinguishable. In exchange nothing it touches may be
//! logged: `ProbeError` renders its upstream detail as `<redacted>`, and the
//! address never appears in an error at all.

mod endpoint;
mod wire;

#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};

use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde_json::Value;

use crate::domain::{
    ModelCatalog, ModelCatalogRejection, ModelProbeOutcome, ModelProbeReply, ModelProbeRequest,
    ProbeModel, ProbeModelKind, ProviderWireProtocol, MAX_PROBE_IMAGE_BASE64_BYTES,
    MAX_PROBE_MODELS,
};
use crate::platform::redact::redact_secrets;

pub use endpoint::classify_model;

const CATALOG_TIMEOUT: Duration = Duration::from_secs(10);
const TEXT_TIMEOUT: Duration = Duration::from_secs(30);
/// Image models routinely take a minute; a 30 s ceiling would report working
/// services as broken.
const IMAGE_TIMEOUT: Duration = Duration::from_secs(120);

const CATALOG_BODY_CAP: usize = 8 * 1024 * 1024;
const TEXT_BODY_CAP: usize = 2 * 1024 * 1024;
const IMAGE_BODY_CAP: usize = 12 * 1024 * 1024;
const ERROR_BODY_CAP: usize = 64 * 1024;
/// Enough of an upstream error to be actionable ("insufficient balance",
/// "model not found") without pasting a stack trace into the dialog.
const ERROR_DETAIL_CHARS: usize = 200;

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Where to send the probe. Assembled in the application layer from the saved
/// service; the renderer never supplies any of it.
pub struct ProbeTarget {
    pub base_url: String,
    pub api_key: String,
    pub protocol: ProviderWireProtocol,
}

/// Machine-readable outcomes. The renderer owns all display copy.
///
/// A service that answered and refused is **not** here: that is a
/// [`ModelProbeReply::Rejected`], because the test succeeded in learning
/// something. These are the cases where nothing was learned at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeError {
    Timeout,
    Network,
    ResponseTooLarge,
    ResponseUnreadable,
    ImageTooLarge,
}

fn transport_error(error: &reqwest::Error) -> ProbeError {
    if error.is_timeout() {
        ProbeError::Timeout
    } else {
        ProbeError::Network
    }
}

/// Applies a dialect's credential header.
///
/// An empty key means the service was saved without one, which local gateways
/// routinely accept; the header is omitted rather than sent empty.
fn authorize(
    builder: RequestBuilder,
    protocol: ProviderWireProtocol,
    api_key: &str,
) -> RequestBuilder {
    let builder = match protocol {
        ProviderWireProtocol::Anthropic => builder.header("anthropic-version", ANTHROPIC_VERSION),
        ProviderWireProtocol::OpenAi
        | ProviderWireProtocol::OpenAiResponses
        | ProviderWireProtocol::Gemini => builder,
    };
    if api_key.is_empty() {
        return builder;
    }
    match protocol {
        ProviderWireProtocol::OpenAi | ProviderWireProtocol::OpenAiResponses => {
            builder.header("authorization", format!("Bearer {api_key}"))
        }
        ProviderWireProtocol::Anthropic => builder.header("x-api-key", api_key),
        ProviderWireProtocol::Gemini => builder.header("x-goog-api-key", api_key),
    }
}

/// Which credential header to send, and which to try once if the first is
/// refused.
///
/// Two cases need the second attempt, in opposite directions. Anthropic-shaped
/// relays commonly only understand `Authorization: Bearer`, and the image route
/// is an OpenAI route even on a service whose chat route is not, so a service
/// that wants its native header there has to be given the chance.
#[derive(Debug, Clone, Copy)]
struct AuthPlan {
    protocol: ProviderWireProtocol,
    fallback: Option<ProviderWireProtocol>,
}

impl AuthPlan {
    /// The tool's own dialect, with the Bearer fallback relays need.
    fn native(protocol: ProviderWireProtocol) -> Self {
        Self {
            protocol,
            fallback: match protocol {
                ProviderWireProtocol::Anthropic => Some(ProviderWireProtocol::OpenAi),
                ProviderWireProtocol::OpenAi
                | ProviderWireProtocol::OpenAiResponses
                | ProviderWireProtocol::Gemini => None,
            },
        }
    }

    /// OpenAI conventions first, falling back to the tool's own dialect.
    fn openai_first(protocol: ProviderWireProtocol) -> Self {
        Self {
            protocol: ProviderWireProtocol::OpenAi,
            // Responses already authorises exactly like OpenAI, so there is
            // nothing to fall back to.
            fallback: (!matches!(
                protocol,
                ProviderWireProtocol::OpenAi | ProviderWireProtocol::OpenAiResponses
            ))
            .then_some(protocol),
        }
    }
}

fn rejected_credential(status: StatusCode) -> bool {
    status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
}

/// Sends one authorized request, retrying once with the plan's fallback header
/// when the first attempt is refused. `build` is called per attempt because a
/// `RequestBuilder` cannot be cloned once it owns a body.
async fn send_authorized<F>(api_key: &str, plan: AuthPlan, build: F) -> Result<Response, ProbeError>
where
    F: Fn() -> RequestBuilder,
{
    let first = authorize(build(), plan.protocol, api_key)
        .send()
        .await
        .map_err(|error| transport_error(&error))?;

    let Some(fallback) = plan.fallback else {
        return Ok(first);
    };
    if api_key.is_empty() || !rejected_credential(first.status()) {
        return Ok(first);
    }

    let retried = authorize(build(), fallback, api_key)
        .send()
        .await
        .map_err(|error| transport_error(&error))?;
    Ok(if rejected_credential(retried.status()) {
        first
    } else {
        retried
    })
}

/// Reads at most `cap` bytes. A body that exceeds the cap is an error rather
/// than a silent truncation, because a truncated JSON document parses as
/// garbage and would be reported as an unusable service.
async fn read_capped(mut response: Response, cap: usize) -> Result<Vec<u8>, ProbeError> {
    let mut body = Vec::new();
    loop {
        let chunk = response
            .chunk()
            .await
            .map_err(|error| transport_error(&error))?;
        let Some(chunk) = chunk else { break };
        if body.len() + chunk.len() > cap {
            return Err(ProbeError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn head_chars(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    trimmed.chars().take(limit).collect::<String>() + "…"
}

/// Turns a non-2xx response into the upstream's own explanation, redacted and
/// truncated. An empty body is reported as an empty detail; the status code
/// carries the rest.
async fn rejection_detail(response: Response) -> String {
    match read_capped(response, ERROR_BODY_CAP).await {
        Ok(body) => head_chars(
            &redact_secrets(&String::from_utf8_lossy(&body)),
            ERROR_DETAIL_CHARS,
        ),
        Err(_) => String::new(),
    }
}

async fn json_body(response: Response, cap: usize) -> Result<Value, ProbeError> {
    let body = read_capped(response, cap).await?;
    serde_json::from_slice(&body).map_err(|_| ProbeError::ResponseUnreadable)
}

/// Statuses that mean "this path is not the catalogue" rather than "this
/// service refused you". Only these justify a second attempt at the origin.
fn catalog_path_missing(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED | StatusCode::GONE
    )
}

fn build_catalog(protocol: ProviderWireProtocol, ids: Vec<String>) -> ModelCatalog {
    let truncated = ids.len() > MAX_PROBE_MODELS;
    ModelCatalog {
        protocol,
        models: ids
            .into_iter()
            .take(MAX_PROBE_MODELS)
            .map(|id| ProbeModel {
                kind: endpoint::classify_model(&id),
                id,
            })
            .collect(),
        truncated,
        rejection: None,
    }
}

fn rejected_catalog(protocol: ProviderWireProtocol, status: u16, detail: String) -> ModelCatalog {
    ModelCatalog {
        protocol,
        models: Vec::new(),
        truncated: false,
        rejection: Some(ModelCatalogRejection { status, detail }),
    }
}

/// What one catalogue request produced.
enum CatalogAttempt {
    Listed(Vec<String>),
    /// The endpoint does not serve a catalogue at this path; worth one retry
    /// somewhere else.
    Absent,
    /// The endpoint refused. Never retried: a rejected key is rejected
    /// everywhere on the same host.
    Refused {
        status: u16,
        detail: String,
    },
}

async fn catalog_attempt(
    client: &Client,
    target: &ProbeTarget,
    url: &str,
) -> Result<CatalogAttempt, ProbeError> {
    let response = send_authorized(&target.api_key, AuthPlan::native(target.protocol), || {
        client.get(url).timeout(CATALOG_TIMEOUT)
    })
    .await?;
    let status = response.status();
    if !status.is_success() {
        if catalog_path_missing(status) {
            return Ok(CatalogAttempt::Absent);
        }
        return Ok(CatalogAttempt::Refused {
            status: status.as_u16(),
            detail: rejection_detail(response).await,
        });
    }
    let body = json_body(response, CATALOG_BODY_CAP).await?;
    Ok(match wire::parse_catalog(target.protocol, &body) {
        Some(ids) => CatalogAttempt::Listed(ids),
        // A 2xx that is not a model list means this path belongs to something
        // else, which is the same situation as a 404.
        None => CatalogAttempt::Absent,
    })
}

/// Fetches the endpoint's model list.
///
/// A service that answers 2xx with something that is not a model list, or 404s
/// the scoped path, gets one retry at the domain root: relays commonly mount
/// their API there while the user saved a deeper base URL.
///
/// Plenty of relays publish no catalogue at all. That is an empty list, not an
/// error: the dialog falls back to a typed model name, and the probe itself
/// still works.
pub async fn list_models(
    client: &Client,
    target: &ProbeTarget,
) -> Result<ModelCatalog, ProbeError> {
    let scoped = endpoint::catalog_url(&target.base_url, target.protocol);
    match catalog_attempt(client, target, &scoped).await? {
        CatalogAttempt::Listed(ids) => return Ok(build_catalog(target.protocol, ids)),
        CatalogAttempt::Refused { status, detail } => {
            return Ok(rejected_catalog(target.protocol, status, detail))
        }
        CatalogAttempt::Absent => {}
    }

    let Some(origin) = endpoint::origin_catalog_url(&target.base_url, target.protocol) else {
        return Ok(build_catalog(target.protocol, Vec::new()));
    };
    match catalog_attempt(client, target, &origin).await? {
        CatalogAttempt::Listed(ids) => Ok(build_catalog(target.protocol, ids)),
        CatalogAttempt::Refused { status, detail } => {
            Ok(rejected_catalog(target.protocol, status, detail))
        }
        CatalogAttempt::Absent => Ok(build_catalog(target.protocol, Vec::new())),
    }
}

/// Statuses that mean "this address does not serve this route", as opposed to
/// "this service refused you".
///
/// `403` is in the list because path allowlists in front of a relay answer with
/// it: an nginx that only forwards `/v1/` returns 403 for `/responses`, which is
/// indistinguishable from a refused key until a second address is tried. That
/// ambiguity costs nothing — a genuinely refused key is refused at the
/// alternative too, and no suggestion is made.
fn route_missing(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::NOT_FOUND
            | StatusCode::METHOD_NOT_ALLOWED
            | StatusCode::GONE
            | StatusCode::FORBIDDEN
    )
}

/// One text request against one base URL.
async fn text_attempt(
    client: &Client,
    target: &ProbeTarget,
    request: &ModelProbeRequest,
    base_url: &str,
) -> Result<(StatusCode, ModelProbeReply), ProbeError> {
    let url = endpoint::text_url(base_url, target.protocol, &request.model);
    let body = wire::text_request_body(target.protocol, &request.model, &request.prompt);
    let response = send_authorized(&target.api_key, AuthPlan::native(target.protocol), || {
        client.post(&url).timeout(TEXT_TIMEOUT).json(&body)
    })
    .await?;

    let status = response.status();
    if !status.is_success() {
        return Ok((
            status,
            ModelProbeReply::Rejected {
                detail: rejection_detail(response).await,
            },
        ));
    }
    let parsed = json_body(response, TEXT_BODY_CAP).await?;
    Ok((status, wire::text_reply(target.protocol, &parsed)))
}

/// Sends the text request, and — only when the saved address turned out not to
/// serve the route at all — checks whether the other spelling of it does.
///
/// The verification is a real request and costs what one costs. It is only ever
/// made after a failure, so nothing was generated by the first attempt, and its
/// own reply is discarded: showing it would amount to reporting that the saved
/// address works, which is the opposite of what was just learned.
async fn probe_text(
    client: &Client,
    target: &ProbeTarget,
    request: &ModelProbeRequest,
) -> Result<(Option<u16>, ModelProbeReply, Option<String>), ProbeError> {
    let (status, reply) = text_attempt(client, target, request, &target.base_url).await?;
    if !route_missing(status) {
        return Ok((Some(status.as_u16()), reply, None));
    }
    let Some(alternate) = endpoint::alternate_base_url(&target.base_url) else {
        return Ok((Some(status.as_u16()), reply, None));
    };

    // A transport failure against the alternative says nothing about the saved
    // address, so it is swallowed rather than replacing the real result.
    let suggestion = match text_attempt(client, target, request, &alternate).await {
        Ok((alternate_status, _)) if alternate_status.is_success() => Some(alternate),
        _ => None,
    };
    Ok((Some(status.as_u16()), reply, suggestion))
}

async fn probe_image(
    client: &Client,
    target: &ProbeTarget,
    request: &ModelProbeRequest,
) -> Result<(Option<u16>, ModelProbeReply), ProbeError> {
    let url = endpoint::image_url(&target.base_url);
    let body = wire::image_request_body(&request.model, &request.prompt);
    // `/v1/images/generations` is the only image interface in practice, so it is
    // spoken here whatever the tool's chat dialect is. A relay configured for
    // Claude Code usually serves it; the official Anthropic and Gemini hosts do
    // not, and answer 404. Reporting that 404 is honest, whereas refusing before
    // asking would be this application making a claim about someone else's
    // service that it has no way to know.
    let response = send_authorized(
        &target.api_key,
        AuthPlan::openai_first(target.protocol),
        || client.post(&url).timeout(IMAGE_TIMEOUT).json(&body),
    )
    .await?;

    let status = response.status();
    if !status.is_success() {
        return Ok((
            Some(status.as_u16()),
            ModelProbeReply::Rejected {
                detail: rejection_detail(response).await,
            },
        ));
    }
    let parsed = json_body(response, IMAGE_BODY_CAP).await?;
    let status = Some(status.as_u16());
    match wire::image_reply(&parsed) {
        wire::ImageOutcome::Data(ModelProbeReply::Image { mime, base64 }) => {
            if base64.len() > MAX_PROBE_IMAGE_BASE64_BYTES {
                return Err(ProbeError::ImageTooLarge);
            }
            Ok((status, ModelProbeReply::Image { mime, base64 }))
        }
        wire::ImageOutcome::Data(reply) => Ok((status, reply)),
        wire::ImageOutcome::UrlOnly => Ok((status, ModelProbeReply::ImageLinkOnly)),
        wire::ImageOutcome::Empty => Ok((status, ModelProbeReply::Empty)),
    }
}

/// Sends one real request to the service and reports what came back.
pub async fn probe_model(
    client: &Client,
    target: &ProbeTarget,
    request: &ModelProbeRequest,
) -> Result<ModelProbeOutcome, ProbeError> {
    let started = Instant::now();
    let (http_status, reply, suggested_base_url) = match request.kind {
        ProbeModelKind::Text => probe_text(client, target, request).await?,
        // The image route is addressed with a version segment whatever the
        // tool's dialect, so a missing one is never the failure there and a
        // suggestion would have nothing to correct.
        ProbeModelKind::Image => {
            let (status, reply) = probe_image(client, target, request).await?;
            (status, reply, None)
        }
    };
    Ok(ModelProbeOutcome {
        model: request.model.clone(),
        latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        http_status,
        reply,
        suggested_base_url,
    })
}
