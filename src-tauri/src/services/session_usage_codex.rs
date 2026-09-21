//! Codex session log usage tracking
//!
//! Extracts exact token usage from the JSONL session files under ~/.codex/sessions/,
//! replacing the previous state_5.sqlite estimation approach.
//!
//! ## Data flow
//! ```text
//! ~/.codex/sessions/YYYY/MM/DD/*.jsonl -> incremental parse -> delta -> cost -> proxy_request_logs table
//! ```
//!
//! ## Parsed event types
//! - `session_meta` -> extract the unique thread_id (a sub-agent's session_id points at the parent thread)
//! - `turn_context` -> extract the current model
//! - `event_msg` (type=token_count) -> extract cumulative token usage and compute the delta

use crate::codex_config::get_codex_config_dir;
use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::proxy::usage::calculator::{CostCalculator, ModelPricing};
use crate::proxy::usage::parser::TokenUsage;
use crate::services::session_usage::{
    metadata_modified_nanos, update_sync_state, update_sync_state_on_conn, SessionSyncResult,
};
use crate::services::usage_stats::{
    find_model_pricing, has_suspected_codex_session_duplicate, should_skip_session_insert, DedupKey,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    FileIdInfo, GetFileInformationByHandleEx, FILE_ID_INFO,
};

const CODEX_THREAD_REQUEST_ID_PREFIX: &str = "codex_session:thread-v1";

/// Cumulative token usage (tracks the total_token_usage field)
#[derive(Debug, Clone, Default)]
struct CumulativeTokens {
    input: u64,
    cached_input: u64,
    output: u64,
}

/// Token delta of a single API call
#[derive(Debug)]
struct DeltaTokens {
    input: u32,
    cached_input: u32,
    output: u32,
}

impl DeltaTokens {
    fn is_zero(&self) -> bool {
        self.input == 0 && self.cached_input == 0 && self.output == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TokenCountersSignature {
    input: Option<u64>,
    cached_input: Option<u64>,
    output: Option<u64>,
    reasoning_output: Option<u64>,
    total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TokenUsageSignature {
    total: Option<TokenCountersSignature>,
    last: Option<TokenCountersSignature>,
}

#[derive(Debug)]
struct TimestampedTokenSignature {
    timestamp: DateTime<Utc>,
    signature: TokenUsageSignature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParentFileStamp {
    modified_nanos: i64,
    size: u64,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume_serial: u64,
    #[cfg(windows)]
    file_id: [u8; 16],
}

impl ParentFileStamp {
    fn from_file(file: &fs::File) -> Option<Self> {
        let metadata = file.metadata().ok()?;
        #[cfg(windows)]
        let (volume_serial, file_id) = windows_file_identity(file)?;
        Some(Self {
            modified_nanos: metadata_modified_nanos(&metadata),
            size: metadata.len(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(windows)]
            volume_serial,
            #[cfg(windows)]
            file_id,
        })
    }
}

#[cfg(windows)]
fn windows_file_identity(file: &fs::File) -> Option<(u64, [u8; 16])> {
    let mut information = FILE_ID_INFO::default();
    // SAFETY: `file` owns a live handle for this call, and `information` is a
    // valid writable FILE_ID_INFO buffer of the size passed to Windows.
    let succeeded = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            std::ptr::addr_of_mut!(information).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    } != 0;
    succeeded.then_some((
        information.VolumeSerialNumber,
        information.FileId.Identifier,
    ))
}

#[derive(Debug)]
struct ParentTokenTimeline {
    events: Vec<TimestampedTokenSignature>,
    max_timestamp: Option<DateTime<Utc>>,
    has_token_without_timestamp: bool,
}

impl ParentTokenTimeline {
    fn signatures_before(
        &self,
        parent_path: &Path,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<TokenUsageSignature>, String> {
        if self.has_token_without_timestamp {
            return Err(format!(
                "token_count of parent rollout {} has no valid timestamp",
                parent_path.display()
            ));
        }
        if self
            .max_timestamp
            .is_none_or(|timestamp| timestamp < cutoff)
        {
            return Err(format!(
                "parent rollout {} has not been written up to the child fork moment yet",
                parent_path.display()
            ));
        }
        Ok(self
            .events
            .iter()
            .filter(|event| event.timestamp <= cutoff)
            .map(|event| event.signature.clone())
            .collect())
    }
}

#[derive(Debug)]
struct CachedParentTimeline {
    stamp: ParentFileStamp,
    timeline: Arc<ParentTokenTimeline>,
}

#[derive(Debug)]
struct CachedReplayPrefix {
    modified: i64,
    size: u64,
    prefix: usize,
}

#[derive(Debug)]
struct ParsedTokenEvent {
    line_offset: i64,
    signature: TokenUsageSignature,
    delta: DeltaTokens,
    event_index: Option<u32>,
    model: String,
    timestamp: Option<String>,
}

#[derive(Debug)]
enum ParentResolution {
    None,
    Parent(String),
    Deferred(String),
}

#[derive(Debug)]
struct ParsedCodexFile {
    root_thread_id: Option<String>,
    /// Thread ID of the root `session_meta` (the stable logical thread): for a single-segment file name
    /// it equals the file name UUID, for revert/resume two-segment names it is the leading UUID.
    meta_thread_id: Option<String>,
    root_meta_seen: bool,
    root_timestamp: Option<DateTime<Utc>>,
    parent: ParentResolution,
    token_events: Vec<ParsedTokenEvent>,
    line_offset: i64,
    has_billable_tokens: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingReason {
    MissingParent(String),
    Stable(String),
    Retryable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingEntry {
    modified: i64,
    size: u64,
    reason: PendingReason,
}

#[derive(Debug, Default)]
struct CodexReplayCaches {
    parent_timelines: HashMap<PathBuf, CachedParentTimeline>,
    replay_prefixes: HashMap<PathBuf, CachedReplayPrefix>,
    pending: HashMap<PathBuf, PendingEntry>,
}

static CODEX_REPLAY_CACHES: OnceLock<Mutex<CodexReplayCaches>> = OnceLock::new();

fn replay_caches() -> &'static Mutex<CodexReplayCaches> {
    CODEX_REPLAY_CACHES.get_or_init(|| Mutex::new(CodexReplayCaches::default()))
}

fn is_rollout_filename(file_name: &str) -> bool {
    if !file_name.starts_with("rollout-") || !file_name.ends_with(".jsonl") {
        return false;
    }
    let stem = file_name.trim_end_matches(".jsonl");
    stem.get(stem.len().saturating_sub(36)..)
        .is_some_and(|candidate| uuid::Uuid::parse_str(candidate).is_ok())
}

fn is_codex_cursor_path(file_path: &str, codex_dir: &Path) -> bool {
    let path = Path::new(file_path);
    let file_name = file_path.rsplit(['/', '\\']).next().unwrap_or_default();
    if !is_rollout_filename(file_name) {
        return false;
    }

    if path.starts_with(codex_dir.join("sessions"))
        || path.starts_with(codex_dir.join("archived_sessions"))
    {
        return true;
    }

    // Tolerates cursors left over after the user changed CODEX_HOME whose source file is gone. Only
    // explicit directory segments + a Codex rollout UUID file name are accepted, so a broad codex_dir does not delete other importers.
    file_path
        .replace('\\', "/")
        .split('/')
        .any(|segment| matches!(segment, "sessions" | "archived_sessions"))
}

fn sqlite_table_exists(conn: &rusqlite::Connection, table: &str) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get(0),
    )
    .map_err(|error| AppError::Database(format!("Failed to query table {table}: {error}")))
}

fn sqlite_column_exists(
    conn: &rusqlite::Connection,
    table: &str,
    column: &str,
) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
        rusqlite::params![table, column],
        |row| row.get(0),
    )
    .map_err(|error| {
        AppError::Database(format!("Failed to query column {table}.{column}: {error}"))
    })
}

pub(crate) fn reset_codex_usage_on_conn(
    conn: &rusqlite::Connection,
    codex_dir: &Path,
) -> Result<(), AppError> {
    if sqlite_table_exists(conn, "proxy_request_logs")?
        && sqlite_column_exists(conn, "proxy_request_logs", "data_source")?
    {
        conn.execute(
            "DELETE FROM proxy_request_logs WHERE data_source = 'codex_session'",
            [],
        )
        .map_err(|error| {
            AppError::Database(format!("Failed to clear Codex session details: {error}"))
        })?;
    }
    if sqlite_table_exists(conn, "usage_daily_rollups")?
        && sqlite_column_exists(conn, "usage_daily_rollups", "provider_id")?
    {
        conn.execute(
            "DELETE FROM usage_daily_rollups WHERE provider_id = '_codex_session'",
            [],
        )
        .map_err(|error| {
            AppError::Database(format!("Failed to clear the Codex usage rollups: {error}"))
        })?;
    }
    if sqlite_table_exists(conn, "session_log_sync")?
        && sqlite_column_exists(conn, "session_log_sync", "file_path")?
    {
        let paths = {
            let mut statement = conn
                .prepare("SELECT file_path FROM session_log_sync")
                .map_err(|error| {
                    AppError::Database(format!("Failed to read the session sync cursor: {error}"))
                })?;
            let paths = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|error| {
                    AppError::Database(format!("Failed to query the session sync cursor: {error}"))
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    AppError::Database(format!("Failed to parse the session sync cursor: {error}"))
                })?;
            paths
        };
        for file_path in paths
            .into_iter()
            .filter(|path| is_codex_cursor_path(path, codex_dir))
        {
            conn.execute(
                "DELETE FROM session_log_sync WHERE file_path = ?1",
                [file_path],
            )
            .map_err(|error| {
                AppError::Database(format!("Failed to clear the Codex sync cursor: {error}"))
            })?;
        }
    }
    Ok(())
}

