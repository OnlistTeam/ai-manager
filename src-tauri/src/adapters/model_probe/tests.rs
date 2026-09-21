//! Network-level tests for the model probe.
//!
//! These run against a real loopback TCP server, mirroring
//! `services/stream_check.rs`. The assertion direction is the inverse of the
//! reachability probe's: this path *must* send the credential.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use reqwest::Client;

use super::*;

/// Serves one canned response per connection and returns every request it saw.
struct LocalService {
    address: String,
    requests: mpsc::Receiver<String>,
    handle: thread::JoinHandle<()>,
}

fn serve(responses: Vec<(u16, &'static str)>) -> LocalService {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local probe server");
    let address = listener
        .local_addr()
        .expect("read local address")
        .to_string();
    let (request_tx, requests) = mpsc::channel();

    let handle = thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set request timeout");

            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            let mut header_end = None;
            while request.len() < 64 * 1024 {
                let Ok(read) = socket.read(&mut chunk) else {
                    break;
                };
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                if header_end.is_none() {
                    header_end = request
                        .windows(4)
                        .position(|window| window == b"\r\n\r\n")
                        .map(|index| index + 4);
                }
                // Keep reading until the announced body has arrived, so POST
                // payloads can be asserted on.
                if let Some(start) = header_end {
                    let headers = String::from_utf8_lossy(&request[..start]).to_ascii_lowercase();
                    let declared = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= start + declared {
                        break;
                    }
                }
            }

            request_tx
                .send(String::from_utf8_lossy(&request).into_owned())
                .expect("return captured request");

            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes());
        }
    });

    LocalService {
        address,
        requests,
        handle,
    }
}

impl LocalService {
    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    fn next_request(&self) -> String {
        self.requests
            .recv_timeout(Duration::from_secs(5))
            .expect("capture probe request")
    }

    fn finish(self) {
        drop(self.requests);
        let _ = self.handle.join();
    }
}

fn client() -> Client {
    Client::builder().no_proxy().build().expect("build client")
}

fn target(base_url: String, protocol: ProviderWireProtocol) -> ProbeTarget {
    ProbeTarget {
        base_url,
        api_key: "sk-probe-secret-value".to_string(),
        protocol,
    }
}

fn text_request(model: &str, prompt: &str) -> ModelProbeRequest {
    serde_json::from_value(serde_json::json!({
        "model": model,
        "kind": "text",
        "prompt": prompt,
    }))
    .expect("build probe request")
}

#[tokio::test]
async fn the_model_probe_does_send_the_credential_header() {
    let service = serve(vec![(
        200,
        r#"{"choices":[{"message":{"content":"pong"}}]}"#,
    )]);
    let target = target(service.base_url(), ProviderWireProtocol::OpenAi);

    let outcome = probe_model(&client(), &target, &text_request("gpt-5.2", "ping"))
        .await
        .expect("probe local service");

    let request = service.next_request();
    assert!(request.starts_with("POST /v1/chat/completions "));
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer sk-probe-secret-value"));
    assert!(request.contains("\"ping\""));
    assert_eq!(
        outcome.reply,
        ModelProbeReply::Text {
            text: "pong".to_string()
        }
    );
    assert_eq!(outcome.http_status, Some(200));
    service.finish();
}

#[tokio::test]
async fn an_outcome_debug_rendering_carries_neither_key_nor_reply() {
    let service = serve(vec![(
        200,
        r#"{"choices":[{"message":{"content":"a private answer"}}]}"#,
    )]);
    let target = target(service.base_url(), ProviderWireProtocol::OpenAi);

    let outcome = probe_model(
        &client(),
        &target,
        &text_request("gpt-5.2", "a private question"),
    )
    .await
    .expect("probe local service");

    let debug = format!("{outcome:?}");
    assert!(debug.contains("gpt-5.2"));
    assert!(!debug.contains("a private answer"));
    assert!(!debug.contains("sk-probe-secret-value"));
    service.next_request();
    service.finish();
}

