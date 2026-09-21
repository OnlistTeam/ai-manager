//! OpenCode session log usage tracking
//!
//! Extracts exact token usage from ~/.local/share/opencode/opencode.db (SQLite).
//!
//! ## Data flow
//! ```text
//! ~/.local/share/opencode/opencode.db
//!   -> read all sessions from the session table
//!   -> read assistant messages from the message table
//!   -> parse the data JSON to extract tokens/cost/model
//!   -> proxy_request_logs table
//! ```

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::opencode_config::get_opencode_db_path;
use crate::proxy::usage::calculator::CostCalculator;
use crate::proxy::usage::parser::TokenUsage;
use crate::services::session_usage::{
    get_sync_state, metadata_modified_nanos, update_sync_state, SessionSyncResult,
};
use crate::services::usage_stats::{find_model_pricing, should_skip_session_insert, DedupKey};
use rust_decimal::Decimal;
use std::fs;
use std::time::SystemTime;

/// Token and cost data extracted from the opencode message.data JSON
struct OpenCodeMessageData {
    input_tokens: u32,
    output_tokens: u32,
    reasoning_tokens: u32,
    cache_read_tokens: u32,
    cache_write_tokens: u32,
    cost: f64,
    model_id: String,
    timestamp_ms: i64,
}

struct OpenCodeMessageQueryResult {
    messages: Vec<(String, OpenCodeMessageData)>,
    has_incomplete_usage: bool,
}

/// Sync OpenCode usage data
pub fn sync_opencode_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    let db_path = get_opencode_db_path();

    if !db_path.exists() {
        return Ok(SessionSyncResult {
            imported: 0,
            skipped: 0,
            files_scanned: 0,
            suspected_duplicates: 0,
            deferred_files: 0,
            errors: vec![],
        });
    }

    let db_path_str = db_path.to_string_lossy().to_string();

    // Check the file modification time.
    // The opencode database runs in WAL mode: new commits land in the -wal file first and the
    // main database file is only updated at checkpoint time. The -wal mtime therefore has to be
    // taken into account, otherwise freshly written sessions are missed before a checkpoint.
    let metadata = fs::metadata(&db_path)
        .map_err(|e| AppError::Config(format!("Unable to read opencode.db metadata: {e}")))?;
    let mut file_modified = metadata_modified_nanos(&metadata);

    let wal_path = db_path.with_extension("db-wal");
    if let Ok(wal_meta) = fs::metadata(&wal_path) {
        file_modified = file_modified.max(metadata_modified_nanos(&wal_meta));
    }

    let (last_modified, _last_offset) = get_sync_state(db, &db_path_str)?;

    // Skip if the file has not changed
    if file_modified <= last_modified {
        return Ok(SessionSyncResult {
            imported: 0,
            skipped: 0,
            files_scanned: 1,
            suspected_duplicates: 0,
            deferred_files: 0,
            errors: vec![],
        });
    }

    // Open the opencode SQLite database (read-only)
    let opencode_conn =
        rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| AppError::Database(format!("Unable to open opencode.db: {e}")))?;

    let mut result = SessionSyncResult {
        imported: 0,
        skipped: 0,
        files_scanned: 1,
        suspected_duplicates: 0,
        deferred_files: 0,
        errors: vec![],
    };
    let mut has_sync_errors = false;

    // Query all sessions
    let sessions = query_sessions(&opencode_conn)?;

    for (session_id, time_updated) in &sessions {
        // Check whether the session needs to be re-synced
        let sync_key = format!("{db_path_str}:{session_id}");
        let (sess_last_modified, _) = get_sync_state(db, &sync_key)?;
        if *time_updated <= sess_last_modified {
            continue; // session unchanged, skip
        }

        let mut session_had_error = false;

        // Query all assistant messages of this session
        let mut session_has_incomplete_usage = false;
        match query_assistant_messages(&opencode_conn, session_id) {
            Ok(query_result) => {
                session_has_incomplete_usage = query_result.has_incomplete_usage;
                for (message_id, msg_data) in &query_result.messages {
                    let request_id = format!("opencode_session:{session_id}:{message_id}");

                    match insert_opencode_message(db, &request_id, msg_data, session_id) {
                        Ok(true) => result.imported += 1,
                        Ok(false) => result.skipped += 1,
                        Err(e) => {
                            let msg = format!("OpenCode message insert failed {request_id}: {e}");
                            log::warn!("[OPENCODE-SYNC] {msg}");
                            result.errors.push(msg);
                            result.skipped += 1;
                            session_had_error = true;
                        }
                    }
                }
            }
            Err(e) => {
                let msg = format!("OpenCode session message query failed {session_id}: {e}");
                log::warn!("[OPENCODE-SYNC] {msg}");
                result.errors.push(msg);
                session_had_error = true;
            }
        }

        if session_had_error {
            has_sync_errors = true;
            continue;
        }

        if session_has_incomplete_usage {
            continue;
        }

        // Update the session-level sync state. On failure do not advance the file-level state, so the next run can retry.
        if let Err(e) = update_sync_state(db, &sync_key, *time_updated, 0) {
            let msg = format!("OpenCode session sync state update failed {session_id}: {e}");
            log::warn!("[OPENCODE-SYNC] {msg}");
            result.errors.push(msg);
            has_sync_errors = true;
        }
    }

    // Only advance the file-level state when this round fully succeeded; otherwise keep the retry entry point.
    if !has_sync_errors {
        update_sync_state(db, &db_path_str, file_modified, 0)?;
    }

    if result.imported > 0 {
        log::info!(
            "[OPENCODE-SYNC] sync finished: imported {}, skipped {}, scanned {} sessions",
            result.imported,
            result.skipped,
            sessions.len()
        );
    }

    Ok(result)
}

