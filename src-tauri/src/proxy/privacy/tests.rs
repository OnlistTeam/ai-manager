//! Round-trip tests across the API shapes the proxy speaks.

use std::sync::Arc;

use serde_json::{json, Value};

use super::engine::Engine;
use super::layer::restore_json_bytes;
use super::placeholder::{HashKey, PLACEHOLDER};
use super::stream::SseRestorer;

const KEY: &str = "sk-ant-api03-Ab3dEf9hIjKlMnOpQrStUvWxYz0123456789";
const EMAIL: &str = "zhang.san@company.cn";
const PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBg\n-----END PRIVATE KEY-----";

fn engine() -> Arc<Engine> {
    Arc::new(Engine::new(HashKey::new([42; 32]), 64))
}

fn mask(engine: &Engine, body: &mut Value) -> usize {
    let mut count = 0;
    engine.mask_value(body, &mut count);
    count
}

fn placeholder_of(engine: &Engine, text: &str) -> String {
    let mut count = 0;
    engine
        .mask_text(text, &mut count)
        .expect("text is sensitive")
}

fn no_raw_values(body: &Value) {
    let text = body.to_string();
    assert!(!text.contains(KEY), "{text}");
    assert!(!text.contains(EMAIL), "{text}");
    assert!(!text.contains("MIIEvQIBADANBg"), "{text}");
}

#[test]
fn anthropic_messages_body_round_trips() {
    let engine = engine();
    let original = json!({
        "model": "claude-sonnet",
        "system": [{"type": "text", "text": format!("User email: {EMAIL}")}],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": format!("my key is {KEY}")}]},
            {"role": "assistant", "content": [
                {"type": "thinking", "thinking": "ok", "signature": KEY},
                {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": format!("export API_KEY={KEY}")}}
            ]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": PEM}]}
        ]
    });
    let mut body = original.clone();
    assert_eq!(mask(&engine, &mut body), 4);
    assert_eq!(
        body["messages"][1]["content"][0]["signature"],
        original["messages"][1]["content"][0]["signature"]
    );
    body["messages"][1]["content"][0]["signature"] = json!("x");
    no_raw_values(&body);
    body["messages"][1]["content"][0]["signature"] =
        original["messages"][1]["content"][0]["signature"].clone();

    assert!(engine.restore_value(&mut body));
    assert_eq!(body, original);
}

#[test]
fn openai_chat_body_round_trips_including_encoded_arguments() {
    let engine = engine();
    let arguments = json!({"path": ".env", "content": format!("KEY={KEY}\n{PEM}")}).to_string();
    let original = json!({
        "model": "gpt-5",
        "messages": [
            {"role": "user", "content": format!("write {EMAIL}")},
            {"role": "assistant", "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "write", "arguments": arguments}}
            ]}
        ]
    });
    let mut body = original.clone();
    assert_eq!(mask(&engine, &mut body), 3);
    no_raw_values(&body);
    let masked_arguments: Value = serde_json::from_str(
        body["messages"][1]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .expect("arguments stay a string"),
    )
    .expect("arguments stay valid JSON");
    assert!(PLACEHOLDER.is_match(masked_arguments["content"].as_str().unwrap()));

    assert!(engine.restore_value(&mut body));
    let restored: Value = serde_json::from_str(
        body["messages"][1]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap(),
    )
    .expect("restored arguments are valid JSON");
    assert_eq!(restored["content"], json!(format!("KEY={KEY}\n{PEM}")));
    assert_eq!(body["messages"][0], original["messages"][0]);
}

#[test]
fn openai_responses_and_gemini_bodies_round_trip() {
    let engine = engine();
    let original = json!({
        "model": "gpt-5",
        "previous_response_id": "resp_1",
        "prompt_cache_key": format!("cache-{EMAIL}"),
        "input": [
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": KEY}]},
            {"type": "function_call", "call_id": "c1", "name": "shell", "arguments": json!({"cmd": format!("mail {EMAIL}")}).to_string()},
            {"type": "function_call_output", "call_id": "c1", "output": format!("sent to {EMAIL}")},
            {"type": "reasoning", "encrypted_content": format!("enc{KEY}")}
        ],
        "contents": [{"role": "user", "parts": [
            {"text": format!("key {KEY}")},
            {"inlineData": {"mimeType": "image/png", "data": "aGVsbG8="}},
            {"functionCall": {"name": "f", "args": {"to": EMAIL}}, "thoughtSignature": "c2ln"}
        ]}]
    });
    let mut body = original.clone();
    assert_eq!(mask(&engine, &mut body), 5);
    assert_eq!(body["prompt_cache_key"], original["prompt_cache_key"]);
    assert_eq!(body["input"][3], original["input"][3]);
    assert!(engine.restore_value(&mut body));
    assert_eq!(body, original);
}

#[test]
fn skipped_keys_and_data_urls_pass_unchanged() {
    let engine = engine();
    let mut body = json!({
        "id": KEY, "name": KEY, "model": KEY, "url": format!("https://u:{KEY}@h/x"),
        "image_url": {"url": format!("https://h/?k={KEY}")},
        "source": {"type": "base64", "media_type": "image/png", "data": KEY},
        "text": format!("data:text/plain,{KEY}")
    });
    let before = body.clone();
    assert_eq!(mask(&engine, &mut body), 0);
    assert_eq!(body, before);
}

