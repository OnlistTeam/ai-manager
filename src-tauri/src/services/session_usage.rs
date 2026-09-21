//! Claude Code session log usage tracking.
//!
//! Extracts token usage data from JSONL session files under
//! ~/.claude/projects/, powering usage stats without a proxy.
//!
//! ## Data flow
//! ```text
//! ~/.claude/projects/*/*.jsonl → incremental parse → dedup → cost calc → proxy_request_logs table
//! ```

use crate::config::get_claude_config_dir;
use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::proxy::usage::calculator::{CostCalculator, ModelPricing};
use crate::proxy::usage::parser::TokenUsage;
use crate::services::usage_stats::{find_model_pricing, should_skip_session_insert, DedupKey};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

mod claude_batch;

/// Sync result
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncResult {
    pub imported: u32,
    pub skipped: u32,
    pub files_scanned: u32,
    pub suspected_duplicates: u32,
    pub deferred_files: u32,
    pub errors: Vec<String>,
}

impl SessionSyncResult {
    pub fn merge(&mut self, other: SessionSyncResult) {
        self.imported = self.imported.saturating_add(other.imported);
        self.skipped = self.skipped.saturating_add(other.skipped);
        self.files_scanned = self.files_scanned.saturating_add(other.files_scanned);
        self.suspected_duplicates = self
            .suspected_duplicates
            .saturating_add(other.suspected_duplicates);
        self.deferred_files = self.deferred_files.saturating_add(other.deferred_files);
        self.errors.extend(other.errors);
    }
}

pub fn session_sync_mutex() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn merge_sync_step(
    aggregate: &mut SessionSyncResult,
    name: &str,
    step: Result<SessionSyncResult, AppError>,
) {
    match step {
        Ok(result) => aggregate.merge(result),
        Err(error) => aggregate
            .errors
            .push(format!("{name} sync failed: {error}")),
    }
}

/// Callers must hold [`session_sync_mutex`]. This function is the sync kernel
/// shared by background tasks, manual sync, and Codex rebuild, avoiding tokio Mutex reentrancy.
pub fn sync_all_unlocked(db: &Database) -> SessionSyncResult {
    let mut result = SessionSyncResult::default();
    merge_sync_step(&mut result, "Claude", sync_claude_session_logs(db));
    merge_sync_step(
        &mut result,
        "Codex",
        crate::services::session_usage_codex::sync_codex_usage(db),
    );
    merge_sync_step(
        &mut result,
        "Gemini",
        crate::services::session_usage_gemini::sync_gemini_usage(db),
    );
    merge_sync_step(
        &mut result,
        "OpenCode",
        crate::services::session_usage_opencode::sync_opencode_usage(db),
    );
    merge_sync_step(
        &mut result,
        "Grok Build",
        crate::services::session_usage_grokbuild::sync_grokbuild_usage(db),
    );
    merge_sync_step(
        &mut result,
        "Pi",
        crate::services::session_usage_pi::sync_pi_usage(db),
    );
    notify_sync_result(&result);
    result
}

pub(crate) fn notify_sync_result(result: &SessionSyncResult) {
    if result.imported > 0 {
        crate::usage_events::notify_log_recorded();
    }
}

/// Assistant message usage data parsed from JSONL
#[derive(Debug)]
struct ParsedAssistantUsage {
    message_id: String,
    model: String,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_tokens: u32,
    cache_creation_tokens: u32,
    stop_reason: Option<String>,
    timestamp: Option<String>,
    session_id: Option<String>,
}