impl Database {}

fn non_empty_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn thread_id_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let candidate = stem.get(stem.len().checked_sub(36)?..)?;
    uuid::Uuid::parse_str(candidate)
        .ok()
        .map(|value| value.hyphenated().to_string())
}

/// The thread UUID before the underscore in a two-segment file name
/// (`rollout-...-<threadId>_<rolloutId>.jsonl`); returns `None` for single-segment names.
///
/// `thread/revert` produces such names when it creates a replacement rollout for the same thread (see
/// openai/codex#38127): the last segment is the newly generated rollout ID, and later resumes keep
/// appending to that file. The root meta `id` is always the original thread ID, so the consistency
/// check has to accept both UUIDs.
fn leading_thread_id_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let len = stem.len();
    // Tail layout: ...<uuidA>_<uuidB>. uuidB takes 36 chars, preceded by '_' (37 in total)
    if !stem.get(len.checked_sub(37)?..)?.starts_with('_') {
        return None;
    }
    let candidate = stem.get(len.checked_sub(73)?..len.checked_sub(37)?)?;
    uuid::Uuid::parse_str(candidate)
        .ok()
        .map(|value| value.hyphenated().to_string())
}

fn explicit_parent_from_meta(payload: &serde_json::Value) -> ParentResolution {
    let forked_from = non_empty_string(payload.get("forked_from_id"));
    let spawned_from = payload
        .get("source")
        .and_then(|source| source.get("subagent"))
        .and_then(|subagent| subagent.get("thread_spawn"))
        .and_then(|spawn| non_empty_string(spawn.get("parent_thread_id")));

    match (forked_from, spawned_from) {
        (None, None) => ParentResolution::None,
        (Some(parent), None) | (None, Some(parent)) => ParentResolution::Parent(parent),
        (Some(forked), Some(spawned)) if forked == spawned => ParentResolution::Parent(forked),
        (Some(forked), Some(spawned)) => ParentResolution::Deferred(format!(
            "forked_from_id ({forked}) does not match thread_spawn.parent_thread_id ({spawned})"
        )),
    }
}

fn parse_timestamp(value: Option<&serde_json::Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(serde_json::Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn parse_signature_counters(value: Option<&serde_json::Value>) -> Option<TokenCountersSignature> {
    let value = value?.as_object()?;
    Some(TokenCountersSignature {
        input: value
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64),
        cached_input: value
            .get("cached_input_tokens")
            .or_else(|| value.get("cache_read_input_tokens"))
            .and_then(serde_json::Value::as_u64),
        output: value
            .get("output_tokens")
            .and_then(serde_json::Value::as_u64),
        reasoning_output: value
            .get("reasoning_output_tokens")
            .and_then(serde_json::Value::as_u64),
        total: value
            .get("total_tokens")
            .and_then(serde_json::Value::as_u64),
    })
}

fn parse_token_signature(info: &serde_json::Value) -> Option<TokenUsageSignature> {
    let total = parse_signature_counters(info.get("total_token_usage"));
    let last = parse_signature_counters(info.get("last_token_usage"));
    (total.is_some() || last.is_some()).then_some(TokenUsageSignature { total, last })
}

fn token_snapshot_source(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("rate_limits")
        .and_then(|rate_limits| rate_limits.get("limit_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Shared state of a single sync pass.
///
/// - `cursors`: a `session_log_sync` snapshot preloaded once at the start of the pass, replacing
///   per-file SELECTs (the `substr` suffix match of archived inheritance in particular cannot use an
///   index, so per-file lookups mean N full table scans per pass). Snapshot semantics: cursors written
///   by other files during the same pass are invisible to later archived inheritance - the only effect
///   is one extra rescan deduplicated by request_id; no data loss and no double counting.
/// - `pricing`: a pass-level model pricing cache. If the pricing table changes mid-pass, this pass
///   keeps the old prices and the next sync pass picks up the new ones.
struct CodexSyncPass {
    cursors: HashMap<String, (i64, i64)>,
    pricing: HashMap<String, Option<ModelPricing>>,
}

impl CodexSyncPass {
    fn load(db: &Database) -> Result<Self, AppError> {
        let conn = lock_conn!(db.conn);
        let mut stmt = conn
            .prepare("SELECT file_path, last_modified, last_line_offset FROM session_log_sync")
            .map_err(|e| AppError::Database(format!("Failed to preload the sync cursors: {e}")))?;
        let cursors = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (row.get::<_, i64>(1)?, row.get::<_, i64>(2)?),
                ))
            })
            .and_then(|rows| rows.collect::<Result<HashMap<_, _>, _>>())
            .map_err(|e| AppError::Database(format!("Failed to preload the sync cursors: {e}")))?;
        Ok(Self {
            cursors,
            pricing: HashMap::new(),
        })
    }
}

fn get_codex_sync_state(
    db: &Database,
    file_path: &Path,
    cursors: &HashMap<String, (i64, i64)>,
) -> Result<(i64, i64), AppError> {
    let file_path_str = file_path.to_string_lossy().to_string();
    let state = cursors.get(&file_path_str).copied().unwrap_or((0, 0));
    if state != (0, 0)
        || file_path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            != Some("archived_sessions")
    {
        return Ok(state);
    }

    let Some(file_name) = file_path.file_name().and_then(|name| name.to_str()) else {
        return Ok(state);
    };
    let slash_suffix = format!("/{file_name}");
    let backslash_suffix = format!("\\{file_name}");
    // Equivalent to the original SQL: ORDER BY last_line_offset DESC, last_modified DESC LIMIT 1
    // -> take the maximum (offset, modified) from the snapshot.
    let inherited = cursors
        .iter()
        .filter(|(path, _)| {
            path.as_str() != file_path_str
                && (path.ends_with(&slash_suffix) || path.ends_with(&backslash_suffix))
        })
        .map(|(_, &(modified, offset))| (offset, modified))
        .max();

    match inherited {
        Some((offset, modified)) => {
            update_sync_state(db, &file_path_str, modified, offset)?;
            Ok((modified, offset))
        }
        None => Ok(state),
    }
}

/// Normalize a Codex model name
///
/// Rules (in order):
/// 1. Lowercase: `GLM-4.6` -> `glm-4.6`
/// 2. Strip the provider prefix: `openai/gpt-5.4` -> `gpt-5.4`
/// 3. Strip an ISO date suffix: `gpt-5.4-2026-03-05` -> `gpt-5.4`
/// 4. Strip a compact date suffix: `gpt-5.4-20260305` -> `gpt-5.4`
fn normalize_codex_model(raw: &str) -> String {
    // Step 1: lowercase
    let mut name = raw.to_lowercase();

    // Step 2: strip the "provider/" prefix (e.g. openai/, azure/)
    if let Some(pos) = name.rfind('/') {
        name = name[pos + 1..].to_string();
    }

    // Step 3: strip an ISO date suffix -YYYY-MM-DD (exactly 11 chars)
    if name.len() > 11 && name.is_char_boundary(name.len() - 11) {
        let suffix = &name[name.len() - 11..];
        if suffix.is_ascii()
            && suffix.as_bytes()[0] == b'-'
            && suffix[1..5].chars().all(|c| c.is_ascii_digit())
            && suffix.as_bytes()[5] == b'-'
            && suffix[6..8].chars().all(|c| c.is_ascii_digit())
            && suffix.as_bytes()[8] == b'-'
            && suffix[9..11].chars().all(|c| c.is_ascii_digit())
        {
            name.truncate(name.len() - 11);
        }
    }

    // Step 4: strip a compact date suffix -YYYYMMDD (exactly 9 chars)
    if name.len() > 9 {
        let parts: Vec<&str> = name.rsplitn(2, '-').collect();
        if parts.len() == 2 {
            if let Some(suffix) = parts.first() {
                if suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_digit()) {
                    name = parts[1].to_string();
                }
            }
        }
    }

    name
}

/// Compute the delta between two cumulative readings
fn compute_delta(prev: &Option<CumulativeTokens>, current: &CumulativeTokens) -> DeltaTokens {
    match prev {
        None => DeltaTokens {
            input: current.input as u32,
            cached_input: current.cached_input as u32,
            output: current.output as u32,
        },
        Some(p) => DeltaTokens {
            input: current.input.saturating_sub(p.input) as u32,
            cached_input: current.cached_input.saturating_sub(p.cached_input) as u32,
            output: current.output.saturating_sub(p.output) as u32,
        },
    }
}

fn update_high_water(high_water: &mut CumulativeTokens, current: &CumulativeTokens) {
    high_water.input = high_water.input.max(current.input);
    high_water.cached_input = high_water.cached_input.max(current.cached_input);
    high_water.output = high_water.output.max(current.output);
}

