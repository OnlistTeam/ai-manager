//! Restores placeholders in a Server-Sent Events reply.
//!
//! Streamed text arrives in small deltas, so a placeholder can be cut in two
//! across events (`… {{API_K` then `EY_k3v9x2mq}} …`). When a delta ends with
//! something that could be the start of a placeholder, the event is held back
//! with that tail removed; the tail is prepended to the next delta of the same
//! block and the whole placeholder is restored there. If the next event
//! belongs to anything else, the held event is released unchanged.

use std::sync::Arc;

use serde_json::Value;

use super::engine::Engine;
use super::placeholder::partial_placeholder_suffix;
use crate::proxy::sse::{append_utf8_safe, strip_sse_field, take_sse_block};

/// Where the streamed text of a delta event lives.
struct DeltaTarget {
    /// Identifies one growing text: same block, same field.
    channel: String,
    /// JSON pointer to the delta string inside the event.
    pointer: String,
    /// The streamed text is JSON source (tool-call arguments).
    encoded: bool,
}

fn single(value: Option<&Value>) -> Option<&Value> {
    match value?.as_array()?.as_slice() {
        [only] => Some(only),
        _ => None,
    }
}

fn anthropic_target(event: &Value) -> Option<DeltaTarget> {
    let index = event.get("index")?.as_u64()?;
    let (field, encoded) = match event.pointer("/delta/type")?.as_str()? {
        "text_delta" => ("text", false),
        "input_json_delta" => ("partial_json", true),
        // Signed thinking text goes back to the provider exactly as issued.
        _ => return None,
    };
    Some(DeltaTarget {
        channel: format!("anthropic:{index}:{field}"),
        pointer: format!("/delta/{field}"),
        encoded,
    })
}

fn responses_target(kind: &str, event: &Value) -> Option<DeltaTarget> {
    event.get("delta")?.as_str()?;
    let part = |name: &str| event.get(name).map(Value::to_string).unwrap_or_default();
    Some(DeltaTarget {
        channel: format!(
            "responses:{kind}:{}:{}:{}:{}",
            part("item_id"),
            part("output_index"),
            part("content_index"),
            part("summary_index")
        ),
        pointer: "/delta".to_owned(),
        encoded: kind.contains("arguments"),
    })
}

fn chat_target(choice: &Value) -> Option<DeltaTarget> {
    let index = choice.get("index").and_then(Value::as_u64).unwrap_or(0);
    let delta = choice.get("delta")?;
    if let Some(call) = single(delta.get("tool_calls")) {
        if call
            .pointer("/function/arguments")
            .is_some_and(Value::is_string)
        {
            let call_index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
            return Some(DeltaTarget {
                channel: format!("chat:{index}:tool:{call_index}"),
                pointer: "/choices/0/delta/tool_calls/0/function/arguments".to_owned(),
                encoded: true,
            });
        }
    }
    ["content", "reasoning_content", "reasoning"]
        .into_iter()
        .find(|field| delta.get(*field).is_some_and(Value::is_string))
        .map(|field| DeltaTarget {
            channel: format!("chat:{index}:{field}"),
            pointer: format!("/choices/0/delta/{field}"),
            encoded: false,
        })
}

fn gemini_target(candidate: &Value) -> Option<DeltaTarget> {
    let part = single(candidate.pointer("/content/parts"))?;
    part.get("text")?.as_str()?;
    let index = candidate.get("index").and_then(Value::as_u64).unwrap_or(0);
    let thought = part
        .get("thought")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(DeltaTarget {
        channel: format!("gemini:{index}:{thought}"),
        pointer: "/candidates/0/content/parts/0/text".to_owned(),
        encoded: false,
    })
}

/// The streamed-text slot of an event in any of the four API shapes.
fn delta_target(event: &Value) -> Option<DeltaTarget> {
    match event.get("type").and_then(Value::as_str) {
        Some("content_block_delta") => return anthropic_target(event),
        Some(kind) if kind.starts_with("response.") && kind.ends_with(".delta") => {
            return responses_target(kind, event)
        }
        _ => {}
    }
    if let Some(choice) = single(event.get("choices")) {
        return chat_target(choice);
    }
    single(event.get("candidates")).and_then(gemini_target)
}

/// One SSE event whose `data` is JSON.
struct Event {
    before: Vec<String>,
    after: Vec<String>,
    json: Value,
    line_end: &'static str,
    delimiter: &'static str,
}

