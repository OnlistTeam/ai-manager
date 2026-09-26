//! Privacy protection for traffic that passes through the local routing proxy.
//!
//! Product-owned module (ADR-0049). Before a request body leaves for the
//! provider, secrets and personal data inside its JSON string values are
//! replaced with stable placeholders (`{{API_KEY_k3v9x2mq}}`); on the way back
//! the reply (whole JSON or an SSE stream) gets the real values again.
//!
//! The inherited proxy calls exactly two entry points: [`mask_request_body`]
//! from the forwarder and [`restore_response`] as a router layer. The product
//! configures it through `compat::ccswitch::proxy_privacy`. Nothing here logs
//! a value or a placeholder; only counts.

mod engine;
mod layer;
mod personal;
mod placeholder;
mod secrets;
mod stream;
mod vault;

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

use serde_json::Value;

pub(crate) use layer::restore_response;

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

/// A value already masked because of its context, looked for verbatim.
pub(crate) struct KnownValue {
    pub value: String,
    pub kind: Kind,
}

/// Every sensitive span in `text`, sorted by position, never overlapping.
#[cfg(test)]
pub(crate) fn detect(text: &str) -> Vec<Span> {
    detect_with_known(text, &[])
}

pub(crate) fn detect_with_known(text: &str, known: &[KnownValue]) -> Vec<Span> {
    let mut detection = Detection {
        protected: placeholder_spans(text),
        spans: Vec::new(),
    };
    secrets::detect_secrets(text, &mut detection);
    for KnownValue { value, kind } in known {
        for (start, _) in text.match_indices(value.as_str()) {
            let end = start + value.len();
            if secrets::bounded(text, start, end, |c| c.is_ascii_alphanumeric()) {
                detection.push(start, end, *kind);
            }
        }
    }
    personal::detect_personal(text, &mut detection);
    let mut spans = detection.spans;
    spans.sort_by_key(|span| span.start);
    spans
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static ENGINE: RwLock<Option<Arc<Engine>>> = RwLock::new(None);

/// Installs the per-install hash key and the user's choice. Called once at
/// startup; until then nothing is masked.
pub(crate) fn configure(key: [u8; 32], enabled: bool) {
    let engine = Arc::new(Engine::new(HashKey::new(key), vault::DEFAULT_CAPACITY));
    *ENGINE.write().unwrap_or_else(PoisonError::into_inner) = Some(engine);
    ENABLED.store(enabled, Ordering::SeqCst);
}

pub(crate) fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
}

pub(crate) fn is_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

fn engine() -> Option<Arc<Engine>> {
    ENGINE
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The engine, but only when a reply could contain one of our placeholders.
/// Replies are restored even after the switch is turned off, so a request
/// that was masked just before still comes back whole.
fn engine_for_restore() -> Option<Arc<Engine>> {
    engine().filter(|engine| engine.has_values())
}

/// Masks the outbound request body in place and returns how many values were
/// replaced. With protection off (or not configured) the body is untouched.
pub(crate) fn mask_request_body(body: &mut Value) -> usize {
    if !is_enabled() {
        return 0;
    }
    let Some(engine) = engine() else {
        return 0;
    };
    let mut count = 0;
    engine.mask_value(body, &mut count);
    if count > 0 {
        log::debug!("[Privacy] replaced {count} value(s) with placeholders before forwarding");
    }
    count
}