/// Extract cumulative token usage from a JSON Value
fn parse_cumulative_tokens(total_usage: &serde_json::Value) -> Option<CumulativeTokens> {
    let fields = total_usage.as_object()?;
    if ![
        "input_tokens",
        "cached_input_tokens",
        "cache_read_input_tokens",
        "output_tokens",
        "reasoning_output_tokens",
        "total_tokens",
    ]
    .iter()
    .any(|field| fields.contains_key(*field))
    {
        return None;
    }
    Some(CumulativeTokens {
        input: total_usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        cached_input: total_usage
            .get("cached_input_tokens")
            .or_else(|| total_usage.get("cache_read_input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        output: total_usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    })
}

type RolloutIndex = HashMap<String, Vec<PathBuf>>;

#[derive(Debug, Default)]
struct CodexFileSyncResult {
    imported: u32,
    skipped: u32,
    suspected_duplicates: u32,
    deferred: bool,
}

/// Sync Codex usage data (from JSONL session logs)
pub fn sync_codex_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    let codex_dir = get_codex_config_dir();
    let files = collect_codex_session_files(&codex_dir);
    let rollout_index = build_rollout_index(&files);
    let mut pass = CodexSyncPass::load(db)?;

    let mut result = SessionSyncResult {
        imported: 0,
        skipped: 0,
        files_scanned: files.len() as u32,
        suspected_duplicates: 0,
        deferred_files: 0,
        errors: vec![],
    };

    for file_path in &files {
        match sync_single_codex_file(db, file_path, &rollout_index, &mut pass) {
            Ok(file_result) => {
                result.imported = result.imported.saturating_add(file_result.imported);
                result.skipped = result.skipped.saturating_add(file_result.skipped);
                result.suspected_duplicates = result
                    .suspected_duplicates
                    .saturating_add(file_result.suspected_duplicates);
                if file_result.deferred {
                    result.deferred_files = result.deferred_files.saturating_add(1);
                }
            }
            Err(e) => {
                let msg = format!(
                    "Failed to parse Codex session file {}: {e}",
                    file_path.display()
                );
                log::warn!("[CODEX-SYNC] {msg}");
                result.errors.push(msg);
            }
        }
    }

    if result.imported > 0 || result.deferred_files > 0 {
        log::info!(
            "[CODEX-SYNC] sync finished: imported {}, skipped {}, deferred {}, scanned {} files",
            result.imported,
            result.skipped,
            result.deferred_files,
            result.files_scanned
        );
    }

    Ok(result)
}

/// Collect all Codex session JSONL files
fn collect_codex_session_files(codex_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    // 1. Scan sessions/YYYY/MM/DD/*.jsonl (date-partitioned directories)
    let sessions_dir = codex_dir.join("sessions");
    if sessions_dir.is_dir() {
        collect_jsonl_recursive(&sessions_dir, &mut files, 0, 3);
    }

    // 2. Scan archived_sessions/*.jsonl (flat archive directory)
    let archived_dir = codex_dir.join("archived_sessions");
    if archived_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&archived_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    files.push(path);
                }
            }
        }
    }

    files.sort();
    files
}

fn build_rollout_index(files: &[PathBuf]) -> RolloutIndex {
    let mut index = RolloutIndex::new();
    for path in files {
        if let Some(thread_id) = thread_id_from_filename(path) {
            index.entry(thread_id).or_default().push(path.clone());
        }
    }
    for paths in index.values_mut() {
        paths.sort();
    }
    index
}

/// Recursively scan a directory for .jsonl files (bounded depth)
fn collect_jsonl_recursive(dir: &Path, files: &mut Vec<PathBuf>, depth: u32, max_depth: u32) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && depth < max_depth {
            collect_jsonl_recursive(&path, files, depth + 1, max_depth);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
}

fn parse_codex_file(
    file_path: &Path,
    root_thread_id: Option<String>,
) -> Result<ParsedCodexFile, AppError> {
    let file = fs::File::open(file_path)
        .map_err(|e| AppError::Config(format!("Unable to open file: {e}")))?;
    let reader = BufReader::new(file);
    let mut root_meta_seen = false;
    let mut root_timestamp = None;
    let mut meta_thread_id = None;
    let mut parent = ParentResolution::None;
    let mut current_model = "unknown".to_string();
    // `total_token_usage` is session-cumulative, including across model and
    // rate-limit bucket changes. Divergent snapshots are handled by preferring
    // exact `last_token_usage`, not by splitting the cumulative baseline.
    let mut total_high_water = None;
    // Rate-limit refreshes can re-emit unchanged token info under another
    // `limit_id`. Same-source repeats are identified by that source's latest
    // full snapshot; cross-source repeats must match the immediately preceding
    // token event. Do not compare against other sources' older snapshots:
    // those stale signatures can legitimately recur after a counter reset.
    let mut last_signature_by_source: HashMap<Option<String>, TokenUsageSignature> = HashMap::new();
    let mut previous_token_signature = None;
    let mut event_index = 0u32;
    let mut token_events = Vec::new();
    let mut line_offset = 0i64;
    let mut has_billable_tokens = false;

    for line_result in reader.lines() {
        line_offset += 1;
        let line = match line_result {
            Ok(line) => line,
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }

        let is_event_msg = line.contains("\"event_msg\"");
        let is_turn_context = line.contains("\"turn_context\"");
        let is_session_meta = line.contains("\"session_meta\"");
        if !is_event_msg && !is_turn_context && !is_session_meta {
            continue;
        }
        if is_event_msg && !line.contains("\"token_count\"") {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(event_type) = value.get("type").and_then(serde_json::Value::as_str) else {
            continue;
        };

        match event_type {
            "session_meta" if !root_meta_seen => {
                root_meta_seen = true;
                root_timestamp = parse_timestamp(value.get("timestamp"));
                let payload = value.get("payload").unwrap_or(&serde_json::Value::Null);
                parent = explicit_parent_from_meta(payload);

                meta_thread_id = non_empty_string(
                    payload
                        .get("id")
                        .or_else(|| payload.get("thread_id"))
                        .or_else(|| payload.get("threadId")),
                )
                .map(|id| {
                    uuid::Uuid::parse_str(&id)
                        .map(|value| value.hyphenated().to_string())
                        .unwrap_or(id)
                });
                if let (Some(filename_id), Some(meta_id)) =
                    (&root_thread_id, meta_thread_id.as_ref())
                {
                    let leading_id = leading_thread_id_from_filename(file_path);
                    let matches =
                        filename_id == meta_id || leading_id.as_deref() == Some(meta_id.as_str());
                    if !matches {
                        parent = ParentResolution::Deferred(format!(
                            "file name thread ID ({filename_id}) does not match root meta ID ({meta_id})"
                        ));
                    }
                }

                if let ParentResolution::Parent(parent_id) = &mut parent {
                    match uuid::Uuid::parse_str(parent_id) {
                        Ok(value) => *parent_id = value.hyphenated().to_string(),
                        Err(_) => {
                            parent = ParentResolution::Deferred(format!(
                                "explicit parent_thread_id is not a valid UUID: {parent_id}"
                            ));
                        }
                    }
                }
                if matches!((&root_thread_id, &parent), (Some(root), ParentResolution::Parent(parent_id)) if root == parent_id)
                {
                    parent = ParentResolution::Deferred(
                        "parent_thread_id equals root_thread_id".to_string(),
                    );
                }
            }
            "turn_context" => {
                if let Some(payload) = value.get("payload") {
                    if let Some(model) = payload
                        .get("model")
                        .or_else(|| payload.get("info").and_then(|info| info.get("model")))
                        .and_then(serde_json::Value::as_str)
                    {
                        current_model = normalize_codex_model(model);
                    }
                }
            }
            "event_msg" => {
                let Some(payload) = value.get("payload") else {
                    continue;
                };
                if payload.get("type").and_then(serde_json::Value::as_str) != Some("token_count") {
                    continue;
                }
                let Some(info) = payload.get("info").filter(|info| !info.is_null()) else {
                    continue;
                };
                let Some(signature) = parse_token_signature(info) else {
                    continue;
                };

                if let Some(model) = info
                    .get("model")
                    .or_else(|| info.get("model_name"))
                    .or_else(|| payload.get("model"))
                    .and_then(serde_json::Value::as_str)
                {
                    current_model = normalize_codex_model(model);
                }

                let snapshot_source = token_snapshot_source(payload);
                let total = info
                    .get("total_token_usage")
                    .and_then(parse_cumulative_tokens);
                let last = info
                    .get("last_token_usage")
                    .and_then(parse_cumulative_tokens);
                if total.is_none() && last.is_none() {
                    continue;
                }
                let has_total_snapshot = total.is_some();
                let duplicate_snapshot = has_total_snapshot
                    && (last_signature_by_source.get(&snapshot_source) == Some(&signature)
                        || previous_token_signature.as_ref() == Some(&signature));
                if has_total_snapshot {
                    last_signature_by_source.insert(snapshot_source, signature.clone());
                }
                previous_token_signature = Some(signature.clone());

                let delta = if duplicate_snapshot {
                    DeltaTokens {
                        input: 0,
                        cached_input: 0,
                        output: 0,
                    }
                } else if let Some(last) = last {
                    // Codex provides the exact per-request usage. Prefer it to
                    // subtracting cumulative snapshots, which may come from
                    // multiple independently advancing rate-limit lanes.
                    DeltaTokens {
                        input: last.input as u32,
                        cached_input: last.cached_input as u32,
                        output: last.output as u32,
                    }
                } else if let Some(total) = total.as_ref() {
                    compute_delta(&total_high_water, total)
                } else {
                    continue;
                };
                if let Some(total) = total {
                    if let Some(high_water) = total_high_water.as_mut() {
                        update_high_water(high_water, &total);
                    } else {
                        total_high_water = Some(total);
                    }
                }
                let delta = DeltaTokens {
                    cached_input: delta.cached_input.min(delta.input),
                    ..delta
                };
                let nonzero_index = if delta.is_zero() {
                    None
                } else {
                    has_billable_tokens = true;
                    event_index = event_index.saturating_add(1);
                    Some(event_index)
                };

                token_events.push(ParsedTokenEvent {
                    line_offset,
                    signature,
                    delta,
                    event_index: nonzero_index,
                    model: current_model.clone(),
                    timestamp: value
                        .get("timestamp")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                });
            }
            _ => {}
        }
    }

    Ok(ParsedCodexFile {
        root_thread_id,
        meta_thread_id,
        root_meta_seen,
        root_timestamp,
        parent,
        token_events,
        line_offset,
        has_billable_tokens,
    })
}

fn parent_signatures_before(
    parent_path: &Path,
    cutoff: DateTime<Utc>,
) -> Result<Vec<TokenUsageSignature>, String> {
    let file = fs::File::open(parent_path).map_err(|error| {
        format!(
            "Unable to open parent rollout {}: {error}",
            parent_path.display()
        )
    })?;
    let stamp = ParentFileStamp::from_file(&file);
    let cached_timeline = stamp.and_then(|stamp| {
        replay_caches().lock().ok().and_then(|caches| {
            caches
                .parent_timelines
                .get(parent_path)
                .filter(|entry| entry.stamp == stamp)
                .map(|entry| Arc::clone(&entry.timeline))
        })
    });
    if let Some(timeline) = cached_timeline {
        return timeline.signatures_before(parent_path, cutoff);
    }

    let mut events = Vec::new();
    let mut max_timestamp: Option<DateTime<Utc>> = None;
    let mut has_token_without_timestamp = false;

    // The whole parent file must be scanned; breaking at the first future timestamp is not allowed
    // because rollout write order does not guarantee monotonic timestamps. Once the full timeline is cached, different child cutoffs are just in-memory filters.
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let timestamp = parse_timestamp(value.get("timestamp"));
        if let Some(timestamp) = timestamp {
            max_timestamp = Some(max_timestamp.map_or(timestamp, |current| current.max(timestamp)));
        }
        if value.get("type").and_then(serde_json::Value::as_str) != Some("event_msg")
            || value
                .get("payload")
                .and_then(|payload| payload.get("type"))
                .and_then(serde_json::Value::as_str)
                != Some("token_count")
        {
            continue;
        }
        let Some(info) = value
            .get("payload")
            .and_then(|payload| payload.get("info"))
            .filter(|info| !info.is_null())
        else {
            continue;
        };
        let Some(signature) = parse_token_signature(info) else {
            continue;
        };
        let Some(timestamp) = timestamp else {
            has_token_without_timestamp = true;
            continue;
        };
        events.push(TimestampedTokenSignature {
            timestamp,
            signature,
        });
    }

    let timeline = Arc::new(ParentTokenTimeline {
        events,
        max_timestamp,
        has_token_without_timestamp,
    });
    let result = timeline.signatures_before(parent_path, cutoff);
    if let (Some(stamp), Ok(mut caches)) = (stamp, replay_caches().lock()) {
        caches.parent_timelines.insert(
            parent_path.to_path_buf(),
            CachedParentTimeline {
                stamp,
                timeline: Arc::clone(&timeline),
            },
        );
    }
    result
}

