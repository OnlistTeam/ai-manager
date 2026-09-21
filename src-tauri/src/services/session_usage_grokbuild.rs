//! Grok Build (Grok CLI) session usage tracking
//!
//! Extracts usage from the `turn_completed` events of
//! `~/.grok/{sessions,archived_sessions}/<enc-cwd>/<session-id>/updates.jsonl` and writes it to
//! proxy_request_logs, providing usage stats for the official OAuth direct mode (no proxy data).
//!
//! ## Data flow
//! ```text
//! updates.jsonl (per-turn turn_completed) -> settle window / takeover guard -> cost -> proxy_request_logs
//! ```
//!
//! ## Event semantics (2026-07-23 single-process two-prompt measurement + CLI binary reversing)
//! - The usage of a `sessionUpdate == "turn_completed"` event is the [standalone total of that one
//!   user prompt turn]: accumulated across inference loops within the turn (`modelCalls`/`numTurns` =
//!   loop count of this turn), and the next turn starts from zero. It is [not] a process or session
//!   total - the process total travels on a separate CLI channel (`GetSessionUsage`, "since start or
//!   last resume") that never lands in updates.jsonl. Do NOT go back to differencing adjacent events:
//!   that mistakes a per-turn total for a cumulative snapshot and hugely under-reports (proven wrong).
//! - Booking each event at face value is the correct per-turn record; two turns with identical
//!   numbers are two real usages and both are booked as usual.
//! - `reasoningTokens` is a subset of `outputTokens` (totalTokens = input + output, and output
//!   back-computed from costUsdTicks does not add reasoning), so it is not billed.
//! - `costUsdTicks` (1 tick = 1e-10 USD) is the CLI's exact self-reported cost for the turn; six
//!   measured samples match the local grok-4.5-build 2/6/0.30 pricing exactly. **When a complete
//!   self-report exists, total_cost follows it** (the backfill only fills rows with total<=0 and never
//!   corrects a wrong price, and there is no repair path after booking, so the pricing drift window
//!   must not bet on local prices); local pricing drives the components and the drift warning.
//!   `costIsPartial` marks it as a lower bound: recompute locally when priced, else book the bound (components 0).
//! - Double counting under takeover is not prevented by fingerprint dedup: under takeover the CLI
//!   still writes updates.jsonl, but turn events are aggregates (summed over loops) and structurally
//!   unequal to per-request proxy rows. Instead a "settle window + takeover activity time window
//!   guard" is used: import only events old enough (takeover proxy rows are persisted by then), and
//!   before inserting, check for nearby proxy rows at the event time (see `has_recent_grokbuild_proxy_activity`).

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::proxy::usage::calculator::CostCalculator;
use crate::proxy::usage::parser::TokenUsage;
use crate::services::session_usage::{
    get_sync_state, metadata_modified_nanos, update_sync_state, SessionSyncResult,
};
use crate::services::sql_helpers::INPUT_TOKEN_SEMANTICS_TOTAL;
use crate::services::usage_stats::{
    find_model_pricing, has_recent_grokbuild_proxy_activity, SESSION_PROXY_DEDUP_WINDOW_SECONDS,
};
use rust_decimal::Decimal;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Event settle window: only import events older than "now - window".
///
/// Under takeover the CLI still writes updates.jsonl while the proxy already billed the same request
/// per request; proxy rows and session events appear almost simultaneously, so an import running
/// before the proxy row is persisted lets the takeover guard pass, leaving a permanent double count.
/// Letting events settle first guarantees the guard sees the persisted proxy row, removing the race at
/// the source. Cost: official-mode usage surfaces at most one window + one background cycle (60s) later.
const SETTLE_WINDOW_SECONDS: i64 = SESSION_PROXY_DEDUP_WINDOW_SECONDS;

/// Per-turn usage of a single model (from `modelUsage` or the top-level usage; both are per-turn)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GrokCounters {
    input: u64,
    output: u64,
    cached: u64,
    api_ms: u64,
    model_calls: u64,
    /// CLI self-reported cost for this turn, 1 tick = 1e-10 USD; 0 = not provided upstream
    cost_ticks: u64,
    /// Upstream marks cost_ticks as only part of the cost (`costIsPartial`): it is then a lower bound
    cost_partial: bool,
}

impl GrokCounters {
    fn is_zero(&self) -> bool {
        self.input == 0 && self.output == 0 && self.cached == 0
    }

    fn reported_cost_usd(&self) -> Option<Decimal> {
        (self.cost_ticks > 0)
            .then(|| Decimal::from(self.cost_ticks) / Decimal::from(10_000_000_000u64))
    }
}

/// A single `turn_completed` usage event
#[derive(Debug)]
struct GrokUsageEvent {
    created_at: i64,
    prompt_id: String,
    /// Event-level `costIsPartial` (observed on the top-level usage; applies to every model of this event)
    cost_is_partial: bool,
    per_model: Vec<(String, GrokCounters)>,
}

/// Sync Grok Build usage data (from the updates.jsonl session logs)
pub fn sync_grokbuild_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    let files = collect_grok_updates_files();

    let mut result = SessionSyncResult {
        files_scanned: files.len() as u32,
        ..Default::default()
    };

    for file_path in &files {
        match sync_single_grok_file(db, file_path) {
            Ok(file_result) => result.merge(file_result),
            Err(e) => {
                let msg = format!(
                    "Failed to parse Grok Build session file {}: {e}",
                    file_path.display()
                );
                log::warn!("[GROK-SYNC] {msg}");
                result.errors.push(msg);
            }
        }
    }

    if result.imported > 0 {
        log::info!(
            "[GROK-SYNC] sync finished: imported {}, skipped {}, scanned {} files, deferred {} files",
            result.imported,
            result.skipped,
            result.files_scanned,
            result.deferred_files
        );
    }

    Ok(result)
}