#[tokio::test]
async fn an_anthropic_service_is_probed_with_its_own_headers() {
    let service = serve(vec![(
        200,
        r#"{"content":[{"type":"text","text":"pong"}]}"#,
    )]);
    let target = target(service.base_url(), ProviderWireProtocol::Anthropic);

    probe_model(&client(), &target, &text_request("claude-opus-5", "ping"))
        .await
        .expect("probe local service");

    let request = service.next_request().to_ascii_lowercase();
    assert!(request.starts_with("post /v1/messages "));
    assert!(request.contains("x-api-key: sk-probe-secret-value"));
    assert!(request.contains("anthropic-version: 2023-06-01"));
    assert!(!request.contains("authorization:"));
    service.finish();
}

#[tokio::test]
async fn an_anthropic_relay_that_only_speaks_bearer_gets_one_retry() {
    let service = serve(vec![
        (401, r#"{"error":{"message":"missing bearer"}}"#),
        (200, r#"{"content":[{"type":"text","text":"pong"}]}"#),
    ]);
    let target = target(service.base_url(), ProviderWireProtocol::Anthropic);

    let outcome = probe_model(&client(), &target, &text_request("claude-opus-5", "ping"))
        .await
        .expect("probe local service");

    let first = service.next_request().to_ascii_lowercase();
    assert!(first.contains("x-api-key:"));
    let retry = service.next_request().to_ascii_lowercase();
    assert!(retry.contains("authorization: bearer sk-probe-secret-value"));
    assert_eq!(
        outcome.reply,
        ModelProbeReply::Text {
            text: "pong".to_string()
        }
    );
    service.finish();
}

#[tokio::test]
async fn a_rejected_key_is_an_answer_not_a_failure() {
    let service = serve(vec![(401, r#"{"error":{"message":"invalid api key"}}"#)]);
    let target = target(service.base_url(), ProviderWireProtocol::OpenAi);

    let outcome = probe_model(&client(), &target, &text_request("gpt-5.2", "ping"))
        .await
        .expect("a refusal is still a completed probe");

    assert_eq!(outcome.http_status, Some(401));
    match &outcome.reply {
        ModelProbeReply::Rejected { detail } => assert!(detail.contains("invalid api key")),
        other => panic!("unexpected reply: {other:?}"),
    }
    // The detail is for the dialog, never for a log.
    assert_eq!(
        format!("{:?}", outcome.reply),
        "Rejected(<redacted>)",
        "an upstream explanation must not be loggable"
    );
    service.next_request();
    service.finish();
}

#[tokio::test]
async fn a_catalogue_is_fetched_and_classified() {
    let service = serve(vec![(
        200,
        r#"{"data":[{"id":"gpt-5.2"},{"id":"gpt-image-2"},{"id":"~auto"}]}"#,
    )]);
    let target = target(service.base_url(), ProviderWireProtocol::OpenAi);

    let catalog = list_models(&client(), &target)
        .await
        .expect("list local models");

    let request = service.next_request();
    assert!(request.starts_with("GET /v1/models "));
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer sk-probe-secret-value"));
    assert_eq!(
        catalog.models,
        vec![
            ProbeModel {
                id: "gpt-5.2".to_string(),
                kind: ProbeModelKind::Text
            },
            ProbeModel {
                id: "gpt-image-2".to_string(),
                kind: ProbeModelKind::Image
            },
        ]
    );
    assert!(!catalog.truncated);
    service.finish();
}

#[tokio::test]
async fn a_scoped_catalogue_that_is_absent_retries_at_the_origin() {
    let service = serve(vec![
        (404, r#"{"error":"not found"}"#),
        (200, r#"{"data":[{"id":"gpt-5.2"}]}"#),
    ]);
    let target = target(
        format!("{}/relay", service.base_url()),
        ProviderWireProtocol::OpenAi,
    );

    let catalog = list_models(&client(), &target)
        .await
        .expect("list local models");

    assert!(service.next_request().starts_with("GET /relay/v1/models "));
    assert!(service.next_request().starts_with("GET /v1/models "));
    assert_eq!(catalog.models.len(), 1);
    service.finish();
}

#[tokio::test]
async fn a_rejected_catalogue_is_not_retried_at_the_origin() {
    let service = serve(vec![(403, r#"{"error":"forbidden"}"#)]);
    let target = target(
        format!("{}/relay", service.base_url()),
        ProviderWireProtocol::OpenAi,
    );

    let catalog = list_models(&client(), &target)
        .await
        .expect("a refusal is still a completed catalogue request");

    let rejection = catalog.rejection.expect("carry the refusal");
    assert_eq!(rejection.status, 403);
    assert!(rejection.detail.contains("forbidden"));
    assert!(catalog.models.is_empty());
    assert!(service.next_request().starts_with("GET /relay/v1/models "));
    assert!(
        service
            .requests
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "a credential rejection must not trigger a second attempt"
    );
    service.finish();
}

#[tokio::test]
async fn an_image_returned_as_a_link_is_reported_rather_than_fetched() {
    let service = serve(vec![(
        200,
        r#"{"data":[{"url":"https://cdn.example.test/a.png"}]}"#,
    )]);
    let target = target(service.base_url(), ProviderWireProtocol::OpenAi);
    let request: ModelProbeRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-image-2",
        "kind": "image",
        "prompt": "a red circle",
    }))
    .expect("build probe request");

    let outcome = probe_model(&client(), &target, &request)
        .await
        .expect("probe local service");

    assert_eq!(outcome.reply, ModelProbeReply::ImageLinkOnly);
    let captured = service.next_request();
    assert!(captured.starts_with("POST /v1/images/generations "));
    assert!(captured.contains("b64_json"));
    service.finish();
}

/// Image generation is an OpenAI route even on a service whose chat route is
/// not, so a relay set up for Claude Code has to be asked rather than refused
/// on a guess about what it supports.
#[tokio::test]
async fn an_anthropic_dialect_still_asks_the_openai_image_route() {
    let service = serve(vec![(
        200,
        r#"{"data":[{"b64_json":"aGk=","mime_type":"image/png"}]}"#,
    )]);
    let target = target(
        format!("http://{}", service.address),
        ProviderWireProtocol::Anthropic,
    );
    let request: ModelProbeRequest = serde_json::from_value(serde_json::json!({
        "model": "openai/gpt-image-2.5-sunburst",
        "kind": "image",
        "prompt": "a flying cat",
    }))
    .expect("build probe request");

    let outcome = probe_model(&client(), &target, &request)
        .await
        .expect("probe local service");

    assert!(matches!(outcome.reply, ModelProbeReply::Image { .. }));
    let captured = service.next_request();
    assert!(
        captured.starts_with("POST /v1/images/generations "),
        "{captured}"
    );
    // The image route wants the OpenAI header even though the tool's chat route
    // wants `x-api-key`.
    let headers = captured.to_ascii_lowercase();
    assert!(
        headers.contains("authorization: bearer sk-probe-secret-value"),
        "{captured}"
    );
    assert!(!headers.contains("x-api-key"), "{captured}");
    service.finish();
}

/// A host with no image route answers 404, and that answer is the result the
/// user sees: it is the service's own statement, not this application guessing.
#[tokio::test]
async fn a_host_without_an_image_route_reports_its_own_refusal() {
    let service = serve(vec![(404, r#"{"error":{"message":"not found"}}"#)]);
    let target = target(
        format!("http://{}", service.address),
        ProviderWireProtocol::Anthropic,
    );
    let request: ModelProbeRequest = serde_json::from_value(serde_json::json!({
        "model": "claude-opus-5",
        "kind": "image",
        "prompt": "a red circle",
    }))
    .expect("build probe request");

    let outcome = probe_model(&client(), &target, &request)
        .await
        .expect("probe local service");

    assert_eq!(outcome.http_status, Some(404));
    assert!(matches!(outcome.reply, ModelProbeReply::Rejected { .. }));
    service.finish();
}