impl Event {
    fn parse(block: &str, delimiter: &'static str) -> Option<Self> {
        let line_end = if block.contains("\r\n") { "\r\n" } else { "\n" };
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut data: Vec<&str> = Vec::new();
        for line in block.split('\n').map(|line| line.trim_end_matches('\r')) {
            match strip_sse_field(line, "data") {
                Some(value) => data.push(value),
                None if data.is_empty() => before.push(line.to_owned()),
                None => after.push(line.to_owned()),
            }
        }
        if data.is_empty() {
            return None;
        }
        let json = serde_json::from_str(&data.join("\n")).ok()?;
        Some(Self {
            before,
            after,
            json,
            line_end,
            delimiter,
        })
    }

    fn render(&self, out: &mut String) {
        for line in &self.before {
            out.push_str(line);
            out.push_str(self.line_end);
        }
        out.push_str("data: ");
        out.push_str(&self.json.to_string());
        for line in &self.after {
            out.push_str(self.line_end);
            out.push_str(line);
        }
        out.push_str(self.delimiter);
    }

    fn set_text(&mut self, pointer: &str, text: String) {
        if let Some(slot) = self.json.pointer_mut(pointer) {
            *slot = Value::String(text);
        }
    }
}

struct Held {
    event: Event,
    channel: String,
    pointer: String,
    head: String,
    tail: String,
}

impl Held {
    /// The next delta continues this text: emit the head, carry the tail.
    fn continue_into(self, out: &mut String) -> String {
        self.event.render(out);
        self.tail
    }

    /// The text ended here: the tail was not a placeholder after all.
    fn release(mut self, out: &mut String) {
        let text = self.head + &self.tail;
        self.event.set_text(&self.pointer, text);
        self.event.render(out);
    }
}

pub(crate) struct SseRestorer {
    engine: Arc<Engine>,
    buffer: String,
    remainder: Vec<u8>,
    held: Option<Held>,
}

impl SseRestorer {
    pub(crate) fn new(engine: Arc<Engine>) -> Self {
        Self {
            engine,
            buffer: String::new(),
            remainder: Vec::new(),
            held: None,
        }
    }

    /// Feeds raw bytes; returns what can be sent to the client now.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> String {
        append_utf8_safe(&mut self.buffer, &mut self.remainder, bytes);
        let mut out = String::new();
        while let Some(block) = take_sse_block(&mut self.buffer) {
            let delimiter = if block.contains('\r') {
                "\r\n\r\n"
            } else {
                "\n\n"
            };
            self.on_block(&block, delimiter, &mut out);
        }
        out
    }

    /// The upstream ended: release everything still buffered.
    pub(crate) fn finish(&mut self) -> String {
        let mut out = String::new();
        if !self.remainder.is_empty() {
            let rest = std::mem::take(&mut self.remainder);
            self.buffer.push_str(&String::from_utf8_lossy(&rest));
        }
        let rest = std::mem::take(&mut self.buffer);
        if !rest.is_empty() {
            self.on_block(&rest, "", &mut out);
        }
        if let Some(held) = self.held.take() {
            held.release(&mut out);
        }
        out
    }

    fn on_block(&mut self, block: &str, delimiter: &'static str, out: &mut String) {
        let Some(mut event) = Event::parse(block, delimiter) else {
            if let Some(held) = self.held.take() {
                held.release(out);
            }
            out.push_str(
                self.engine
                    .restore_text(block, false)
                    .as_deref()
                    .unwrap_or(block),
            );
            out.push_str(delimiter);
            return;
        };
        let target = delta_target(&event.json);
        let carried = match (self.held.take(), &target) {
            (Some(held), Some(target)) if held.channel == target.channel => held.continue_into(out),
            (Some(held), _) => {
                held.release(out);
                String::new()
            }
            (None, _) => String::new(),
        };
        let Some(target) = target else {
            if self.engine.restore_value(&mut event.json) {
                event.render(out);
            } else {
                out.push_str(block);
                out.push_str(delimiter);
            }
            return;
        };

        let original = event
            .json
            .pointer(&target.pointer)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let combined = carried + &original;
        let tail = partial_placeholder_suffix(&combined).to_owned();
        let head = &combined[..combined.len() - tail.len()];
        let head = self
            .engine
            .restore_text(head, target.encoded)
            .unwrap_or_else(|| head.to_owned());
        let target_changed = head != original;
        event.set_text(&target.pointer, head.clone());
        let other_changed = self.engine.restore_value(&mut event.json);

        if !tail.is_empty() {
            self.held = Some(Held {
                event,
                channel: target.channel,
                pointer: target.pointer,
                head,
                tail,
            });
        } else if target_changed || other_changed {
            event.render(out);
        } else {
            out.push_str(block);
            out.push_str(delimiter);
        }
    }
}