fn resolve_parent_signatures(
    parent_id: &str,
    cutoff: DateTime<Utc>,
    rollout_index: &RolloutIndex,
) -> Result<Vec<TokenUsageSignature>, String> {
    let Some(candidates) = rollout_index.get(parent_id) else {
        return Err(format!("Parent rollout not found: {parent_id}"));
    };

    let mut snapshots = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        snapshots.push(parent_signatures_before(candidate, cutoff)?);
    }
    let Some(first) = snapshots.first() else {
        return Err(format!("Parent rollout not found: {parent_id}"));
    };
    if snapshots.iter().skip(1).any(|snapshot| snapshot != first) {
        return Err(format!(
            "Parent rollout UUID {parent_id} maps to several files with inconsistent content"
        ));
    }
    Ok(first.clone())
}

fn matching_replay_prefix(child: &[ParsedTokenEvent], parent: &[TokenUsageSignature]) -> usize {
    let mut parent_offset = 0usize;
    let mut matched = 0usize;
    for event in child {
        let Some(relative_match) = parent[parent_offset..]
            .iter()
            .position(|signature| signature == &event.signature)
        else {
            break;
        };
        parent_offset += relative_match + 1;
        matched += 1;
    }
    matched
}

fn mark_deferred(
    file_path: &Path,
    modified: i64,
    size: u64,
    reason: PendingReason,
) -> CodexFileSyncResult {
    let entry = PendingEntry {
        modified,
        size,
        reason,
    };
    let should_warn = replay_caches()
        .lock()
        .ok()
        .and_then(|mut caches| {
            caches
                .pending
                .insert(file_path.to_path_buf(), entry.clone())
        })
        .as_ref()
        != Some(&entry);
    if should_warn {
        let reason = match &entry.reason {
            PendingReason::MissingParent(parent) => format!("parent rollout {parent} not found"),
            PendingReason::Stable(reason) | PendingReason::Retryable(reason) => reason.clone(),
        };
        log::warn!("[CODEX-SYNC] deferred {}: {reason}", file_path.display());
    }
    CodexFileSyncResult {
        deferred: true,
        ..CodexFileSyncResult::default()
    }
}

/// Transaction granularity of the batched inserts for one file. Inside a batch, UI queries are blocked
/// by the connection mutex for a few milliseconds; between batches the lock is released so readers can
/// slip in - balancing throughput (avoiding a per-row fsync from row-by-row autocommit) against panel
/// responsiveness while a large file is being re-imported.
const CODEX_INSERT_BATCH_SIZE: usize = 1000;

/// Sync a single Codex JSONL file.
fn sync_single_codex_file(
    db: &Database,
    file_path: &Path,
    rollout_index: &RolloutIndex,
    pass: &mut CodexSyncPass,
) -> Result<CodexFileSyncResult, AppError> {
    let file_path_str = file_path.to_string_lossy().to_string();

    // Read file metadata
    let metadata = fs::metadata(file_path)
        .map_err(|e| AppError::Config(format!("Unable to read file metadata: {e}")))?;
    let file_modified = metadata_modified_nanos(&metadata);
    let file_size = metadata.len();

    // Check sync state
    let (last_modified, last_offset) = get_codex_sync_state(db, file_path, &pass.cursors)?;

    // Skip if the file has not changed
    if file_modified <= last_modified {
        return Ok(CodexFileSyncResult::default());
    }

    if let Ok(mut caches) = replay_caches().lock() {
        if let Some(pending) = caches.pending.get(file_path).cloned() {
            if pending.modified == file_modified && pending.size == file_size {
                match &pending.reason {
                    PendingReason::MissingParent(parent) if !rollout_index.contains_key(parent) => {
                        return Ok(CodexFileSyncResult {
                            deferred: true,
                            ..CodexFileSyncResult::default()
                        });
                    }
                    PendingReason::Stable(_) => {
                        return Ok(CodexFileSyncResult {
                            deferred: true,
                            ..CodexFileSyncResult::default()
                        });
                    }
                    PendingReason::Retryable(_) => {
                        caches.pending.remove(file_path);
                    }
                    _ => {
                        caches.pending.remove(file_path);
                    }
                }
            }
        }
    }

    let parsed = parse_codex_file(file_path, thread_id_from_filename(file_path))?;
    if !parsed.has_billable_tokens {
        update_sync_state(db, &file_path_str, file_modified, parsed.line_offset)?;
        return Ok(CodexFileSyncResult::default());
    }
    let Some(root_thread_id) = parsed.root_thread_id.as_deref() else {
        return Ok(mark_deferred(
            file_path,
            file_modified,
            file_size,
            PendingReason::Stable("file name has no valid trailing UUID".to_string()),
        ));
    };
    if !parsed.root_meta_seen {
        return Ok(mark_deferred(
            file_path,
            file_modified,
            file_size,
            PendingReason::Stable("has billable tokens but no session_meta yet".to_string()),
        ));
    }

    let replay_prefix = match &parsed.parent {
        ParentResolution::None => 0,
        ParentResolution::Deferred(reason) => {
            return Ok(mark_deferred(
                file_path,
                file_modified,
                file_size,
                PendingReason::Stable(reason.clone()),
            ));
        }
        ParentResolution::Parent(parent_id) => {
            let Some(cutoff) = parsed.root_timestamp else {
                return Ok(mark_deferred(
                    file_path,
                    file_modified,
                    file_size,
                    PendingReason::Stable(
                        "root meta of a parented rollout has no valid timestamp".to_string(),
                    ),
                ));
            };
            if let Ok(caches) = replay_caches().lock() {
                if let Some(prefix) = caches
                    .replay_prefixes
                    .get(file_path)
                    .filter(|cached| cached.modified == file_modified && cached.size == file_size)
                    .map(|cached| cached.prefix)
                {
                    prefix
                } else {
                    drop(caches);
                    let parent_signatures =
                        match resolve_parent_signatures(parent_id, cutoff, rollout_index) {
                            Ok(signatures) => signatures,
                            Err(reason) => {
                                let pending_reason = if rollout_index.contains_key(parent_id) {
                                    PendingReason::Retryable(reason)
                                } else {
                                    PendingReason::MissingParent(parent_id.clone())
                                };
                                return Ok(mark_deferred(
                                    file_path,
                                    file_modified,
                                    file_size,
                                    pending_reason,
                                ));
                            }
                        };
                    let prefix = matching_replay_prefix(&parsed.token_events, &parent_signatures);
                    if let Ok(mut caches) = replay_caches().lock() {
                        caches.replay_prefixes.insert(
                            file_path.to_path_buf(),
                            CachedReplayPrefix {
                                modified: file_modified,
                                size: file_size,
                                prefix,
                            },
                        );
                    }
                    prefix
                }
            } else {
                let parent_signatures = resolve_parent_signatures(parent_id, cutoff, rollout_index)
                    .map_err(AppError::Config)?;
                matching_replay_prefix(&parsed.token_events, &parent_signatures)
            }
        }
    };

    if let Ok(mut caches) = replay_caches().lock() {
        caches.pending.remove(file_path);
    }

    let mut result = CodexFileSyncResult::default();
    let mut to_insert: Vec<(&ParsedTokenEvent, u32)> = Vec::new();
    for (token_offset, event) in parsed.token_events.iter().enumerate() {
        let Some(event_index) = event.event_index else {
            continue;
        };
        if token_offset < replay_prefix {
            if event.line_offset > last_offset {
                result.skipped = result.skipped.saturating_add(1);
            }
            continue;
        }
        if event.line_offset <= last_offset {
            continue;
        }
        to_insert.push((event, event_index));
    }

    // Batched transactional writes: row-by-row autocommit (under journal_mode=delete every row means a
    // full journal create/fsync/delete) is the biggest cost of a full re-import. A single failed insert
    // inside a batch keeps the old behaviour and skips that row; a failed batch commit rolls the whole
    // batch back without advancing the cursor, and the next pass rescans with request_id + fingerprint dedup.
    //
    // session_id records the root meta thread ID: for two-segment file names (the replacement rollout of
    // thread/revert) that is the leading UUID, matching the session identity used by the session manager;
    // the trailing rollout ID only serves as the request_id dedup key (event_index counts per physical
    // file, so it cannot switch to the leading ID).
    let session_thread_id = parsed.meta_thread_id.as_deref().unwrap_or(root_thread_id);
    let batch_count = to_insert.len().div_ceil(CODEX_INSERT_BATCH_SIZE);
    for (batch_index, batch) in to_insert.chunks(CODEX_INSERT_BATCH_SIZE).enumerate() {
        let is_last_batch = batch_index + 1 == batch_count;
        let conn = lock_conn!(db.conn);
        let tx = conn.unchecked_transaction().map_err(|e| {
            AppError::Database(format!(
                "Failed to open the Codex session write transaction: {e}"
            ))
        })?;

        let mut batch_imported = 0u32;
        let mut batch_skipped = 0u32;
        let mut batch_suspected = 0u32;
        for (event, event_index) in batch {
            let request_id =
                format!("{CODEX_THREAD_REQUEST_ID_PREFIX}:{root_thread_id}:{event_index}");
            match insert_codex_session_entry_on_conn(
                &tx,
                &request_id,
                &event.delta,
                &event.model,
                Some(session_thread_id),
                event.timestamp.as_deref(),
                &mut batch_suspected,
                &mut pass.pricing,
            ) {
                Ok(true) => batch_imported += 1,
                Ok(false) => batch_skipped += 1,
                Err(e) => {
                    log::warn!("[CODEX-SYNC] insert failed ({request_id}): {e}");
                    batch_skipped += 1;
                }
            }
        }
        if is_last_batch {
            // The cursor advance is committed in the same transaction as the last batch: a crash rolls
            // both back, so there is no "cursor advanced but data missing" data-loss window.
            update_sync_state_on_conn(&tx, &file_path_str, file_modified, parsed.line_offset)?;
        }
        tx.commit().map_err(|e| {
            AppError::Database(format!(
                "Failed to commit the Codex session write transaction: {e}"
            ))
        })?;

        result.imported = result.imported.saturating_add(batch_imported);
        result.skipped = result.skipped.saturating_add(batch_skipped);
        result.suspected_duplicates = result.suspected_duplicates.saturating_add(batch_suspected);
    }

    if to_insert.is_empty() {
        update_sync_state(db, &file_path_str, file_modified, parsed.line_offset)?;
    }
    Ok(result)
}

