//! In-memory live routing trace (ADR-0050).
//!
//! A bounded ring of the most recent requests the local route handled, plus
//! running totals. Nothing is written to disk; the trace starts empty on every
//! launch. Every change is pushed to the renderer as one small
//! `routing://trace` event carrying only the changed entry.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::domain::{
    RoutingAttemptOutcome, RoutingErrorCategory, RoutingTraceAttempt, RoutingTraceCounts,
    RoutingTraceEntry, RoutingTraceSnapshot, RoutingTraceStatus, RoutingTraceUpdate, ToolId,
    MAX_ROUTING_TRACE_ATTEMPTS, MAX_ROUTING_TRACE_ENTRIES,
};

pub const ROUTING_TRACE_EVENT: &str = "routing://trace";

/// Event sink, so the log can be tested without a Tauri runtime.
pub trait RoutingTraceEvents: Send + Sync {
    fn emit(&self, update: &RoutingTraceUpdate);
}

pub struct TauriRoutingTraceEvents {
    app: tauri::AppHandle,
}

impl TauriRoutingTraceEvents {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl RoutingTraceEvents for TauriRoutingTraceEvents {
    fn emit(&self, update: &RoutingTraceUpdate) {
        use tauri::Emitter;
        if let Err(e) = self.app.emit(ROUTING_TRACE_EVENT, update) {
            log::warn!("failed to emit {ROUTING_TRACE_EVENT}: {e}");
        }
    }
}

#[derive(Default)]
struct TraceState {
    /// Oldest first; the snapshot reverses it.
    entries: VecDeque<RoutingTraceEntry>,
    next_seq: u64,
    revision: u64,
    counts: RoutingTraceCounts,
}

pub struct RoutingTraceLog {
    state: Mutex<TraceState>,
    events: Box<dyn RoutingTraceEvents>,
}

impl RoutingTraceLog {
    pub fn new(events: Box<dyn RoutingTraceEvents>) -> Self {
        Self {
            state: Mutex::new(TraceState::default()),
            events,
        }
    }

    /// The trace is display-only; a poisoned lock still holds consistent
    /// entries because every mutation is a single assignment.
    fn lock(&self) -> MutexGuard<'_, TraceState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn snapshot(&self) -> RoutingTraceSnapshot {
        let state = self.lock();
        RoutingTraceSnapshot {
            revision: state.revision,
            counts: state.counts,
            entries: state.entries.iter().rev().cloned().collect(),
        }
    }

    /// Opens a pending entry and returns its sequence number.
    pub fn begin(&self, tool: ToolId, model: Option<String>, started_at: i64) -> u64 {
        let update = {
            let mut state = self.lock();
            state.next_seq += 1;
            state.revision += 1;
            state.counts.requests += 1;
            let entry = RoutingTraceEntry {
                seq: state.next_seq,
                revision: state.revision,
                started_at,
                tool,
                model,
                attempts: Vec::new(),
                status: RoutingTraceStatus::Pending,
                error: None,
                total_ms: None,
                failed_over: false,
            };
            if state.entries.len() == MAX_ROUTING_TRACE_ENTRIES {
                state.entries.pop_front();
            }
            state.entries.push_back(entry.clone());
            RoutingTraceUpdate {
                revision: state.revision,
                counts: state.counts,
                entry,
            }
        };
        self.events.emit(&update);
        update.entry.seq
    }

    /// Appends one try, finished or still `pending`. Tries beyond the bound
    /// are dropped.
    pub fn push_attempt(&self, seq: u64, attempt: RoutingTraceAttempt) {
        self.change(seq, |entry, _| {
            if entry.attempts.len() < MAX_ROUTING_TRACE_ATTEMPTS {
                entry.attempts.push(attempt);
            }
        });
    }

    /// Ends a try: replaces the last attempt when it is the same service's
    /// `pending` try, and appends it otherwise.
    pub fn settle_attempt(&self, seq: u64, attempt: RoutingTraceAttempt) {
        self.change(seq, |entry, _| {
            let replaces_pending = entry.attempts.last().is_some_and(|last| {
                last.outcome == RoutingAttemptOutcome::Pending
                    && last.provider_id == attempt.provider_id
            });
            if replaces_pending {
                entry.attempts.pop();
            }
            if entry.attempts.len() < MAX_ROUTING_TRACE_ATTEMPTS {
                entry.attempts.push(attempt);
            }
        });
    }

    /// Closes the entry. Later calls for the same entry are ignored.
    pub fn finish(
        &self,
        seq: u64,
        status: RoutingTraceStatus,
        error: Option<RoutingErrorCategory>,
        total_ms: u64,
    ) {
        self.change(seq, |entry, counts| {
            if entry.status != RoutingTraceStatus::Pending {
                return;
            }
            entry.status = status;
            entry.error = (status == RoutingTraceStatus::Failed)
                .then_some(error)
                .flatten();
            entry.total_ms = Some(total_ms);
            if entry.attempts.len() > 1 {
                counts.rerouted += 1;
            }
            if status == RoutingTraceStatus::Failed {
                counts.failed += 1;
            }
        });
    }

    /// Applies one change to a retained entry and pushes it. An entry that
    /// already fell out of the ring is left alone: the renderer no longer
    /// shows it either.
    fn change(
        &self,
        seq: u64,
        apply: impl FnOnce(&mut RoutingTraceEntry, &mut RoutingTraceCounts),
    ) {
        let update = {
            let mut guard = self.lock();
            let state = &mut *guard;
            let Some(entry) = state.entries.iter_mut().find(|entry| entry.seq == seq) else {
                return;
            };
            let before = entry.clone();
            apply(entry, &mut state.counts);
            entry.failed_over = entry.attempts.len() > 1;
            if *entry == before {
                return;
            }
            state.revision += 1;
            entry.revision = state.revision;
            RoutingTraceUpdate {
                revision: state.revision,
                counts: state.counts,
                entry: entry.clone(),
            }
        };
        self.events.emit(&update);
    }
}

#[cfg(test)]
#[path = "routing_trace_tests.rs"]
mod tests;