/// Sync Claude Code session logs into the usage stats database
pub fn sync_claude_session_logs(db: &Database) -> Result<SessionSyncResult, AppError> {
    let projects_dir = get_claude_config_dir().join("projects");
    if !projects_dir.exists() {
        return Ok(SessionSyncResult {
            imported: 0,
            skipped: 0,
            files_scanned: 0,
            suspected_duplicates: 0,
            deferred_files: 0,
            errors: vec![],
        });
    }

    let mut result = SessionSyncResult {
        imported: 0,
        skipped: 0,
        files_scanned: 0,
        suspected_duplicates: 0,
        deferred_files: 0,
        errors: vec![],
    };

    // Collect all .jsonl files
    let jsonl_files = collect_jsonl_files(&projects_dir);

    for file_path in &jsonl_files {
        result.files_scanned += 1;

        match sync_single_file(db, file_path) {
            Ok((imported, skipped)) => {
                result.imported += imported;
                result.skipped += skipped;
            }
            Err(e) => {
                let msg = format!("{}: {e}", file_path.display());
                log::warn!("[SESSION-SYNC] File parse failed: {msg}");
                result.errors.push(msg);
            }
        }
    }

    if result.imported > 0 {
        log::info!(
            "[SESSION-SYNC] Sync complete: imported {} rows, skipped {} rows, scanned {} files",
            result.imported,
            result.skipped,
            result.files_scanned
        );
    }

    Ok(result)
}