#[test]
fn placeholders_are_stable_and_never_masked_twice() {
    let engine = engine();
    let first = placeholder_of(&engine, KEY);
    assert_eq!(first, placeholder_of(&engine, KEY));
    let mut body = json!({"text": format!("{first} and API_KEY={first}")});
    let before = body.clone();
    assert_eq!(mask(&engine, &mut body), 0);
    assert_eq!(body, before);
}

#[test]
fn unknown_placeholders_are_left_as_they_are() {
    let engine = engine();
    let mut reply = json!({"text": "{{API_KEY_aaaaaaaa}}"});
    assert!(!engine.restore_value(&mut reply));
    assert_eq!(reply["text"], "{{API_KEY_aaaaaaaa}}");
}

#[test]
fn switched_off_protection_leaves_the_body_byte_identical() {
    // Configured but off: the global engine exists and stays empty, so other
    // proxy tests running in parallel are unaffected.
    super::configure([9; 32], false);
    let body = json!({"messages": [{"role": "user", "content": format!("{KEY} {EMAIL}")}]});
    let bytes = serde_json::to_vec(&body).unwrap();
    let mut forwarded = body.clone();
    assert_eq!(super::mask_request_body(&mut forwarded), 0);
    assert_eq!(serde_json::to_vec(&forwarded).unwrap(), bytes);
}

#[test]
fn json_reply_restores_with_escaping_inside_encoded_arguments() {
    let engine = engine();
    let key_placeholder = placeholder_of(&engine, PEM);
    let reply = json!({
        "choices": [{"message": {"content": format!("done {key_placeholder}"), "tool_calls": [
            {"function": {"name": "write", "arguments": json!({"content": key_placeholder}).to_string()}}
        ]}}]
    });
    let restored: Value = serde_json::from_slice(
        &restore_json_bytes(&engine, &serde_json::to_vec(&reply).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        restored["choices"][0]["message"]["content"],
        json!(format!("done {PEM}"))
    );
    let arguments: Value = serde_json::from_str(
        restored["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap(),
    )
    .expect("the multi-line value is escaped inside the JSON text");
    assert_eq!(arguments["content"], json!(PEM));
    assert!(restore_json_bytes(&engine, br#"{"text":"nothing here"}"#).is_none());
}

fn run_stream(engine: &Arc<Engine>, chunks: &[String]) -> String {
    let mut restorer = SseRestorer::new(engine.clone());
    let mut out = String::new();
    for chunk in chunks {
        out.push_str(&restorer.push(chunk.as_bytes()));
    }
    out.push_str(&restorer.finish());
    out
}

fn sse(event: &Value) -> String {
    format!("data: {event}\n\n")
}

fn data_events(stream: &str) -> Vec<Value> {
    stream
        .split("\n\n")
        .filter_map(|block| block.lines().find_map(|line| line.strip_prefix("data: ")))
        .filter_map(|data| serde_json::from_str(data).ok())
        .collect()
}

fn joined(events: &[Value], pointer: &str) -> String {
    events
        .iter()
        .filter_map(|event| event.pointer(pointer).and_then(Value::as_str))
        .collect()
}

#[test]
fn anthropic_stream_restores_a_placeholder_split_across_events() {
    let engine = engine();
    let placeholder = placeholder_of(&engine, EMAIL);
    let (left, right) = placeholder.split_at(7);
    let delta = |text: &str| {
        sse(
            &json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}}),
        )
    };
    let chunks = vec![
        format!(
            "event: content_block_delta\n{}",
            delta(&format!("mail {left}"))
        ),
        delta(&format!("{right} now")),
        sse(&json!({"type": "content_block_stop", "index": 0})),
    ];
    let out = run_stream(&engine, &chunks);
    assert!(out.starts_with("event: content_block_delta\n"));
    assert!(!out.contains("{{"), "{out}");
    let events = data_events(&out);
    assert_eq!(joined(&events, "/delta/text"), format!("mail {EMAIL} now"));
    assert_eq!(events.len(), 3);
}

#[test]
fn anthropic_tool_input_stream_is_escaped_on_restore() {
    let engine = engine();
    let placeholder = placeholder_of(&engine, PEM);
    let encoded = json!({"content": placeholder}).to_string();
    let (left, right) = encoded.split_at(encoded.len() - 10);
    let delta = |text: &str| {
        sse(
            &json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": text}}),
        )
    };
    let out = run_stream(&engine, &[delta(left), delta(right)]);
    let arguments = joined(&data_events(&out), "/delta/partial_json");
    let parsed: Value = serde_json::from_str(&arguments).expect("valid JSON after restore");
    assert_eq!(parsed["content"], json!(PEM));
}

#[test]
fn chat_and_responses_streams_restore_split_placeholders() {
    let engine = engine();
    let placeholder = placeholder_of(&engine, KEY);
    let (left, right) = placeholder.split_at(12);

    let chat = |text: &str| sse(&json!({"choices": [{"index": 0, "delta": {"content": text}}]}));
    let out = run_stream(
        &engine,
        &[
            chat(&format!("k={left}")),
            chat(right),
            "data: [DONE]\n\n".into(),
        ],
    );
    assert_eq!(
        joined(&data_events(&out), "/choices/0/delta/content"),
        format!("k={KEY}")
    );
    assert!(out.ends_with("data: [DONE]\n\n"));

    let arguments = json!({"key": placeholder}).to_string();
    let (a, b) = arguments.split_at(15);
    let tool = |text: &str| {
        sse(
            &json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": text}}]}}]}),
        )
    };
    let out = run_stream(&engine, &[tool(a), tool(b)]);
    let restored: Value = serde_json::from_str(&joined(
        &data_events(&out),
        "/choices/0/delta/tool_calls/0/function/arguments",
    ))
    .unwrap();
    assert_eq!(restored["key"], json!(KEY));

    let responses = |text: &str| {
        sse(
            &json!({"type": "response.output_text.delta", "item_id": "m1", "output_index": 0, "content_index": 0, "delta": text}),
        )
    };
    let done =
        sse(&json!({"type": "response.output_text.done", "item_id": "m1", "text": placeholder}));
    let out = run_stream(&engine, &[responses(left), responses(right), done]);
    let events = data_events(&out);
    assert_eq!(joined(&events, "/delta"), KEY);
    assert_eq!(events[2]["text"], json!(KEY));
}

#[test]
fn gemini_stream_restores_split_placeholders() {
    let engine = engine();
    let placeholder = placeholder_of(&engine, EMAIL);
    let (left, right) = placeholder.split_at(3);
    let part = |text: &str| {
        sse(
            &json!({"candidates": [{"index": 0, "content": {"role": "model", "parts": [{"text": text}]}}]}),
        )
    };
    let out = run_stream(&engine, &[part(&format!("to {left}")), part(right)]);
    assert_eq!(
        joined(&data_events(&out), "/candidates/0/content/parts/0/text"),
        format!("to {EMAIL}")
    );
}

#[test]
fn a_held_tail_that_is_not_a_placeholder_is_released_unchanged() {
    let engine = engine();
    placeholder_of(&engine, EMAIL);
    let delta = |index: u64, text: &str| {
        sse(
            &json!({"type": "content_block_delta", "index": index, "delta": {"type": "text_delta", "text": text}}),
        )
    };
    let out = run_stream(
        &engine,
        &[delta(0, "fn main() {"), delta(1, "other"), delta(0, "}")],
    );
    let events = data_events(&out);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["delta"]["text"], json!("fn main() {"));
    assert_eq!(events[2]["delta"]["text"], json!("}"));

    let out = run_stream(&engine, &[delta(0, "tail {{EM")]);
    assert_eq!(data_events(&out)[0]["delta"]["text"], json!("tail {{EM"));
}