/// Collect the updates.jsonl of every Grok session (including archived ones, same root as the session browser)
fn collect_grok_updates_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in crate::session_manager::providers::grokbuild::session_roots() {
        collect_files_named(&root, "updates.jsonl", &mut files, 0);
    }
    files
}

/// Read limit for a single updates.jsonl file (50 MiB). A JSONL event line is usually a few KiB and a
/// normally active session never reaches this in months; anything larger is treated as broken/malicious and skipped.
const MAX_GROK_FILE_BYTES: u64 = 50 * 1024 * 1024;
/// Maximum directory depth when recursively collecting session logs, guarding against symlink cycles that overflow the stack.
const MAX_COLLECT_DEPTH: usize = 16;

/// Recursively collect files with the given name (tolerates layout depth changes, matching the session browser)
fn collect_files_named(root: &Path, name: &str, files: &mut Vec<PathBuf>, depth: usize) {
    if depth > MAX_COLLECT_DEPTH {
        log::warn!(
            "Grok session directory traversal exceeded max depth {} at {}",
            MAX_COLLECT_DEPTH,
            root.display()
        );
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `entry.metadata()` does not follow symlinks (unlike `path.is_dir()`), which is used here to
        // **skip every symlink unconditionally**: directory symlinks are not recursed into (avoiding
        // cycles) and file symlinks are not collected either - a same-named file pointing outside the
        // sessions root would be read as a session log. Cost: a user who symlinks the whole sessions
        // directory syncs nothing, so the skip must be logged to make silently missing usage diagnosable.
        let metadata = entry.metadata();
        if metadata.as_ref().map(|m| m.is_symlink()).unwrap_or(false) {
            log::info!(
                "[GROK-SYNC] skipping symlink (not followed): {}",
                path.display()
            );
            continue;
        }
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        if is_dir {
            collect_files_named(&path, name, files, depth + 1);
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            files.push(path);
        }
    }
}