/// Collect all .jsonl files under the directory (including subagent files).
///
/// Scans a fixed depth without recursion, to avoid infinite loops:
///   projects_dir/project_dir/*.jsonl                                    (main session)
///   projects_dir/project_dir/SESSION_ID/subagents/*.jsonl               (Task/Agent subagent)
///   projects_dir/project_dir/SESSION_ID/subagents/workflows/wf_*/*.jsonl (Workflow subagent)
///
/// The last layer is the subagent transcript produced by the Claude Code
/// Workflow feature, nested one level deeper under `workflows/wf_<ID>/` than
/// a plain subagent. Missing this layer would leave Workflow token usage
/// completely uncounted; `journal.jsonl` has no `type=="assistant"` lines, so
/// `sync_single_file` naturally skips it during parsing — no filename filter
/// is needed here.
fn collect_jsonl_files(projects_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    let entries = match fs::read_dir(projects_dir) {
        Ok(e) => e,
        Err(_) => return files,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // .jsonl files directly under each project directory
        if let Ok(sub_entries) = fs::read_dir(&path) {
            for sub_entry in sub_entries.flatten() {
                let sub_path = sub_entry.path();
                if sub_path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    // Main session JSONL file
                    files.push(sub_path);
                } else if sub_path.is_dir() {
                    // Scan subagent directory: project/SESSION_ID/subagents/*.jsonl
                    let subagents_dir = sub_path.join("subagents");
                    if subagents_dir.is_dir() {
                        push_jsonl_children(&subagents_dir, &mut files);

                        // Additionally descend into Workflow subagents:
                        // project/SESSION_ID/subagents/workflows/wf_<ID>/*.jsonl
                        let workflows_dir = subagents_dir.join("workflows");
                        if workflows_dir.is_dir() {
                            if let Ok(wf_entries) = fs::read_dir(&workflows_dir) {
                                for wf_entry in wf_entries.flatten() {
                                    let wf_path = wf_entry.path();
                                    if wf_path.is_dir() {
                                        push_jsonl_children(&wf_path, &mut files);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    files
}

/// Append all `.jsonl` files directly under `dir` to `files` (no recursion).
fn push_jsonl_children(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                files.push(path);
            }
        }
    }
}

/// Sync a single JSONL file, returning (imported, skipped)
fn sync_single_file(db: &Database, file_path: &Path) -> Result<(u32, u32), AppError> {
    let file_path_str = file_path.to_string_lossy().to_string();

    // Get file metadata
    let metadata = fs::metadata(file_path)
        .map_err(|e| AppError::Config(format!("Failed to read file metadata: {e}")))?;
    let file_modified = metadata_modified_nanos(&metadata);

    // Check sync state
    let (last_modified, last_offset) = get_sync_state(db, &file_path_str)?;

    // Skip if the file hasn't changed
    if file_modified <= last_modified {
        return Ok((0, 0));
    }

    // Incrementally parse starting from the last offset
    let file = fs::File::open(file_path)
        .map_err(|e| AppError::Config(format!("Failed to open file: {e}")))?;
    let reader = BufReader::new(file);

    let mut line_offset: i64 = 0;
    let mut messages: HashMap<String, ParsedAssistantUsage> = HashMap::new();
    let mut current_session_id: Option<String> = None;

    for line_result in reader.lines() {
        line_offset += 1;

        // Skip already-processed lines
        if line_offset <= last_offset {
            continue;
        }

        let line = match line_result {
            Ok(l) => l,
            Err(_) => continue, // Tolerate an incomplete final line
        };

        if line.trim().is_empty() {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Extract session ID (from system or the first message)
        if current_session_id.is_none() {
            if let Some(sid) = value.get("sessionId").and_then(|v| v.as_str()) {
                current_session_id = Some(sid.to_string());
            }
        }

        // Only process assistant-type messages
        if value.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }

        let message = match value.get("message") {
            Some(m) => m,
            None => continue,
        };

        let msg_id = match message.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };

        let usage = match message.get("usage") {
            Some(u) => u,
            None => continue,
        };

        let parsed = ParsedAssistantUsage {
            message_id: msg_id.clone(),
            model: message
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            input_tokens: usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            output_tokens: usage
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            cache_read_tokens: usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            cache_creation_tokens: usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            stop_reason: message
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            timestamp: value
                .get("timestamp")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            session_id: current_session_id.clone(),
        };

        // Dedup by message.id: prefer the entry with stop_reason, otherwise keep the latest
        let should_replace = match messages.get(&msg_id) {
            None => true,
            Some(existing) => {
                // New entry has stop_reason, old one doesn't → replace
                if parsed.stop_reason.is_some() && existing.stop_reason.is_none() {
                    true
                }
                // Both have or both lack stop_reason → keep the one with larger output_tokens
                else if parsed.stop_reason.is_some() == existing.stop_reason.is_some() {
                    parsed.output_tokens > existing.output_tokens
                } else {
                    false
                }
            }
        };

        if should_replace {
            messages.insert(msg_id, parsed);
        }
    }

    claude_batch::persist(
        db,
        &file_path_str,
        file_modified,
        line_offset,
        messages.values(),
    )
}

/// Get the sync progress of an entry in the session_log_sync table.
///
/// Shared by all session_usage_* parsers.
pub(crate) fn get_sync_state(db: &Database, file_path: &str) -> Result<(i64, i64), AppError> {
    let conn = lock_conn!(db.conn);
    let result = conn.query_row(
        "SELECT last_modified, last_line_offset FROM session_log_sync WHERE file_path = ?1",
        rusqlite::params![file_path],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    );
    Ok(result.unwrap_or((0, 0)))
}

/// Return the file's mtime as a nanosecond timestamp.
///
/// `session_log_sync.last_modified` holds second-precision timestamps for
/// old data; writing nanosecond values needs no schema migration — old
/// values naturally trigger one incremental rescan and still rely on the
/// line offset to avoid duplicate imports.
pub(crate) fn metadata_modified_nanos(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

/// Update the sync progress of an entry in the session_log_sync table.
///
/// Shared by all session_usage_* parsers.
pub(crate) fn update_sync_state(
    db: &Database,
    file_path: &str,
    last_modified: i64,
    last_offset: i64,
) -> Result<(), AppError> {
    let conn = lock_conn!(db.conn);
    update_sync_state_on_conn(&conn, file_path, last_modified, last_offset)
}

/// Lock-free version of [`update_sync_state`], for callers inside an
/// already-locked transaction to bind cursor advancement and data insertion
/// into one atomic commit.
pub(crate) fn update_sync_state_on_conn(
    conn: &rusqlite::Connection,
    file_path: &str,
    last_modified: i64,
    last_offset: i64,
) -> Result<(), AppError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    conn.prepare_cached(
        "INSERT OR REPLACE INTO session_log_sync (file_path, last_modified, last_line_offset, last_synced_at)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .and_then(|mut stmt| stmt.execute(rusqlite::params![file_path, last_modified, last_offset, now]))
    .map_err(|e| AppError::Database(format!("Failed to update sync state: {e}")))?;
    Ok(())
}

/// Insert one session log entry into proxy_request_logs, returning whether it was newly inserted (true=inserted, false=already exists)
#[cfg(test)]
fn insert_session_log_entry(
    db: &Database,
    request_id: &str,
    msg: &ParsedAssistantUsage,
) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);
    insert_session_log_entry_on_conn(&conn, request_id, msg, &mut HashMap::new())
}

fn insert_session_log_entry_on_conn(
    conn: &rusqlite::Connection,
    request_id: &str,
    msg: &ParsedAssistantUsage,
    pricing_cache: &mut HashMap<String, Option<ModelPricing>>,
) -> Result<bool, AppError> {
    let created_at = msg
        .timestamp
        .as_ref()
        .and_then(|ts| {
            chrono::DateTime::parse_from_rfc3339(ts)
                .ok()
                .map(|dt| dt.timestamp())
        })
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });

    let dedup_key = DedupKey {
        app_type: "claude",
        model: &msg.model,
        input_tokens: msg.input_tokens,
        output_tokens: msg.output_tokens,
        cache_read_tokens: msg.cache_read_tokens,
        cache_creation_tokens: msg.cache_creation_tokens,
        created_at,
    };
    if should_skip_session_insert(conn, request_id, &dedup_key)? {
        return Ok(false);
    }

    // Calculate cost
    let usage = TokenUsage {
        input_tokens: msg.input_tokens,
        output_tokens: msg.output_tokens,
        cache_read_tokens: msg.cache_read_tokens,
        cache_creation_tokens: msg.cache_creation_tokens,
        model: Some(msg.model.clone()),
        message_id: None,
    };

    let pricing = pricing_cache
        .entry(msg.model.clone())
        .or_insert_with(|| find_model_pricing_for_session(conn, &msg.model));
    let multiplier = Decimal::from(1);
    let (input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost) = match pricing
    {
        Some(p) => {
            let cost = CostCalculator::calculate(&usage, p, multiplier);
            (
                cost.input_cost.to_string(),
                cost.output_cost.to_string(),
                cost.cache_read_cost.to_string(),
                cost.cache_creation_cost.to_string(),
                cost.total_cost.to_string(),
            )
        }
        None => (
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
        ),
    };

    let inserted_rows = conn
        .execute(
            "INSERT OR IGNORE INTO proxy_request_logs (
            request_id, provider_id, app_type, model, request_model,
            input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
            input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
            latency_ms, first_token_ms, status_code, error_message, session_id,
            provider_type, is_streaming, cost_multiplier, created_at, data_source
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            rusqlite::params![
                request_id,
                "_session",         // provider_id: marks this as sourced from a session log
                "claude",           // app_type
                msg.model,
                msg.model,          // request_model = model
                msg.input_tokens,
                msg.output_tokens,
                msg.cache_read_tokens,
                msg.cache_creation_tokens,
                input_cost,
                output_cost,
                cache_read_cost,
                cache_creation_cost,
                total_cost,
                0i64,               // latency_ms: session logs have no such data
                Option::<i64>::None, // first_token_ms
                200i64,             // status_code: a session-log request is considered successful as soon as it produces billable tokens
                Option::<String>::None, // error_message
                msg.session_id,
                Some("session_log"), // provider_type
                1i64,               // is_streaming: Claude Code usually streams
                "1.0",              // cost_multiplier
                created_at,
                "session_log",      // data_source
            ],
        )
        .map_err(|e| AppError::Database(format!("Failed to insert session log: {e}")))?;

    Ok(inserted_rows > 0)
}

/// Look up model pricing in the model_pricing table (supports fuzzy matching)
fn find_model_pricing_for_session(
    conn: &rusqlite::Connection,
    model_id: &str,
) -> Option<ModelPricing> {
    find_model_pricing(conn, model_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_result_notification_is_coalesced_to_one_call() {
        crate::usage_events::take_test_notify_count();
        notify_sync_result(&SessionSyncResult::default());
        let result = SessionSyncResult {
            imported: 25,
            ..SessionSyncResult::default()
        };
        notify_sync_result(&result);
        assert_eq!(crate::usage_events::take_test_notify_count(), 1);
    }

    #[tokio::test]
    async fn session_sync_mutex_serializes_callers() {
        let first = session_sync_mutex().lock().await;
        assert!(session_sync_mutex().try_lock().is_err());
        drop(first);
        assert!(session_sync_mutex().try_lock().is_ok());
    }

    #[test]
    fn test_parse_usage_from_jsonl_line() {
        let line = r#"{"type":"assistant","message":{"id":"msg_test123","model":"claude-opus-4-6","usage":{"input_tokens":3,"output_tokens":150,"cache_read_input_tokens":5000,"cache_creation_input_tokens":10000},"stop_reason":"end_turn"},"timestamp":"2026-04-05T12:00:00Z","sessionId":"session-abc"}"#;

        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(
            value.get("type").and_then(|t| t.as_str()),
            Some("assistant")
        );

        let message = value.get("message").unwrap();
        let usage = message.get("usage").unwrap();

        assert_eq!(usage.get("input_tokens").unwrap().as_u64().unwrap(), 3);
        assert_eq!(usage.get("output_tokens").unwrap().as_u64().unwrap(), 150);
        assert_eq!(
            usage
                .get("cache_read_input_tokens")
                .unwrap()
                .as_u64()
                .unwrap(),
            5000
        );
        assert_eq!(
            usage
                .get("cache_creation_input_tokens")
                .unwrap()
                .as_u64()
                .unwrap(),
            10000
        );
        assert_eq!(
            message.get("stop_reason").unwrap().as_str().unwrap(),
            "end_turn"
        );
    }

    #[test]
    fn test_dedup_by_message_id() {
        // Multiple rows share the same message.id; the one with a stop_reason wins
        let mut messages: HashMap<String, ParsedAssistantUsage> = HashMap::new();

        // Intermediate entry (no stop_reason)
        let intermediate = ParsedAssistantUsage {
            message_id: "msg_1".to_string(),
            model: "claude-opus-4-6".to_string(),
            input_tokens: 3,
            output_tokens: 26,
            cache_read_tokens: 5000,
            cache_creation_tokens: 10000,
            stop_reason: None,
            timestamp: Some("2026-04-05T12:00:00Z".to_string()),
            session_id: None,
        };
        messages.insert("msg_1".to_string(), intermediate);

        // Final entry (has stop_reason)
        let final_entry = ParsedAssistantUsage {
            message_id: "msg_1".to_string(),
            model: "claude-opus-4-6".to_string(),
            input_tokens: 3,
            output_tokens: 1349,
            cache_read_tokens: 5000,
            cache_creation_tokens: 10000,
            stop_reason: Some("end_turn".to_string()),
            timestamp: Some("2026-04-05T12:00:00Z".to_string()),
            session_id: None,
        };

        // Should replace
        let should_replace = final_entry.stop_reason.is_some()
            && messages.get("msg_1").unwrap().stop_reason.is_none();
        assert!(should_replace);

        messages.insert("msg_1".to_string(), final_entry);
        assert_eq!(messages.get("msg_1").unwrap().output_tokens, 1349);
    }

    #[test]
    fn test_insert_claude_session_skips_matching_proxy_log() -> Result<(), AppError> {
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
                    "proxy-different-id",
                    "openai-compatible",
                    "claude",
                    "claude-sonnet-4-5",
                    "claude-sonnet-4-5",
                    100,
                    20,
                    10,
                    5,
                    "0.10",
                    100,
                    200,
                    1000,
                    "proxy"
                ],
            )?;
        }

        let msg = ParsedAssistantUsage {
            message_id: "msg_1".to_string(),
            model: "claude-sonnet-4-5".to_string(),
            input_tokens: 100,
            output_tokens: 20,
            cache_read_tokens: 10,
            cache_creation_tokens: 5,
            stop_reason: Some("end_turn".to_string()),
            timestamp: Some("1970-01-01T00:16:45Z".to_string()),
            session_id: Some("session-1".to_string()),
        };

        let inserted = insert_session_log_entry(&db, "session:msg_1", &msg)?;
        assert!(!inserted);

        let conn = lock_conn!(db.conn);
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM proxy_request_logs", [], |row| {
            row.get(0)
        })?;
        assert_eq!(count, 1);

        Ok(())
    }

    #[test]
    fn test_collect_jsonl_files_includes_subagents() {
        let tmp = std::env::temp_dir().join(format!("cc-switch-test-{}", uuid::Uuid::new_v4()));
        let project = tmp.join("project");
        let session_dir = project.join("test-session");
        let subagents_dir = session_dir.join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        fs::write(project.join("main.jsonl"), "{}").unwrap();
        fs::write(subagents_dir.join("agent-abc.jsonl"), "{}").unwrap();

        let files = collect_jsonl_files(&tmp);
        assert_eq!(files.len(), 2);
        let paths: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        assert!(paths.iter().any(|p| p.contains("main.jsonl")));
        assert!(paths.iter().any(|p| p.contains("agent-abc.jsonl")));

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_collect_jsonl_files_includes_workflow_subagents() {
        // Claude Code Workflow nests sub-agent transcripts under
        // project/SESSION_ID/subagents/workflows/wf_<ID>/, one level deeper than a plain sub-agent.
        let tmp = std::env::temp_dir().join(format!("cc-switch-test-{}", uuid::Uuid::new_v4()));
        let project = tmp.join("project");
        let session_dir = project.join("test-session");
        let subagents_dir = session_dir.join("subagents");
        let wf_dir = subagents_dir.join("workflows").join("wf_test123");
        fs::create_dir_all(&wf_dir).unwrap();

        fs::write(project.join("main.jsonl"), "{}").unwrap();
        fs::write(subagents_dir.join("agent-plain.jsonl"), "{}").unwrap();
        fs::write(wf_dir.join("agent-wf.jsonl"), "{}").unwrap();
        // journal.jsonl is collected too, but parsing yields 0 entries since it has no assistant lines
        fs::write(wf_dir.join("journal.jsonl"), "{}").unwrap();

        let files = collect_jsonl_files(&tmp);
        let paths: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        // main session + plain sub-agent + Workflow sub-agent (agent-wf + journal) = 4
        assert_eq!(files.len(), 4);
        assert!(paths.iter().any(|p| p.contains("main.jsonl")));
        assert!(paths.iter().any(|p| p.contains("agent-plain.jsonl")));
        assert!(
            paths.iter().any(|p| p.contains("agent-wf.jsonl")),
            "Workflow sub-agent transcript must be collected"
        );

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_sync_imports_billable_message_without_stop_reason() -> Result<(), AppError> {
        // Regression: a message missing stop_reason but with real cache/input cost (the
        // "message_start snapshot only, no final block" shape common in Workflow / sub-agents)
        // must be counted, not dropped; all-zero-token placeholder rows are still skipped.
        let db = Database::memory()?;
        let tmp = std::env::temp_dir().join(format!("cc-switch-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("agent-wf.jsonl");

        // First line: no stop_reason, output=1, but large cache_read/cache_creation -> should import
        // Second line: all tokens 0 -> should be skipped (nothing billable)
        let billable = r#"{"type":"assistant","message":{"id":"msg_nostop","model":"claude-opus-4-8","usage":{"input_tokens":2,"output_tokens":1,"cache_read_input_tokens":48719,"cache_creation_input_tokens":2061}},"timestamp":"2026-06-07T13:01:23Z","sessionId":"session-wf"}"#;
        let empty = r#"{"type":"assistant","message":{"id":"msg_empty","model":"claude-opus-4-8","usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}},"timestamp":"2026-06-07T13:01:24Z","sessionId":"session-wf"}"#;
        fs::write(&file, format!("{billable}\n{empty}\n")).unwrap();

        let (imported, _skipped) = sync_single_file(&db, &file)?;
        assert_eq!(
            imported, 1,
            "a message with cache cost but no stop_reason must be imported"
        );

        let conn = lock_conn!(db.conn);
        let cache_read: i64 = conn.query_row(
            "SELECT cache_read_tokens FROM proxy_request_logs WHERE request_id = 'session:msg_nostop'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(cache_read, 48719, "cache_read must be recorded in full");
        let empty_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM proxy_request_logs WHERE request_id = 'session:msg_empty')",
            [],
            |row| row.get(0),
        )?;
        assert!(
            !empty_exists,
            "a message with all-zero tokens must be skipped"
        );
        drop(conn);

        fs::remove_dir_all(&tmp).ok();
        Ok(())
    }
}
