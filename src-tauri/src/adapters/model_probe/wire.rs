//! Request bodies and response parsing for the three provider dialects.
//!
//! Everything here is pure JSON shaping so it can be tested without a network.
//! Parsing is deliberately forgiving about envelopes and strict about content:
//! a 2xx that carries no assistant text is an empty reply, not a success.

use serde_json::{json, Value};

use crate::domain::{ModelProbeReply, ProbeModelKind, ProviderWireProtocol, MAX_PROBE_REPLY_CHARS};

const PROBE_MAX_TOKENS: u32 = 300;

pub fn text_request_body(protocol: ProviderWireProtocol, model: &str, prompt: &str) -> Value {
    match protocol {
        ProviderWireProtocol::OpenAi => json!({
            "model": model,
            "messages": [{ "role": "user", "content": prompt }],
            "max_tokens": PROBE_MAX_TOKENS,
        }),
        // Codex sends the structured item form rather than a bare string, and
        // `store: false`, so a relay that mirrors Codex's shape sees what it
        // expects. `stream` is explicitly false: Codex streams, but a probe
        // wants one JSON document, and every field a stream would add is one
        // more thing that can differ between the test and the truth.
        ProviderWireProtocol::OpenAiResponses => json!({
            "model": model,
            "input": [{
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": prompt }],
            }],
            "max_output_tokens": PROBE_MAX_TOKENS,
            "store": false,
            "stream": false,
        }),
        ProviderWireProtocol::Anthropic => json!({
            "model": model,
            "max_tokens": PROBE_MAX_TOKENS,
            "messages": [{ "role": "user", "content": prompt }],
        }),
        // The model lives in the Gemini path, not the body.
        ProviderWireProtocol::Gemini => json!({
            "contents": [{ "parts": [{ "text": prompt }] }],
            "generationConfig": { "maxOutputTokens": PROBE_MAX_TOKENS },
        }),
    }
}

pub fn image_request_body(model: &str, prompt: &str) -> Value {
    json!({
        "model": model,
        "prompt": prompt,
        "n": 1,
        "response_format": "b64_json",
    })
}

