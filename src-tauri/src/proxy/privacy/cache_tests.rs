//! Prompt-cache safety (ADR-0049): the masked body is a pure function of the
//! request body, the rules, the install key and the context-recognised values
//! already remembered, so a resent conversation keeps its exact bytes.

use serde_json::{json, Value};

use super::engine::Engine;
use super::placeholder::{HashKey, PLACEHOLDER};
use super::Rules;

const KEY: &str = "sk-ant-api03-Ab3dEf9hIjKlMnOpQrStUvWxYz0123456789";
const EMAIL: &str = "zhang.san@company.cn";
const PASSWORD: &str = "Tr0ub4dor9x";
const WORD: &str = "Project Kite";

fn engine() -> Engine {
    Engine::new(HashKey::new([42; 32]), 64)
}

fn everything() -> Rules {
    Rules::new(true, true, &[WORD.to_owned(), "张三".to_owned()])
}

fn masked(engine: &Engine, body: &Value, rules: &Rules) -> Value {
    let mut body = body.clone();
    engine.mask_value(&mut body, rules);
    body
}

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("serialize")
}

fn text(role: &str, content: String) -> Value {
    json!({"role": role, "content": content})
}

/// A conversation that names every category.
fn history() -> Vec<Value> {
    vec![
        text(
            "user",
            format!("Deploy {WORD} for 张三 <{EMAIL}> with DB_PASSWORD={PASSWORD} and key {KEY}"),
        ),
        text("assistant", format!("Using {PASSWORD} for the database.")),
    ]
}

fn body(messages: &[Value]) -> Value {
    json!({"model": "claude-sonnet", "messages": messages})
}

#[test]
fn the_same_history_masks_to_identical_bytes() {
    let engine = engine();
    let request = body(&history());
    let first = masked(&engine, &request, &everything());
    let second = masked(&engine, &request, &everything());
    assert_eq!(bytes(&first), bytes(&second));
    let sent = first.to_string();
    for real in [KEY, EMAIL, PASSWORD, WORD, "张三"] {
        assert!(!sent.contains(real), "{real} leaked: {sent}");
    }
}

#[test]
fn a_masked_turn_is_a_byte_identical_prefix_of_the_next_turn() {
    let engine = engine();
    let turn = history();
    let first = masked(&engine, &body(&turn), &everything());

    let mut next = turn.clone();
    next.push(text("user", format!("Now email {EMAIL} the {WORD} notes")));
    let second = masked(&engine, &body(&next), &everything());

    let earlier = serde_json::to_string(&first["messages"]).expect("serialize");
    let later = serde_json::to_string(&second["messages"]).expect("serialize");
    let open_prefix = &earlier[..earlier.len() - 1];
    assert!(later.starts_with(open_prefix), "{earlier}\n{later}");
}

#[test]
fn a_restored_reply_sent_back_as_history_is_masked_to_the_same_placeholders() {
    let engine = engine();
    let question = text(
        "user",
        format!("Set DB_PASSWORD={PASSWORD} for {WORD}, owner {EMAIL}"),
    );
    let first = masked(
        &engine,
        &body(std::slice::from_ref(&question)),
        &everything(),
    );
    let sent = first["messages"][0]["content"].as_str().expect("text");
    let placeholders: Vec<&str> = PLACEHOLDER.find_iter(sent).map(|m| m.as_str()).collect();
    assert_eq!(placeholders.len(), 3, "{sent}");

    // The provider answers with the placeholders; the tool receives real values.
    let provider_reply = format!(
        "Done: {} is set for {}, told {}.",
        placeholders[0], placeholders[1], placeholders[2]
    );
    let mut reply = text("assistant", provider_reply.clone());
    assert!(engine.restore_value(&mut reply));
    let restored = reply["content"].as_str().expect("text").to_owned();
    assert!(restored.contains(PASSWORD) && restored.contains(WORD) && restored.contains(EMAIL));

    // Next turn: the tool resends the question and the restored reply.
    let history = [question, reply, text("user", "Thanks".to_owned())];
    let second = masked(&engine, &body(&history), &everything());
    assert_eq!(second["messages"][0], first["messages"][0]);
    assert_eq!(second["messages"][1]["content"], json!(provider_reply));
}

#[test]
fn the_whole_body_is_learned_before_any_of_it_is_masked() {
    let bare = text("assistant", format!("I will reuse {PASSWORD} later."));
    let context = text("user", format!("DB_PASSWORD={PASSWORD}"));
    let rules = everything();

    let forward = masked(&engine(), &body(&[bare.clone(), context.clone()]), &rules);
    let backward = masked(&engine(), &body(&[context, bare]), &rules);
    assert!(!forward.to_string().contains(PASSWORD), "{forward}");
    assert_eq!(forward["messages"][0], backward["messages"][1]);
    assert_eq!(forward["messages"][1], backward["messages"][0]);
}

