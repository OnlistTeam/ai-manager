//! Router layer that restores placeholders in every reply the local proxy
//! sends back to a tool, whatever handler produced it and whichever API shape
//! it was translated into.

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use http::header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE};
use http::StatusCode;
use serde_json::Value;

use super::engine::Engine;
use super::stream::SseRestorer;

pub(crate) async fn restore_response(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    match super::engine_for_restore() {
        Some(engine) => restore_with(response, engine).await,
        None => response,
    }
}

async fn restore_with(response: Response, engine: Arc<Engine>) -> Response {
    let encoded = response
        .headers()
        .get(CONTENT_ENCODING)
        .is_some_and(|value| !value.as_bytes().eq_ignore_ascii_case(b"identity"));
    if encoded {
        // A compressed body cannot be rewritten; handlers decode replies they
        // rebuild, so only unusual pass-through encodings end up here.
        return response;
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if content_type.starts_with("text/event-stream") {
        restore_stream(response, engine)
    } else if content_type.contains("json") {
        restore_json(response, engine).await
    } else {
        response
    }
}

fn restore_stream(response: Response, engine: Arc<Engine>) -> Response {
    let (mut parts, body) = response.into_parts();
    parts.headers.remove(CONTENT_LENGTH);
    let mut restorer = SseRestorer::new(engine);
    let mut upstream = body.into_data_stream();
    let restored = async_stream::stream! {
        while let Some(chunk) = upstream.next().await {
            match chunk {
                Ok(bytes) => {
                    let out = restorer.push(&bytes);
                    if !out.is_empty() {
                        yield Ok::<Bytes, axum::Error>(Bytes::from(out));
                    }
                }
                Err(error) => {
                    let out = restorer.finish();
                    if !out.is_empty() {
                        yield Ok(Bytes::from(out));
                    }
                    yield Err(error);
                    return;
                }
            }
        }
        let out = restorer.finish();
        if !out.is_empty() {
            yield Ok(Bytes::from(out));
        }
    };
    Response::from_parts(parts, Body::from_stream(restored))
}

async fn restore_json(response: Response, engine: Arc<Engine>) -> Response {
    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => {
            log::warn!("[Privacy] reply body could not be read for restoring");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    match restore_json_bytes(&engine, &bytes) {
        Some(restored) => {
            parts.headers.remove(CONTENT_LENGTH);
            Response::from_parts(parts, Body::from(restored))
        }
        None => Response::from_parts(parts, Body::from(bytes)),
    }
}

/// `{{` never occurs in JSON syntax, only inside strings, so a body without
/// it has nothing to restore and is passed on byte for byte.
pub(crate) fn restore_json_bytes(engine: &Engine, bytes: &[u8]) -> Option<Vec<u8>> {
    if !bytes.windows(2).any(|pair| pair == b"{{") {
        return None;
    }
    let mut value: Value = serde_json::from_slice(bytes).ok()?;
    if !engine.restore_value(&mut value) {
        return None;
    }
    serde_json::to_vec(&value).ok()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{Body, Bytes};
    use axum::response::Response;
    use futures::stream;
    use http::header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE};

    use super::super::engine::Engine;
    use super::super::placeholder::HashKey;
    use super::restore_with;

    const EMAIL: &str = "li.si@mail.qq.com";

    fn engine_with_email() -> (Arc<Engine>, String) {
        let engine = Arc::new(Engine::new(HashKey::new([3; 32]), 16));
        let mut count = 0;
        let placeholder = engine.mask_text(EMAIL, &mut count).expect("an email");
        (engine, placeholder)
    }

    async fn body_text(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        String::from_utf8(bytes.to_vec()).expect("utf-8")
    }

    #[tokio::test]
    async fn streamed_replies_are_restored_across_network_chunks() {
        let (engine, placeholder) = engine_with_email();
        let event = |text: &str| {
            format!(
                "data: {}\n\n",
                serde_json::json!({"choices": [{"index": 0, "delta": {"content": text}}]})
            )
        };
        let wire = format!("{}{}", event(&placeholder[..9]), event(&placeholder[9..]));
        let (first, second) = wire.split_at(wire.len() / 2 + 3);
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from(first.to_owned())),
            Ok(Bytes::from(second.to_owned())),
        ];
        let response = Response::builder()
            .header(CONTENT_TYPE, "text/event-stream")
            .body(Body::from_stream(stream::iter(chunks)))
            .expect("response");
        let text = body_text(restore_with(response, engine).await).await;
        assert!(text.contains(EMAIL), "{text}");
        assert!(!text.contains("{{"), "{text}");
    }

    #[tokio::test]
    async fn json_replies_are_restored_and_lose_their_stale_length() {
        let (engine, placeholder) = engine_with_email();
        let body =
            serde_json::json!({"content": [{"type": "text", "text": placeholder}]}).to_string();
        let response = Response::builder()
            .header(CONTENT_TYPE, "application/json")
            .header(CONTENT_LENGTH, body.len())
            .body(Body::from(body))
            .expect("response");
        let restored = restore_with(response, engine).await;
        assert!(restored.headers().get(CONTENT_LENGTH).is_none());
        assert!(body_text(restored).await.contains(EMAIL));
    }

    #[tokio::test]
    async fn compressed_and_non_json_replies_pass_through() {
        let (engine, placeholder) = engine_with_email();
        for (header, value) in [(CONTENT_ENCODING, "gzip"), (CONTENT_TYPE, "text/plain")] {
            let response = Response::builder()
                .header(header, value)
                .body(Body::from(placeholder.clone()))
                .expect("response");
            let text = body_text(restore_with(response, engine.clone()).await).await;
            assert_eq!(text, placeholder);
        }
    }
}