#[test]
fn untouched_events_pass_through_byte_for_byte() {
    let engine = engine();
    placeholder_of(&engine, EMAIL);
    let stream = "event: ping\ndata: {\"type\": \"ping\"}\n\n: comment\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"}}]}\n\n";
    let halves = stream.split_at(20);
    assert_eq!(
        run_stream(&engine, &[halves.0.into(), halves.1.into()]),
        stream
    );
}

#[test]
fn a_value_found_by_its_context_stays_masked_without_that_context() {
    let engine = engine();
    let mut body = json!({"messages": [
        {"role": "user", "content": "DB_PASSWORD=Tr0ub4dor9x"},
        {"role": "assistant", "content": "I will use Tr0ub4dor9x for the database."}
    ]});
    assert_eq!(mask(&engine, &mut body), 2);
    let first = body["messages"][0]["content"].as_str().unwrap().to_owned();
    let placeholder = first.trim_start_matches("DB_PASSWORD=");
    assert_eq!(
        body["messages"][1]["content"],
        json!(format!("I will use {placeholder} for the database."))
    );

    // A later request that mentions the value alone is masked the same way.
    let mut next = json!({"content": "password Tr0ub4dor9x, not Tr0ub4dor9xy"});
    assert_eq!(mask(&engine, &mut next), 1);
    assert_eq!(
        next["content"],
        json!(format!("password {placeholder}, not Tr0ub4dor9xy"))
    );
}

#[test]
fn signed_thinking_is_neither_masked_nor_restored() {
    let engine = engine();
    let placeholder = placeholder_of(&engine, EMAIL);
    let thinking = json!({"type": "thinking", "thinking": format!("{EMAIL} {placeholder}"), "signature": "c2ln"});
    let mut body = json!({"messages": [{"role": "assistant", "content": [thinking.clone()]}]});
    assert_eq!(mask(&engine, &mut body), 0);
    assert!(!engine.restore_value(&mut body));
    assert_eq!(body["messages"][0]["content"][0], thinking);

    let delta = sse(
        &json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": format!("see {placeholder}")}}),
    );
    assert_eq!(run_stream(&engine, std::slice::from_ref(&delta)), delta);
}