/// Query (id, sync_watermark) for all sessions
fn query_sessions(conn: &rusqlite::Connection) -> Result<Vec<(String, i64)>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id,
                    MAX(s.time_updated, COALESCE(MAX(m.time_updated), s.time_updated)) AS sync_watermark
             FROM session s
             LEFT JOIN message m ON m.session_id = s.id
             GROUP BY s.id
             ORDER BY sync_watermark",
        )
        .map_err(|e| AppError::Database(format!("Failed to prepare session query: {e}")))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| AppError::Database(format!("Failed to query sessions: {e}")))?;

    let mut sessions = Vec::new();
    for row in rows {
        sessions
            .push(row.map_err(|e| AppError::Database(format!("Failed to read session row: {e}")))?);
    }

    Ok(sessions)
}

/// Query the completed assistant messages of a session and flag whether incomplete usage messages remain.
fn query_assistant_messages(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Result<OpenCodeMessageQueryResult, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, data FROM message WHERE session_id = ?1 ORDER BY time_created")
        .map_err(|e| AppError::Database(format!("Failed to prepare message query: {e}")))?;

    let rows = stmt
        .query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(format!("Failed to query messages: {e}")))?;

    let mut messages = Vec::new();
    let mut has_incomplete_usage = false;
    for row in rows {
        let (message_id, data_json) =
            row.map_err(|e| AppError::Database(format!("Failed to read message row: {e}")))?;

        // Only handle assistant messages
        let value: serde_json::Value = match serde_json::from_str(&data_json) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if value.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }

        // The tokens field is required
        if value.get("tokens").is_none() {
            continue;
        }

        // Skip incomplete messages: in-flight ones only carry partial tokens and INSERT OR IGNORE cannot backfill them
        if value.get("time").and_then(|t| t.get("completed")).is_none() {
            has_incomplete_usage = true;
            continue;
        }

        if let Some(msg_data) = parse_message_data(&value) {
            messages.push((message_id, msg_data));
        }
    }

    Ok(OpenCodeMessageQueryResult {
        messages,
        has_incomplete_usage,
    })
}