#[test]
fn after_a_restart_the_same_history_masks_to_the_same_bytes() {
    let request = body(&history());
    let before = engine();
    // Memory from unrelated earlier requests must not change this one.
    masked(
        &before,
        &body(&[text("user", "API_TOKEN=Zz9yX8wV7u".into())]),
        &everything(),
    );
    let first = masked(&before, &request, &everything());

    let after_restart = engine();
    let second = masked(&after_restart, &request, &everything());
    assert_eq!(bytes(&first), bytes(&second));
}

/// The one case that still rewrites earlier bytes, kept on purpose: a value
/// that first went out bare and is recognised only later. Hiding it from then
/// on is worth one cache miss. The reverse also holds: a value recognised
/// only in another request and remembered in memory is no longer hidden after
/// a restart, until its context is seen again.
#[test]
fn a_value_first_seen_bare_changes_earlier_bytes_once_it_is_recognised() {
    let bare = text("user", format!("try {PASSWORD} on staging"));
    let running = engine();
    let first = masked(&running, &body(std::slice::from_ref(&bare)), &everything());
    assert_eq!(first["messages"][0], bare);

    let later = [
        bare.clone(),
        text("user", format!("DB_PASSWORD={PASSWORD}")),
    ];
    let second = masked(&running, &body(&later), &everything());
    assert_ne!(second["messages"][0], bare);
    let third = masked(&running, &body(&later), &everything());
    assert_eq!(bytes(&second), bytes(&third));

    // Recognised only elsewhere: remembered here, forgotten after a restart.
    let elsewhere = masked(&running, &body(std::slice::from_ref(&bare)), &everything());
    assert_ne!(elsewhere["messages"][0], bare);
    let restarted = masked(&engine(), &body(std::slice::from_ref(&bare)), &everything());
    assert_eq!(restarted["messages"][0], bare);
}

#[test]
fn changing_a_setting_changes_the_bytes_once() {
    let engine = engine();
    let request = body(&history());
    let keys_only = Rules::new(true, false, &[]);
    let a = masked(&engine, &request, &keys_only);
    let b = masked(&engine, &request, &everything());
    assert_ne!(bytes(&a), bytes(&b));
    assert_eq!(bytes(&b), bytes(&masked(&engine, &request, &everything())));
    assert_eq!(bytes(&a), bytes(&masked(&engine, &request, &keys_only)));
}

#[test]
fn each_category_masks_only_what_it_covers() {
    let content = format!("{KEY} {EMAIL} {WORD} DB_PASSWORD={PASSWORD}");
    let request = json!({"content": content});
    let sent = |rules: Rules| masked(&engine(), &request, &rules)["content"].to_string();

    let keys = sent(Rules::new(true, false, &[]));
    assert!(!keys.contains(KEY) && !keys.contains(PASSWORD));
    assert!(keys.contains(EMAIL) && keys.contains(WORD));

    let personal = sent(Rules::new(false, true, &[]));
    assert!(!personal.contains(EMAIL));
    assert!(personal.contains(KEY) && personal.contains(PASSWORD) && personal.contains(WORD));

    let words = sent(Rules::new(false, false, &[WORD.to_owned()]));
    assert!(!words.contains(WORD) && words.contains("{{WORD_"));
    assert!(words.contains(KEY) && words.contains(EMAIL) && words.contains(PASSWORD));

    let nothing = sent(Rules::new(false, false, &[]));
    assert_eq!(nothing, json!(content).to_string());
}

#[test]
fn skipped_keys_stay_untouched_with_every_category_on() {
    let engine = engine();
    let mut request = json!({
        "id": WORD, "name": format!("{WORD} {EMAIL}"), "model": KEY,
        "prompt_cache_key": format!("{WORD}-{EMAIL}"),
        "messages": [{"role": "assistant", "content": [
            {"type": "thinking", "thinking": format!("{WORD} {EMAIL} {KEY}"), "signature": WORD},
            {"type": "reasoning", "encrypted_content": format!("{WORD}{KEY}")},
            {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"cmd": "ls"}}
        ]}]
    });
    let before = request.clone();
    assert_eq!(engine.mask_value(&mut request, &everything()), 0);
    assert_eq!(bytes(&request), bytes(&before));
}