/// Insert a single Codex session record into proxy_request_logs (self-locking convenience wrapper, test
/// only; the production path uses [`insert_codex_session_entry_on_conn`] to reuse the batch transaction and pricing cache)
#[cfg(test)]
fn insert_codex_session_entry(
    db: &Database,
    request_id: &str,
    delta: &DeltaTokens,
    model: &str,
    session_id: Option<&str>,
    timestamp: Option<&str>,
    suspected_duplicates: &mut u32,
) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);
    insert_codex_session_entry_on_conn(
        &conn,
        request_id,
        delta,
        model,
        session_id,
        timestamp,
        suspected_duplicates,
        &mut HashMap::new(),
    )
}

/// Insert a single Codex session record into proxy_request_logs.
///
/// The caller owns the lock/transaction; `pricing_cache` is keyed by the raw model string
/// (`find_codex_pricing` is a pure lookup, so the same string always yields the same result), which
/// turns one pricing SELECT per event into one per model during a full re-import.
#[allow(clippy::too_many_arguments)]
fn insert_codex_session_entry_on_conn(
    conn: &rusqlite::Connection,
    request_id: &str,
    delta: &DeltaTokens,
    model: &str,
    session_id: Option<&str>,
    timestamp: Option<&str>,
    suspected_duplicates: &mut u32,
    pricing_cache: &mut HashMap<String, Option<ModelPricing>>,
) -> Result<bool, AppError> {
    let created_at = timestamp
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
        app_type: "codex",
        model,
        input_tokens: delta.input,
        output_tokens: delta.output,
        cache_read_tokens: delta.cached_input,
        cache_creation_tokens: 0,
        created_at,
    };
    if should_skip_session_insert(conn, request_id, &dedup_key)? {
        return Ok(false);
    }
    if has_suspected_codex_session_duplicate(conn, request_id, &dedup_key)? {
        *suspected_duplicates = suspected_duplicates.saturating_add(1);
        log::warn!(
            "[CODEX-SYNC] suspected duplicate session usage: request_id={request_id}, model={model}, input={}, output={}, cache_read={}",
            delta.input,
            delta.output,
            delta.cached_input
        );
    }

    // Compute cost
    let usage = TokenUsage {
        input_tokens: delta.input,
        output_tokens: delta.output,
        cache_read_tokens: delta.cached_input,
        cache_creation_tokens: 0,
        model: Some(model.to_string()),
        message_id: None,
    };

    let pricing = pricing_cache
        .entry(model.to_string())
        .or_insert_with(|| find_codex_pricing(conn, model));
    let multiplier = Decimal::from(1);
    let (input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost) = match pricing
    {
        Some(p) => {
            let cost = CostCalculator::calculate_for_app("codex", &usage, p, multiplier);
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
        .prepare_cached(
            "INSERT OR IGNORE INTO proxy_request_logs (
            request_id, provider_id, app_type, model, request_model,
            input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
            input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
            latency_ms, first_token_ms, status_code, error_message, session_id,
            provider_type, is_streaming, cost_multiplier, created_at, data_source
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        )
        .and_then(|mut stmt| stmt.execute(rusqlite::params![
                request_id,
                "_codex_session",    // provider_id
                "codex",             // app_type
                model,
                model,               // request_model = model
                delta.input,
                delta.output,
                delta.cached_input,
                0i64,                // cache_creation_tokens: not available in Codex logs
                input_cost,
                output_cost,
                cache_read_cost,
                cache_creation_cost,
                total_cost,
                0i64,                // latency_ms
                Option::<i64>::None, // first_token_ms
                200i64,              // status_code
                Option::<String>::None, // error_message
                session_id.map(|s| s.to_string()),
                Some("codex_session"), // provider_type
                1i64,                // is_streaming
                "1.0",               // cost_multiplier
                created_at,
                "codex_session",     // data_source
            ]))
        .map_err(|e| AppError::Database(format!("Failed to insert the Codex session log: {e}")))?;

    Ok(inserted_rows > 0)
}

/// Look up Codex model pricing (with normalization)
fn find_codex_pricing(conn: &rusqlite::Connection, model_id: &str) -> Option<ModelPricing> {
    find_model_pricing(conn, &normalize_codex_model(model_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::session_usage::get_sync_state;
    use tempfile::tempdir;

    const PARENT_ID: &str = "00000000-0000-4000-8000-000000000001";
    const CHILD_A_ID: &str = "00000000-0000-4000-8000-000000000002";
    const CHILD_B_ID: &str = "00000000-0000-4000-8000-000000000003";

    fn write_jsonl(path: &Path, values: &[serde_json::Value]) {
        let contents = values
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(path, contents).unwrap();
    }

    fn rollout_path(dir: &Path, thread_id: &str) -> PathBuf {
        dir.join(format!("rollout-2026-07-10T03-00-00-{thread_id}.jsonl"))
    }

    fn session_meta_at(
        thread_id: &str,
        forked_from_id: Option<&str>,
        spawned_from_id: Option<&str>,
        timestamp: &str,
    ) -> serde_json::Value {
        let source = spawned_from_id.map_or_else(
            || serde_json::Value::String("cli".to_string()),
            |parent| {
                serde_json::json!({
                    "subagent": {
                        "thread_spawn": { "parent_thread_id": parent }
                    }
                })
            },
        );
        serde_json::json!({
            "timestamp": timestamp,
            "type": "session_meta",
            "payload": {
                "id": thread_id,
                "forked_from_id": forked_from_id,
                "source": source
            }
        })
    }

    fn session_meta(thread_id: &str) -> serde_json::Value {
        session_meta_at(thread_id, None, None, "2026-07-10T03:00:00Z")
    }

    fn turn_context_for_model_at(model: &str, timestamp: &str) -> serde_json::Value {
        serde_json::json!({
            "timestamp": timestamp,
            "type": "turn_context",
            "payload": { "model": model }
        })
    }

    fn turn_context_at(timestamp: &str) -> serde_json::Value {
        turn_context_for_model_at("gpt-5.6-sol", timestamp)
    }

    fn turn_context() -> serde_json::Value {
        turn_context_at("2026-07-10T03:00:01Z")
    }

    fn token_count_at(input: u64, cached: u64, output: u64, timestamp: &str) -> serde_json::Value {
        serde_json::json!({
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": { "total_token_usage": {
                    "input_tokens": input,
                    "cached_input_tokens": cached,
                    "output_tokens": output,
                    "reasoning_output_tokens": 0,
                    "total_tokens": input + output
                }}
            }
        })
    }

    fn token_count(input: u64, cached: u64, output: u64) -> serde_json::Value {
        token_count_at(input, cached, output, "2026-07-10T03:00:02Z")
    }

    #[allow(clippy::too_many_arguments)]
    fn token_count_with_last_at(
        total_input: u64,
        total_cached: u64,
        total_output: u64,
        last_input: u64,
        last_cached: u64,
        last_output: u64,
        limit_id: &str,
        timestamp: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": total_input,
                        "cached_input_tokens": total_cached,
                        "output_tokens": total_output,
                        "reasoning_output_tokens": 0,
                        "total_tokens": total_input + total_output
                    },
                    "last_token_usage": {
                        "input_tokens": last_input,
                        "cached_input_tokens": last_cached,
                        "output_tokens": last_output,
                        "reasoning_output_tokens": 0,
                        "total_tokens": last_input + last_output
                    }
                },
                "rate_limits": { "limit_id": limit_id }
            }
        })
    }

    fn sync_test_file(
        db: &Database,
        file: &Path,
        all_files: &[&Path],
    ) -> Result<CodexFileSyncResult, AppError> {
        let files = all_files
            .iter()
            .map(|path| path.to_path_buf())
            .collect::<Vec<_>>();
        let mut pass = CodexSyncPass::load(db)?;
        sync_single_codex_file(db, file, &build_rollout_index(&files), &mut pass)
    }

    /// A revert produces a replacement rollout with a `<threadId>_<rolloutId>` two-segment file name;
    /// the root meta id is the original thread ID (the first UUID) and forked_from_id is empty.
    /// The old check only accepted the trailing UUID and deferred such files forever, losing all usage
    /// appended by later resumes.
    #[test]
    fn test_resumed_rollout_meta_id_matching_leading_uuid_is_not_deferred() -> Result<(), AppError>
    {
        let dir = tempdir().unwrap();
        let file = dir.path().join(format!(
            "rollout-2026-08-26T17-18-13-{PARENT_ID}_{CHILD_A_ID}.jsonl"
        ));
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                token_count_at(100, 50, 20, "2026-08-26T09:18:20Z"),
            ],
        );

        let parsed = parse_codex_file(&file, thread_id_from_filename(&file))?;

        // A resumed session has no explicit parent and must not be rejected for an ID mismatch
        assert!(
            !matches!(parsed.parent, ParentResolution::Deferred(_)),
            "a resumed session must not be deferred, got: {:?}",
            parsed.parent
        );
        assert_eq!(parsed.root_thread_id.as_deref(), Some(CHILD_A_ID));
        assert!(parsed.has_billable_tokens);
        Ok(())
    }

    /// Roles of the two UUIDs of a two-segment rollout in the full sync path: request_id uses the
    /// trailing rollout ID (event_index counts per physical file, so the dedup key cannot change), while
    /// the stored session_id is the leading thread ID, matching the session manager's session identity.
    #[test]
    fn test_resumed_rollout_session_id_uses_leading_thread_id() -> Result<(), AppError> {
        let db = Database::memory()?;
        let temp = tempdir().unwrap();
        let file = temp.path().join(format!(
            "rollout-2026-08-26T17-18-13-{PARENT_ID}_{CHILD_A_ID}.jsonl"
        ));
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                token_count(100, 50, 10),
            ],
        );

        assert_eq!(sync_test_file(&db, &file, &[&file])?.imported, 1);

        let conn = lock_conn!(db.conn);
        let (request_id, session_id) = conn
            .prepare(
                "SELECT request_id, session_id FROM proxy_request_logs
                 WHERE data_source = 'codex_session'",
            )?
            .query_row([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
        assert_eq!(
            request_id,
            format!("{CODEX_THREAD_REQUEST_ID_PREFIX}:{CHILD_A_ID}:1")
        );
        assert_eq!(session_id, PARENT_ID);
        Ok(())
    }

    /// A mismatch on a single-segment file name must still be rejected.
    #[test]
    fn test_single_uuid_filename_meta_mismatch_still_deferred() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        // meta carries the ID of another thread - a shape the revert two-segment name cannot explain
        write_jsonl(
            &file,
            &[
                session_meta(CHILD_B_ID),
                turn_context(),
                token_count_at(1, 1, 1, "2026-07-10T03:00:02Z"),
            ],
        );

        let parsed = parse_codex_file(&file, thread_id_from_filename(&file))?;
        assert!(matches!(parsed.parent, ParentResolution::Deferred(_)));
        Ok(())
    }

    #[test]
    fn test_delta_first_event() {
        let prev = None;
        let current = CumulativeTokens {
            input: 17934,
            cached_input: 9600,
            output: 454,
        };
        let delta = compute_delta(&prev, &current);
        assert_eq!(delta.input, 17934);
        assert_eq!(delta.cached_input, 9600);
        assert_eq!(delta.output, 454);
        assert!(!delta.is_zero());
    }

    #[test]
    fn test_delta_subsequent_event() {
        let prev = Some(CumulativeTokens {
            input: 17934,
            cached_input: 9600,
            output: 454,
        });
        let current = CumulativeTokens {
            input: 36722,
            cached_input: 27904,
            output: 804,
        };
        let delta = compute_delta(&prev, &current);
        assert_eq!(delta.input, 36722 - 17934);
        assert_eq!(delta.cached_input, 27904 - 9600);
        assert_eq!(delta.output, 804 - 454);
    }

    #[test]
    fn test_delta_zero_at_task_boundary() {
        let prev = Some(CumulativeTokens {
            input: 58346,
            cached_input: 46976,
            output: 1045,
        });
        // task boundary: identical cumulative values
        let current = CumulativeTokens {
            input: 58346,
            cached_input: 46976,
            output: 1045,
        };
        let delta = compute_delta(&prev, &current);
        assert!(delta.is_zero());
    }

    #[test]
    fn test_delta_saturating_sub() {
        // Abnormal case: the current value is below the previous one (should not happen, but guard anyway)
        let prev = Some(CumulativeTokens {
            input: 100,
            cached_input: 50,
            output: 30,
        });
        let current = CumulativeTokens {
            input: 80,
            cached_input: 40,
            output: 20,
        };
        let delta = compute_delta(&prev, &current);
        assert_eq!(delta.input, 0);
        assert_eq!(delta.cached_input, 0);
        assert_eq!(delta.output, 0);
        assert!(delta.is_zero());
    }

    #[test]
    fn test_interleaved_counter_lanes_use_exact_last_usage() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        let bengal_event = token_count_with_last_at(
            87_709_262,
            83_563_008,
            240_919,
            151_258,
            147_200,
            87,
            "codex_bengalfox",
            "2026-07-10T03:00:03Z",
        );
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                token_count_with_last_at(
                    76_780_408,
                    73_010_432,
                    243_036,
                    175_074,
                    169_728,
                    6_827,
                    "codex",
                    "2026-07-10T03:00:02Z",
                ),
                bengal_event.clone(),
                token_count_with_last_at(
                    76_962_538,
                    73_180_160,
                    243_258,
                    182_130,
                    169_728,
                    222,
                    "codex",
                    "2026-07-10T03:00:04Z",
                ),
                // Repeated snapshots are notifications, not additional API usage.
                bengal_event,
            ],
        );

        let parsed = parse_codex_file(&file, Some(PARENT_ID.to_string()))?;
        let deltas = parsed
            .token_events
            .iter()
            .filter(|event| !event.delta.is_zero())
            .map(|event| {
                (
                    event.delta.input,
                    event.delta.cached_input,
                    event.delta.output,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            deltas,
            vec![
                (175_074, 169_728, 6_827),
                (151_258, 147_200, 87),
                (182_130, 169_728, 222),
            ]
        );
        assert!(parsed.token_events[3].delta.is_zero());
        Ok(())
    }

    #[test]
    fn test_cross_limit_snapshot_replay_is_not_double_counted() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                token_count_with_last_at(1_000, 0, 10, 100, 0, 10, "codex", "2026-07-10T03:00:02Z"),
                token_count_with_last_at(
                    1_000,
                    0,
                    10,
                    100,
                    0,
                    10,
                    "codex_bengalfox",
                    "2026-07-10T03:00:03Z",
                ),
            ],
        );

        let parsed = parse_codex_file(&file, Some(PARENT_ID.to_string()))?;
        let deltas = parsed
            .token_events
            .iter()
            .filter(|event| !event.delta.is_zero())
            .map(|event| event.delta.input)
            .collect::<Vec<_>>();

        assert_eq!(deltas, vec![100]);
        Ok(())
    }

    #[test]
    fn test_adjacent_replay_burst_across_multiple_sources_is_deduped() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                token_count_with_last_at(1_000, 0, 10, 100, 0, 10, "codex", "2026-07-10T03:00:02Z"),
                token_count_with_last_at(
                    1_000,
                    0,
                    10,
                    100,
                    0,
                    10,
                    "codex_bengalfox",
                    "2026-07-10T03:00:03Z",
                ),
                token_count_with_last_at(
                    1_000,
                    0,
                    10,
                    100,
                    0,
                    10,
                    "codex_spark",
                    "2026-07-10T03:00:04Z",
                ),
            ],
        );

        let parsed = parse_codex_file(&file, Some(PARENT_ID.to_string()))?;
        let deltas = parsed
            .token_events
            .iter()
            .filter(|event| !event.delta.is_zero())
            .map(|event| event.delta.input)
            .collect::<Vec<_>>();

        assert_eq!(deltas, vec![100]);
        Ok(())
    }

    #[test]
    fn test_cross_source_replay_remains_adjacent_across_non_token_events() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                token_count_with_last_at(1_000, 0, 10, 100, 0, 10, "codex", "2026-07-10T03:00:02Z"),
                turn_context_for_model_at("gpt-5.6-sol", "2026-07-10T03:00:03Z"),
                token_count_with_last_at(
                    1_000,
                    0,
                    10,
                    100,
                    0,
                    10,
                    "codex_bengalfox",
                    "2026-07-10T03:00:04Z",
                ),
            ],
        );

        let parsed = parse_codex_file(&file, Some(PARENT_ID.to_string()))?;
        let deltas = parsed
            .token_events
            .iter()
            .filter(|event| !event.delta.is_zero())
            .map(|event| event.delta.input)
            .collect::<Vec<_>>();

        assert_eq!(deltas, vec![100]);
        Ok(())
    }

    #[test]
    fn test_same_source_repeat_is_deduped_after_another_source_advances() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                token_count_with_last_at(1_000, 0, 10, 100, 0, 10, "codex", "2026-07-10T03:00:02Z"),
                token_count_with_last_at(
                    2_000,
                    0,
                    20,
                    100,
                    0,
                    10,
                    "codex_bengalfox",
                    "2026-07-10T03:00:03Z",
                ),
                // `codex` has not advanced since its X snapshot, so this is a
                // same-source replay even though another source was interleaved.
                token_count_with_last_at(1_000, 0, 10, 100, 0, 10, "codex", "2026-07-10T03:00:04Z"),
            ],
        );

        let parsed = parse_codex_file(&file, Some(PARENT_ID.to_string()))?;
        let deltas = parsed
            .token_events
            .iter()
            .filter(|event| !event.delta.is_zero())
            .map(|event| event.delta.input)
            .collect::<Vec<_>>();

        assert_eq!(deltas, vec![100, 100]);
        Ok(())
    }

    #[test]
    fn test_stale_cross_source_signature_does_not_swallow_reset() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                // `codex` emits snapshot X.
                token_count_with_last_at(1_000, 0, 10, 100, 0, 10, "codex", "2026-07-10T03:00:02Z"),
                // X is replayed under another rate-limit source.
                token_count_with_last_at(
                    1_000,
                    0,
                    10,
                    100,
                    0,
                    10,
                    "codex_bengalfox",
                    "2026-07-10T03:00:03Z",
                ),
                // The original source advances to Y.
                token_count_with_last_at(2_000, 0, 20, 100, 0, 10, "codex", "2026-07-10T03:00:04Z"),
                // A genuine reset later reproduces X. The stale copy retained
                // by `codex_bengalfox` must not classify this as a replay.
                token_count_with_last_at(1_000, 0, 10, 100, 0, 10, "codex", "2026-07-10T03:00:05Z"),
            ],
        );

        let parsed = parse_codex_file(&file, Some(PARENT_ID.to_string()))?;
        let deltas = parsed
            .token_events
            .iter()
            .filter(|event| !event.delta.is_zero())
            .map(|event| event.delta.input)
            .collect::<Vec<_>>();

        assert_eq!(deltas, vec![100, 100, 100]);
        Ok(())
    }

    #[test]
    fn test_full_snapshot_dedupe_allows_counter_reset() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        let first =
            token_count_with_last_at(100, 50, 10, 100, 50, 10, "codex", "2026-07-10T03:00:02Z");
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                first.clone(),
                first,
                token_count_with_last_at(
                    200,
                    100,
                    20,
                    100,
                    50,
                    10,
                    "codex",
                    "2026-07-10T03:00:04Z",
                ),
                // A restarted counter may legitimately return to an older
                // total after another full snapshot has advanced the source.
                token_count_with_last_at(100, 50, 10, 50, 25, 5, "codex", "2026-07-10T03:00:05Z"),
            ],
        );

        let parsed = parse_codex_file(&file, Some(PARENT_ID.to_string()))?;
        let deltas = parsed
            .token_events
            .iter()
            .filter(|event| !event.delta.is_zero())
            .map(|event| event.delta.input)
            .collect::<Vec<_>>();

        assert_eq!(deltas, vec![100, 100, 50]);
        Ok(())
    }

    #[test]
    fn test_empty_last_usage_falls_back_to_total() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                serde_json::json!({
                    "timestamp": "2026-07-10T03:00:02Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "total_token_usage": {
                                "input_tokens": 100,
                                "cached_input_tokens": 0,
                                "output_tokens": 10,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 110
                            },
                            "last_token_usage": {}
                        }
                    }
                }),
            ],
        );

        let parsed = parse_codex_file(&file, Some(PARENT_ID.to_string()))?;
        let deltas = parsed
            .token_events
            .iter()
            .filter(|event| !event.delta.is_zero())
            .map(|event| event.delta.input)
            .collect::<Vec<_>>();

        assert_eq!(deltas, vec![100]);
        Ok(())
    }

    #[test]
    fn test_empty_total_does_not_enable_snapshot_deduplication() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        let event = |limit_id: &str, timestamp: &str| {
            serde_json::json!({
                "timestamp": timestamp,
                "type": "event_msg",
                "payload": {
                    "type": "token_count",
                    "info": {
                        "total_token_usage": {},
                        "last_token_usage": {
                            "input_tokens": 100,
                            "cached_input_tokens": 0,
                            "output_tokens": 10,
                            "reasoning_output_tokens": 0,
                            "total_tokens": 110
                        }
                    },
                    "rate_limits": { "limit_id": limit_id }
                }
            })
        };
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                event("codex", "2026-07-10T03:00:02Z"),
                // Without a usable cumulative total, identical per-request
                // usage is not enough evidence that this is a replay.
                event("codex_bengalfox", "2026-07-10T03:00:03Z"),
            ],
        );

        let parsed = parse_codex_file(&file, Some(PARENT_ID.to_string()))?;
        let deltas = parsed
            .token_events
            .iter()
            .filter(|event| !event.delta.is_zero())
            .map(|event| event.delta.input)
            .collect::<Vec<_>>();

        assert_eq!(deltas, vec![100, 100]);
        Ok(())
    }

    #[test]
    fn test_total_fallback_uses_session_baseline_across_model_switch() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let file = rollout_path(dir.path(), PARENT_ID);
        write_jsonl(
            &file,
            &[
                session_meta(PARENT_ID),
                turn_context_for_model_at("model-a", "2026-07-10T03:00:01Z"),
                token_count_at(100, 50, 10, "2026-07-10T03:00:02Z"),
                turn_context_for_model_at("model-b", "2026-07-10T03:00:03Z"),
                token_count_at(150, 75, 15, "2026-07-10T03:00:04Z"),
            ],
        );

        let parsed = parse_codex_file(&file, Some(PARENT_ID.to_string()))?;
        let deltas = parsed
            .token_events
            .iter()
            .filter(|event| !event.delta.is_zero())
            .map(|event| event.delta.input)
            .collect::<Vec<_>>();

        assert_eq!(deltas, vec![100, 50]);
        Ok(())
    }

    #[test]
    fn test_parse_cumulative_tokens_valid() {
        let json: serde_json::Value = serde_json::json!({
            "input_tokens": 17934,
            "cached_input_tokens": 9600,
            "output_tokens": 454,
            "reasoning_output_tokens": 233,
            "total_tokens": 18388
        });
        let tokens = parse_cumulative_tokens(&json).unwrap();
        assert_eq!(tokens.input, 17934);
        assert_eq!(tokens.cached_input, 9600);
        assert_eq!(tokens.output, 454);
    }

    #[test]
    fn test_parse_cumulative_tokens_null() {
        let json = serde_json::Value::Null;
        assert!(parse_cumulative_tokens(&json).is_none());
    }

    #[test]
    fn test_parse_cumulative_tokens_rejects_empty_object_but_accepts_explicit_zero() {
        assert!(parse_cumulative_tokens(&serde_json::json!({})).is_none());

        let tokens = parse_cumulative_tokens(&serde_json::json!({ "input_tokens": 0 }))
            .expect("an explicit zero is valid usage");
        assert_eq!(tokens.input, 0);
        assert_eq!(tokens.cached_input, 0);
        assert_eq!(tokens.output, 0);
    }

    #[test]
    fn test_parse_cumulative_tokens_alt_field_names() {
        // Some versions may use cache_read_input_tokens instead of cached_input_tokens
        let json: serde_json::Value = serde_json::json!({
            "input_tokens": 1000,
            "cache_read_input_tokens": 500,
            "output_tokens": 200
        });
        let tokens = parse_cumulative_tokens(&json).unwrap();
        assert_eq!(tokens.cached_input, 500);
    }

    #[test]
    fn test_collect_codex_session_files_nonexistent() {
        let files = collect_codex_session_files(Path::new("/nonexistent/path"));
        assert!(files.is_empty());
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn test_parent_file_stamp_distinguishes_same_size_same_mtime_files() {
        let temp = tempdir().unwrap();
        let parent = rollout_path(temp.path(), PARENT_ID);
        let replacement = temp.path().join("replacement.jsonl");
        let values = [session_meta(PARENT_ID), token_count(100, 50, 10)];
        write_jsonl(&parent, &values);
        write_jsonl(&replacement, &values);
        let original_file = fs::File::open(&parent).unwrap();
        let original_metadata = original_file.metadata().unwrap();
        let replacement_file = fs::OpenOptions::new()
            .write(true)
            .open(&replacement)
            .unwrap();
        replacement_file
            .set_times(fs::FileTimes::new().set_modified(original_metadata.modified().unwrap()))
            .unwrap();
        let original_stamp = ParentFileStamp::from_file(&original_file).unwrap();
        let replacement_stamp = ParentFileStamp::from_file(&replacement_file).unwrap();
        assert_eq!(
            (original_stamp.size, original_stamp.modified_nanos),
            (replacement_stamp.size, replacement_stamp.modified_nanos)
        );
        assert_ne!(original_stamp, replacement_stamp);
    }

    #[test]
    fn test_archived_log_inherits_cursor_and_only_imports_appended_usage() -> Result<(), AppError> {
        let db = Database::memory()?;
        let temp = tempdir().unwrap();
        let sessions = temp.path().join("sessions");
        let archived = temp.path().join("archived_sessions");
        fs::create_dir_all(&sessions).unwrap();
        fs::create_dir_all(&archived).unwrap();
        let source = rollout_path(&sessions, PARENT_ID);
        let archived_file = rollout_path(&archived, PARENT_ID);
        write_jsonl(
            &archived_file,
            &[
                session_meta(PARENT_ID),
                turn_context(),
                token_count(100, 50, 10),
                token_count(200, 100, 20),
            ],
        );

        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model, request_model,
                    input_tokens, output_tokens, cache_read_tokens,
                    total_cost_usd, latency_ms, status_code, session_id,
                    created_at, data_source
                ) VALUES ('codex_session:parent:2', '_codex_session', 'codex',
                          'gpt-5.6-sol', 'gpt-5.6-sol', 999, 99, 0, '0', 0,
                          200, 'parent', 1, 'codex_session')",
                [],
            )?;
        }
        let source_path = source.to_string_lossy().to_string();
        update_sync_state(&db, &source_path, 1, 3)?;

        assert_eq!(
            sync_test_file(&db, &archived_file, &[&archived_file])?.imported,
            1
        );
        assert_eq!(
            sync_test_file(&db, &archived_file, &[&archived_file])?.imported,
            0
        );

        let conn = lock_conn!(db.conn);
        let old_row_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM proxy_request_logs
             WHERE request_id = 'codex_session:parent:2'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(old_row_count, 1);
        let usage: (i64, i64, i64) = conn.query_row(
            "SELECT input_tokens, cache_read_tokens, output_tokens
             FROM proxy_request_logs
             WHERE request_id = ?1",
            [format!("{CODEX_THREAD_REQUEST_ID_PREFIX}:{PARENT_ID}:2")],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(usage, (100, 50, 10));
        drop(conn);
        assert_eq!(get_sync_state(&db, &archived_file.to_string_lossy())?.1, 4);

        Ok(())
    }

    #[test]
    fn test_insert_codex_session_skips_matching_proxy_log() -> Result<(), AppError> {
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
                    "codex-proxy",
                    "openai",
                    "codex",
                    "gpt-5.4",
                    "gpt-5.4",
                    10,
                    2,
                    1,
                    7,
                    "0.01",
                    100,
                    200,
                    1000,
                    "proxy"
                ],
            )?;
        }

        let delta = DeltaTokens {
            input: 10,
            cached_input: 1,
            output: 2,
        };
        let mut suspected_duplicates = 0;
        let inserted = insert_codex_session_entry(
            &db,
            "codex-session-dup",
            &delta,
            "gpt-5.4",
            Some("session-1"),
            Some("1970-01-01T00:16:45Z"),
            &mut suspected_duplicates,
        )?;
        assert!(!inserted);

        let conn = lock_conn!(db.conn);
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM proxy_request_logs", [], |row| {
            row.get(0)
        })?;
        assert_eq!(count, 1);

        Ok(())
    }

    #[test]
    fn test_codex_session_duplicate_is_observed_but_still_inserted() -> Result<(), AppError> {
        let db = Database::memory()?;
        let delta = DeltaTokens {
            input: 10,
            cached_input: 1,
            output: 2,
        };
        let mut suspected_duplicates = 0;
        assert!(insert_codex_session_entry(
            &db,
            "codex-session-a",
            &delta,
            "gpt-5.4",
            Some("session-a"),
            Some("1970-01-01T00:16:40Z"),
            &mut suspected_duplicates,
        )?);
        assert!(insert_codex_session_entry(
            &db,
            "codex-session-b",
            &delta,
            "gpt-5.4",
            Some("session-b"),
            Some("1970-01-01T00:16:45Z"),
            &mut suspected_duplicates,
        )?);
        assert_eq!(suspected_duplicates, 1);

        let conn = lock_conn!(db.conn);
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM proxy_request_logs WHERE data_source = 'codex_session'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 2);
        Ok(())
    }

    #[test]
    fn reset_codex_usage_only_removes_codex_rows_and_structural_cursors() -> Result<(), AppError> {
        let db = Database::memory()?;
        let temp = tempdir().unwrap();
        let wide_dir = temp.path();
        let current_codex = rollout_path(&wide_dir.join("sessions"), CHILD_A_ID);
        let legacy_codex =
            format!("C:\\old-codex\\archived_sessions\\rollout-old-{CHILD_B_ID}.jsonl");
        let gemini_cursor = wide_dir.join("gemini/sessions/session-123.json");
        let claude_cursor = wide_dir.join(format!("projects/rollout-{PARENT_ID}.jsonl"));

        {
            let conn = lock_conn!(db.conn);
            conn.execute_batch(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model, input_tokens,
                    output_tokens, cache_read_tokens, latency_ms, status_code,
                    created_at, data_source
                 ) VALUES
                    ('codex-row', '_codex_session', 'codex', 'gpt', 1, 1, 0, 0, 200, 1, 'codex_session'),
                    ('gemini-row', '_gemini_session', 'gemini', 'gemini', 1, 1, 0, 0, 200, 1, 'gemini_session');
                 INSERT INTO usage_daily_rollups (date, app_type, provider_id, model)
                 VALUES
                    ('2026-07-10', 'codex', '_codex_session', 'gpt'),
                    ('2026-07-10', 'gemini', '_gemini_session', 'gemini');",
            )?;
            for path in [
                current_codex.to_string_lossy().to_string(),
                legacy_codex,
                gemini_cursor.to_string_lossy().to_string(),
                claude_cursor.to_string_lossy().to_string(),
            ] {
                conn.execute(
                    "INSERT INTO session_log_sync
                     (file_path, last_modified, last_line_offset, last_synced_at)
                     VALUES (?1, 1, 1, 1)",
                    [path],
                )?;
            }

            reset_codex_usage_on_conn(&conn, wide_dir)?;
            let codex_rows: i64 = conn.query_row(
                "SELECT COUNT(*) FROM proxy_request_logs WHERE data_source = 'codex_session'",
                [],
                |row| row.get(0),
            )?;
            let gemini_rows: i64 = conn.query_row(
                "SELECT COUNT(*) FROM proxy_request_logs WHERE data_source = 'gemini_session'",
                [],
                |row| row.get(0),
            )?;
            let codex_rollups: i64 = conn.query_row(
                "SELECT COUNT(*) FROM usage_daily_rollups WHERE provider_id = '_codex_session'",
                [],
                |row| row.get(0),
            )?;
            let remaining_cursors: i64 =
                conn.query_row("SELECT COUNT(*) FROM session_log_sync", [], |row| {
                    row.get(0)
                })?;
            assert_eq!((codex_rows, gemini_rows, codex_rollups), (0, 1, 0));
            assert_eq!(remaining_cursors, 2);
        }
        Ok(())
    }

    // -- model name normalization tests --

    #[test]
    fn test_normalize_codex_model_lowercase() {
        assert_eq!(normalize_codex_model("GLM-4.6"), "glm-4.6");
        assert_eq!(normalize_codex_model("DeepSeek-Chat"), "deepseek-chat");
        assert_eq!(normalize_codex_model("GPT-5.4"), "gpt-5.4");
    }

    #[test]
    fn test_normalize_codex_model_strip_prefix() {
        assert_eq!(normalize_codex_model("openai/gpt-5.4"), "gpt-5.4");
        assert_eq!(
            normalize_codex_model("azure/gpt-5.2-codex"),
            "gpt-5.2-codex"
        );
        assert_eq!(normalize_codex_model("OPENAI/GPT-5.4"), "gpt-5.4");
    }

    #[test]
    fn test_normalize_codex_model_strip_iso_date() {
        assert_eq!(normalize_codex_model("gpt-5.4-2026-03-05"), "gpt-5.4");
        assert_eq!(
            normalize_codex_model("gpt-5.4-pro-2026-03-05"),
            "gpt-5.4-pro"
        );
    }

    #[test]
    fn test_normalize_codex_model_strip_compact_date() {
        assert_eq!(normalize_codex_model("gpt-5.4-20260305"), "gpt-5.4");
        assert_eq!(
            normalize_codex_model("claude-opus-4-6-20260206"),
            "claude-opus-4-6"
        );
    }

    #[test]
    fn test_normalize_codex_model_no_change() {
        assert_eq!(normalize_codex_model("gpt-5.4"), "gpt-5.4");
        assert_eq!(normalize_codex_model("gpt-5.2-codex"), "gpt-5.2-codex");
        assert_eq!(normalize_codex_model("o3"), "o3");
        assert_eq!(normalize_codex_model("deepseek-chat"), "deepseek-chat");
    }

    #[test]
    fn test_normalize_codex_model_combined() {
        // prefix + uppercase + ISO date
        assert_eq!(
            normalize_codex_model("openai/GPT-5.4-2026-03-05"),
            "gpt-5.4"
        );
        // prefix + compact date
        assert_eq!(normalize_codex_model("openai/gpt-5.4-20260305"), "gpt-5.4");
    }

    #[test]
    fn test_cached_clamped_to_input() {
        // The abnormal cached > input case must be clamped by min()
        let prev = Some(CumulativeTokens {
            input: 100,
            cached_input: 0,
            output: 50,
        });
        let current = CumulativeTokens {
            input: 110,       // delta = 10
            cached_input: 80, // delta = 80 (abnormal: larger than the input delta)
            output: 60,
        };
        let delta = compute_delta(&prev, &current);
        // Before clamping: cached_input = 80, input = 10
        assert_eq!(delta.cached_input, 80);
        assert_eq!(delta.input, 10);
        // The actual clamp happens at the call site: delta.cached_input.min(delta.input)
        let clamped = delta.cached_input.min(delta.input);
        assert_eq!(clamped, 10);
    }
}