/// Parse the opencode message.data JSON into structured data
fn parse_message_data(value: &serde_json::Value) -> Option<OpenCodeMessageData> {
    let tokens = value.get("tokens")?;

    let input_tokens = tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let output_tokens = tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let reasoning_tokens = tokens
        .get("reasoning")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let cache_obj = tokens.get("cache");
    let cache_read_tokens = cache_obj
        .and_then(|c| c.get("read"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let cache_write_tokens = cache_obj
        .and_then(|c| c.get("write"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    // Skip messages whose tokens are all zero
    if input_tokens == 0
        && output_tokens == 0
        && reasoning_tokens == 0
        && cache_read_tokens == 0
        && cache_write_tokens == 0
    {
        return None;
    }

    let cost = value.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let model_id = value
        .get("modelID")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp_ms = value
        .get("time")
        .and_then(|t| t.get("created"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    Some(OpenCodeMessageData {
        input_tokens,
        output_tokens,
        reasoning_tokens,
        cache_read_tokens,
        cache_write_tokens,
        cost,
        model_id,
        timestamp_ms,
    })
}

/// Insert a single OpenCode message record into proxy_request_logs
fn insert_opencode_message(
    db: &Database,
    request_id: &str,
    msg: &OpenCodeMessageData,
    session_id: &str,
) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);

    let created_at = if msg.timestamp_ms > 0 {
        msg.timestamp_ms / 1000
    } else {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    };

    // OpenCode follows the Anthropic style: input is fresh input, cache is counted separately,
    // output includes reasoning tokens (billed as output)
    let output_with_reasoning = msg.output_tokens + msg.reasoning_tokens;

    let dedup_key = DedupKey {
        app_type: "opencode",
        model: &msg.model_id,
        input_tokens: msg.input_tokens,
        output_tokens: output_with_reasoning,
        cache_read_tokens: msg.cache_read_tokens,
        cache_creation_tokens: msg.cache_write_tokens,
        created_at,
    };
    if should_skip_session_insert(&conn, request_id, &dedup_key)? {
        return Ok(false);
    }

    // Use the cost opencode already provides if any; otherwise compute it from model pricing
    let (input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost) =
        if msg.cost > 0.0 {
            // opencode already computed the cost, use it directly
            // Simplification: everything goes into total_cost (opencode's cost is an aggregate and cannot be split exactly)
            (
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                msg.cost.to_string(),
            )
        } else {
            // opencode cost is 0 (e.g. a free model); try computing it from cc-switch's own model pricing
            let usage = TokenUsage {
                input_tokens: msg.input_tokens,
                output_tokens: output_with_reasoning,
                cache_read_tokens: msg.cache_read_tokens,
                cache_creation_tokens: msg.cache_write_tokens,
                model: Some(msg.model_id.clone()),
                message_id: None,
            };

            match find_model_pricing(&conn, &msg.model_id) {
                Some(pricing) => {
                    let cost = CostCalculator::calculate_for_app(
                        "opencode",
                        &usage,
                        &pricing,
                        Decimal::from(1),
                    );
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
            }
        };

    let inserted_rows = conn.execute(
        "INSERT OR IGNORE INTO proxy_request_logs (
            request_id, provider_id, app_type, model, request_model,
            input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
            input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
            latency_ms, first_token_ms, status_code, error_message, session_id,
            provider_type, is_streaming, cost_multiplier, created_at, data_source
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        rusqlite::params![
            request_id,
            "_opencode_session",   // provider_id
            "opencode",            // app_type
            msg.model_id,
            msg.model_id,          // request_model = model
            msg.input_tokens,
            output_with_reasoning,
            msg.cache_read_tokens,
            msg.cache_write_tokens,
            input_cost,
            output_cost,
            cache_read_cost,
            cache_creation_cost,
            total_cost,
            0i64,                  // latency_ms
            Option::<i64>::None,   // first_token_ms
            200i64,                // status_code
            Option::<String>::None,// error_message
            Some(session_id.to_string()),
            Some("opencode_session"), // provider_type
            1i64,                  // is_streaming
            "1.0",                 // cost_multiplier
            created_at,
            "opencode_session",    // data_source
        ],
    )
    .map_err(|e| AppError::Database(format!("Failed to insert OpenCode session log: {e}")))?;

    Ok(inserted_rows > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_message_data_full() {
        let json: serde_json::Value = serde_json::json!({
            "role": "assistant",
            "cost": 0.0023113,
            "tokens": {
                "total": 56554,
                "input": 3272,
                "output": 383,
                "reasoning": 419,
                "cache": {
                    "write": 0,
                    "read": 52480
                }
            },
            "modelID": "deepseek-v4-pro",
            "providerID": "deepseek",
            "time": {
                "created": 1779755333700i64,
                "completed": 1779755350639i64
            }
        });
        let data = parse_message_data(&json).unwrap();
        assert_eq!(data.input_tokens, 3272);
        assert_eq!(data.output_tokens, 383);
        assert_eq!(data.reasoning_tokens, 419);
        assert_eq!(data.cache_read_tokens, 52480);
        assert_eq!(data.cache_write_tokens, 0);
        assert!((data.cost - 0.0023113).abs() < 1e-10);
        assert_eq!(data.model_id, "deepseek-v4-pro");
        assert_eq!(data.timestamp_ms, 1779755333700);
    }

    #[test]
    fn test_parse_message_data_missing_cache() {
        let json: serde_json::Value = serde_json::json!({
            "role": "assistant",
            "cost": 0.0,
            "tokens": {
                "input": 1000,
                "output": 200
            },
            "modelID": "mimo-v2.5-pro",
            "time": { "created": 1779755333700i64 }
        });
        let data = parse_message_data(&json).unwrap();
        assert_eq!(data.input_tokens, 1000);
        assert_eq!(data.output_tokens, 200);
        assert_eq!(data.reasoning_tokens, 0);
        assert_eq!(data.cache_read_tokens, 0);
        assert_eq!(data.cache_write_tokens, 0);
    }

    #[test]
    fn test_parse_message_data_skips_zero_tokens() {
        let json: serde_json::Value = serde_json::json!({
            "role": "assistant",
            "tokens": {
                "input": 0,
                "output": 0,
                "reasoning": 0,
                "cache": { "read": 0, "write": 0 }
            },
            "modelID": "test"
        });
        assert!(parse_message_data(&json).is_none());
    }

    #[test]
    fn test_parse_message_data_ignores_role() {
        // parse_message_data does not filter by role; that's the caller's job
        let json: serde_json::Value = serde_json::json!({
            "role": "user",
            "tokens": { "input": 100, "output": 0 }
        });
        let data = parse_message_data(&json).unwrap();
        assert_eq!(data.input_tokens, 100);
    }

    #[test]
    fn test_query_assistant_messages_skips_incomplete() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
        )
        .unwrap();

        let done = serde_json::json!({
            "role": "assistant",
            "tokens": { "input": 1000, "output": 200 },
            "modelID": "m",
            "time": { "created": 1, "completed": 2 }
        })
        .to_string();
        let in_progress = serde_json::json!({
            "role": "assistant",
            "tokens": { "input": 500, "output": 0 },
            "modelID": "m",
            "time": { "created": 3 }
        })
        .to_string();

        conn.execute(
            "INSERT INTO message VALUES ('done', 's1', 1, ?1), ('wip', 's1', 2, ?2)",
            rusqlite::params![done, in_progress],
        )
        .unwrap();

        let result = query_assistant_messages(&conn, "s1").unwrap();
        // Only completed messages (with time.completed) are returned; partial ones are skipped
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].0, "done");
        assert!(result.has_incomplete_usage);
    }

    #[test]
    fn test_query_sessions_uses_message_update_watermark() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT, time_updated INTEGER);
             CREATE TABLE message (
                 id TEXT,
                 session_id TEXT,
                 time_created INTEGER,
                 time_updated INTEGER,
                 data TEXT
             );
             INSERT INTO session VALUES ('s1', 100);
             INSERT INTO message VALUES ('m1', 's1', 90, 200, '{}');",
        )
        .unwrap();

        let sessions = query_sessions(&conn).unwrap();
        assert_eq!(sessions, vec![("s1".to_string(), 200)]);
    }
}
