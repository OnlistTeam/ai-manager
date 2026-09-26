use std::sync::{Arc, Mutex};

use super::{RoutingTraceEvents, RoutingTraceLog};
use crate::domain::{
    RoutingAttemptOutcome, RoutingErrorCategory, RoutingTraceAttempt, RoutingTraceStatus,
    RoutingTraceUpdate, ToolId, MAX_ROUTING_TRACE_ATTEMPTS, MAX_ROUTING_TRACE_ENTRIES,
};

#[derive(Default)]
struct Recorded(Mutex<Vec<RoutingTraceUpdate>>);

impl RoutingTraceEvents for Arc<Recorded> {
    fn emit(&self, update: &RoutingTraceUpdate) {
        self.0.lock().expect("recorded events").push(update.clone());
    }
}

fn log() -> (RoutingTraceLog, Arc<Recorded>) {
    let recorded = Arc::new(Recorded::default());
    (RoutingTraceLog::new(Box::new(recorded.clone())), recorded)
}

fn attempt(id: &str, outcome: RoutingAttemptOutcome) -> RoutingTraceAttempt {
    RoutingTraceAttempt {
        provider_id: id.to_string(),
        provider_name: id.to_uppercase(),
        outcome,
        http_status: None,
        error: None,
        ms: 5,
    }
}

#[test]
fn a_request_moves_from_pending_to_ok_and_every_change_is_pushed() {
    let (log, recorded) = log();
    let seq = log.begin(ToolId::Codex, Some("gpt-5".to_string()), 1_000);
    log.push_attempt(seq, attempt("a", RoutingAttemptOutcome::Ok));
    log.finish(seq, RoutingTraceStatus::Ok, None, 42);

    let snapshot = log.snapshot();
    assert_eq!(snapshot.revision, 3);
    assert_eq!(snapshot.counts.requests, 1);
    assert_eq!(snapshot.counts.rerouted, 0);
    assert_eq!(snapshot.counts.failed, 0);
    let entry = &snapshot.entries[0];
    assert_eq!(entry.status, RoutingTraceStatus::Ok);
    assert_eq!(entry.total_ms, Some(42));
    assert_eq!(entry.revision, 3);
    assert!(!entry.failed_over);

    let events = recorded.0.lock().expect("events");
    assert_eq!(
        events
            .iter()
            .map(|event| event.revision)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(events[0].entry.status, RoutingTraceStatus::Pending);
    assert_eq!(events[2].entry, *entry);
}

#[test]
fn a_second_try_marks_failover_and_counts_once_when_finished() {
    let (log, _) = log();
    let seq = log.begin(ToolId::ClaudeCode, None, 0);
    log.push_attempt(seq, attempt("a", RoutingAttemptOutcome::Failed));
    log.push_attempt(seq, attempt("b", RoutingAttemptOutcome::Ok));
    log.finish(seq, RoutingTraceStatus::Ok, None, 10);
    // A late second close is ignored and does not count again.
    log.finish(
        seq,
        RoutingTraceStatus::Failed,
        Some(RoutingErrorCategory::Other),
        99,
    );

    let snapshot = log.snapshot();
    assert!(snapshot.entries[0].failed_over);
    assert_eq!(snapshot.entries[0].status, RoutingTraceStatus::Ok);
    assert_eq!(snapshot.entries[0].total_ms, Some(10));
    assert_eq!(snapshot.counts.rerouted, 1);
    assert_eq!(snapshot.counts.failed, 0);
    assert_eq!(snapshot.revision, 4);
}

#[test]
fn a_failed_request_keeps_its_category_and_counts_as_seen_by_the_tool() {
    let (log, _) = log();
    let seq = log.begin(ToolId::GeminiCli, None, 0);
    log.finish(
        seq,
        RoutingTraceStatus::Failed,
        Some(RoutingErrorCategory::Unavailable),
        3,
    );
    let snapshot = log.snapshot();
    assert_eq!(
        snapshot.entries[0].error,
        Some(RoutingErrorCategory::Unavailable)
    );
    assert_eq!(snapshot.counts.failed, 1);
}

#[test]
fn the_ring_keeps_only_the_newest_entries_newest_first_but_totals_keep_counting() {
    let (log, recorded) = log();
    let first = log.begin(ToolId::Codex, None, 0);
    for index in 1..(MAX_ROUTING_TRACE_ENTRIES + 5) {
        log.begin(ToolId::Codex, None, index as i64);
    }
    let snapshot = log.snapshot();
    assert_eq!(snapshot.entries.len(), MAX_ROUTING_TRACE_ENTRIES);
    assert_eq!(
        snapshot.counts.requests,
        (MAX_ROUTING_TRACE_ENTRIES + 5) as u64
    );
    let seqs = snapshot
        .entries
        .iter()
        .map(|entry| entry.seq)
        .collect::<Vec<_>>();
    assert_eq!(seqs[0], (MAX_ROUTING_TRACE_ENTRIES + 5) as u64);
    assert!(seqs.windows(2).all(|pair| pair[0] > pair[1]));

    // An entry that fell out of the ring is neither changed nor pushed.
    let pushed = recorded.0.lock().expect("events").len();
    log.push_attempt(first, attempt("a", RoutingAttemptOutcome::Ok));
    log.finish(first, RoutingTraceStatus::Ok, None, 1);
    assert_eq!(recorded.0.lock().expect("events").len(), pushed);
    assert_eq!(log.snapshot().revision, snapshot.revision);
}

#[test]
fn tries_per_request_are_bounded() {
    let (log, _) = log();
    let seq = log.begin(ToolId::GrokBuild, None, 0);
    for index in 0..(MAX_ROUTING_TRACE_ATTEMPTS + 3) {
        log.push_attempt(
            seq,
            attempt(&format!("p{index}"), RoutingAttemptOutcome::Skipped),
        );
    }
    assert_eq!(
        log.snapshot().entries[0].attempts.len(),
        MAX_ROUTING_TRACE_ATTEMPTS
    );
}