/// Truncates on a character boundary and marks that it happened, so a long
/// answer cannot be mistaken for a complete one.
pub fn clamp_reply(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_PROBE_REPLY_CHARS {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(MAX_PROBE_REPLY_CHARS).collect();
    format!("{head}…")
}

/// Accepts a string content, or the array-of-parts form that several
/// OpenAI-compatible relays emit.
fn openai_content_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    let Some(parts) = content.as_array() else {
        return String::new();
    };
    parts
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

fn openai_reply_text(body: &Value) -> String {
    let Some(message) = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
    else {
        return String::new();
    };
    let content = message
        .get("content")
        .map(openai_content_text)
        .unwrap_or_default();
    if !content.trim().is_empty() {
        return content;
    }
    // Reasoning models answer in `reasoning_content` when the visible content
    // is empty; treating that as silence reports a working service as broken.
    message
        .get("reasoning_content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Responses answers with a list of output items, of which only the assistant
/// message carries visible text; reasoning items sit alongside it and must not
/// be concatenated into the reply.
///
/// The convenience `output_text` field some relays add is accepted first,
/// because a relay that provides it usually provides nothing else.
fn responses_reply_text(body: &Value) -> String {
    if let Some(text) = body.get("output_text").and_then(Value::as_str) {
        if !text.trim().is_empty() {
            return text.to_string();
        }
    }
    let Some(output) = body.get("output").and_then(Value::as_array) else {
        return String::new();
    };
    output
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("output_text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

fn anthropic_reply_text(body: &Value) -> String {
    body.get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn gemini_reply_text(body: &Value) -> String {
    body.get("candidates")
        .and_then(Value::as_array)
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

pub fn text_reply(protocol: ProviderWireProtocol, body: &Value) -> ModelProbeReply {
    let text = match protocol {
        ProviderWireProtocol::OpenAi => openai_reply_text(body),
        ProviderWireProtocol::OpenAiResponses => responses_reply_text(body),
        ProviderWireProtocol::Anthropic => anthropic_reply_text(body),
        ProviderWireProtocol::Gemini => gemini_reply_text(body),
    };
    let text = clamp_reply(&text);
    if text.is_empty() {
        ModelProbeReply::Empty
    } else {
        ModelProbeReply::Text { text }
    }
}

/// Base64 payloads carry their own format in the first bytes. Reading it is
/// cheaper and more reliable than trusting a `response_format` echo.
pub fn sniff_image_mime(base64: &str) -> &'static str {
    if base64.starts_with("iVBORw0KGgo") {
        "image/png"
    } else if base64.starts_with("/9j/") {
        "image/jpeg"
    } else if base64.starts_with("UklGR") {
        "image/webp"
    } else if base64.starts_with("R0lGOD") {
        "image/gif"
    } else {
        "image/png"
    }
}

/// What an image response turned out to contain.
pub enum ImageOutcome {
    Data(ModelProbeReply),
    /// The service ignored `response_format` and answered with a link. The
    /// application CSP is `img-src 'self' data:`, so it cannot be shown, and
    /// fetching it here would point an arbitrary URL at the user's machine.
    UrlOnly,
    Empty,
}

pub fn image_reply(body: &Value) -> ImageOutcome {
    let Some(first) = body
        .get("data")
        .and_then(Value::as_array)
        .and_then(|data| data.first())
    else {
        return ImageOutcome::Empty;
    };
    if let Some(base64) = first.get("b64_json").and_then(Value::as_str) {
        if !base64.is_empty() {
            return ImageOutcome::Data(ModelProbeReply::Image {
                mime: sniff_image_mime(base64).to_string(),
                base64: base64.to_string(),
            });
        }
    }
    if first
        .get("url")
        .and_then(Value::as_str)
        .is_some_and(|url| !url.is_empty())
    {
        return ImageOutcome::UrlOnly;
    }
    ImageOutcome::Empty
}

/// Model ids, in the order the endpoint served them.
///
/// One model as the catalogue described it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: String,
    /// What the service itself said this model produces, when it said anything.
    /// `None` means the entry carried an id and nothing more, which is the
    /// common case and the only case where the name heuristic is consulted.
    pub declared: Option<ProbeModelKind>,
}

/// Three envelopes are accepted: the OpenAI/Anthropic `{"data":[…]}` shape, the
/// Gemini `{"models":[…]}` shape, and a bare array. `None` means the body was
/// not a model list at all, which is what lets the caller decide whether to
/// retry at the origin.
pub fn parse_catalog(protocol: ProviderWireProtocol, body: &Value) -> Option<Vec<CatalogEntry>> {
    let entries = body
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| body.get("models").and_then(Value::as_array))
        .or_else(|| body.as_array())?;

    Some(
        entries
            .iter()
            .filter_map(|entry| {
                let id = model_id(protocol, entry)?;
                if id.is_empty() || id.starts_with('~') {
                    return None;
                }
                Some(CatalogEntry {
                    id,
                    declared: declared_kind(protocol, entry),
                })
            })
            .collect(),
    )
}

/// Reads a model's output type out of the catalogue entry, when the service
/// publishes one.
///
/// Two fields are load-bearing in practice. Gemini documents
/// `supportedGenerationMethods`, where an image model offers `predict` and no
/// `generateContent`. OpenRouter publishes `architecture.output_modalities`, and
/// so does every relay that mirrors its catalogue shape.
///
/// A model whose output includes text is text even when it also lists image:
/// those are the conversational image models, which are driven through the chat
/// route and would fail against the image-generation route.
fn declared_kind(protocol: ProviderWireProtocol, entry: &Value) -> Option<ProbeModelKind> {
    let names = |value: Option<&Value>| -> Vec<String> {
        value
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_ascii_lowercase)
                    .collect()
            })
            .unwrap_or_default()
    };

    if protocol == ProviderWireProtocol::Gemini {
        let methods = names(entry.get("supportedGenerationMethods"));
        if methods.is_empty() {
            return None;
        }
        if methods
            .iter()
            .any(|method| method.contains("generatecontent"))
        {
            return Some(ProbeModelKind::Text);
        }
        if methods.iter().any(|method| method.starts_with("predict")) {
            return Some(ProbeModelKind::Image);
        }
        return None;
    }

    let modalities = names(
        entry
            .get("architecture")
            .and_then(|architecture| architecture.get("output_modalities")),
    );
    if modalities.iter().any(|modality| modality == "text") {
        return Some(ProbeModelKind::Text);
    }
    if modalities.iter().any(|modality| modality == "image") {
        return Some(ProbeModelKind::Image);
    }
    None
}

fn model_id(protocol: ProviderWireProtocol, entry: &Value) -> Option<String> {
    if let Some(id) = entry.as_str() {
        return Some(id.trim().to_string());
    }
    let raw = match protocol {
        // Gemini names its models `models/gemini-3-pro`; the request path adds
        // that prefix back, so it must not be stored twice.
        ProviderWireProtocol::Gemini => entry
            .get("name")
            .and_then(Value::as_str)
            .map(|name| name.trim_start_matches("models/")),
        ProviderWireProtocol::OpenAi
        | ProviderWireProtocol::OpenAiResponses
        | ProviderWireProtocol::Anthropic => entry.get("id").and_then(Value::as_str),
    };
    raw.map(|id| id.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_dialect_builds_its_own_request_body() {
        let openai = text_request_body(ProviderWireProtocol::OpenAi, "gpt-5.2", "hi");
        assert_eq!(openai["messages"][0]["content"], "hi");
        assert_eq!(openai["max_tokens"], 300);
        assert!(
            openai.get("stream").is_none(),
            "the probe is never streamed"
        );

        let anthropic = text_request_body(ProviderWireProtocol::Anthropic, "claude-opus-5", "hi");
        assert_eq!(anthropic["max_tokens"], 300);
        assert_eq!(anthropic["model"], "claude-opus-5");

        let gemini = text_request_body(ProviderWireProtocol::Gemini, "gemini-3-pro", "hi");
        assert_eq!(gemini["contents"][0]["parts"][0]["text"], "hi");
        assert!(
            gemini.get("model").is_none(),
            "Gemini carries the model in the path"
        );
    }

    #[test]
    fn a_responses_request_uses_the_item_form_and_never_streams() {
        let body = text_request_body(ProviderWireProtocol::OpenAiResponses, "gpt-5.2", "hi");
        assert_eq!(body["model"], "gpt-5.2");
        assert_eq!(body["input"][0]["type"], "message");
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(body["input"][0]["content"][0]["text"], "hi");
        assert_eq!(body["max_output_tokens"], 300);
        // Codex sets both of these; a probe that stored its trial prompt or
        // asked for a stream would be testing something the user did not ask
        // for.
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], false);
        assert!(
            body.get("messages").is_none(),
            "Responses does not take chat messages"
        );
    }

    #[test]
    fn a_responses_reply_reads_the_message_item_and_ignores_reasoning() {
        let body = json!({
            "output": [
                {"type": "reasoning", "summary": [{"type": "summary_text", "text": "thinking"}]},
                {"type": "message", "role": "assistant", "content": [
                    {"type": "output_text", "text": "hello "},
                    {"type": "output_text", "text": "there"}
                ]}
            ]
        });
        assert_eq!(
            text_reply(ProviderWireProtocol::OpenAiResponses, &body),
            ModelProbeReply::Text {
                text: "hello there".to_string()
            }
        );

        // Relays that add the SDK's convenience field are believed first.
        let convenience = json!({"output_text": "short answer", "output": []});
        assert_eq!(
            text_reply(ProviderWireProtocol::OpenAiResponses, &convenience),
            ModelProbeReply::Text {
                text: "short answer".to_string()
            }
        );

        // Reasoning alone is not an answer.
        let reasoning_only = json!({
            "output": [{"type": "reasoning", "summary": [{"type": "summary_text", "text": "hm"}]}]
        });
        assert_eq!(
            text_reply(ProviderWireProtocol::OpenAiResponses, &reasoning_only),
            ModelProbeReply::Empty
        );
    }

    #[test]
    fn an_image_request_always_asks_for_inline_data() {
        let body = image_request_body("gpt-image-2", "a red circle");
        assert_eq!(body["response_format"], "b64_json");
        assert_eq!(body["n"], 1);
    }

    #[test]
    fn openai_replies_fall_back_to_reasoning_content() {
        let visible = json!({"choices":[{"message":{"content":"hello"}}]});
        assert_eq!(
            text_reply(ProviderWireProtocol::OpenAi, &visible),
            ModelProbeReply::Text {
                text: "hello".to_string()
            }
        );

        let reasoning_only = json!({
            "choices":[{"message":{"content":"","reasoning_content":"thought it through"}}]
        });
        assert_eq!(
            text_reply(ProviderWireProtocol::OpenAi, &reasoning_only),
            ModelProbeReply::Text {
                text: "thought it through".to_string()
            }
        );

        let parts = json!({
            "choices":[{"message":{"content":[{"type":"text","text":"from "},{"type":"text","text":"parts"}]}}]
        });
        assert_eq!(
            text_reply(ProviderWireProtocol::OpenAi, &parts),
            ModelProbeReply::Text {
                text: "from parts".to_string()
            }
        );
    }

    #[test]
    fn a_two_hundred_with_no_content_is_empty_not_successful() {
        for (protocol, body) in [
            (ProviderWireProtocol::OpenAi, json!({"choices":[]})),
            (ProviderWireProtocol::Anthropic, json!({"content":[]})),
            (ProviderWireProtocol::Gemini, json!({"candidates":[]})),
        ] {
            assert_eq!(text_reply(protocol, &body), ModelProbeReply::Empty);
        }
    }

    #[test]
    fn anthropic_and_gemini_replies_are_concatenated_text_blocks() {
        let anthropic = json!({
            "content":[
                {"type":"thinking","thinking":"ignored"},
                {"type":"text","text":"hello "},
                {"type":"text","text":"there"}
            ]
        });
        assert_eq!(
            text_reply(ProviderWireProtocol::Anthropic, &anthropic),
            ModelProbeReply::Text {
                text: "hello there".to_string()
            }
        );

        let gemini = json!({
            "candidates":[{"content":{"parts":[{"text":"hello "},{"text":"there"}]}}]
        });
        assert_eq!(
            text_reply(ProviderWireProtocol::Gemini, &gemini),
            ModelProbeReply::Text {
                text: "hello there".to_string()
            }
        );
    }

    #[test]
    fn a_long_reply_is_clamped_on_a_character_boundary() {
        let long = "字".repeat(MAX_PROBE_REPLY_CHARS + 50);
        let clamped = clamp_reply(&long);
        assert_eq!(clamped.chars().count(), MAX_PROBE_REPLY_CHARS + 1);
        assert!(clamped.ends_with('…'));
    }

    #[test]
    fn image_format_is_read_from_the_payload() {
        assert_eq!(sniff_image_mime("iVBORw0KGgoAAAA"), "image/png");
        assert_eq!(sniff_image_mime("/9j/4AAQSkZJRg"), "image/jpeg");
        assert_eq!(sniff_image_mime("UklGRiQAAABXRUJQ"), "image/webp");
        assert_eq!(sniff_image_mime("R0lGODlhAQAB"), "image/gif");
        assert_eq!(sniff_image_mime("unknown"), "image/png");
    }

    #[test]
    fn an_image_returned_as_a_link_is_reported_rather_than_fetched() {
        let url_only = json!({"data":[{"url":"https://cdn.example.test/a.png"}]});
        assert!(matches!(image_reply(&url_only), ImageOutcome::UrlOnly));

        let inline = json!({"data":[{"b64_json":"iVBORw0KGgoAAAA"}]});
        match image_reply(&inline) {
            ImageOutcome::Data(ModelProbeReply::Image { mime, base64 }) => {
                assert_eq!(mime, "image/png");
                assert_eq!(base64, "iVBORw0KGgoAAAA");
            }
            _ => panic!("expected inline image data"),
        }

        assert!(matches!(
            image_reply(&json!({"data":[]})),
            ImageOutcome::Empty
        ));
    }

    #[test]
    fn catalogues_keep_upstream_order_and_drop_unusable_ids() {
        let body = json!({"data":[
            {"id":"gpt-5.2"},
            {"id":""},
            {"id":"~openrouter/auto"},
            {"id":"  claude-opus-5  "},
            {"id":"dall-e-3"}
        ]});
        assert_eq!(
            ids(parse_catalog(ProviderWireProtocol::OpenAi, &body).unwrap()),
            vec!["gpt-5.2", "claude-opus-5", "dall-e-3"]
        );
    }

    /// Ids alone, for the assertions that are about order and filtering.
    fn ids(entries: Vec<CatalogEntry>) -> Vec<String> {
        entries.into_iter().map(|entry| entry.id).collect()
    }

    fn declared(protocol: ProviderWireProtocol, body: &Value) -> Vec<Option<ProbeModelKind>> {
        parse_catalog(protocol, body)
            .unwrap()
            .into_iter()
            .map(|entry| entry.declared)
            .collect()
    }

    #[test]
    fn a_catalogue_that_names_its_output_is_believed_over_the_model_name() {
        // `recraft-v3` has no image keyword in it and `gpt-5.4-image` has one
        // while being a chat model, so both would be classified backwards by
        // the name heuristic alone.
        let body = json!({"data":[
            {"id":"recraft-v3","architecture":{"output_modalities":["image"]}},
            {"id":"gpt-5.4-image","architecture":{"output_modalities":["text","image"]}},
            {"id":"gpt-5.2","architecture":{"output_modalities":["TEXT"]}},
            {"id":"mystery-1","architecture":{"output_modalities":[]}},
            {"id":"plain-1"}
        ]});
        assert_eq!(
            declared(ProviderWireProtocol::OpenAi, &body),
            vec![
                Some(ProbeModelKind::Image),
                Some(ProbeModelKind::Text),
                Some(ProbeModelKind::Text),
                None,
                None,
            ]
        );
    }

    #[test]
    fn gemini_reads_its_generation_methods() {
        let body = json!({"models":[
            {"name":"models/gemini-3-pro","supportedGenerationMethods":["generateContent","countTokens"]},
            {"name":"models/imagen-4.0","supportedGenerationMethods":["predict"]},
            {"name":"models/text-embedding-5","supportedGenerationMethods":["embedContent"]},
            {"name":"models/unlisted"}
        ]});
        assert_eq!(
            declared(ProviderWireProtocol::Gemini, &body),
            vec![
                Some(ProbeModelKind::Text),
                Some(ProbeModelKind::Image),
                None,
                None,
            ]
        );
    }

    #[test]
    fn gemini_names_lose_their_collection_prefix() {
        let body = json!({"models":[{"name":"models/gemini-3-pro"},{"name":"models/imagen-4.0"}]});
        assert_eq!(
            ids(parse_catalog(ProviderWireProtocol::Gemini, &body).unwrap()),
            vec!["gemini-3-pro", "imagen-4.0"]
        );
    }

    #[test]
    fn a_bare_array_is_accepted_and_a_non_list_is_not() {
        let bare = json!([{"id":"gpt-5.2"}]);
        assert_eq!(
            ids(parse_catalog(ProviderWireProtocol::OpenAi, &bare).unwrap()),
            vec!["gpt-5.2"]
        );
        assert!(parse_catalog(
            ProviderWireProtocol::OpenAi,
            &json!({"error":{"message":"not found"}})
        )
        .is_none());
    }
}