/// Sync a single updates.jsonl file
fn sync_single_grok_file(db: &Database, file_path: &Path) -> Result<SessionSyncResult, AppError> {
    let file_path_str = file_path.to_string_lossy().to_string();

    let metadata = fs::metadata(file_path)
        .map_err(|e| AppError::Config(format!("Unable to read file metadata: {e}")))?;
    let file_modified = metadata_modified_nanos(&metadata);

    // Skip abnormally large files so a single read cannot exhaust memory.
    if metadata.len() > MAX_GROK_FILE_BYTES {
        log::warn!(
            "Grok session log too large ({} bytes), skipping: {}",
            metadata.len(),
            file_path.display()
        );
        return Ok(SessionSyncResult::default());
    }

    let (last_modified, _last_offset) = get_sync_state(db, &file_path_str)?;
    if file_modified <= last_modified {
        return Ok(SessionSyncResult::default());
    }

    // Full re-read whenever the file changed: the UPSERT is idempotent so re-reading is harmless, and
    // events deferred by the settle window rely on the next re-read. Events are already standalone
    // per-turn values, so incremental offset reads would be correct (no differencing baseline), but
    // they would need extra offset rollback handling for deferred events; the gain is not worth it yet.
    let content = fs::read_to_string(file_path)
        .map_err(|e| AppError::Config(format!("Unable to read file: {e}")))?;
    let events = parse_grok_usage_events(&content);

    // Session ID = session directory name (same as info.id in summary.json). request_id uniqueness
    // rests on that UUIDv7 being globally unique: archived and active copies of the same ID converge
    // idempotently through the UPSERT (intended), and an ID collision across <enc-cwd> is impossible.
    let session_id = file_path
        .parent()
        .and_then(|dir| dir.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut result = SessionSyncResult::default();
    let mut deferred = false;

    for (idx, event) in events.iter().enumerate() {
        // Settle window: events are time-monotonic in append order, so stop at the first unsettled one
        // and let the rest wait for the next round (keeping the simple "file prefix imported" invariant).
        // Known limitation: a future timestamp (misset clock) keeps deferring that file and re-scanning
        // it whole until the wall clock passes event time + window; a full re-read per cycle on active sessions is the accepted cost.
        if now.saturating_sub(event.created_at) < SETTLE_WINDOW_SECONDS {
            deferred = true;
            break;
        }

        // The takeover guard is evaluated once per event time and all model rows of the event share the
        // outcome; tokens skipped by the guard were already billed by proxy rows, so skipping is final
        // (sync state still advances). Known limitation: the guard has no session dimension, see usage_stats.rs.
        let takeover_active = {
            let conn = lock_conn!(db.conn);
            has_recent_grokbuild_proxy_activity(&conn, event.created_at)?
        };

        for (model, turn) in &event.per_model {
            if turn.is_zero() {
                continue;
            }
            if takeover_active {
                // Counted as skipped (same semantics as the gemini fingerprint dedup skip: not booked,
                // the proxy row is authoritative). Do not switch to suspected_duplicates - codex uses
                // the opposite meaning there (booked, pending review) and merge() would just sum both.
                result.skipped += 1;
                continue;
            }

            // The idempotency key is anchored on a stable upstream ID (prompt_id is a per-turn UUID) and
            // contains no in-file index: when the updates.jsonl prefix is rewritten (e.g. a rewind
            // truncation) and event indexes shift forward, surviving turns still hit their original rows
            // instead of double counting, and rows of removed turns are kept - a rewind does not refund
            // consumed tokens, so keeping them is correct. If upstream ever wrote several turn_completed
            // events for one prompt_id (never observed), the UPSERT keeps the latter, erring towards
            // under-counting. A missing prompt_id falls back to "idx{N}" (a UUID-shaped prompt_id cannot collide).
            let turn_key = if event.prompt_id.is_empty() {
                format!("idx{idx}")
            } else {
                event.prompt_id.clone()
            };
            let request_id = format!("grok_session:{session_id}:{turn_key}:{model}");
            match insert_grok_session_entry(
                db,
                &request_id,
                turn,
                event.cost_is_partial || turn.cost_partial,
                model,
                &session_id,
                event.created_at,
            ) {
                Ok(true) => result.imported += 1,
                Ok(false) => result.skipped += 1,
                Err(e) => {
                    log::warn!("[GROK-SYNC] insert failed ({request_id}): {e}");
                    result.skipped += 1;
                }
            }
        }
    }

    if deferred {
        // Do not persist the sync state: the next round re-reads the whole file and picks up the settled events.
        result.deferred_files += 1;
    } else {
        update_sync_state(db, &file_path_str, file_modified, events.len() as i64)?;
    }

    Ok(result)
}

/// Parse every per-turn usage event out of the updates.jsonl content (keeping file order)
fn parse_grok_usage_events(content: &str) -> Vec<GrokUsageEvent> {
    let mut events = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if record.get("method").and_then(|v| v.as_str()) != Some("_x.ai/session/update") {
            continue;
        }
        let update = record.get("params").and_then(|p| p.get("update"));
        // Only turn_completed is accepted (measurements show every event carrying usage is of this kind;
        // the discriminator field is sessionUpdate, serde internally-tagged). A missing field is let
        // through for backward compatibility, but an event explicitly tagged as another kind is not
        // imported even with usage - a mid-turn snapshot alongside the end-of-turn event would double count.
        let kind = update
            .and_then(|u| u.get("sessionUpdate"))
            .and_then(|v| v.as_str());
        if kind.is_some() && kind != Some("turn_completed") {
            continue;
        }
        let Some(usage) = update
            .and_then(|u| u.get("usage"))
            .filter(|u| u.is_object())
        else {
            continue;
        };
        // Both the settle window and the takeover guard need the event time, so events without a timestamp cannot be imported safely.
        let Some(created_at) = parse_event_timestamp(record.get("timestamp")) else {
            continue;
        };

        let prompt_id = update
            .and_then(|u| u.get("prompt_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut per_model: Vec<(String, GrokCounters)> = usage
            .get("modelUsage")
            .and_then(|m| m.as_object())
            .map(|map| {
                map.iter()
                    .map(|(model, counters)| (model.clone(), parse_grok_counters(counters)))
                    .collect()
            })
            .unwrap_or_default();
        if per_model.is_empty() {
            // Without modelUsage, fall back to the top-level per-turn values; the model name is unknown and the pricing lookup handles it.
            per_model.push(("unknown".to_string(), parse_grok_counters(usage)));
        }
        // modelUsage is a JSON object whose iteration order is not stable; sorting makes the insert order
        // and the logs deterministic across repeated rescans.
        per_model.sort_by(|a, b| a.0.cmp(&b.0));

        events.push(GrokUsageEvent {
            created_at,
            prompt_id,
            cost_is_partial: usage
                .get("costIsPartial")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            per_model,
        });
    }

    events
}

fn parse_grok_counters(value: &serde_json::Value) -> GrokCounters {
    let get = |key: &str| value.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
    GrokCounters {
        input: get("inputTokens"),
        output: get("outputTokens"),
        cached: get("cachedReadTokens"),
        api_ms: get("apiDurationMs"),
        model_calls: get("modelCalls"),
        cost_ticks: get("costUsdTicks"),
        cost_partial: value
            .get("costIsPartial")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

/// The top-level `timestamp` of updates.jsonl is measured to be a numeric epoch in seconds (not to be
/// confused with the RFC3339 string in summary.json); the string form is only a defensive fallback.
fn parse_event_timestamp(value: Option<&serde_json::Value>) -> Option<i64> {
    let value = value?;
    if let Some(n) = value.as_i64() {
        // Guard against a future millisecond form: above 1e11 treat it as milliseconds
        return Some(if n > 100_000_000_000 { n / 1000 } else { n });
    }
    value
        .as_str()
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.timestamp())
}

/// Insert a single Grok session record into proxy_request_logs
fn insert_grok_session_entry(
    db: &Database,
    request_id: &str,
    turn: &GrokCounters,
    cost_is_partial: bool,
    model: &str,
    session_id: &str,
    created_at: i64,
) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);

    let clamp = |v: u64| v.min(u32::MAX as u64) as u32;
    let usage = TokenUsage {
        input_tokens: clamp(turn.input),
        output_tokens: clamp(turn.output),
        cache_read_tokens: clamp(turn.cached),
        cache_creation_tokens: 0,
        model: Some(model.to_string()),
        message_id: None,
    };

    let pricing = find_model_pricing(&conn, model);
    let multiplier = Decimal::from(1);
    let reported = turn.reported_cost_usd();
    // Only emitted after a successful insert (changed), so a rescan does not spam the log
    let mut deferred_warn: Option<String> = None;

    // total_cost precedence (the backfill only fills rows with total<=0 and never corrects an existing
    // positive value, see backfill_missing_usage_costs; this importer's UPSERT also does not update on a
    // cost change alone - so it must be written correctly at insert time, there is no repair path):
    // 1. Complete self-reported value -> use it (upstream ground truth, accurate even during a pricing
    //    drift window; local pricing drives the components and the drift warning, and components may temporarily disagree with total);
    // 2. Incomplete self-report (costIsPartial) -> with a local price, recompute the full local amount
    //    (token counts are complete) and suppress the then-meaningless drift warning; without a price, keep the reported lower bound (better than 0);
    // 3. No self-report -> recompute locally; only with no price at all is the whole entry recorded as 0.
    let (input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost) = match pricing
    {
        Some(p) => {
            let cost = CostCalculator::calculate_for_app("grokbuild", &usage, &p, multiplier);
            let total = match reported {
                Some(reported) if !cost_is_partial => {
                    // A deviation above 1% (with a 1e-6 floor for tiny amounts) means local pricing drift
                    // - the earliest observable signal of an xAI price change, prompting a seed/repair update.
                    let tolerance = (reported * Decimal::new(1, 2)).max(Decimal::new(1, 6));
                    if (cost.total_cost - reported).abs() > tolerance {
                        deferred_warn = Some(format!(
                            "Local pricing deviates from the CLI self-reported cost beyond the threshold; total used the self-reported value, please update local pricing: model={model} local={} reported={reported} request_id={request_id}",
                            cost.total_cost
                        ));
                    }
                    reported
                }
                _ => cost.total_cost,
            };
            (
                cost.input_cost.to_string(),
                cost.output_cost.to_string(),
                cost.cache_read_cost.to_string(),
                cost.cache_creation_cost.to_string(),
                total.to_string(),
            )
        }
        None => {
            // A new alias that is not seeded: tokens are booked as usual; with a self-reported cost it is
            // used directly (components recorded as 0), and only with no price at all is the entry 0.
            // xAI internal aliases change periodically (grok-4.5-build is the precedent), so both cases must leave a diagnosable trace.
            let total = match reported {
                Some(reported) => {
                    if model != "unknown" {
                        let partial_note = if cost_is_partial {
                            " (upstream marked it as partial, so it is a lower bound)"
                        } else {
                            ""
                        };
                        deferred_warn = Some(format!(
                            "Model pricing not found, booking the CLI self-reported cost{partial_note}: model={model} total={reported} request_id={request_id}"
                        ));
                    }
                    reported.to_string()
                }
                None => {
                    if model != "unknown" {
                        deferred_warn = Some(format!(
                            "Model pricing not found and no self-reported cost, recording cost 0: model={model} request_id={request_id}"
                        ));
                    }
                    "0".to_string()
                }
            };
            (
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                total,
            )
        }
    };

    // UPSERT: rescans are idempotent; after a parsing semantics fix, a rescan updates existing rows
    // (token/cost/latency; created_at keeps its first value so rows do not drift across the settle window and rollup boundaries).
    // The data_source guard in WHERE is defense in depth: the request_id prefix already isolates the
    // namespace, but a collision with a row from another importer must never rewrite it.
    // input_token_semantics is written as TOTAL explicitly - the xAI inputTokens include cache reads,
    // matching the grokbuild proxy path rows (logger); do not rely on the column default.
    conn.execute(
        "INSERT INTO proxy_request_logs (
            request_id, provider_id, app_type, model, request_model,
            input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
            input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
            latency_ms, first_token_ms, status_code, error_message, session_id,
            provider_type, is_streaming, cost_multiplier, created_at, data_source,
            input_token_semantics
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
        ON CONFLICT(request_id) DO UPDATE SET
            model = excluded.model,
            input_tokens = excluded.input_tokens,
            output_tokens = excluded.output_tokens,
            cache_read_tokens = excluded.cache_read_tokens,
            input_cost_usd = excluded.input_cost_usd,
            output_cost_usd = excluded.output_cost_usd,
            cache_read_cost_usd = excluded.cache_read_cost_usd,
            cache_creation_cost_usd = excluded.cache_creation_cost_usd,
            total_cost_usd = excluded.total_cost_usd,
            latency_ms = excluded.latency_ms
        WHERE data_source = 'grok_session'
          AND (input_tokens != excluded.input_tokens
           OR output_tokens != excluded.output_tokens
           OR cache_read_tokens != excluded.cache_read_tokens
           OR latency_ms != excluded.latency_ms
           OR model != excluded.model)",
        rusqlite::params![
            request_id,
            "_grok_session",     // provider_id
            "grokbuild",         // app_type
            model,
            model,               // request_model = model
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            0i64,                // cache_creation_tokens
            input_cost,
            output_cost,
            cache_read_cost,
            cache_creation_cost,
            total_cost,
            turn.api_ms.min(i64::MAX as u64) as i64, // latency_ms (API duration of this turn)
            Option::<i64>::None, // first_token_ms
            200i64,              // status_code
            Option::<String>::None, // error_message
            session_id,
            Some("grok_session"), // provider_type
            1i64,                // is_streaming
            "1.0",               // cost_multiplier
            created_at,
            "grok_session",      // data_source
            INPUT_TOKEN_SEMANTICS_TOTAL,
        ],
    )
    .map_err(|e| AppError::Database(format!("Failed to insert the Grok Build session log: {e}")))?;

    // changes() > 0 means inserted or updated, == 0 means the values were identical (no real change)
    let changed = conn.changes() > 0;
    if changed {
        if let Some(msg) = deferred_warn {
            log::warn!("[GROK-SYNC] {msg}");
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    /// Fixed reference moment older than the settle window (2023-11-14T22:13:20Z)
    const OLD_EPOCH: i64 = 1_700_000_000;

    fn epoch_to_rfc3339(epoch: i64) -> String {
        chrono::DateTime::from_timestamp(epoch, 0)
            .expect("valid epoch")
            .to_rfc3339()
    }

    /// The top-level timestamp uses the real numeric epoch seconds form (the RFC3339 fallback is covered by the parses test)
    fn usage_event_line(epoch: i64, prompt_id: &str, model_usage: &str) -> String {
        format!(
            r#"{{"timestamp":{epoch},"method":"_x.ai/session/update","params":{{"update":{{"sessionUpdate":"turn_completed","prompt_id":"{prompt_id}","stop_reason":"end_turn","usage":{{"modelUsage":{{{model_usage}}}}}}}}}}}"#
        )
    }

    /// Variant carrying an event-level costIsPartial flag
    fn usage_event_line_partial(epoch: i64, prompt_id: &str, model_usage: &str) -> String {
        format!(
            r#"{{"timestamp":{epoch},"method":"_x.ai/session/update","params":{{"update":{{"sessionUpdate":"turn_completed","prompt_id":"{prompt_id}","stop_reason":"end_turn","usage":{{"costIsPartial":true,"modelUsage":{{{model_usage}}}}}}}}}}}"#
        )
    }

    fn model_counters(model: &str, input: u64, output: u64, cached: u64, calls: u64) -> String {
        model_counters_with_ticks(model, input, output, cached, calls, 0)
    }

    fn model_counters_with_ticks(
        model: &str,
        input: u64,
        output: u64,
        cached: u64,
        calls: u64,
        ticks: u64,
    ) -> String {
        format!(
            r#""{model}":{{"inputTokens":{input},"outputTokens":{output},"cachedReadTokens":{cached},"reasoningTokens":0,"modelCalls":{calls},"apiDurationMs":1000,"costUsdTicks":{ticks}}}"#
        )
    }

    fn write_session_file(dir: &Path, session_id: &str, lines: &[String]) -> PathBuf {
        let session_dir = dir.join("sessions").join("enc-project").join(session_id);
        std::fs::create_dir_all(&session_dir).expect("create session dir");
        let path = session_dir.join("updates.jsonl");
        let mut file = std::fs::File::create(&path).expect("create updates.jsonl");
        for line in lines {
            writeln!(file, "{line}").expect("write line");
        }
        path
    }

    /// (request_id, input, output, cache_read, input_token_semantics)
    type GrokSessionRow = (String, u32, u32, u32, i64);

    fn query_rows(db: &Database) -> Result<Vec<GrokSessionRow>, AppError> {
        let conn = lock_conn!(db.conn);
        let mut stmt = conn
            .prepare(
                "SELECT request_id, input_tokens, output_tokens, cache_read_tokens, input_token_semantics
                 FROM proxy_request_logs WHERE data_source = 'grok_session' ORDER BY request_id",
            )
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .expect("query")
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    fn query_costs(db: &Database) -> Result<Vec<(String, String)>, AppError> {
        let conn = lock_conn!(db.conn);
        let mut stmt = conn
            .prepare(
                "SELECT request_id, total_cost_usd FROM proxy_request_logs
                 WHERE data_source = 'grok_session' ORDER BY created_at, request_id",
            )
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query")
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    #[test]
    fn parses_turn_completed_and_ignores_noise_and_other_kinds() {
        let content = concat!(
            "{\"timestamp\":\"2026-07-20T13:26:10Z\",\"method\":\"session/update\",\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{}}}}\n",
            "not json at all\n",
            // Explicitly tagged as something other than turn_completed but carrying usage: must not be imported (mid-turn snapshot double count)
            "{\"timestamp\":\"2026-07-20T13:26:20Z\",\"method\":\"_x.ai/session/update\",\"params\":{\"update\":{\"sessionUpdate\":\"usage_snapshot\",\"prompt_id\":\"px\",\"usage\":{\"inputTokens\":9999,\"outputTokens\":9,\"cachedReadTokens\":0}}}}\n",
            "{\"timestamp\":\"2026-07-20T13:26:24Z\",\"method\":\"_x.ai/session/update\",\"params\":{\"update\":{\"sessionUpdate\":\"turn_completed\",\"prompt_id\":\"p1\",\"usage\":{\"inputTokens\":16632,\"outputTokens\":104,\"cachedReadTokens\":0,\"modelUsage\":{\"grok-4.5-build\":{\"inputTokens\":16632,\"outputTokens\":104,\"cachedReadTokens\":0,\"apiDurationMs\":5342,\"costUsdTicks\":338880000}}}}}}\n",
        );
        let events = parse_grok_usage_events(content);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].prompt_id, "p1");
        assert_eq!(events[0].per_model.len(), 1);
        assert_eq!(events[0].per_model[0].0, "grok-4.5-build");
        assert_eq!(
            events[0].per_model[0].1,
            GrokCounters {
                input: 16632,
                output: 104,
                cached: 0,
                api_ms: 5342,
                model_calls: 0,
                cost_ticks: 338_880_000,
                cost_partial: false,
            }
        );
    }

    #[test]
    fn missing_model_usage_falls_back_to_top_level_counters() {
        // Also covers: a missing sessionUpdate field is let through for backward compatibility
        let line = format!(
            r#"{{"timestamp":"{}","method":"_x.ai/session/update","params":{{"update":{{"prompt_id":"p1","usage":{{"inputTokens":100,"outputTokens":10,"cachedReadTokens":5}}}}}}}}"#,
            epoch_to_rfc3339(OLD_EPOCH)
        );
        let events = parse_grok_usage_events(&line);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].per_model[0].0, "unknown");
        assert_eq!(events[0].per_model[0].1.input, 100);
    }

    #[test]
    fn two_turns_import_at_face_value_matching_reported_ticks() -> Result<(), AppError> {
        use std::str::FromStr;
        // Raw values measured on 2026-07-23 with two prompts in one process: turn_completed is a standalone per-turn total.
        // Differencing adjacent events would record the second turn as a fake 53/28/6144 delta (an old bug).
        // The per-turn ticks pin down both the per-turn semantics and the 2/6/0.30 pricing:
        //   turn 1 (17294-11136)*2 + 11136*0.30 + 28*6 = 15824.8 uUSD = 158248000 ticks
        //   turn 2 (17347-17280)*2 + 17280*0.30 + 56*6 =  5654.0 uUSD =  56540000 ticks
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let lines = vec![
            usage_event_line(
                OLD_EPOCH,
                "p1",
                &model_counters_with_ticks("grok-4.5-build", 17294, 28, 11136, 1, 158_248_000),
            ),
            usage_event_line(
                OLD_EPOCH + 60,
                "p2",
                &model_counters_with_ticks("grok-4.5-build", 17347, 56, 17280, 1, 56_540_000),
            ),
        ];
        let path = write_session_file(temp.path(), "sess-two-turns", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(result.imported, 2);
        assert_eq!(result.deferred_files, 0);

        let rows = query_rows(&db)?;
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].1, rows[0].2, rows[0].3), (17294, 28, 11136));
        assert_eq!((rows[1].1, rows[1].2, rows[1].3), (17347, 56, 17280));
        // The semantics column is explicitly TOTAL, matching the proxy path
        assert!(rows.iter().all(|r| r.4 == INPUT_TOKEN_SEMANTICS_TOTAL));

        // The local pricing recomputation must match the CLI self-reported ticks exactly (the drift warning stays silent within this threshold)
        let costs = query_costs(&db)?;
        let expected1 = Decimal::from(158_248_000u64) / Decimal::from(10_000_000_000u64);
        let expected2 = Decimal::from(56_540_000u64) / Decimal::from(10_000_000_000u64);
        assert_eq!(Decimal::from_str(&costs[0].1).expect("decimal"), expected1);
        assert_eq!(Decimal::from_str(&costs[1].1).expect("decimal"), expected2);
        Ok(())
    }

    #[test]
    fn second_turn_with_smaller_counters_imports_at_face_value() -> Result<(), AppError> {
        // Raw values measured on 2026-07-23 across processes (process A one turn 27386/74/15360, the
        // --resume process B one turn 13793/21/13696). Under per-turn semantics a "smaller second turn"
        // is normal, unrelated to crossing processes, and is always booked at face value.
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let lines = vec![
            usage_event_line(
                OLD_EPOCH,
                "p1",
                &model_counters("grok-4.5-build", 27386, 74, 15360, 2),
            ),
            usage_event_line(
                OLD_EPOCH + 15,
                "p2",
                &model_counters("grok-4.5-build", 13793, 21, 13696, 1),
            ),
        ];
        let path = write_session_file(temp.path(), "sess-resume", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(result.imported, 2);

        let rows = query_rows(&db)?;
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].1, rows[0].2, rows[0].3), (27386, 74, 15360));
        assert_eq!((rows[1].1, rows[1].2, rows[1].3), (13793, 21, 13696));
        Ok(())
    }

    #[test]
    fn identical_turns_both_import() -> Result<(), AppError> {
        // Regression (per-turn semantics): two turns with identical numbers are two real usages and both must be booked.
        // Differencing semantics would skip the second turn as a zero delta - exactly the disproven old behaviour.
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let lines = vec![
            usage_event_line(
                OLD_EPOCH,
                "p1",
                &model_counters("grok-4.5-build", 100, 10, 0, 1),
            ),
            usage_event_line(
                OLD_EPOCH + 60,
                "p2",
                &model_counters("grok-4.5-build", 100, 10, 0, 1),
            ),
        ];
        let path = write_session_file(temp.path(), "sess-identical", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(
            result.imported, 2,
            "two turns with identical numbers are both real usage"
        );
        assert_eq!(query_rows(&db)?.len(), 2);
        Ok(())
    }

    #[test]
    fn multi_model_event_produces_row_per_model() -> Result<(), AppError> {
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let both = format!(
            "{},{}",
            model_counters("grok-4.5-build", 100, 10, 0, 1),
            model_counters("grok-4.3", 30, 3, 0, 1)
        );
        let lines = vec![usage_event_line(OLD_EPOCH, "p1", &both)];
        let path = write_session_file(temp.path(), "sess-multi", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(result.imported, 2);
        let rows = query_rows(&db)?;
        assert!(rows[0].0.ends_with(":grok-4.3"));
        assert!(rows[1].0.ends_with(":grok-4.5-build"));
        Ok(())
    }

    #[test]
    fn settle_window_defers_recent_events_without_recording_sync_state() -> Result<(), AppError> {
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("now")
            .as_secs() as i64;
        let lines = vec![
            usage_event_line(
                OLD_EPOCH,
                "p1",
                &model_counters("grok-4.5-build", 100, 10, 0, 1),
            ),
            // An unsettled new event: deferred this round, and the sync state is not persisted so the next round re-reads
            usage_event_line(now, "p2", &model_counters("grok-4.5-build", 250, 30, 0, 1)),
        ];
        let path = write_session_file(temp.path(), "sess-settle", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(result.imported, 1);
        assert_eq!(result.deferred_files, 1);
        assert_eq!(query_rows(&db)?.len(), 1);

        let (last_modified, _) = get_sync_state(&db, &path.to_string_lossy())?;
        assert_eq!(
            last_modified, 0,
            "the sync state must not be recorded while deferring"
        );

        // Next round re-read: the old event UPSERTs unchanged, the new event is still unsettled and stays deferred
        let rerun = sync_single_grok_file(&db, &path)?;
        assert_eq!(rerun.imported, 0);
        assert_eq!(rerun.skipped, 1);
        assert_eq!(rerun.deferred_files, 1);
        assert_eq!(query_rows(&db)?.len(), 1);
        Ok(())
    }

    #[test]
    fn takeover_guard_skips_events_near_proxy_activity() -> Result<(), AppError> {
        let db = Database::memory()?;
        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model, request_model,
                    input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                    total_cost_usd, latency_ms, status_code, created_at, data_source
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    "grok-proxy-req",
                    "some-provider",
                    "grokbuild",
                    "grok-4.5",
                    "grok-4.5",
                    999,
                    88,
                    0,
                    0,
                    "0.01",
                    100,
                    200,
                    OLD_EPOCH + 30,
                    "proxy"
                ],
            )?;
        }
        let temp = tempdir().expect("tempdir");
        let lines = vec![
            // The event time falls inside the proxy row +/- window -> takeover, skipped (the proxy row is authoritative)
            usage_event_line(
                OLD_EPOCH,
                "p1",
                &model_counters("grok-4.5-build", 100, 10, 0, 1),
            ),
            // A later event far from the takeover window imports normally at face value
            usage_event_line(
                OLD_EPOCH + SESSION_PROXY_DEDUP_WINDOW_SECONDS + 3600,
                "p2",
                &model_counters("grok-4.5-build", 250, 30, 0, 1),
            ),
        ];
        let path = write_session_file(temp.path(), "sess-guard", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(
            result.skipped, 1,
            "a guard skip counts as skipped (not booked)"
        );
        assert_eq!(result.imported, 1);

        let rows = query_rows(&db)?;
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].1, 250,
            "an event outside the guard is booked at this turn's face value"
        );
        Ok(())
    }

    #[test]
    fn rescan_is_idempotent() -> Result<(), AppError> {
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let lines = vec![
            usage_event_line(
                OLD_EPOCH,
                "p1",
                &model_counters("grok-4.5-build", 100, 10, 0, 1),
            ),
            usage_event_line(
                OLD_EPOCH + 60,
                "p2",
                &model_counters("grok-4.5-build", 250, 30, 50, 1),
            ),
        ];
        let path = write_session_file(temp.path(), "sess-idem", &lines);

        let first = sync_single_grok_file(&db, &path)?;
        assert_eq!(first.imported, 2);

        // mtime unchanged -> short circuit
        let second = sync_single_grok_file(&db, &path)?;
        assert_eq!(second.imported + second.skipped, 0);

        // Force a re-read (clear the sync state) -> the UPSERT changes nothing
        {
            let conn = lock_conn!(db.conn);
            conn.execute("DELETE FROM session_log_sync", [])?;
        }
        let third = sync_single_grok_file(&db, &path)?;
        assert_eq!(third.imported, 0);
        assert_eq!(third.skipped, 2);
        assert_eq!(query_rows(&db)?.len(), 2);
        Ok(())
    }

    #[test]
    fn rewind_truncation_does_not_double_count() -> Result<(), AppError> {
        // Regression (found in review): if the idempotency key contained the in-file index, rewriting the
        // updates.jsonl prefix (a rewind truncation) would shift surviving event indexes forward and
        // generate new request_ids, double counting. With the prompt_id key: surviving turns hit their
        // original rows, and rows of removed turns are kept (a rewind does not refund consumed tokens).
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let full = vec![
            usage_event_line(
                OLD_EPOCH,
                "p1",
                &model_counters("grok-4.5-build", 100, 10, 0, 1),
            ),
            usage_event_line(
                OLD_EPOCH + 60,
                "p2",
                &model_counters("grok-4.5-build", 200, 20, 0, 1),
            ),
            usage_event_line(
                OLD_EPOCH + 120,
                "p3",
                &model_counters("grok-4.5-build", 300, 30, 0, 1),
            ),
        ];
        let path = write_session_file(temp.path(), "sess-rewind", &full);
        assert_eq!(sync_single_grok_file(&db, &path)?.imported, 3);

        // Simulate a rewind cutting p2: p3 moves from idx2 to idx1
        let truncated = vec![full[0].clone(), full[2].clone()];
        write_session_file(temp.path(), "sess-rewind", &truncated);
        {
            let conn = lock_conn!(db.conn);
            conn.execute("DELETE FROM session_log_sync", [])?;
        }

        let rescan = sync_single_grok_file(&db, &path)?;
        assert_eq!(
            rescan.imported, 0,
            "a surviving turn must not be re-booked because its index shifted"
        );

        let rows = query_rows(&db)?;
        assert_eq!(
            rows.len(),
            3,
            "rows of truncated turns are kept (the tokens were really consumed)"
        );
        let p3: Vec<_> = rows.iter().filter(|r| r.0.contains(":p3:")).collect();
        assert_eq!(p3.len(), 1);
        assert_eq!(p3[0].1, 300);
        Ok(())
    }

    #[test]
    fn empty_prompt_id_falls_back_to_index_key() -> Result<(), AppError> {
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let lines = vec![usage_event_line(
            OLD_EPOCH,
            "",
            &model_counters("grok-4.5-build", 100, 10, 0, 1),
        )];
        let path = write_session_file(temp.path(), "sess-noprompt", &lines);

        assert_eq!(sync_single_grok_file(&db, &path)?.imported, 1);
        let rows = query_rows(&db)?;
        assert!(
            rows[0].0.contains(":idx0:"),
            "an empty prompt_id falls back to the index key"
        );
        Ok(())
    }

    #[test]
    fn cost_matches_cli_reported_ticks_for_seeded_grok45_build() -> Result<(), AppError> {
        use std::str::FromStr;
        // Real sample: inputTokens=16632, outputTokens=104, cache=0,
        // costUsdTicks=338880000 (1 tick = 1e-10 USD). The seeded grok-4.5-build pricing (2/6) must
        // reproduce the CLI self-reported cost exactly. The fixture deliberately carries no ticks, so
        // this verifies the independent local pricing recomputation.
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let lines = vec![usage_event_line(
            OLD_EPOCH,
            "p1",
            &model_counters("grok-4.5-build", 16632, 104, 0, 1),
        )];
        let path = write_session_file(temp.path(), "sess-ticks", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(result.imported, 1);

        let conn = lock_conn!(db.conn);
        let total: String = conn.query_row(
            "SELECT total_cost_usd FROM proxy_request_logs WHERE data_source = 'grok_session'",
            [],
            |row| row.get(0),
        )?;
        let expected = Decimal::from(338_880_000u64) / Decimal::from(10_000_000_000u64);
        assert_eq!(Decimal::from_str(&total).expect("decimal"), expected);
        Ok(())
    }

    #[test]
    fn cost_matches_cli_reported_ticks_with_cache_reads() -> Result<(), AppError> {
        use std::str::FromStr;
        // Cached sample measured on 2026-07-23: 13793/21/13696, costUsdTicks=44288000.
        // Pins the measured cache read unit price 0.30: billable_input=(13793-13696)*2/1M
        // + 21*6/1M + 13696*0.30/1M = 0.0044288. This test goes red if the seed reverts to 0.50.
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let lines = vec![usage_event_line(
            OLD_EPOCH,
            "p1",
            &model_counters("grok-4.5-build", 13793, 21, 13696, 1),
        )];
        let path = write_session_file(temp.path(), "sess-ticks-cache", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(result.imported, 1);

        let conn = lock_conn!(db.conn);
        let total: String = conn.query_row(
            "SELECT total_cost_usd FROM proxy_request_logs WHERE data_source = 'grok_session'",
            [],
            |row| row.get(0),
        )?;
        let expected = Decimal::from(44_288_000u64) / Decimal::from(10_000_000_000u64);
        assert_eq!(Decimal::from_str(&total).expect("decimal"), expected);
        Ok(())
    }

    #[test]
    fn reported_ticks_override_stale_local_pricing() -> Result<(), AppError> {
        use std::str::FromStr;
        // Pricing drift window: the CLI self-report is twice the local recomputation (338880000 ticks),
        // simulating an xAI price change with a stale seed. total must follow the self-report (the
        // backfill never corrects positive rows, so a wrong local price would be permanent); components still use local pricing (temporarily inconsistent, with a drift warning).
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let lines = vec![usage_event_line(
            OLD_EPOCH,
            "p1",
            &model_counters_with_ticks("grok-4.5-build", 16632, 104, 0, 1, 677_760_000),
        )];
        let path = write_session_file(temp.path(), "sess-drift", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(result.imported, 1);

        let conn = lock_conn!(db.conn);
        let (input_cost, total): (String, String) = conn.query_row(
            "SELECT input_cost_usd, total_cost_usd FROM proxy_request_logs
             WHERE data_source = 'grok_session'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let expected_total = Decimal::from(677_760_000u64) / Decimal::from(10_000_000_000u64);
        assert_eq!(
            Decimal::from_str(&total).expect("decimal"),
            expected_total,
            "total follows the self-reported value"
        );
        assert!(
            Decimal::from_str(&input_cost).expect("decimal") > Decimal::ZERO,
            "components still use local pricing"
        );
        Ok(())
    }

    #[test]
    fn partial_reported_cost_prefers_local_pricing_when_priced() -> Result<(), AppError> {
        use std::str::FromStr;
        // costIsPartial=true: the report is only a lower bound and cannot be the total. Token counts are
        // complete, so with a local price the full local amount is recomputed (338880000 ticks here).
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let lines = vec![usage_event_line_partial(
            OLD_EPOCH,
            "p1",
            &model_counters_with_ticks("grok-4.5-build", 16632, 104, 0, 1, 1_000),
        )];
        let path = write_session_file(temp.path(), "sess-partial", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(result.imported, 1);

        let conn = lock_conn!(db.conn);
        let total: String = conn.query_row(
            "SELECT total_cost_usd FROM proxy_request_logs WHERE data_source = 'grok_session'",
            [],
            |row| row.get(0),
        )?;
        let expected = Decimal::from(338_880_000u64) / Decimal::from(10_000_000_000u64);
        assert_eq!(Decimal::from_str(&total).expect("decimal"), expected);
        Ok(())
    }

    #[test]
    fn unpriced_model_falls_back_to_reported_ticks() -> Result<(), AppError> {
        use std::str::FromStr;
        // A new alias that is not seeded: total_cost uses the CLI self-reported ticks (components 0)
        // instead of recording the whole entry as 0.
        let db = Database::memory()?;
        let temp = tempdir().expect("tempdir");
        let lines = vec![usage_event_line(
            OLD_EPOCH,
            "p1",
            &model_counters_with_ticks("grok-6-future-alias", 1000, 100, 0, 1, 56_540_000),
        )];
        let path = write_session_file(temp.path(), "sess-unpriced", &lines);

        let result = sync_single_grok_file(&db, &path)?;
        assert_eq!(result.imported, 1);

        let conn = lock_conn!(db.conn);
        let (input_cost, total): (String, String) = conn.query_row(
            "SELECT input_cost_usd, total_cost_usd FROM proxy_request_logs
             WHERE data_source = 'grok_session'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(
            Decimal::from_str(&input_cost).expect("decimal"),
            Decimal::ZERO
        );
        let expected = Decimal::from(56_540_000u64) / Decimal::from(10_000_000_000u64);
        assert_eq!(Decimal::from_str(&total).expect("decimal"), expected);
        Ok(())
    }

    #[test]
    fn oversized_updates_jsonl_is_skipped_without_reading_into_memory() {
        let db = Database::memory().expect("memory db");
        let temp = tempdir().expect("tempdir");
        let path = write_session_file(temp.path(), "sess-huge", &[]);

        // Build a file larger than 50 MiB whose content is empty (nothing to parse).
        let huge = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .expect("open");
        huge.set_len(MAX_GROK_FILE_BYTES + 1).expect("set_len");
        drop(huge);

        let result = sync_single_grok_file(&db, &path).expect("sync should not fail");
        assert_eq!(result.imported, 0, "oversized file must not be imported");
        assert_eq!(result.skipped, 0);
        assert_eq!(result.deferred_files, 0);
    }

    #[test]
    fn symlink_cycle_does_not_cause_stack_overflow() {
        let temp = tempdir().expect("tempdir");
        let sessions = temp.path().join("sessions");
        let enc = sessions.join("enc-project");
        let sub = enc.join("sub");
        std::fs::create_dir_all(&sub).expect("create dirs");

        // Build a cycle: sub/cycle -> the enc parent directory
        #[cfg(unix)]
        std::os::unix::fs::symlink(&enc, sub.join("cycle")).expect("symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&enc, sub.join("cycle")).expect("symlink");

        // Also drop in a real target file to confirm normal traversal still works
        std::fs::write(enc.join("updates.jsonl"), b"{}\n").expect("write real file");

        let mut files = Vec::new();
        collect_files_named(&sessions, "updates.jsonl", &mut files, 0);

        assert_eq!(
            files.len(),
            1,
            "only the real updates.jsonl should be collected; symlink cycle must not crash"
        );
    }
}
