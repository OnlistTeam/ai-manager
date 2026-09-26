//! Privacy protection for traffic that passes through the local routing proxy.
//!
//! Product-owned module (ADR-0049). Before a request body leaves for the
//! provider, whatever the user chose to hide (keys and passwords, personal
//! information, their own words) inside its JSON string values is replaced
//! with stable placeholders (`{{API_KEY_k3v9x2mq}}`); on the way back the
//! reply (whole JSON or an SSE stream) gets the real values again.
//!
//! Masking is a pure function of the body, the rules, the install key and the
//! context-recognised values remembered from earlier requests, so the same
//! conversation history masks to the same bytes on every turn and the
//! provider's prompt cache keeps hitting (see `Engine::mask_value`).
//!
//! The inherited proxy calls exactly two entry points: [`mask_request_body`]
//! from the forwarder and [`restore_response`] as a router layer. The product
//! configures it through `compat::ccswitch::proxy_privacy`. Nothing here logs
//! a value or a placeholder; only counts.

mod engine;
mod layer;
mod personal;
mod placeholder;
mod rules;
mod secrets;
mod stream;
mod vault;
mod words;

#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod tests;

use std::sync::{Arc, PoisonError, RwLock};

use serde_json::Value;

pub(crate) use layer::restore_response;
pub(crate) use rules::Rules;

use engine::Engine;
use placeholder::{placeholder_spans, HashKey, Kind};

/// A detected sensitive substring (byte range) and what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

/// Collects spans in detector priority order; a span overlapping an earlier
/// one, or an existing placeholder, is dropped.
pub(crate) struct Detection {
    protected: Vec<(usize, usize)>,
    spans: Vec<Span>,
}

impl Detection {
    fn new(text: &str) -> Self {
        Self {
            protected: placeholder_spans(text),
            spans: Vec::new(),
        }
    }

    fn push(&mut self, start: usize, end: usize, kind: Kind) {
        let overlaps =
            |(other_start, other_end): (usize, usize)| start < other_end && other_start < end;
        if start >= end
            || self.protected.iter().copied().any(overlaps)
            || self
                .spans
                .iter()
                .any(|span| overlaps((span.start, span.end)))
        {
            return;
        }
        self.spans.push(Span { start, end, kind });
    }
}

/// A value recognised by its context (`password=…`), looked for verbatim
/// wherever it appears without that context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnownValue {
    pub value: String,
    pub kind: Kind,
}

/// Every span the key and password and personal information detectors find
/// in `text`, sorted by position, never overlapping.
#[cfg(test)]
pub(crate) fn detect(text: &str) -> Vec<Span> {
    detect_with(text, &Rules::detectors(), &[])
}

/// Every span `rules` asks for, sorted by position, never overlapping.
/// Priority on overlap: keys and passwords, then values already known from
/// their context, then personal information, then the user's words. `known`
/// must be in a fixed order (see `Engine::known_values`) so the outcome never
/// depends on the order values were learned in.
pub(crate) fn detect_with(text: &str, rules: &Rules, known: &[KnownValue]) -> Vec<Span> {
    let mut detection = Detection::new(text);
    if rules.secrets {
        secrets::detect_secrets(text, &mut detection);
        for KnownValue { value, kind } in known {
            for (start, _) in text.match_indices(value.as_str()) {
                let end = start + value.len();
                if secrets::bounded(text, start, end, |c| c.is_ascii_alphanumeric()) {
                    detection.push(start, end, *kind);
                }
            }
        }
    }
    if rules.personal {
        personal::detect_personal(text, &mut detection);
    }
    words::detect_words(text, rules.words(), &mut detection);
    let mut spans = detection.spans;
    spans.sort_by_key(|span| span.start);
    spans
}

/// Values in `text` recognised only by their context, as a request-wide
/// learning pass sees them before anything is masked.
pub(crate) fn contextual_values(text: &str) -> Vec<KnownValue> {
    let mut detection = Detection::new(text);
    secrets::detect_secrets(text, &mut detection);
    detection
        .spans
        .into_iter()
        .filter(|span| span.kind.is_contextual())
        .map(|span| KnownValue {
            value: text[span.start..span.end].to_owned(),
            kind: span.kind,
        })
        .collect()
}

static ENGINE: RwLock<Option<Arc<Engine>>> = RwLock::new(None);
static RULES: RwLock<Option<Arc<Rules>>> = RwLock::new(None);

/// Installs the per-install hash key and the user's choices. Called once at
/// startup; until then nothing is masked.
pub(crate) fn configure(key: [u8; 32], rules: Rules) {
    let engine = Arc::new(Engine::new(HashKey::new(key), vault::DEFAULT_CAPACITY));
    *ENGINE.write().unwrap_or_else(PoisonError::into_inner) = Some(engine);
    set_rules(rules);
}

/// Applies changed choices to requests forwarded from now on.
pub(crate) fn set_rules(rules: Rules) {
    *RULES.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(rules));
}

fn rules() -> Option<Arc<Rules>> {
    RULES
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .filter(|rules| rules.is_active())
}

fn engine() -> Option<Arc<Engine>> {
    ENGINE
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The engine, but only when a reply could contain one of our placeholders.
/// Replies are restored even after masking is turned off, so a request that
/// was masked just before still comes back whole.
fn engine_for_restore() -> Option<Arc<Engine>> {
    engine().filter(|engine| engine.has_values())
}

/// Masks the outbound request body in place and returns how many values were
/// replaced. With nothing chosen (or not configured) the body is untouched.
pub(crate) fn mask_request_body(body: &mut Value) -> usize {
    let (Some(rules), Some(engine)) = (rules(), engine()) else {
        return 0;
    };
    let count = engine.mask_value(body, &rules);
    if count > 0 {
        log::debug!("[Privacy] replaced {count} value(s) with placeholders before forwarding");
    }
    count
}
