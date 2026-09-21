//! Usage statistics service
//!
//! Provides aggregated queries over usage data

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::proxy::usage::calculator::ModelPricing;
use crate::services::sql_helpers::{
    fresh_input_sql, INPUT_TOKEN_SEMANTICS_FRESH, INPUT_TOKEN_SEMANTICS_TOTAL,
};
use chrono::{Local, NaiveDate, TimeZone, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::LazyLock;

/// Usage summary
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub total_requests: u64,
    pub total_cost: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub success_rate: f32,
    /// input + output + cache_creation + cache_read — the total tokens
    /// actually processed by the model (including cache hits). Used as the
    /// headline "real consumption" number in the usage hero.
    pub real_total_tokens: u64,
    /// cache_read / (input + cache_creation + cache_read). Range 0.0–1.0.
    /// Reported as a fraction; multiply by 100 in UI for percentage display.
    pub cache_hit_rate: f64,
}

/// Per-app-type usage summary used by the dashboard breakdown rail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummaryByApp {
    pub app_type: String,
    pub summary: UsageSummary,
}

/// Helper: compute (real_total, hit_rate) from the four token counters.
/// All inputs must already be cache-normalized (i.e. input excludes cache).
fn derive_real_total_and_hit_rate(
    fresh_input: u64,
    output: u64,
    cache_creation: u64,
    cache_read: u64,
) -> (u64, f64) {
    let real_total = fresh_input + output + cache_creation + cache_read;
    let cacheable_input = fresh_input + cache_creation + cache_read;
    let hit_rate = if cacheable_input > 0 {
        cache_read as f64 / cacheable_input as f64
    } else {
        0.0
    };
    (real_total, hit_rate)
}

/// Daily statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyStats {
    pub date: String,
    pub request_count: u64,
    pub total_cost: String,
    pub total_tokens: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
}

/// Provider statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStats {
    pub provider_id: String,
    pub provider_name: String,
    pub request_count: u64,
    pub total_tokens: u64,
    pub total_cost: String,
    pub success_rate: f32,
    pub avg_latency_ms: u64,
}

/// Model statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStats {
    pub model: String,
    pub request_count: u64,
    pub total_tokens: u64,
    pub total_cost: String,
    pub avg_cost_per_request: String,
}

/// Request log filters
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFilters {
    pub app_type: Option<String>,
    pub provider_name: Option<String>,
    pub model: Option<String>,
    pub status_code: Option<u16>,
    pub start_date: Option<i64>,
    pub end_date: Option<i64>,
}

/// Paginated request log response
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedLogs {
    pub data: Vec<RequestLogDetail>,
    pub total: u32,
    pub page: u32,
    pub page_size: u32,
}

/// Request log detail
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogDetail {
    pub request_id: String,
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<String>,
    pub app_type: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_model: Option<String>,
    pub cost_multiplier: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
    /// Internal storage semantics; omitted from the UI/API payload.
    #[serde(skip)]
    pub input_token_semantics: i64,
    pub input_cost_usd: String,
    pub output_cost_usd: String,
    pub cache_read_cost_usd: String,
    pub cache_creation_cost_usd: String,
    pub total_cost_usd: String,
    pub is_streaming: bool,
    pub latency_ms: u64,
    pub first_token_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub status_code: u16,
    pub error_message: Option<String>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source: Option<String>,
    /// Model name actually used for pricing at write time. None = a row from before v11, "" = an unpriced error row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_model: Option<String>,
}

/// Map a 26-column query result into `RequestLogDetail`.
///
/// The caller's SELECT **must** return these 26 columns in this order:
/// `request_id, provider_id, provider_name, app_type, model, request_model,
///  cost_multiplier, input_tokens, output_tokens, cache_read_tokens,
///  cache_creation_tokens, input_cost_usd, output_cost_usd, cache_read_cost_usd,
///  cache_creation_cost_usd, total_cost_usd, is_streaming, latency_ms,
///  first_token_ms, duration_ms, status_code, error_message, created_at,
///  data_source, pricing_model, input_token_semantics`
///
/// When provider_name is not needed (e.g. backfill) SELECT `NULL AS provider_name` as a placeholder.
fn row_to_request_log_detail(row: &rusqlite::Row<'_>) -> rusqlite::Result<RequestLogDetail> {
    Ok(RequestLogDetail {
        request_id: row.get(0)?,
        provider_id: row.get(1)?,
        provider_name: row.get(2)?,
        app_type: row.get(3)?,
        model: row.get(4)?,
        request_model: row.get(5)?,
        cost_multiplier: row
            .get::<_, Option<String>>(6)?
            .unwrap_or_else(|| "1".to_string()),
        input_tokens: row.get::<_, i64>(7)? as u32,
        output_tokens: row.get::<_, i64>(8)? as u32,
        cache_read_tokens: row.get::<_, i64>(9)? as u32,
        cache_creation_tokens: row.get::<_, i64>(10)? as u32,
        input_cost_usd: row.get(11)?,
        output_cost_usd: row.get(12)?,
        cache_read_cost_usd: row.get(13)?,
        cache_creation_cost_usd: row.get(14)?,
        total_cost_usd: row.get(15)?,
        is_streaming: row.get::<_, i64>(16)? != 0,
        latency_ms: row.get::<_, i64>(17)? as u64,
        first_token_ms: row.get::<_, Option<i64>>(18)?.map(|v| v as u64),
        duration_ms: row.get::<_, Option<i64>>(19)?.map(|v| v as u64),
        status_code: row.get::<_, i64>(20)? as u16,
        error_message: row.get(21)?,
        created_at: row.get(22)?,
        data_source: row.get(23)?,
        pricing_model: row.get(24)?,
        input_token_semantics: row.get::<_, i64>(25)?,
    })
}

/// SQL fragment: resolve provider_name with fallback for session-based entries.
/// Session logs use placeholder provider_ids (e.g., `_session`, `_<app>_session`)
/// that don't exist in the providers table — the CASE expression below is the
/// authoritative mapping from placeholder to readable name.
fn provider_name_coalesce(log_alias: &str, provider_alias: &str) -> String {
    format!(
        "COALESCE({provider_alias}.name, CASE {log_alias}.provider_id \
         WHEN '_session' THEN 'Claude (Session)' \
         WHEN '_codex_session' THEN 'Codex (Session)' \
         WHEN '_gemini_session' THEN 'Gemini (Session)' \
         WHEN '_opencode_session' THEN 'OpenCode (Session)' \
         WHEN '_grok_session' THEN 'Grok Build (Session)' \
         WHEN '_pi_session' THEN 'Pi (Session)' \
         ELSE {log_alias}.provider_id END)"
    )
}

pub(crate) const SESSION_PROXY_DEDUP_WINDOW_SECONDS: i64 = 10 * 60;

/// SQL fragment: wrap the `data_source` of the given alias in COALESCE, treating NULL as 'proxy'.
///
/// Guards against rows with a NULL data_source that schema versions before v9 could write (see
/// `tests::create_legacy_nullable_logs_table`). Every query touching data_source should build its
/// fragment through this helper so none is missed.
fn data_source_expr(log_alias: &str) -> String {
    format!("COALESCE({log_alias}.data_source, 'proxy')")
}

fn dedup_app_type_match_sql(left: &str, right: &str) -> String {
    format!(
        "{left} IN ({right}, CASE WHEN {right} = 'claude' THEN 'claude-desktop' ELSE {right} END)"
    )
}

/// SQL scalar expression: fold the `claude-desktop` app_type of the Claude Desktop gateway into
/// `claude` for **display purposes**, returning every other app_type unchanged.
///
/// Background: Desktop gateway traffic is recorded as `app_type='claude-desktop'` per entry point to
/// preserve billing audit precision for route takeover (do not regress this). But the Dashboard shows
/// it as Claude Code - it is literally the embedded Claude Code runtime inside the Desktop shell, and
/// Desktop chat usage never passes through this app, so a separate bucket would mislead users.
///
/// Usage: wrap any `app_type` column that participates in per-app filtering/grouping in this
/// expression, so a `= 'claude'` filter also matches `claude-desktop` and `GROUP BY` merges the two
/// without changing any stored row (the detail panel still reads the raw `app_type`).
///
/// Note: indexes on the wrapped column no longer apply to this comparison, but these are aggregate
/// scans that already filter by time and app_type is not the primary access path, so it is acceptable.
/// Read side only; cross-source dedup uses the narrower [`dedup_app_type_match_sql`], and quota checks (`check_provider_limits`) keep the exact comparison.
fn folded_app_type_sql(column: &str) -> String {
    format!("CASE WHEN {column} = 'claude-desktop' THEN 'claude' ELSE {column} END")
}

/// SQL fragment: LEFT JOIN a log/rollup row to the providers table to get the provider name.
/// `proxy_request_logs` and `usage_daily_rollups` have the same (provider_id, app_type) shape, so
/// either can be the `log_alias`. The providers primary key is (id, app_type), so the join is at most
/// 1:1 and never multiplies rows.
fn providers_join(log_alias: &str, provider_alias: &str) -> String {
    format!(
        "LEFT JOIN providers {provider_alias} \
         ON {log_alias}.provider_id = {provider_alias}.id \
         AND {log_alias}.app_type = {provider_alias}.app_type"
    )
}

/// SQL scalar expression: the row's "effective pricing model" - pricing_model wins when non-empty,
/// NULL/'' falls back to model. This is the grouping key of `get_model_stats` and the matching basis
/// of the Dashboard model filter: the filter values come from the model stats, so both sides must use the same expression.
fn effective_model_sql(alias: &str) -> String {
    format!("COALESCE(NULLIF({alias}.pricing_model, ''), {alias}.model)")
}

/// Append the Provider/model filters from the top of the Dashboard to the query conditions.
///
/// Provider matches the display name exactly (reusing [`provider_name_coalesce`], so readable names
/// of session placeholder rows such as "Claude (Session)" also match); the model matches [`effective_model_sql`].
/// Note: when `provider_name` is passed the caller must splice [`providers_join`] into the FROM,
/// otherwise `{provider_alias}.name` cannot be resolved.
fn push_provider_model_filters(
    conditions: &mut Vec<String>,
    params: &mut Vec<Box<dyn rusqlite::ToSql>>,
    log_alias: &str,
    provider_alias: &str,
    provider_name: Option<&str>,
    model: Option<&str>,
) {
    if let Some(name) = provider_name {
        conditions.push(format!(
            "{} = ?",
            provider_name_coalesce(log_alias, provider_alias)
        ));
        params.push(Box::new(name.to_string()));
    }
    if let Some(m) = model {
        conditions.push(format!("{} = ?", effective_model_sql(log_alias)));
        params.push(Box::new(m.to_string()));
    }
}

pub(crate) fn effective_usage_log_filter(log_alias: &str) -> String {
    let data_source = data_source_expr(log_alias);
    let proxy_data_source = data_source_expr("proxy_dedup");
    let app_type_match =
        dedup_app_type_match_sql("proxy_dedup.app_type", &format!("{log_alias}.app_type"));
    format!(
        "NOT (
            {data_source} IN ('session_log', 'codex_session', 'gemini_session', 'opencode_session')
            AND EXISTS (
                SELECT 1
                FROM proxy_request_logs proxy_dedup
                WHERE {proxy_data_source} = 'proxy'
                  AND {app_type_match}
                  AND proxy_dedup.status_code >= 200
                  AND proxy_dedup.status_code < 300
                  AND proxy_dedup.input_tokens = {log_alias}.input_tokens
                  AND proxy_dedup.output_tokens = {log_alias}.output_tokens
                  AND proxy_dedup.cache_read_tokens = {log_alias}.cache_read_tokens
                  AND (
                      proxy_dedup.cache_creation_tokens = {log_alias}.cache_creation_tokens
                      OR (
                          {log_alias}.cache_creation_tokens = 0
                          AND {data_source} IN ('codex_session', 'gemini_session', 'opencode_session')
                      )
                  )
                  AND proxy_dedup.created_at BETWEEN
                      {log_alias}.created_at - {SESSION_PROXY_DEDUP_WINDOW_SECONDS}
                      AND {log_alias}.created_at + {SESSION_PROXY_DEDUP_WINDOW_SECONDS}
                  AND (
                      LOWER(proxy_dedup.model) = LOWER({log_alias}.model)
                      OR LOWER(proxy_dedup.model) = 'unknown'
                      OR LOWER({log_alias}.model) = 'unknown'
                  )
            )
        )"
    )
}

/// Cross-source dedup fingerprint key.
///
/// `cache_creation_tokens`: Codex/Gemini session logs do not expose this field, so callers pass 0 to
/// mean "unknown" and the matcher then accepts any cache_creation_tokens value on the proxy side.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DedupKey<'a> {
    pub app_type: &'a str,
    pub model: &'a str,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
    pub created_at: i64,
}

/// Unified dedup decision before writing a session log.
///
/// The insert is skipped when either holds: (1) the `request_id` already exists; (2) a proxy log
/// matching `key` exists inside the time window (fingerprint dedup).
pub(crate) fn should_skip_session_insert(
    conn: &Connection,
    request_id: &str,
    key: &DedupKey,
) -> Result<bool, AppError> {
    if proxy_request_id_exists(conn, request_id)? {
        return Ok(true);
    }
    has_matching_proxy_usage_log(conn, key)
}

fn proxy_request_id_exists(conn: &Connection, request_id: &str) -> Result<bool, AppError> {
    conn.prepare_cached("SELECT EXISTS(SELECT 1 FROM proxy_request_logs WHERE request_id = ?1)")
        .and_then(|mut stmt| stmt.query_row(params![request_id], |row| row.get::<_, bool>(0)))
        .map_err(|e| AppError::Database(format!("Failed to query request_id: {e}")))
}

// A session re-import runs this query for every token event; keeping the SQL text static makes
// prepare_cached hit reliably and avoids a per-row format! allocation.
static MATCHING_PROXY_USAGE_LOG_SQL: LazyLock<String> = LazyLock::new(|| {
    let l_data_source = data_source_expr("l");
    let app_type_match = dedup_app_type_match_sql("l.app_type", "?1");
    format!(
        "SELECT EXISTS (
            SELECT 1
            FROM proxy_request_logs l
            WHERE {l_data_source} = 'proxy'
              AND {app_type_match}
              AND l.status_code >= 200
              AND l.status_code < 300
              AND l.input_tokens = ?3
              AND l.output_tokens = ?4
              AND l.cache_read_tokens = ?5
              AND (l.cache_creation_tokens = ?6 OR ?9 = 1)
              AND l.created_at BETWEEN ?7 - ?8 AND ?7 + ?8
              AND (
                  LOWER(l.model) = LOWER(?2)
                  OR LOWER(l.model) = 'unknown'
                  OR LOWER(?2) = 'unknown'
              )
        )"
    )
});

pub(crate) fn has_matching_proxy_usage_log(
    conn: &Connection,
    key: &DedupKey,
) -> Result<bool, AppError> {
    let allow_missing_cache_creation =
        matches!(key.app_type, "codex" | "gemini" | "opencode") && key.cache_creation_tokens == 0;

    conn.prepare_cached(&MATCHING_PROXY_USAGE_LOG_SQL)
        .and_then(|mut stmt| {
            stmt.query_row(
                params![
                    key.app_type,
                    key.model,
                    key.input_tokens as i64,
                    key.output_tokens as i64,
                    key.cache_read_tokens as i64,
                    key.cache_creation_tokens as i64,
                    key.created_at,
                    SESSION_PROXY_DEDUP_WINDOW_SECONDS,
                    allow_missing_cache_creation as i64,
                ],
                |row| row.get::<_, bool>(0),
            )
        })
        .map_err(|e| AppError::Database(format!("Failed to query duplicate proxy usage logs: {e}")))
}

/// Takeover activity guard for grokbuild session imports: if any grokbuild proxy row exists within
/// +/- the window around a given moment, takeover was active then and the session events must be
/// skipped entirely - the proxy already billed the same request per request, so booking it again on
/// the session side would double count.
///
/// The fingerprint matching of [`has_matching_proxy_usage_log`] is not reused: Grok session events are
/// per-turn aggregates whose token values are structurally unequal to per-request proxy rows, so the
/// fingerprint would never match. This is designed as "takeover detection" rather than "row matching",
/// hence status_code is not filtered - a failed proxy request still proves traffic goes through the proxy.
///
/// Known limitation (deliberate, conservative: it may miss but never double counts): the window has no
/// session dimension, so any grokbuild proxy row shadows every session event inside +/- the window -
/// when takeover and official are alternated or used in parallel within ten minutes, official turns are skipped (missed, not doubled).
pub(crate) fn has_recent_grokbuild_proxy_activity(
    conn: &Connection,
    created_at: i64,
) -> Result<bool, AppError> {
    let l_data_source = data_source_expr("l");
    let sql = format!(
        "SELECT EXISTS (
            SELECT 1
            FROM proxy_request_logs l
            WHERE {l_data_source} = 'proxy'
              AND l.app_type = 'grokbuild'
              AND l.created_at BETWEEN ?1 - ?2 AND ?1 + ?2
        )"
    );
    conn.query_row(
        &sql,
        params![created_at, SESSION_PROXY_DEDUP_WINDOW_SECONDS],
        |row| row.get::<_, bool>(0),
    )
    .map_err(|e| AppError::Database(format!("Failed to query Grok takeover activity: {e}")))
}

static SUSPECTED_CODEX_DUPLICATE_SQL: LazyLock<String> = LazyLock::new(|| {
    let data_source = data_source_expr("l");
    format!(
        "SELECT EXISTS (
            SELECT 1
            FROM proxy_request_logs l
            WHERE l.app_type = 'codex'
              AND {data_source} = 'codex_session'
              AND l.request_id <> ?1
              AND LOWER(l.model) = LOWER(?2)
              AND l.input_tokens = ?3
              AND l.output_tokens = ?4
              AND l.cache_read_tokens = ?5
              AND l.created_at BETWEEN ?6 - ?7 AND ?6 + ?7
        )"
    )
});

pub(crate) fn has_suspected_codex_session_duplicate(
    conn: &Connection,
    request_id: &str,
    key: &DedupKey,
) -> Result<bool, AppError> {
    conn.prepare_cached(&SUSPECTED_CODEX_DUPLICATE_SQL)
        .and_then(|mut stmt| {
            stmt.query_row(
                params![
                    request_id,
                    key.model,
                    key.input_tokens as i64,
                    key.output_tokens as i64,
                    key.cache_read_tokens as i64,
                    key.created_at,
                    SESSION_PROXY_DEDUP_WINDOW_SECONDS,
                ],
                |row| row.get::<_, bool>(0),
            )
        })
        .map_err(|error| {
            AppError::Database(format!(
                "Failed to query suspected duplicate Codex session usage: {error}"
            ))
        })
}

#[derive(Debug, Clone, Default)]
struct RollupDateBounds {
    start: Option<String>,
    end: Option<String>,
    is_empty: bool,
}

fn local_datetime_from_timestamp(ts: i64) -> Result<chrono::DateTime<Local>, AppError> {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .ok_or_else(|| AppError::Database(format!("Unable to resolve local timestamp: {ts}")))
}

fn compute_rollup_date_bounds(
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<RollupDateBounds, AppError> {
    let start = match start_ts {
        Some(ts) => {
            let local = local_datetime_from_timestamp(ts)?;
            let day = local.date_naive();
            if local.time().num_seconds_from_midnight() == 0 {
                Some(day.format("%Y-%m-%d").to_string())
            } else {
                day.succ_opt()
                    .map(|next| next.format("%Y-%m-%d").to_string())
            }
        }
        None => None,
    };

    let end = match end_ts {
        Some(ts) => {
            let local = local_datetime_from_timestamp(ts)?;
            let day = local.date_naive();
            if local.time().hour() == 23 && local.time().minute() == 59 {
                Some(day.format("%Y-%m-%d").to_string())
            } else {
                day.pred_opt()
                    .map(|prev| prev.format("%Y-%m-%d").to_string())
            }
        }
        None => None,
    };

    let is_empty = matches!((&start, &end), (Some(start), Some(end)) if start > end);

    Ok(RollupDateBounds {
        start,
        end,
        is_empty,
    })
}

fn push_rollup_date_filters(
    conditions: &mut Vec<String>,
    params: &mut Vec<Box<dyn rusqlite::ToSql>>,
    column: &str,
    bounds: &RollupDateBounds,
) {
    if bounds.is_empty {
        conditions.push("1 = 0".to_string());
        return;
    }

    if let Some(start) = &bounds.start {
        conditions.push(format!("{column} >= ?"));
        params.push(Box::new(start.clone()));
    }

    if let Some(end) = &bounds.end {
        conditions.push(format!("{column} <= ?"));
        params.push(Box::new(end.clone()));
    }
}

fn local_day_start_rfc3339(day: NaiveDate) -> String {
    let local_midnight = day
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| match Local.from_local_datetime(&naive) {
            chrono::LocalResult::Single(dt) => Some(dt),
            chrono::LocalResult::Ambiguous(earliest, _) => Some(earliest),
            chrono::LocalResult::None => None,
        })
        .unwrap_or_else(Local::now);

    local_midnight.to_rfc3339()
}

impl Database {
    /// Get the usage summary
    pub fn get_usage_summary(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<UsageSummary, AppError> {
        let conn = lock_conn!(self.conn);

        // Build detail WHERE clause
        let mut conditions = vec![effective_usage_log_filter("l")];
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(start) = start_date {
            conditions.push("l.created_at >= ?".to_string());
            params_vec.push(Box::new(start));
        }
        if let Some(end) = end_date {
            conditions.push("l.created_at <= ?".to_string());
            params_vec.push(Box::new(end));
        }
        if let Some(at) = app_type {
            conditions.push(format!("{} = ?", folded_app_type_sql("l.app_type")));
            params_vec.push(Box::new(at.to_string()));
        }
        push_provider_model_filters(
            &mut conditions,
            &mut params_vec,
            "l",
            "p",
            provider_name,
            model,
        );

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let detail_join = if provider_name.is_some() {
            providers_join("l", "p")
        } else {
            String::new()
        };

        // Only include rolled-up rows for full local days that are fully covered by the range.
        let mut rollup_conditions: Vec<String> = Vec::new();
        let mut rollup_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let rollup_bounds = compute_rollup_date_bounds(start_date, end_date)?;

        push_rollup_date_filters(
            &mut rollup_conditions,
            &mut rollup_params,
            "r.date",
            &rollup_bounds,
        );
        if let Some(at) = app_type {
            rollup_conditions.push(format!("{} = ?", folded_app_type_sql("r.app_type")));
            rollup_params.push(Box::new(at.to_string()));
        }
        push_provider_model_filters(
            &mut rollup_conditions,
            &mut rollup_params,
            "r",
            "p2",
            provider_name,
            model,
        );

        let rollup_where = if rollup_conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", rollup_conditions.join(" AND "))
        };
        let rollup_join = if provider_name.is_some() {
            providers_join("r", "p2")
        } else {
            String::new()
        };

        let fresh_input_detail = fresh_input_sql("l");
        let fresh_input_rollup = fresh_input_sql("r");
        let sql = format!(
            "SELECT
                COALESCE(d.total_requests, 0) + COALESCE(r.total_requests, 0),
                COALESCE(d.total_cost, 0) + COALESCE(r.total_cost, 0),
                COALESCE(d.total_input_tokens, 0) + COALESCE(r.total_input_tokens, 0),
                COALESCE(d.total_output_tokens, 0) + COALESCE(r.total_output_tokens, 0),
                COALESCE(d.total_cache_creation_tokens, 0) + COALESCE(r.total_cache_creation_tokens, 0),
                COALESCE(d.total_cache_read_tokens, 0) + COALESCE(r.total_cache_read_tokens, 0),
                COALESCE(d.success_count, 0) + COALESCE(r.success_count, 0)
            FROM
                (SELECT
                    COUNT(*) as total_requests,
                    COALESCE(SUM(CAST(l.total_cost_usd AS REAL)), 0) as total_cost,
                    COALESCE(SUM({fresh_input_detail}), 0) as total_input_tokens,
                    COALESCE(SUM(l.output_tokens), 0) as total_output_tokens,
                    COALESCE(SUM(l.cache_creation_tokens), 0) as total_cache_creation_tokens,
                    COALESCE(SUM(l.cache_read_tokens), 0) as total_cache_read_tokens,
                    COALESCE(SUM(CASE WHEN l.status_code >= 200 AND l.status_code < 300 THEN 1 ELSE 0 END), 0) as success_count
                 FROM proxy_request_logs l {detail_join} {where_clause}) d,
                (SELECT
                    COALESCE(SUM(r.request_count), 0) as total_requests,
                    COALESCE(SUM(CAST(r.total_cost_usd AS REAL)), 0) as total_cost,
                    COALESCE(SUM({fresh_input_rollup}), 0) as total_input_tokens,
                    COALESCE(SUM(r.output_tokens), 0) as total_output_tokens,
                    COALESCE(SUM(r.cache_creation_tokens), 0) as total_cache_creation_tokens,
                    COALESCE(SUM(r.cache_read_tokens), 0) as total_cache_read_tokens,
                    COALESCE(SUM(r.success_count), 0) as success_count
                 FROM usage_daily_rollups r {rollup_join} {rollup_where}) r"
        );

        // Combine params: detail params first, then rollup params
        let mut all_params: Vec<Box<dyn rusqlite::ToSql>> = params_vec;
        all_params.extend(rollup_params);
        let param_refs: Vec<&dyn rusqlite::ToSql> = all_params.iter().map(|p| p.as_ref()).collect();

        let result = conn.query_row(&sql, param_refs.as_slice(), |row| {
            let total_requests: i64 = row.get(0)?;
            let total_cost: f64 = row.get(1)?;
            let total_input_tokens: i64 = row.get(2)?;
            let total_output_tokens: i64 = row.get(3)?;
            let total_cache_creation_tokens: i64 = row.get(4)?;
            let total_cache_read_tokens: i64 = row.get(5)?;
            let success_count: i64 = row.get(6)?;

            let success_rate = if total_requests > 0 {
                (success_count as f32 / total_requests as f32) * 100.0
            } else {
                0.0
            };

            let (real_total_tokens, cache_hit_rate) = derive_real_total_and_hit_rate(
                total_input_tokens as u64,
                total_output_tokens as u64,
                total_cache_creation_tokens as u64,
                total_cache_read_tokens as u64,
            );

            Ok(UsageSummary {
                total_requests: total_requests as u64,
                total_cost: format!("{total_cost:.6}"),
                total_input_tokens: total_input_tokens as u64,
                total_output_tokens: total_output_tokens as u64,
                total_cache_creation_tokens: total_cache_creation_tokens as u64,
                total_cache_read_tokens: total_cache_read_tokens as u64,
                success_rate,
                real_total_tokens,
                cache_hit_rate,
            })
        })?;

        Ok(result)
    }

    /// Usage summary split by app_type, used by the per-app bar of the Dashboard.
    /// Returns every app_type with data, ordered by real_total_tokens descending.
    ///
    /// Single SQL with `GROUP BY app_type` — avoids the N+1 round-trip that
    /// would result from invoking `get_usage_summary` once per app_type.
    pub fn get_usage_summary_by_app(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<UsageSummaryByApp>, AppError> {
        let conn = lock_conn!(self.conn);

        let mut detail_conditions = vec![effective_usage_log_filter("l")];
        let mut detail_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(start) = start_date {
            detail_conditions.push("l.created_at >= ?".to_string());
            detail_params.push(Box::new(start));
        }
        if let Some(end) = end_date {
            detail_conditions.push("l.created_at <= ?".to_string());
            detail_params.push(Box::new(end));
        }
        push_provider_model_filters(
            &mut detail_conditions,
            &mut detail_params,
            "l",
            "p",
            provider_name,
            model,
        );
        let detail_where = format!("WHERE {}", detail_conditions.join(" AND "));
        let detail_join = if provider_name.is_some() {
            providers_join("l", "p")
        } else {
            String::new()
        };

        let rollup_bounds = compute_rollup_date_bounds(start_date, end_date)?;
        let mut rollup_conditions: Vec<String> = Vec::new();
        let mut rollup_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        push_rollup_date_filters(
            &mut rollup_conditions,
            &mut rollup_params,
            "r.date",
            &rollup_bounds,
        );
        push_provider_model_filters(
            &mut rollup_conditions,
            &mut rollup_params,
            "r",
            "p2",
            provider_name,
            model,
        );
        let rollup_where = if rollup_conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", rollup_conditions.join(" AND "))
        };
        let rollup_join = if provider_name.is_some() {
            providers_join("r", "p2")
        } else {
            String::new()
        };

        let fresh_input_detail = fresh_input_sql("l");
        let fresh_input_rollup = fresh_input_sql("r");
        // Fold claude-desktop -> claude: the inner projection uses the same bucket name and the outer GROUP BY merges them.
        let detail_app_type = folded_app_type_sql("l.app_type");
        let rollup_app_type = folded_app_type_sql("r.app_type");

        let sql = format!(
            "SELECT app_type,
                SUM(req_count) as req_count,
                SUM(cost) as cost,
                SUM(input_t) as input_t,
                SUM(output_t) as output_t,
                SUM(cache_create_t) as cache_create_t,
                SUM(cache_read_t) as cache_read_t,
                SUM(success_count) as success_count
            FROM (
                SELECT {detail_app_type} as app_type,
                    COUNT(*) as req_count,
                    COALESCE(SUM(CAST(l.total_cost_usd AS REAL)), 0) as cost,
                    COALESCE(SUM({fresh_input_detail}), 0) as input_t,
                    COALESCE(SUM(l.output_tokens), 0) as output_t,
                    COALESCE(SUM(l.cache_creation_tokens), 0) as cache_create_t,
                    COALESCE(SUM(l.cache_read_tokens), 0) as cache_read_t,
                    COALESCE(SUM(CASE WHEN l.status_code >= 200 AND l.status_code < 300 THEN 1 ELSE 0 END), 0) as success_count
                FROM proxy_request_logs l {detail_join} {detail_where}
                GROUP BY l.app_type
                UNION ALL
                SELECT {rollup_app_type} as app_type,
                    COALESCE(SUM(r.request_count), 0),
                    COALESCE(SUM(CAST(r.total_cost_usd AS REAL)), 0),
                    COALESCE(SUM({fresh_input_rollup}), 0),
                    COALESCE(SUM(r.output_tokens), 0),
                    COALESCE(SUM(r.cache_creation_tokens), 0),
                    COALESCE(SUM(r.cache_read_tokens), 0),
                    COALESCE(SUM(r.success_count), 0)
                FROM usage_daily_rollups r {rollup_join} {rollup_where}
                GROUP BY r.app_type
            )
            GROUP BY app_type"
        );

        let mut combined: Vec<Box<dyn rusqlite::ToSql>> = detail_params;
        combined.extend(rollup_params);
        let refs: Vec<&dyn rusqlite::ToSql> = combined.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(refs.as_slice(), |row| {
            let app_type: String = row.get(0)?;
            let total_requests: i64 = row.get(1)?;
            let total_cost: f64 = row.get(2)?;
            let total_input_tokens: i64 = row.get(3)?;
            let total_output_tokens: i64 = row.get(4)?;
            let total_cache_creation_tokens: i64 = row.get(5)?;
            let total_cache_read_tokens: i64 = row.get(6)?;
            let success_count: i64 = row.get(7)?;

            let success_rate = if total_requests > 0 {
                (success_count as f32 / total_requests as f32) * 100.0
            } else {
                0.0
            };
            let (real_total_tokens, cache_hit_rate) = derive_real_total_and_hit_rate(
                total_input_tokens as u64,
                total_output_tokens as u64,
                total_cache_creation_tokens as u64,
                total_cache_read_tokens as u64,
            );

            Ok(UsageSummaryByApp {
                app_type,
                summary: UsageSummary {
                    total_requests: total_requests as u64,
                    total_cost: format!("{total_cost:.6}"),
                    total_input_tokens: total_input_tokens as u64,
                    total_output_tokens: total_output_tokens as u64,
                    total_cache_creation_tokens: total_cache_creation_tokens as u64,
                    total_cache_read_tokens: total_cache_read_tokens as u64,
                    success_rate,
                    real_total_tokens,
                    cache_hit_rate,
                },
            })
        })?;

        let mut summaries = Vec::new();
        for row in rows {
            let item = row?;
            if item.summary.total_requests == 0 && item.summary.real_total_tokens == 0 {
                continue;
            }
            summaries.push(item);
        }
        summaries.sort_by(|a, b| {
            b.summary
                .real_total_tokens
                .cmp(&a.summary.real_total_tokens)
        });
        Ok(summaries)
    }

    /// Get daily trends (sliding window, <=24h hourly, >24h daily, same window as the summary)
    pub fn get_daily_trends(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<DailyStats>, AppError> {
        let conn = lock_conn!(self.conn);

        let end_ts = end_date.unwrap_or_else(|| Local::now().timestamp());
        let mut start_ts = start_date.unwrap_or_else(|| end_ts - 24 * 60 * 60);

        if start_ts >= end_ts {
            start_ts = end_ts - 24 * 60 * 60;
        }

        let duration = end_ts - start_ts;
        if duration <= 24 * 60 * 60 {
            let bucket_seconds: i64 = 60 * 60;
            let mut bucket_count: i64 = if duration <= 0 {
                1
            } else {
                (duration + bucket_seconds - 1) / bucket_seconds
            };

            if bucket_count < 1 {
                bucket_count = 1;
            }

            let mut extra_conditions: Vec<String> = Vec::new();
            let mut extra_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            if let Some(at) = app_type {
                extra_conditions.push(format!("{} = ?", folded_app_type_sql("l.app_type")));
                extra_params.push(Box::new(at.to_string()));
            }
            push_provider_model_filters(
                &mut extra_conditions,
                &mut extra_params,
                "l",
                "p",
                provider_name,
                model,
            );
            let extra_filter = extra_conditions
                .iter()
                .map(|c| format!("AND {c}"))
                .collect::<Vec<_>>()
                .join(" ");
            let detail_join = if provider_name.is_some() {
                providers_join("l", "p")
            } else {
                String::new()
            };

            let effective_filter = effective_usage_log_filter("l");
            let fresh_input = fresh_input_sql("l");
            let sql = format!(
                "SELECT
                    CAST((l.created_at - ?1) / ?3 AS INTEGER) as bucket_idx,
                    COUNT(*) as request_count,
                    COALESCE(SUM(CAST(l.total_cost_usd AS REAL)), 0) as total_cost,
                    COALESCE(SUM({fresh_input} + l.output_tokens), 0) as total_tokens,
                    COALESCE(SUM({fresh_input}), 0) as total_input_tokens,
                    COALESCE(SUM(l.output_tokens), 0) as total_output_tokens,
                    COALESCE(SUM(l.cache_creation_tokens), 0) as total_cache_creation_tokens,
                    COALESCE(SUM(l.cache_read_tokens), 0) as total_cache_read_tokens
                FROM proxy_request_logs l {detail_join}
                WHERE l.created_at >= ?1 AND l.created_at <= ?2
                  AND {effective_filter} {extra_filter}
                GROUP BY bucket_idx
                ORDER BY bucket_idx ASC"
            );

            let mut stmt = conn.prepare(&sql)?;
            let row_mapper = |row: &rusqlite::Row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    DailyStats {
                        date: String::new(),
                        request_count: row.get::<_, i64>(1)? as u64,
                        total_cost: format!("{:.6}", row.get::<_, f64>(2)?),
                        total_tokens: row.get::<_, i64>(3)? as u64,
                        total_input_tokens: row.get::<_, i64>(4)? as u64,
                        total_output_tokens: row.get::<_, i64>(5)? as u64,
                        total_cache_creation_tokens: row.get::<_, i64>(6)? as u64,
                        total_cache_read_tokens: row.get::<_, i64>(7)? as u64,
                    },
                ))
            };

            let mut map: HashMap<i64, DailyStats> = HashMap::new();

            let mut all_params: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(start_ts),
                Box::new(end_ts),
                Box::new(bucket_seconds),
            ];
            all_params.extend(extra_params);
            let param_refs: Vec<&dyn rusqlite::ToSql> =
                all_params.iter().map(|p| p.as_ref()).collect();
            let rows = stmt.query_map(param_refs.as_slice(), row_mapper)?;
            for row in rows {
                let (mut bucket_idx, stat) = row?;
                if bucket_idx < 0 {
                    continue;
                }
                if bucket_idx >= bucket_count {
                    bucket_idx = bucket_count - 1;
                }
                map.insert(bucket_idx, stat);
            }

            let mut stats = Vec::with_capacity(bucket_count as usize);
            for i in 0..bucket_count {
                let bucket_start_ts = start_ts + i * bucket_seconds;
                let bucket_start = local_datetime_from_timestamp(bucket_start_ts)?;
                let date = bucket_start.to_rfc3339();

                if let Some(mut stat) = map.remove(&i) {
                    stat.date = date;
                    stats.push(stat);
                } else {
                    stats.push(DailyStats {
                        date,
                        request_count: 0,
                        total_cost: "0.000000".to_string(),
                        total_tokens: 0,
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                        total_cache_creation_tokens: 0,
                        total_cache_read_tokens: 0,
                    });
                }
            }

            return Ok(stats);
        }

        let start_day = local_datetime_from_timestamp(start_ts)?.date_naive();
        let end_day = local_datetime_from_timestamp(end_ts)?.date_naive();
        let bucket_count = (end_day.signed_duration_since(start_day).num_days() + 1) as usize;

        let mut extra_conditions: Vec<String> = Vec::new();
        let mut extra_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(at) = app_type {
            extra_conditions.push(format!("{} = ?", folded_app_type_sql("l.app_type")));
            extra_params.push(Box::new(at.to_string()));
        }
        push_provider_model_filters(
            &mut extra_conditions,
            &mut extra_params,
            "l",
            "p",
            provider_name,
            model,
        );
        let extra_filter = extra_conditions
            .iter()
            .map(|c| format!("AND {c}"))
            .collect::<Vec<_>>()
            .join(" ");
        let detail_join = if provider_name.is_some() {
            providers_join("l", "p")
        } else {
            String::new()
        };

        let effective_filter = effective_usage_log_filter("l");
        let fresh_input = fresh_input_sql("l");
        let detail_sql = format!(
            "SELECT
                date(l.created_at, 'unixepoch', 'localtime') as bucket_date,
                COUNT(*) as request_count,
                COALESCE(SUM(CAST(l.total_cost_usd AS REAL)), 0) as total_cost,
                COALESCE(SUM({fresh_input} + l.output_tokens), 0) as total_tokens,
                COALESCE(SUM({fresh_input}), 0) as total_input_tokens,
                COALESCE(SUM(l.output_tokens), 0) as total_output_tokens,
                COALESCE(SUM(l.cache_creation_tokens), 0) as total_cache_creation_tokens,
                COALESCE(SUM(l.cache_read_tokens), 0) as total_cache_read_tokens
            FROM proxy_request_logs l {detail_join}
            WHERE l.created_at >= ?1 AND l.created_at <= ?2
              AND {effective_filter} {extra_filter}
            GROUP BY bucket_date
            ORDER BY bucket_date ASC"
        );

        let mut detail_stmt = conn.prepare(&detail_sql)?;
        let detail_row_mapper = |row: &rusqlite::Row| {
            Ok((
                row.get::<_, String>(0)?,
                DailyStats {
                    date: String::new(),
                    request_count: row.get::<_, i64>(1)? as u64,
                    total_cost: format!("{:.6}", row.get::<_, f64>(2)?),
                    total_tokens: row.get::<_, i64>(3)? as u64,
                    total_input_tokens: row.get::<_, i64>(4)? as u64,
                    total_output_tokens: row.get::<_, i64>(5)? as u64,
                    total_cache_creation_tokens: row.get::<_, i64>(6)? as u64,
                    total_cache_read_tokens: row.get::<_, i64>(7)? as u64,
                },
            ))
        };

        let mut map: HashMap<NaiveDate, DailyStats> = HashMap::new();
        let mut detail_all_params: Vec<Box<dyn rusqlite::ToSql>> =
            vec![Box::new(start_ts), Box::new(end_ts)];
        detail_all_params.extend(extra_params);
        let detail_param_refs: Vec<&dyn rusqlite::ToSql> =
            detail_all_params.iter().map(|p| p.as_ref()).collect();
        let detail_rows = detail_stmt.query_map(detail_param_refs.as_slice(), detail_row_mapper)?;

        for row in detail_rows {
            let (bucket_date, stat) = row?;
            let date = NaiveDate::parse_from_str(&bucket_date, "%Y-%m-%d").map_err(|err| {
                AppError::Database(format!("Failed to parse the trend date: {err}"))
            })?;
            map.insert(date, stat);
        }

        let rollup_bounds = compute_rollup_date_bounds(Some(start_ts), Some(end_ts))?;
        let mut rollup_conditions = Vec::new();
        let mut rollup_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        push_rollup_date_filters(
            &mut rollup_conditions,
            &mut rollup_params,
            "r.date",
            &rollup_bounds,
        );
        if let Some(at) = app_type {
            rollup_conditions.push(format!("{} = ?", folded_app_type_sql("r.app_type")));
            rollup_params.push(Box::new(at.to_string()));
        }
        push_provider_model_filters(
            &mut rollup_conditions,
            &mut rollup_params,
            "r",
            "p2",
            provider_name,
            model,
        );

        let rollup_where = if rollup_conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", rollup_conditions.join(" AND "))
        };
        let rollup_join = if provider_name.is_some() {
            providers_join("r", "p2")
        } else {
            String::new()
        };

        let fresh_input_rollup = fresh_input_sql("r");
        let rollup_sql = format!(
            "SELECT
                r.date,
                COALESCE(SUM(r.request_count), 0),
                COALESCE(SUM(CAST(r.total_cost_usd AS REAL)), 0),
                COALESCE(SUM({fresh_input_rollup} + r.output_tokens), 0),
                COALESCE(SUM({fresh_input_rollup}), 0),
                COALESCE(SUM(r.output_tokens), 0),
                COALESCE(SUM(r.cache_creation_tokens), 0),
                COALESCE(SUM(r.cache_read_tokens), 0)
            FROM usage_daily_rollups r {rollup_join}
            {rollup_where}
            GROUP BY r.date
            ORDER BY r.date ASC"
        );

        let mut rollup_stmt = conn.prepare(&rollup_sql)?;
        let rollup_row_mapper = |row: &rusqlite::Row| {
            Ok((
                row.get::<_, String>(0)?,
                (
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, f64>(2)?,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, i64>(4)? as u64,
                    row.get::<_, i64>(5)? as u64,
                    row.get::<_, i64>(6)? as u64,
                    row.get::<_, i64>(7)? as u64,
                ),
            ))
        };
        let rollup_param_refs: Vec<&dyn rusqlite::ToSql> =
            rollup_params.iter().map(|param| param.as_ref()).collect();
        let rollup_rows = rollup_stmt.query_map(rollup_param_refs.as_slice(), rollup_row_mapper)?;

        for row in rollup_rows {
            let (bucket_date, (req, cost, tok, inp, out, cc, cr)) = row?;
            let date = NaiveDate::parse_from_str(&bucket_date, "%Y-%m-%d").map_err(|err| {
                AppError::Database(format!("Failed to parse the rollup trend date: {err}"))
            })?;
            let entry = map.entry(date).or_insert_with(|| DailyStats {
                date: String::new(),
                request_count: 0,
                total_cost: "0.000000".to_string(),
                total_tokens: 0,
                total_input_tokens: 0,
                total_output_tokens: 0,
                total_cache_creation_tokens: 0,
                total_cache_read_tokens: 0,
            });
            entry.request_count += req;
            let existing_cost: f64 = entry.total_cost.parse().unwrap_or(0.0);
            entry.total_cost = format!("{:.6}", existing_cost + cost);
            entry.total_tokens += tok;
            entry.total_input_tokens += inp;
            entry.total_output_tokens += out;
            entry.total_cache_creation_tokens += cc;
            entry.total_cache_read_tokens += cr;
        }

        let mut stats = Vec::with_capacity(bucket_count);
        let mut current_day = start_day;
        for _ in 0..bucket_count {
            let date = local_day_start_rfc3339(current_day);

            if let Some(mut stat) = map.remove(&current_day) {
                stat.date = date;
                stats.push(stat);
            } else {
                stats.push(DailyStats {
                    date,
                    request_count: 0,
                    total_cost: "0.000000".to_string(),
                    total_tokens: 0,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    total_cache_creation_tokens: 0,
                    total_cache_read_tokens: 0,
                });
            }

            current_day = current_day.succ_opt().unwrap_or(current_day);
        }

        Ok(stats)
    }

    /// Get provider statistics
    pub fn get_provider_stats(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<ProviderStats>, AppError> {
        let conn = lock_conn!(self.conn);

        let mut detail_conditions = vec![effective_usage_log_filter("l")];
        let mut detail_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(start) = start_date {
            detail_conditions.push("l.created_at >= ?".to_string());
            detail_params.push(Box::new(start));
        }
        if let Some(end) = end_date {
            detail_conditions.push("l.created_at <= ?".to_string());
            detail_params.push(Box::new(end));
        }
        if let Some(at) = app_type {
            detail_conditions.push(format!("{} = ?", folded_app_type_sql("l.app_type")));
            detail_params.push(Box::new(at.to_string()));
        }
        push_provider_model_filters(
            &mut detail_conditions,
            &mut detail_params,
            "l",
            "p",
            provider_name,
            model,
        );
        let detail_where = if detail_conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", detail_conditions.join(" AND "))
        };

        let mut rollup_conditions = Vec::new();
        let mut rollup_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let rollup_bounds = compute_rollup_date_bounds(start_date, end_date)?;
        push_rollup_date_filters(
            &mut rollup_conditions,
            &mut rollup_params,
            "r.date",
            &rollup_bounds,
        );
        if let Some(at) = app_type {
            rollup_conditions.push(format!("{} = ?", folded_app_type_sql("r.app_type")));
            rollup_params.push(Box::new(at.to_string()));
        }
        push_provider_model_filters(
            &mut rollup_conditions,
            &mut rollup_params,
            "r",
            "p2",
            provider_name,
            model,
        );
        let rollup_where = if rollup_conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", rollup_conditions.join(" AND "))
        };

        // UNION detail logs + rollup data, then aggregate
        let detail_pname = provider_name_coalesce("l", "p");
        let rollup_pname = provider_name_coalesce("r", "p2");
        let fresh_input_detail = fresh_input_sql("l");
        let fresh_input_rollup = fresh_input_sql("r");
        let sql = format!(
            "SELECT
                provider_id, app_type, provider_name,
                SUM(request_count) as request_count,
                SUM(total_tokens) as total_tokens,
                SUM(total_cost) as total_cost,
                SUM(success_count) as success_count,
                CASE WHEN SUM(request_count) > 0
                    THEN SUM(latency_sum) / SUM(request_count)
                    ELSE 0 END as avg_latency
            FROM (
                SELECT l.provider_id, l.app_type,
                    {detail_pname} as provider_name,
                    COUNT(*) as request_count,
                    COALESCE(SUM({fresh_input_detail} + l.output_tokens), 0) as total_tokens,
                    COALESCE(SUM(CAST(l.total_cost_usd AS REAL)), 0) as total_cost,
                    COALESCE(SUM(CASE WHEN l.status_code >= 200 AND l.status_code < 300 THEN 1 ELSE 0 END), 0) as success_count,
                    COALESCE(SUM(l.latency_ms), 0) as latency_sum
                FROM proxy_request_logs l
                LEFT JOIN providers p ON l.provider_id = p.id AND l.app_type = p.app_type
                {detail_where}
                GROUP BY l.provider_id, l.app_type
                UNION ALL
                SELECT r.provider_id, r.app_type,
                    {rollup_pname} as provider_name,
                    COALESCE(SUM(r.request_count), 0),
                    COALESCE(SUM({fresh_input_rollup} + r.output_tokens), 0),
                    COALESCE(SUM(CAST(r.total_cost_usd AS REAL)), 0),
                    COALESCE(SUM(r.success_count), 0),
                    COALESCE(SUM(r.avg_latency_ms * r.request_count), 0)
                FROM usage_daily_rollups r
                LEFT JOIN providers p2 ON r.provider_id = p2.id AND r.app_type = p2.app_type
                {rollup_where}
                GROUP BY r.provider_id, r.app_type
            )
            GROUP BY provider_id, app_type
            ORDER BY total_cost DESC"
        );

        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = detail_params;
        params.extend(rollup_params);
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let row_mapper = |row: &rusqlite::Row| {
            let request_count: i64 = row.get(3)?;
            let success_count: i64 = row.get(6)?;
            let success_rate = if request_count > 0 {
                (success_count as f32 / request_count as f32) * 100.0
            } else {
                0.0
            };

            Ok(ProviderStats {
                provider_id: row.get(0)?,
                provider_name: row.get(2)?,
                request_count: request_count as u64,
                total_tokens: row.get::<_, i64>(4)? as u64,
                total_cost: format!("{:.6}", row.get::<_, f64>(5)?),
                success_rate,
                avg_latency_ms: row.get::<_, f64>(7)? as u64,
            })
        };

        let rows = stmt.query_map(param_refs.as_slice(), row_mapper)?;

        let mut stats = Vec::new();
        for row in rows {
            stats.push(row?);
        }

        Ok(stats)
    }

    /// Get model statistics
    pub fn get_model_stats(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<ModelStats>, AppError> {
        let conn = lock_conn!(self.conn);

        let mut detail_conditions = vec![effective_usage_log_filter("l")];
        let mut detail_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(start) = start_date {
            detail_conditions.push("l.created_at >= ?".to_string());
            detail_params.push(Box::new(start));
        }
        if let Some(end) = end_date {
            detail_conditions.push("l.created_at <= ?".to_string());
            detail_params.push(Box::new(end));
        }
        if let Some(at) = app_type {
            detail_conditions.push(format!("{} = ?", folded_app_type_sql("l.app_type")));
            detail_params.push(Box::new(at.to_string()));
        }
        push_provider_model_filters(
            &mut detail_conditions,
            &mut detail_params,
            "l",
            "p",
            provider_name,
            model,
        );
        let detail_where = if detail_conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", detail_conditions.join(" AND "))
        };
        let detail_join = if provider_name.is_some() {
            providers_join("l", "p")
        } else {
            String::new()
        };

        let mut rollup_conditions = Vec::new();
        let mut rollup_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let rollup_bounds = compute_rollup_date_bounds(start_date, end_date)?;
        push_rollup_date_filters(
            &mut rollup_conditions,
            &mut rollup_params,
            "r.date",
            &rollup_bounds,
        );
        if let Some(at) = app_type {
            rollup_conditions.push(format!("{} = ?", folded_app_type_sql("r.app_type")));
            rollup_params.push(Box::new(at.to_string()));
        }
        push_provider_model_filters(
            &mut rollup_conditions,
            &mut rollup_params,
            "r",
            "p2",
            provider_name,
            model,
        );
        let rollup_where = if rollup_conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", rollup_conditions.join(" AND "))
        };
        let rollup_join = if provider_name.is_some() {
            providers_join("r", "p2")
        } else {
            String::new()
        };

        // UNION detail logs + rollup data
        //
        // The grouping key is the "effective pricing model": pricing_model wins when non-empty (cost was
        // computed from its price, so amounts stay consistent with the pricing table), NULL/'' falls back
        // to model. Under the default response pricing mode both are identical and behaviour is unchanged;
        // under request mode + route takeover the money is booked under the real pricing basis instead of the upstream echo / client alias.
        let fresh_input_detail = fresh_input_sql("l");
        let fresh_input_rollup = fresh_input_sql("r");
        let detail_model = effective_model_sql("l");
        let rollup_model = effective_model_sql("r");
        let sql = format!(
            "SELECT
                model,
                SUM(request_count) as request_count,
                SUM(total_tokens) as total_tokens,
                SUM(total_cost) as total_cost
            FROM (
                SELECT {detail_model} as model,
                    COUNT(*) as request_count,
                    COALESCE(SUM({fresh_input_detail} + l.output_tokens), 0) as total_tokens,
                    COALESCE(SUM(CAST(l.total_cost_usd AS REAL)), 0) as total_cost
                FROM proxy_request_logs l
                {detail_join}
                {detail_where}
                GROUP BY {detail_model}
                UNION ALL
                SELECT {rollup_model},
                    COALESCE(SUM(r.request_count), 0),
                    COALESCE(SUM({fresh_input_rollup} + r.output_tokens), 0),
                    COALESCE(SUM(CAST(r.total_cost_usd AS REAL)), 0)
                FROM usage_daily_rollups r
                {rollup_join}
                {rollup_where}
                GROUP BY {rollup_model}
            )
            GROUP BY model
            ORDER BY total_cost DESC"
        );

        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = detail_params;
        params.extend(rollup_params);
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let row_mapper = |row: &rusqlite::Row| {
            let request_count: i64 = row.get(1)?;
            let total_cost: f64 = row.get(3)?;
            let avg_cost = if request_count > 0 {
                total_cost / request_count as f64
            } else {
                0.0
            };

            Ok(ModelStats {
                model: row.get(0)?,
                request_count: request_count as u64,
                total_tokens: row.get::<_, i64>(2)? as u64,
                total_cost: format!("{total_cost:.6}"),
                avg_cost_per_request: format!("{avg_cost:.6}"),
            })
        };

        let rows = stmt.query_map(param_refs.as_slice(), row_mapper)?;

        let mut stats = Vec::new();
        for row in rows {
            stats.push(row?);
        }

        Ok(stats)
    }

    /// Get the request log list (paginated)
    pub fn get_request_logs(
        &self,
        filters: &LogFilters,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedLogs, AppError> {
        let conn = lock_conn!(self.conn);

        let mut conditions = vec![effective_usage_log_filter("l")];
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref app_type) = filters.app_type {
            // Only the filter side folds claude-desktop -> claude; the row projection still returns the
            // raw app_type so the detail panel shows the real entry point (needed for takeover billing audits).
            conditions.push(format!("{} = ?", folded_app_type_sql("l.app_type")));
            params.push(Box::new(app_type.clone()));
        }
        // Same basis as the Dashboard dropdown filters: Provider matches the display name exactly
        // (session placeholder rows such as "Claude (Session)" match too), the model matches the effective pricing model.
        push_provider_model_filters(
            &mut conditions,
            &mut params,
            "l",
            "p",
            filters.provider_name.as_deref(),
            filters.model.as_deref(),
        );
        if let Some(status) = filters.status_code {
            conditions.push("l.status_code = ?".to_string());
            params.push(Box::new(status as i64));
        }
        if let Some(start) = filters.start_date {
            conditions.push("l.created_at >= ?".to_string());
            params.push(Box::new(start));
        }
        if let Some(end) = filters.end_date {
            conditions.push("l.created_at <= ?".to_string());
            params.push(Box::new(end));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Get the total count
        let count_sql = format!(
            "SELECT COUNT(*) FROM proxy_request_logs l
             LEFT JOIN providers p ON l.provider_id = p.id AND l.app_type = p.app_type
             {where_clause}"
        );
        let count_params: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let total: u32 = conn.query_row(&count_sql, count_params.as_slice(), |row| {
            row.get::<_, i64>(0).map(|v| v as u32)
        })?;

        // Get the data
        let offset = page * page_size;
        params.push(Box::new(page_size as i64));
        params.push(Box::new(offset as i64));

        let logs_pname = provider_name_coalesce("l", "p");
        let sql = format!(
            "SELECT l.request_id, l.provider_id, {logs_pname} as provider_name, l.app_type, l.model,
                    l.request_model, l.cost_multiplier,
                    l.input_tokens, l.output_tokens, l.cache_read_tokens, l.cache_creation_tokens,
                    l.input_cost_usd, l.output_cost_usd, l.cache_read_cost_usd, l.cache_creation_cost_usd, l.total_cost_usd,
                    l.is_streaming, l.latency_ms, l.first_token_ms, l.duration_ms,
                    l.status_code, l.error_message, l.created_at, l.data_source, l.pricing_model,
                    l.input_token_semantics
             FROM proxy_request_logs l
             LEFT JOIN providers p ON l.provider_id = p.id AND l.app_type = p.app_type
             {where_clause}
             ORDER BY l.created_at DESC
             LIMIT ? OFFSET ?"
        );

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), row_to_request_log_detail)?;

        let mut logs = Vec::new();
        let mut pricing_cache = HashMap::new();

        for row in rows {
            let mut log = row?;
            Self::maybe_backfill_log_costs(&conn, &mut log, &mut pricing_cache)?;
            logs.push(log);
        }

        Ok(PaginatedLogs {
            data: logs,
            total,
            page,
            page_size,
        })
    }

    /// Get the detail of a single request
    pub fn get_request_detail(
        &self,
        request_id: &str,
    ) -> Result<Option<RequestLogDetail>, AppError> {
        let conn = lock_conn!(self.conn);

        let detail_pname = provider_name_coalesce("l", "p");
        let detail_sql = format!(
            "SELECT l.request_id, l.provider_id, {detail_pname} as provider_name, l.app_type, l.model,
                    l.request_model, l.cost_multiplier,
                    input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                    input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
                    is_streaming, latency_ms, first_token_ms, duration_ms,
                    status_code, error_message, created_at, l.data_source, l.pricing_model,
                    l.input_token_semantics
             FROM proxy_request_logs l
             LEFT JOIN providers p ON l.provider_id = p.id AND l.app_type = p.app_type
             WHERE l.request_id = ?"
        );
        let result = conn.query_row(&detail_sql, [request_id], row_to_request_log_detail);

        match result {
            Ok(mut detail) => {
                let mut pricing_cache = HashMap::new();
                Self::maybe_backfill_log_costs(&conn, &mut detail, &mut pricing_cache)?;
                Ok(Some(detail))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// Check the provider usage quota
    pub fn check_provider_limits(
        &self,
        provider_id: &str,
        app_type: &str,
    ) -> Result<ProviderLimitStatus, AppError> {
        let conn = lock_conn!(self.conn);

        // Read the provider quota settings
        let (limit_daily, limit_monthly) = conn
            .query_row(
                "SELECT meta FROM providers WHERE id = ? AND app_type = ?",
                params![provider_id, app_type],
                |row| {
                    let meta_str: String = row.get(0)?;
                    Ok(meta_str)
                },
            )
            .ok()
            .and_then(|meta_str| serde_json::from_str::<serde_json::Value>(&meta_str).ok())
            .map(|meta| {
                let daily = meta
                    .get("limitDailyUsd")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok());
                let monthly = meta
                    .get("limitMonthlyUsd")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok());
                (daily, monthly)
            })
            .unwrap_or((None, None));

        // Compute today's usage (detail logs + rollup)
        let daily_usage: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(cost), 0) FROM (
                    SELECT CAST(total_cost_usd AS REAL) as cost
                    FROM proxy_request_logs
                    WHERE provider_id = ? AND app_type = ?
                      AND date(datetime(created_at, 'unixepoch', 'localtime')) = date('now', 'localtime')
                    UNION ALL
                    SELECT CAST(total_cost_usd AS REAL)
                    FROM usage_daily_rollups
                    WHERE provider_id = ? AND app_type = ?
                      AND date = date('now', 'localtime')
                )",
                params![provider_id, app_type, provider_id, app_type],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        // Compute this month's usage (detail logs + rollup)
        let monthly_usage: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(cost), 0) FROM (
                    SELECT CAST(total_cost_usd AS REAL) as cost
                    FROM proxy_request_logs
                    WHERE provider_id = ? AND app_type = ?
                      AND strftime('%Y-%m', datetime(created_at, 'unixepoch', 'localtime')) = strftime('%Y-%m', 'now', 'localtime')
                    UNION ALL
                    SELECT CAST(total_cost_usd AS REAL)
                    FROM usage_daily_rollups
                    WHERE provider_id = ? AND app_type = ?
                      AND strftime('%Y-%m', date) = strftime('%Y-%m', 'now', 'localtime')
                )",
                params![provider_id, app_type, provider_id, app_type],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let daily_exceeded = limit_daily
            .map(|limit| daily_usage >= limit)
            .unwrap_or(false);
        let monthly_exceeded = limit_monthly
            .map(|limit| monthly_usage >= limit)
            .unwrap_or(false);

        Ok(ProviderLimitStatus {
            provider_id: provider_id.to_string(),
            daily_usage: format!("{daily_usage:.6}"),
            daily_limit: limit_daily.map(|l| format!("{l:.2}")),
            daily_exceeded,
            monthly_usage: format!("{monthly_usage:.6}"),
            monthly_limit: limit_monthly.map(|l| format!("{l:.2}")),
            monthly_exceeded,
        })
    }
}

/// Provider quota status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLimitStatus {
    pub provider_id: String,
    pub daily_usage: String,
    pub daily_limit: Option<String>,
    pub daily_exceeded: bool,
    pub monthly_usage: String,
    pub monthly_limit: Option<String>,
    pub monthly_exceeded: bool,
}

#[derive(Clone)]
struct PricingInfo {
    input: rust_decimal::Decimal,
    output: rust_decimal::Decimal,
    cache_read: rust_decimal::Decimal,
    cache_creation: rust_decimal::Decimal,
}

impl Database {
    /// Recalculate stored zero-cost usage rows once pricing becomes available.
    pub(crate) fn backfill_missing_usage_costs(&self) -> Result<u64, AppError> {
        let conn = lock_conn!(self.conn);
        Self::backfill_missing_usage_costs_on_conn(&conn, None)
    }

    pub(crate) fn backfill_missing_usage_costs_on_conn(
        conn: &Connection,
        only_model_id: Option<&str>,
    ) -> Result<u64, AppError> {
        const BASE_SQL: &str =
            "SELECT request_id, provider_id, NULL AS provider_name, app_type, model, request_model,
                        cost_multiplier,
                        input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                        input_cost_usd, output_cost_usd, cache_read_cost_usd,
                        cache_creation_cost_usd, total_cost_usd, is_streaming, latency_ms,
                        first_token_ms, duration_ms, status_code, error_message, created_at,
                        data_source, pricing_model, input_token_semantics
             FROM proxy_request_logs
             WHERE CAST(total_cost_usd AS REAL) <= 0
               AND (input_tokens > 0 OR output_tokens > 0
                    OR cache_read_tokens > 0 OR cache_creation_tokens > 0)";

        let mut logs = {
            let mut stmt = conn.prepare(BASE_SQL)?;
            let rows = stmt.query_map([], row_to_request_log_detail)?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        // Row filtering for a targeted backfill must share the candidate normalization with the pricing
        // lookup: an exact SQL match would miss rows stored under the raw alias (e.g.
        // openrouter/anthropic/claude-sonnet-4.5:free), which do normalize onto the new pricing at lookup
        // time. Over-inclusion is harmless - rows with no price found are skipped.
        if let Some(model_id) = only_model_id {
            let target = model_pricing_candidates(model_id);
            logs.retain(|log| log_pricing_scope_matches(log, &target));
        }

        if logs.is_empty() {
            return Ok(0);
        }

        let tx = conn.unchecked_transaction().map_err(|e| {
            AppError::Database(format!(
                "Failed to start the usage cost backfill transaction: {e}"
            ))
        })?;

        let mut updated = 0u64;
        let mut pricing_cache = HashMap::new();
        for log in &mut logs {
            if Self::maybe_backfill_log_costs(&tx, log, &mut pricing_cache)? {
                updated += 1;
            }
        }
        tx.commit().map_err(|e| {
            AppError::Database(format!(
                "Failed to commit the usage cost backfill transaction: {e}"
            ))
        })?;

        if updated > 0 {
            log::info!("Backfilled {updated} rows with missing usage costs");
        }

        Ok(updated)
    }

    /// Try to backfill the cost fields of a single log. Returns whether a write happened (true = UPDATE, false = skipped).
    fn maybe_backfill_log_costs(
        conn: &Connection,
        log: &mut RequestLogDetail,
        pricing_cache: &mut HashMap<String, PricingInfo>,
    ) -> Result<bool, AppError> {
        let existing_cost = rust_decimal::Decimal::from_str(&log.total_cost_usd)
            .unwrap_or(rust_decimal::Decimal::ZERO);
        let has_cost = existing_cost > rust_decimal::Decimal::ZERO;
        let has_usage = log.input_tokens > 0
            || log.output_tokens > 0
            || log.cache_read_tokens > 0
            || log.cache_creation_tokens > 0;

        if has_cost || !has_usage {
            return Ok(false);
        }

        let pricing = match Self::get_log_model_pricing_cached(conn, pricing_cache, log)? {
            Some(info) => info,
            None => return Ok(false),
        };
        let multiplier =
            rust_decimal::Decimal::from_str(&log.cost_multiplier).unwrap_or_else(|e| {
                log::warn!(
                    "Failed to parse the historical usage multiplier request_id={}: {} - {e}",
                    log.request_id,
                    log.cost_multiplier
                );
                rust_decimal::Decimal::ONE
            });

        let million = rust_decimal::Decimal::from(1_000_000u64);

        // Same computation as CostCalculator::calculate_for_app:
        // 1. Historical cache-inclusive rows only include cache read; new total rows also include cache write.
        // 2. The input_tokens of Claude/Anthropic are already fresh input and must not be subtracted again
        // 3. Each cost component is a base cost (no multiplier); the multiplier applies to the final total only
        let cache_inclusive_app =
            crate::services::sql_helpers::is_cache_inclusive_app(log.app_type.as_str());
        let billable_input_tokens =
            if !cache_inclusive_app || log.input_token_semantics == INPUT_TOKEN_SEMANTICS_FRESH {
                log.input_tokens as u64
            } else if log.input_token_semantics == INPUT_TOKEN_SEMANTICS_TOTAL {
                (log.input_tokens as u64)
                    .saturating_sub(log.cache_read_tokens as u64)
                    .saturating_sub(log.cache_creation_tokens as u64)
            } else {
                // v12 and earlier: input included cache reads but excluded cache writes.
                (log.input_tokens as u64).saturating_sub(log.cache_read_tokens as u64)
            };
        let input_cost =
            rust_decimal::Decimal::from(billable_input_tokens) * pricing.input / million;
        let output_cost =
            rust_decimal::Decimal::from(log.output_tokens as u64) * pricing.output / million;
        let cache_read_cost = rust_decimal::Decimal::from(log.cache_read_tokens as u64)
            * pricing.cache_read
            / million;
        let cache_creation_cost = rust_decimal::Decimal::from(log.cache_creation_tokens as u64)
            * pricing.cache_creation
            / million;
        // Total cost = sum of base costs * multiplier
        let base_total = input_cost + output_cost + cache_read_cost + cache_creation_cost;
        let total_cost = base_total * multiplier;

        log.input_cost_usd = format!("{input_cost:.6}");
        log.output_cost_usd = format!("{output_cost:.6}");
        log.cache_read_cost_usd = format!("{cache_read_cost:.6}");
        log.cache_creation_cost_usd = format!("{cache_creation_cost:.6}");
        log.total_cost_usd = format!("{total_cost:.6}");

        conn.execute(
            "UPDATE proxy_request_logs
             SET input_cost_usd = ?1,
                 output_cost_usd = ?2,
                 cache_read_cost_usd = ?3,
                 cache_creation_cost_usd = ?4,
                 total_cost_usd = ?5
             WHERE request_id = ?6",
            params![
                log.input_cost_usd,
                log.output_cost_usd,
                log.cache_read_cost_usd,
                log.cache_creation_cost_usd,
                log.total_cost_usd,
                log.request_id
            ],
        )
        .map_err(|e| AppError::Database(format!("Failed to update the request cost: {e}")))?;

        Ok(true)
    }

    fn get_model_pricing_cached(
        conn: &Connection,
        cache: &mut HashMap<String, PricingInfo>,
        model: &str,
    ) -> Result<Option<PricingInfo>, AppError> {
        if let Some(info) = cache.get(model) {
            return Ok(Some(info.clone()));
        }

        let row = find_model_pricing_row(conn, model)?;
        let Some((input, output, cache_read, cache_creation)) = row else {
            return Ok(None);
        };

        let pricing = PricingInfo {
            input: rust_decimal::Decimal::from_str(&input)
                .map_err(|e| AppError::Database(format!("Failed to parse the input price: {e}")))?,
            output: rust_decimal::Decimal::from_str(&output).map_err(|e| {
                AppError::Database(format!("Failed to parse the output price: {e}"))
            })?,
            cache_read: rust_decimal::Decimal::from_str(&cache_read).map_err(|e| {
                AppError::Database(format!("Failed to parse the cache read price: {e}"))
            })?,
            cache_creation: rust_decimal::Decimal::from_str(&cache_creation).map_err(|e| {
                AppError::Database(format!("Failed to parse the cache write price: {e}"))
            })?,
        };

        cache.insert(model.to_string(), pricing.clone());
        Ok(Some(pricing))
    }

    fn get_log_model_pricing_cached(
        conn: &Connection,
        cache: &mut HashMap<String, PricingInfo>,
        log: &RequestLogDetail,
    ) -> Result<Option<PricingInfo>, AppError> {
        // The pricing basis at write time is stored (v11+): the backfill recomputes from it only, and
        // keeps a 0 cost awaiting a price when it is missing. Guessing via model/request_model is not
        // allowed - under route takeover + request pricing mode the three can all differ (model = upstream
        // echo, request_model = client alias, pricing_model = real outbound model) and switching the basis
        // would freeze a wrong price. Placeholders ("" = unpriced error row / "unknown") count as missing.
        if let Some(pricing_model) = log
            .pricing_model
            .as_deref()
            .filter(|pm| !is_placeholder_pricing_model(pm))
        {
            return Self::get_model_pricing_cached(conn, cache, pricing_model);
        }

        if let Some(pricing) = Self::get_model_pricing_cached(conn, cache, &log.model)? {
            return Ok(Some(pricing));
        }

        // Only fall back to request_model pricing when the model column is a placeholder (""/"unknown"
        // left by a parse failure). When model is a real name without pricing, the cost must stay 0 and
        // wait for a price: under route takeover request_model is the client alias (e.g. claude-sonnet-4-6)
        // and backfilling by the alias would permanently freeze the real upstream model's tokens at the
        // wrong price (a row with a cost no longer enters the backfill scope).
        if !is_placeholder_pricing_model(&log.model) {
            return Ok(None);
        }

        let Some(request_model) = log.request_model.as_deref() else {
            return Ok(None);
        };
        if request_model == log.model {
            return Ok(None);
        }

        Self::get_model_pricing_cached(conn, cache, request_model)
    }
}

pub(crate) fn find_model_pricing(conn: &Connection, model_id: &str) -> Option<ModelPricing> {
    find_model_pricing_row(conn, model_id)
        .ok()
        .flatten()
        .and_then(|(input, output, cache_read, cache_creation)| {
            ModelPricing::from_strings(&input, &output, &cache_read, &cache_creation).ok()
        })
}

pub(crate) fn find_model_pricing_row(
    conn: &Connection,
    model_id: &str,
) -> Result<Option<(String, String, String, String)>, AppError> {
    let candidates = model_pricing_candidates(model_id);
    if candidates.is_empty() {
        return Ok(None);
    }

    for candidate in &candidates {
        if let Some(row) = query_model_pricing_exact(conn, candidate)? {
            return Ok(Some(row));
        }
    }

    for candidate in &candidates {
        if should_try_pricing_prefix_match(candidate) {
            if let Some(row) = query_model_pricing_prefix(conn, candidate)? {
                return Ok(Some(row));
            }
        }
    }

    Ok(None)
}

/// Row filtering for a targeted backfill: a log is relevant when any of its model fields, once
/// normalized, intersects the target model's candidates, or when the pricing layer's prefix rules would
/// hit the target. Mirrors the matching semantics of find_model_pricing_row: over-include (the pricing lookup catches it) rather than under-filter.
fn log_pricing_scope_matches(log: &RequestLogDetail, target_candidates: &[String]) -> bool {
    [
        Some(log.model.as_str()),
        log.request_model.as_deref(),
        log.pricing_model.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|field| {
        model_pricing_candidates(field).iter().any(|candidate| {
            target_candidates.iter().any(|target| {
                target == candidate
                    || (should_try_pricing_prefix_match(candidate)
                        && target
                            .strip_prefix(candidate.as_str())
                            .is_some_and(|rest| rest.starts_with('-')))
            })
        })
    })
}

pub(crate) fn is_placeholder_pricing_model(model_id: &str) -> bool {
    let normalized = model_id.trim().to_ascii_lowercase();
    normalized.is_empty() || matches!(normalized.as_str(), "unknown" | "null" | "none")
}

fn query_model_pricing_exact(
    conn: &Connection,
    model_id: &str,
) -> Result<Option<(String, String, String, String)>, AppError> {
    conn.query_row(
        "SELECT input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
         FROM model_pricing
         WHERE model_id = ?1",
        [model_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )
    .optional()
    .map_err(|e| AppError::Database(format!("Failed to query model pricing: {e}")))
}

fn query_model_pricing_prefix(
    conn: &Connection,
    model_id: &str,
) -> Result<Option<(String, String, String, String)>, AppError> {
    let pattern = format!("{model_id}-%");
    conn.query_row(
        "SELECT input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
         FROM model_pricing
         WHERE model_id LIKE ?1
         ORDER BY LENGTH(model_id) ASC
         LIMIT 1",
        [pattern],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )
    .optional()
    .map_err(|e| AppError::Database(format!("Failed to query model prefix pricing: {e}")))
}

fn model_pricing_candidates(model_id: &str) -> Vec<String> {
    let cleaned = clean_model_id_for_pricing(model_id);
    if is_placeholder_pricing_model(&cleaned) {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    let mut queue = vec![cleaned];

    while let Some(candidate) = queue.pop() {
        if !push_unique_candidate(&mut candidates, candidate.clone()) {
            continue;
        }

        if let Some(stripped) = strip_known_model_namespace(&candidate) {
            queue.push(stripped);
        }
        if let Some(stripped) = strip_claude_desktop_non_anthropic_prefix(&candidate) {
            queue.push(stripped);
        }
        if let Some(stripped) = strip_bedrock_model_version_suffix(&candidate) {
            queue.push(stripped);
        }
        if let Some(stripped) = strip_model_date_suffix(&candidate) {
            queue.push(stripped);
        }
        if let Some(stripped) = strip_reasoning_effort_suffix(&candidate) {
            queue.push(stripped);
        }
        if candidate.starts_with("claude-") && candidate.contains('.') {
            queue.push(candidate.replace('.', "-"));
        }
    }

    candidates
}

fn clean_model_id_for_pricing(model_id: &str) -> String {
    let normalized = model_id
        .rsplit_once('/')
        .map_or(model_id, |(_, r)| r)
        .split(':')
        .next()
        .unwrap_or(model_id)
        .trim()
        .replace('@', "-")
        .to_ascii_lowercase();

    normalized
        .trim_end_matches(crate::claude_desktop_config::ONE_M_CONTEXT_MARKER)
        .trim()
        .to_string()
}

fn push_unique_candidate(candidates: &mut Vec<String>, candidate: String) -> bool {
    if candidate.is_empty() || candidates.iter().any(|existing| existing == &candidate) {
        return false;
    }
    candidates.push(candidate);
    true
}

fn strip_known_model_namespace(model_id: &str) -> Option<String> {
    if let Some(pos) = model_id.rfind("claude-") {
        if pos > 0 {
            return Some(model_id[pos..].to_string());
        }
    }

    for marker in [
        "openai.",
        "anthropic.",
        "google.",
        "moonshot.",
        "moonshotai.",
        "bedrock.",
        "global.",
    ] {
        if let Some(stripped) = model_id.strip_prefix(marker) {
            return Some(stripped.to_string());
        }
    }

    None
}

fn strip_claude_desktop_non_anthropic_prefix(model_id: &str) -> Option<String> {
    const NON_ANTHROPIC_MARKERS: &[&str] = &[
        "abab",
        "ark-code",
        "arctic",
        "astron",
        "codex",
        "command-r",
        "deepseek",
        "doubao",
        "ernie",
        "gemini",
        "gemma",
        "glm",
        "gpt",
        "grok",
        "hermes",
        "hy3",
        "hunyuan",
        "jamba",
        "kimi",
        "lfm",
        "llama",
        "longcat",
        "mercury",
        "mimo",
        "minimax",
        "mistral",
        "mixtral",
        "moonshot",
        "nemotron",
        "nova-",
        "openai",
        "qianfan",
        "qwen",
        "seed-",
        "solar",
        "stepfun",
    ];

    let rest = model_id.strip_prefix("claude-")?;
    NON_ANTHROPIC_MARKERS
        .iter()
        .any(|marker| rest.starts_with(marker))
        .then(|| rest.to_string())
}

fn strip_bedrock_model_version_suffix(model_id: &str) -> Option<String> {
    let (base, suffix) = model_id.rsplit_once("-v")?;
    (!base.is_empty() && !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()))
        .then(|| base.to_string())
}

fn strip_model_date_suffix(model_id: &str) -> Option<String> {
    let bytes = model_id.as_bytes();
    if bytes.len() > 11 {
        let start = bytes.len() - 11;
        let suffix = &bytes[start..];
        let is_iso_date = suffix[0] == b'-'
            && suffix[1..5].iter().all(|b| b.is_ascii_digit())
            && suffix[5] == b'-'
            && suffix[6..8].iter().all(|b| b.is_ascii_digit())
            && suffix[8] == b'-'
            && suffix[9..11].iter().all(|b| b.is_ascii_digit());
        if is_iso_date {
            return Some(model_id[..start].to_string());
        }
    }

    let (base, suffix) = model_id.rsplit_once('-')?;
    if base.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // 8 digits YYYYMMDD (e.g. -20250615; OpenAI / Claude / Qwen and others).
    if suffix.len() == 8 {
        return Some(base.to_string());
    }
    // 6 digits YYMMDD (e.g. -260628; Volcengine Ark doubao-seed-*, some Chinese vendors).
    // 6 digits hit non-date tails (such as a -123456 version number) more easily than 8, so month 01-12
    // and day 01-31 are validated before stripping; when it cannot strip it returns None and the caller falls back to an exact match.
    if suffix.len() == 6 {
        let month: u32 = suffix[2..4].parse().unwrap_or(0);
        let day: u32 = suffix[4..6].parse().unwrap_or(0);
        if (1..=12).contains(&month) && (1..=31).contains(&day) {
            return Some(base.to_string());
        }
    }
    None
}

fn strip_reasoning_effort_suffix(model_id: &str) -> Option<String> {
    for suffix in ["-minimal", "-low", "-medium", "-high", "-xhigh"] {
        if let Some(stripped) = model_id.strip_suffix(suffix) {
            if !stripped.is_empty() {
                return Some(stripped.to_string());
            }
        }
    }
    None
}

fn should_try_pricing_prefix_match(model_id: &str) -> bool {
    let dash_count = model_id.matches('-').count();

    if model_id.starts_with("claude-") {
        return dash_count >= 3;
    }

    if ["o1", "o3", "o4", "o5"]
        .iter()
        .any(|prefix| model_id.starts_with(prefix))
    {
        return dash_count >= 1;
    }

    const PREFIX_MATCH_FAMILIES: &[&str] = &[
        "gpt-",
        "gemini-",
        "deepseek-",
        "qwen-",
        "glm-",
        "kimi-",
        "minimax-",
    ];

    PREFIX_MATCH_FAMILIES
        .iter()
        .any(|prefix| model_id.starts_with(prefix))
        && dash_count >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_ts(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> i64 {
        match Local.with_ymd_and_hms(year, month, day, hour, minute, second) {
            chrono::LocalResult::Single(dt) => dt.timestamp(),
            chrono::LocalResult::Ambiguous(earliest, _) => earliest.timestamp(),
            chrono::LocalResult::None => panic!("valid local datetime"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_usage_log(
        conn: &Connection,
        request_id: &str,
        app_type: &str,
        provider_id: &str,
        model: &str,
        data_source: &str,
        created_at: i64,
        input_tokens: i64,
        output_tokens: i64,
        cache_read_tokens: i64,
        cache_creation_tokens: i64,
        status_code: i64,
        total_cost_usd: &str,
    ) -> Result<(), AppError> {
        conn.execute(
            "INSERT INTO proxy_request_logs (
                request_id, provider_id, app_type, model, request_model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd,
                total_cost_usd, latency_ms, status_code, created_at, data_source
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, '0', '0', '0', '0', ?, 100, ?, ?, ?)",
            params![
                request_id,
                provider_id,
                app_type,
                model,
                model,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
                total_cost_usd,
                status_code,
                created_at,
                data_source
            ],
        )?;
        Ok(())
    }

    fn create_legacy_nullable_logs_table(conn: &Connection) -> Result<(), AppError> {
        conn.execute(
            "CREATE TABLE proxy_request_logs (
                request_id TEXT PRIMARY KEY,
                app_type TEXT NOT NULL,
                model TEXT NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                cache_read_tokens INTEGER NOT NULL,
                cache_creation_tokens INTEGER NOT NULL,
                status_code INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                data_source TEXT
            )",
            [],
        )?;
        Ok(())
    }

    #[test]
    fn test_effective_filter_keeps_legacy_null_data_source_proxy_rows() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        create_legacy_nullable_logs_table(&conn)?;
        conn.execute(
            "INSERT INTO proxy_request_logs (
                request_id, app_type, model, input_tokens, output_tokens,
                cache_read_tokens, cache_creation_tokens, status_code, created_at, data_source
            ) VALUES ('legacy-proxy', 'codex', 'gpt-5.5', 10, 2, 1, 0, 200, 1000, NULL)",
            [],
        )?;

        let filter = effective_usage_log_filter("l");
        let sql = format!("SELECT COUNT(*) FROM proxy_request_logs l WHERE {filter}");
        let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
        assert_eq!(count, 1);

        Ok(())
    }

    #[test]
    fn test_matching_proxy_log_treats_legacy_null_data_source_as_proxy() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        create_legacy_nullable_logs_table(&conn)?;
        conn.execute(
            "INSERT INTO proxy_request_logs (
                request_id, app_type, model, input_tokens, output_tokens,
                cache_read_tokens, cache_creation_tokens, status_code, created_at, data_source
            ) VALUES ('legacy-proxy', 'codex', 'gpt-5.5', 10, 2, 1, 0, 200, 1000, NULL)",
            [],
        )?;

        let key = DedupKey {
            app_type: "codex",
            model: "gpt-5.5",
            input_tokens: 10,
            output_tokens: 2,
            cache_read_tokens: 1,
            cache_creation_tokens: 0,
            created_at: 1000,
        };
        assert!(has_matching_proxy_usage_log(&conn, &key)?);

        Ok(())
    }

    #[test]
    fn test_matching_proxy_log_matches_claude_desktop_for_claude_session() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        create_legacy_nullable_logs_table(&conn)?;
        conn.execute(
            "INSERT INTO proxy_request_logs (
                request_id, app_type, model, input_tokens, output_tokens,
                cache_read_tokens, cache_creation_tokens, status_code, created_at, data_source
            ) VALUES ('desktop-proxy', 'claude-desktop', 'claude-sonnet-4-5', 100, 20, 10, 5, 200, 1000, 'proxy')",
            [],
        )?;

        let key = DedupKey {
            app_type: "claude",
            model: "claude-sonnet-4-5",
            input_tokens: 100,
            output_tokens: 20,
            cache_read_tokens: 10,
            cache_creation_tokens: 5,
            created_at: 1060,
        };
        assert!(has_matching_proxy_usage_log(&conn, &key)?);

        let mut outside_window = key;
        outside_window.created_at = 1_601;
        assert!(!has_matching_proxy_usage_log(&conn, &outside_window)?);

        let mut different_model = key;
        different_model.model = "claude-opus-4-5";
        assert!(!has_matching_proxy_usage_log(&conn, &different_model)?);

        let mut different_input = key;
        different_input.input_tokens += 1;
        assert!(!has_matching_proxy_usage_log(&conn, &different_input)?);

        let mut different_cache_creation = key;
        different_cache_creation.cache_creation_tokens += 1;
        assert!(!has_matching_proxy_usage_log(
            &conn,
            &different_cache_creation
        )?);

        Ok(())
    }

    #[test]
    fn test_effective_filter_dedups_claude_session_against_desktop_proxy() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        create_legacy_nullable_logs_table(&conn)?;
        conn.execute_batch(
            "INSERT INTO proxy_request_logs (
                request_id, app_type, model, input_tokens, output_tokens,
                cache_read_tokens, cache_creation_tokens, status_code, created_at, data_source
            ) VALUES
                ('desktop-proxy', 'claude-desktop', 'claude-sonnet-4-5', 100, 20, 10, 5, 200, 1000, 'proxy'),
                ('claude-session', 'claude', 'claude-sonnet-4-5', 100, 20, 10, 5, 200, 1060, 'session_log');",
        )?;

        let filter = effective_usage_log_filter("l");
        let sql = format!("SELECT request_id FROM proxy_request_logs l WHERE {filter}");
        let request_ids = conn
            .prepare(&sql)?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(request_ids, vec!["desktop-proxy"]);

        Ok(())
    }

    #[test]
    fn test_claude_desktop_folds_into_claude_for_display() -> Result<(), AppError> {
        let db = Database::memory()?;
        let ts = local_ts(2026, 6, 10, 12, 0, 0);

        {
            let conn = lock_conn!(db.conn);
            // One Claude Code row + one Claude Desktop gateway row in the same time window.
            insert_usage_log(
                &conn,
                "cc-1",
                "claude",
                "p-claude",
                "claude-sonnet-4-5",
                "proxy",
                ts,
                100,
                10,
                0,
                0,
                200,
                "0.5",
            )?;
            insert_usage_log(
                &conn,
                "cd-1",
                "claude-desktop",
                "p-desktop",
                "claude-opus-4-8",
                "proxy",
                ts,
                200,
                20,
                0,
                0,
                200,
                "1.5",
            )?;
        }

        // (1) Per-app summary: desktop folds into claude, no separate claude-desktop bucket.
        let by_app = db.get_usage_summary_by_app(None, None, None, None)?;
        assert_eq!(
            by_app.len(),
            1,
            "only one merged claude bucket should remain"
        );
        assert_eq!(by_app[0].app_type, "claude");
        assert_eq!(
            by_app[0].summary.total_requests, 2,
            "both rows count towards claude"
        );
        assert!(
            !by_app.iter().any(|a| a.app_type == "claude-desktop"),
            "the claude-desktop bucket must not appear any more"
        );

        // (2) Filtering by claude: the summary must cover the desktop row too.
        let claude_summary = db.get_usage_summary(None, None, Some("claude"), None, None)?;
        assert_eq!(claude_summary.total_requests, 2);

        // (3) Request logs filtered by claude return both rows, and the desktop row still projects the raw app_type.
        let logs = db.get_request_logs(
            &LogFilters {
                app_type: Some("claude".to_string()),
                ..Default::default()
            },
            0, // page index starts at 0
            50,
        )?;
        assert_eq!(logs.total, 2, "the claude filter includes the desktop row");
        assert!(
            logs.data.iter().any(|r| r.app_type == "claude-desktop"),
            "the detail panel needs the real entry point, the row projection must not be folded"
        );

        // (4) Folding does not spill over: the codex filter is empty.
        let codex_summary = db.get_usage_summary(None, None, Some("codex"), None, None)?;
        assert_eq!(codex_summary.total_requests, 0);

        Ok(())
    }

    #[test]
    fn test_backfill_missing_usage_costs_uses_new_gpt_5_5_pricing() -> Result<(), AppError> {
        let db = Database::memory()?;

        {
            let conn = lock_conn!(db.conn);
            insert_usage_log(
                &conn,
                "codex-gpt-5-5-zero-cost",
                "codex",
                "_codex_session",
                "gpt-5.5",
                "codex_session",
                1000,
                1_000_000,
                1_000_000,
                0,
                0,
                200,
                "0",
            )?;
        }

        assert_eq!(db.backfill_missing_usage_costs()?, 1);

        let conn = lock_conn!(db.conn);
        let (input_cost, output_cost, total_cost): (String, String, String) = conn.query_row(
            "SELECT input_cost_usd, output_cost_usd, total_cost_usd
             FROM proxy_request_logs WHERE request_id = 'codex-gpt-5-5-zero-cost'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(input_cost, "5.000000");
        assert_eq!(output_cost, "30.000000");
        assert_eq!(total_cost, "35.000000");

        Ok(())
    }

    #[test]
    fn test_backfill_distinguishes_legacy_and_total_cache_semantics() -> Result<(), AppError> {
        let db = Database::memory()?;

        {
            let conn = lock_conn!(db.conn);
            // v12 mirror row: input = fresh + read; creation was reported separately.
            insert_usage_log(
                &conn,
                "legacy-cache-semantics",
                "codex",
                "p1",
                "gpt-5.5",
                "proxy",
                1000,
                800_000,
                0,
                600_000,
                200_000,
                200,
                "0",
            )?;
            // v13 proxy row: input = fresh + read + creation.
            insert_usage_log(
                &conn,
                "total-cache-semantics",
                "codex",
                "p1",
                "gpt-5.5",
                "proxy",
                1001,
                1_000_000,
                0,
                600_000,
                200_000,
                200,
                "0",
            )?;
            conn.execute(
                "UPDATE proxy_request_logs
                 SET input_token_semantics = ?1
                 WHERE request_id = 'total-cache-semantics'",
                [INPUT_TOKEN_SEMANTICS_TOTAL],
            )?;
        }

        assert_eq!(db.backfill_missing_usage_costs()?, 2);

        let conn = lock_conn!(db.conn);
        let mut stmt = conn.prepare(
            "SELECT request_id, input_cost_usd
             FROM proxy_request_logs
             WHERE request_id IN ('legacy-cache-semantics', 'total-cache-semantics')
             ORDER BY request_id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            rows,
            vec![
                ("legacy-cache-semantics".to_string(), "1.000000".to_string()),
                ("total-cache-semantics".to_string(), "1.000000".to_string()),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_backfill_deducts_cache_read_for_grokbuild_total_rows() -> Result<(), AppError> {
        // Regression: the cache-inclusive decision on the backfill side used to hardcode codex|gemini and
        // miss grokbuild, so TOTAL rows were priced on the full input and cache_read was double counted.
        // Once the decision moved to sql_helpers::is_cache_inclusive_app it is priced on 450 fresh.
        let db = Database::memory()?;
        {
            let conn = lock_conn!(db.conn);
            insert_usage_log(
                &conn,
                "grokbuild-total-backfill",
                "grokbuild",
                "_grok_session",
                "grok-4.5",
                "grok_session",
                1000,
                700,
                100,
                250,
                0,
                200,
                "0",
            )?;
            conn.execute(
                "UPDATE proxy_request_logs
                 SET input_token_semantics = ?1
                 WHERE request_id = 'grokbuild-total-backfill'",
                [INPUT_TOKEN_SEMANTICS_TOTAL],
            )?;
        }

        assert_eq!(db.backfill_missing_usage_costs()?, 1);

        let conn = lock_conn!(db.conn);
        let (input_cost, cache_read_cost, total_cost): (String, String, String) = conn.query_row(
            "SELECT input_cost_usd, cache_read_cost_usd, total_cost_usd
             FROM proxy_request_logs WHERE request_id = 'grokbuild-total-backfill'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        // grok-4.5 pricing 2/6/0.30: input = (700-250)*2/1M, cache_read = 250*0.3/1M
        assert_eq!(input_cost, "0.000900");
        assert_eq!(cache_read_cost, "0.000075");
        assert_eq!(total_cost, "0.001575");
        Ok(())
    }

    #[test]
    fn test_backfill_missing_usage_costs_uses_stored_multiplier() -> Result<(), AppError> {
        let db = Database::memory()?;

        {
            let conn = lock_conn!(db.conn);
            insert_usage_log(
                &conn,
                "codex-gpt-5-5-multiplier",
                "codex",
                "_codex_session",
                "gpt-5.5",
                "codex_session",
                1000,
                1_000_000,
                0,
                0,
                0,
                200,
                "0",
            )?;
            conn.execute(
                "UPDATE proxy_request_logs
                 SET cost_multiplier = '1.5'
                 WHERE request_id = 'codex-gpt-5-5-multiplier'",
                [],
            )?;
        }

        assert_eq!(db.backfill_missing_usage_costs()?, 1);

        let conn = lock_conn!(db.conn);
        let (input_cost, total_cost): (String, String) = conn.query_row(
            "SELECT input_cost_usd, total_cost_usd
             FROM proxy_request_logs WHERE request_id = 'codex-gpt-5-5-multiplier'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(input_cost, "5.000000");
        assert_eq!(total_cost, "7.500000");

        Ok(())
    }

    #[test]
    fn test_backfill_missing_usage_costs_falls_back_to_request_model() -> Result<(), AppError> {
        let db = Database::memory()?;

        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model, request_model,
                    input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                    input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd,
                    total_cost_usd, latency_ms, status_code, created_at, data_source
                ) VALUES (
                    'codex-request-model-fallback', '_codex_session', 'codex', 'unknown', 'gpt-5.5',
                    1000000, 0, 0, 0,
                    '0', '0', '0', '0',
                    '0', 100, 200, 1000, 'codex_session'
                )",
                [],
            )?;
        }

        assert_eq!(db.backfill_missing_usage_costs()?, 1);

        let conn = lock_conn!(db.conn);
        let total_cost: String = conn.query_row(
            "SELECT total_cost_usd
             FROM proxy_request_logs WHERE request_id = 'codex-request-model-fallback'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(total_cost, "5.000000");

        Ok(())
    }

    #[test]
    fn test_backfill_skips_request_model_fallback_for_real_unpriced_model() -> Result<(), AppError>
    {
        let db = Database::memory()?;

        {
            let conn = lock_conn!(db.conn);
            // Route takeover scenario: model is the real upstream echo (no pricing), request_model is
            // the client alias (priced). The backfill must not price by the alias and must keep cost 0.
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model, request_model,
                    input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                    input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd,
                    total_cost_usd, latency_ms, status_code, created_at, data_source
                ) VALUES (
                    'takeover-unpriced-model', 'provider-1', 'claude',
                    'takeover-real-model-unpriced', 'claude-sonnet-4-6',
                    1000000, 0, 0, 0,
                    '0', '0', '0', '0',
                    '0', 100, 200, 1000, 'proxy'
                )",
                [],
            )?;
        }

        // request_model (claude-sonnet-4-6) is priced, but model is a real model name: no fallback allowed
        assert_eq!(db.backfill_missing_usage_costs()?, 0);

        {
            let conn = lock_conn!(db.conn);
            let total_cost: String = conn.query_row(
                "SELECT total_cost_usd
                 FROM proxy_request_logs WHERE request_id = 'takeover-unpriced-model'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(total_cost, "0");

            // Once the real model pricing exists, the backfill must repair it with that price (the 0-cost row was never frozen)
            conn.execute(
                "INSERT INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million)
                 VALUES ('takeover-real-model-unpriced', 'Takeover Real Model', '0.6', '2.5')",
                [],
            )?;
        }

        assert_eq!(db.backfill_missing_usage_costs()?, 1);

        let conn = lock_conn!(db.conn);
        let total_cost: String = conn.query_row(
            "SELECT total_cost_usd
             FROM proxy_request_logs WHERE request_id = 'takeover-unpriced-model'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(total_cost, "0.600000");

        Ok(())
    }

    #[test]
    fn test_backfill_missing_usage_costs_keeps_claude_fresh_input() -> Result<(), AppError> {
        let db = Database::memory()?;

        {
            let conn = lock_conn!(db.conn);
            insert_usage_log(
                &conn,
                "claude-cache-fresh-input",
                "claude",
                "_session",
                "claude-haiku-4-5",
                "session_log",
                1000,
                100,
                0,
                200,
                0,
                200,
                "0",
            )?;
        }

        assert_eq!(db.backfill_missing_usage_costs()?, 1);

        let conn = lock_conn!(db.conn);
        let (input_cost, cache_read_cost, total_cost): (String, String, String) = conn.query_row(
            "SELECT input_cost_usd, cache_read_cost_usd, total_cost_usd
             FROM proxy_request_logs WHERE request_id = 'claude-cache-fresh-input'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(input_cost, "0.000100");
        assert_eq!(cache_read_cost, "0.000020");
        assert_eq!(total_cost, "0.000120");

        Ok(())
    }

    #[test]
    fn test_get_usage_summary() -> Result<(), AppError> {
        let db = Database::memory()?;

        // Insert test data
        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params!["req1", "p1", "claude", "claude-3", 100, 50, "0.01", 100, 200, 1000],
            )?;
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params!["req2", "p1", "claude", "claude-3", 200, 100, "0.02", 150, 200, 2000],
            )?;
        }

        let summary = db.get_usage_summary(None, None, None, None, None)?;
        assert_eq!(summary.total_requests, 2);
        assert_eq!(summary.success_rate, 100.0);

        Ok(())
    }

    #[test]
    fn test_get_usage_summary_excludes_partial_rollup_boundary_days() -> Result<(), AppError> {
        let db = Database::memory()?;
        let start = local_ts(2024, 1, 1, 12, 0, 0);
        let end = local_ts(2024, 1, 3, 12, 0, 0);

        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-01-01",
                    "claude",
                    "p1",
                    "claude-3",
                    10,
                    10,
                    1000,
                    500,
                    0,
                    0,
                    "1.00",
                    100
                ],
            )?;
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-01-02",
                    "claude",
                    "p1",
                    "claude-3",
                    20,
                    19,
                    2000,
                    1000,
                    0,
                    0,
                    "2.00",
                    120
                ],
            )?;
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-01-03",
                    "claude",
                    "p1",
                    "claude-3",
                    30,
                    29,
                    3000,
                    1500,
                    0,
                    0,
                    "3.00",
                    140
                ],
            )?;
        }

        let summary = db.get_usage_summary(Some(start), Some(end), Some("claude"), None, None)?;
        assert_eq!(summary.total_requests, 20);
        assert_eq!(summary.total_input_tokens, 2000);
        assert_eq!(summary.total_output_tokens, 1000);

        Ok(())
    }

    #[test]
    fn test_provider_and_model_filters_cover_detail_and_rollup() -> Result<(), AppError> {
        let db = Database::memory()?;
        let detail_ts = local_ts(2026, 6, 10, 12, 0, 0);

        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config) VALUES
                 ('prov-a', 'claude', 'Packy', '{}'),
                 ('prov-b', 'claude', 'DeepSeek', '{}')",
                [],
            )?;

            insert_usage_log(
                &conn,
                "a-1",
                "claude",
                "prov-a",
                "claude-sonnet-4-6",
                "proxy",
                detail_ts,
                100,
                10,
                0,
                0,
                200,
                "1.0",
            )?;
            insert_usage_log(
                &conn,
                "b-1",
                "claude",
                "prov-b",
                "deepseek-v3",
                "proxy",
                detail_ts,
                200,
                20,
                0,
                0,
                200,
                "2.0",
            )?;
            // Session placeholder row: the providers table has no such id, so the display name comes from the CASE mapping.
            insert_usage_log(
                &conn,
                "s-1",
                "claude",
                "_session",
                "claude-sonnet-4-6",
                "session_log",
                detail_ts,
                999,
                99,
                0,
                0,
                200,
                "0.5",
            )?;
            // A row whose pricing model differs from the request model: the model filter must match the effective pricing model.
            insert_usage_log(
                &conn,
                "a-2",
                "claude",
                "prov-a",
                "alias-model",
                "proxy",
                detail_ts,
                50,
                5,
                0,
                0,
                200,
                "0.3",
            )?;
            conn.execute(
                "UPDATE proxy_request_logs SET pricing_model = 'real-model' WHERE request_id = 'a-2'",
                [],
            )?;

            // Historical rollup day rows: all counted when there is no range filter.
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES
                ('2026-06-08', 'claude', 'prov-a', 'claude-sonnet-4-6', 5, 5, 500, 50, 0, 0, '5.0', 100),
                ('2026-06-08', 'claude', 'prov-b', 'deepseek-v3', 7, 7, 700, 70, 0, 0, '7.0', 100)",
                [],
            )?;
        }

        // (1) Summary filtered by provider display name: both detail and rollup match.
        let packy = db.get_usage_summary(None, None, None, Some("Packy"), None)?;
        assert_eq!(packy.total_requests, 7, "a-1 + a-2 + rollup 5");

        // (2) Summary filtered by model (effective pricing model basis).
        let deepseek = db.get_usage_summary(None, None, None, None, Some("deepseek-v3"))?;
        assert_eq!(deepseek.total_requests, 8, "b-1 + rollup 7");

        // (3) pricing_model wins over model: alias-model finds nothing, real-model does.
        let by_alias = db.get_usage_summary(None, None, None, None, Some("alias-model"))?;
        assert_eq!(by_alias.total_requests, 0);
        let by_real = db.get_usage_summary(None, None, None, None, Some("real-model"))?;
        assert_eq!(by_real.total_requests, 1);

        // (4) A session placeholder row can be selected by its readable name.
        let session = db.get_usage_summary(None, None, None, Some("Claude (Session)"), None)?;
        assert_eq!(session.total_requests, 1);

        // (5) Provider stats + model filter: only the DeepSeek row remains.
        let provider_stats = db.get_provider_stats(None, None, None, None, Some("deepseek-v3"))?;
        assert_eq!(provider_stats.len(), 1);
        assert_eq!(provider_stats[0].provider_name, "DeepSeek");
        assert_eq!(provider_stats[0].request_count, 8);

        // (6) Model stats + provider filter: only the models under Packy remain.
        let model_stats = db.get_model_stats(None, None, None, Some("Packy"), None)?;
        let models: Vec<&str> = model_stats.iter().map(|m| m.model.as_str()).collect();
        assert!(models.contains(&"claude-sonnet-4-6"));
        assert!(models.contains(&"real-model"));
        assert!(!models.contains(&"deepseek-v3"));

        // (7) The per-app summary (data source of the hero cards) is filtered as well.
        let by_app = db.get_usage_summary_by_app(None, None, Some("Packy"), None)?;
        assert_eq!(by_app.len(), 1);
        assert_eq!(by_app[0].app_type, "claude");
        assert_eq!(by_app[0].summary.total_requests, 7);

        // (8) Trends (>24h takes the daily bucket + rollup branch).
        let t_start = local_ts(2026, 6, 8, 0, 0, 0);
        let t_end = local_ts(2026, 6, 10, 23, 59, 0);
        let trends = db.get_daily_trends(Some(t_start), Some(t_end), None, Some("Packy"), None)?;
        let total_req: u64 = trends.iter().map(|d| d.request_count).sum();
        assert_eq!(total_req, 7, "detail 2 + rollup 5");

        // (9) Trends <=24h take the hourly bucket branch (the path mixing ?1/?2/?3 numbered parameters
        //     with appended filters), also verifying the combined provider + model filter.
        let h_start = local_ts(2026, 6, 10, 0, 0, 0);
        let h_end = local_ts(2026, 6, 10, 20, 0, 0);
        let hourly = db.get_daily_trends(
            Some(h_start),
            Some(h_end),
            None,
            Some("Packy"),
            Some("claude-sonnet-4-6"),
        )?;
        let hourly_req: u64 = hourly.iter().map(|d| d.request_count).sum();
        assert_eq!(
            hourly_req, 1,
            "only a-1 matches (a-2 has a different pricing model)"
        );

        // (10) The request log list uses the same basis as the dropdowns: exact name + effective pricing model.
        let logs = db.get_request_logs(
            &LogFilters {
                provider_name: Some("Packy".to_string()),
                model: Some("real-model".to_string()),
                ..Default::default()
            },
            0,
            10,
        )?;
        assert_eq!(logs.total, 1);
        assert_eq!(logs.data[0].request_id, "a-2");

        Ok(())
    }

    #[test]
    fn test_get_usage_summary_includes_end_day_rollup_for_minute_precision_end_time(
    ) -> Result<(), AppError> {
        let db = Database::memory()?;
        let start = local_ts(2024, 1, 1, 0, 0, 0);
        let end = local_ts(2024, 1, 2, 23, 59, 0);

        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-01-01",
                    "claude",
                    "p1",
                    "claude-3",
                    10,
                    10,
                    1000,
                    500,
                    0,
                    0,
                    "1.00",
                    100
                ],
            )?;
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-01-02",
                    "claude",
                    "p1",
                    "claude-3",
                    20,
                    19,
                    2000,
                    1000,
                    0,
                    0,
                    "2.00",
                    120
                ],
            )?;
        }

        let summary = db.get_usage_summary(Some(start), Some(end), Some("claude"), None, None)?;
        assert_eq!(summary.total_requests, 30);
        assert_eq!(summary.total_input_tokens, 3000);
        assert_eq!(summary.total_output_tokens, 1500);

        Ok(())
    }

    #[test]
    fn test_effective_usage_dedup_keeps_non_matching_session_rows() -> Result<(), AppError> {
        let db = Database::memory()?;

        {
            let conn = lock_conn!(db.conn);
            insert_usage_log(
                &conn,
                "proxy-base",
                "codex",
                "openai",
                "gpt-5.4",
                "proxy",
                10_000,
                100,
                20,
                10,
                0,
                200,
                "0.10",
            )?;
            insert_usage_log(
                &conn,
                "session-outside-window",
                "codex",
                "_codex_session",
                "gpt-5.4",
                "codex_session",
                10_601,
                100,
                20,
                10,
                0,
                200,
                "0.10",
            )?;
            insert_usage_log(
                &conn,
                "session-token-mismatch",
                "codex",
                "_codex_session",
                "gpt-5.4",
                "codex_session",
                10_060,
                101,
                20,
                10,
                0,
                200,
                "0.10",
            )?;
            insert_usage_log(
                &conn,
                "session-app-mismatch",
                "gemini",
                "_gemini_session",
                "gpt-5.4",
                "gemini_session",
                10_060,
                100,
                20,
                10,
                0,
                200,
                "0.10",
            )?;
            insert_usage_log(
                &conn,
                "session-model-mismatch",
                "codex",
                "_codex_session",
                "different-model",
                "codex_session",
                10_060,
                100,
                20,
                10,
                0,
                200,
                "0.10",
            )?;
            insert_usage_log(
                &conn,
                "proxy-error",
                "codex",
                "openai",
                "gpt-5.4",
                "proxy",
                20_000,
                300,
                60,
                0,
                0,
                500,
                "0.00",
            )?;
            insert_usage_log(
                &conn,
                "session-matches-error-proxy",
                "codex",
                "_codex_session",
                "gpt-5.4",
                "codex_session",
                20_060,
                300,
                60,
                0,
                0,
                200,
                "0.30",
            )?;
            insert_usage_log(
                &conn,
                "claude-proxy-cache-creation",
                "claude",
                "anthropic",
                "claude-sonnet-4-5",
                "proxy",
                30_000,
                100,
                20,
                10,
                5,
                200,
                "0.10",
            )?;
            insert_usage_log(
                &conn,
                "claude-session-cache-creation-mismatch",
                "claude",
                "_session",
                "claude-sonnet-4-5",
                "session_log",
                30_060,
                100,
                20,
                10,
                0,
                200,
                "0.10",
            )?;
        }

        let summary = db.get_usage_summary(None, None, None, None, None)?;
        assert_eq!(summary.total_requests, 9);

        let logs = db.get_request_logs(&LogFilters::default(), 0, 10)?;
        let request_ids: Vec<&str> = logs
            .data
            .iter()
            .map(|log| log.request_id.as_str())
            .collect();
        assert_eq!(logs.total, 9);
        assert!(request_ids.contains(&"session-outside-window"));
        assert!(request_ids.contains(&"session-token-mismatch"));
        assert!(request_ids.contains(&"session-app-mismatch"));
        assert!(request_ids.contains(&"session-model-mismatch"));
        assert!(request_ids.contains(&"session-matches-error-proxy"));
        assert!(request_ids.contains(&"claude-session-cache-creation-mismatch"));

        Ok(())
    }

    #[test]
    fn test_get_model_stats() -> Result<(), AppError> {
        let db = Database::memory()?;

        // Insert test data
        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "req1",
                    "p1",
                    "claude",
                    "claude-3-sonnet",
                    100,
                    50,
                    "0.01",
                    100,
                    200,
                    1000
                ],
            )?;
        }

        let stats = db.get_model_stats(None, None, None, None, None)?;
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].model, "claude-3-sonnet");
        assert_eq!(stats[0].request_count, 1);

        Ok(())
    }

    #[test]
    fn test_get_provider_stats_with_time_filter() -> Result<(), AppError> {
        let db = Database::memory()?;

        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params!["old", "p1", "claude", "claude-3", 100, 50, "0.01", 100, 200, 1000],
            )?;
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params!["new", "p1", "claude", "claude-3", 200, 75, "0.02", 120, 200, 2000],
            )?;
        }

        let stats = db.get_provider_stats(Some(1500), Some(2500), Some("claude"), None, None)?;
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].provider_id, "p1");
        assert_eq!(stats[0].request_count, 1);
        assert_eq!(stats[0].total_tokens, 275);

        Ok(())
    }

    #[test]
    fn test_get_provider_stats_labels_opencode_session_provider() -> Result<(), AppError> {
        let db = Database::memory()?;

        {
            let conn = lock_conn!(db.conn);
            insert_usage_log(
                &conn,
                "opencode-session",
                "opencode",
                "_opencode_session",
                "opencode-model",
                "opencode_session",
                1000,
                100,
                50,
                0,
                0,
                200,
                "0.01",
            )?;
        }

        let stats = db.get_provider_stats(None, None, Some("opencode"), None, None)?;
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].provider_id, "_opencode_session");
        assert_eq!(stats[0].provider_name, "OpenCode (Session)");

        Ok(())
    }

    #[test]
    fn test_get_provider_stats_excludes_partial_rollup_boundary_days() -> Result<(), AppError> {
        let db = Database::memory()?;
        let start = local_ts(2024, 2, 1, 12, 0, 0);
        let end = local_ts(2024, 2, 3, 12, 0, 0);

        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-02-01",
                    "claude",
                    "p-rollup",
                    "claude-3",
                    5,
                    5,
                    500,
                    250,
                    0,
                    0,
                    "0.50",
                    100
                ],
            )?;
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-02-02",
                    "claude",
                    "p-rollup",
                    "claude-3",
                    8,
                    7,
                    800,
                    400,
                    0,
                    0,
                    "0.80",
                    120
                ],
            )?;
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-02-03",
                    "claude",
                    "p-rollup",
                    "claude-3",
                    12,
                    11,
                    1200,
                    600,
                    0,
                    0,
                    "1.20",
                    140
                ],
            )?;
        }

        let stats = db.get_provider_stats(Some(start), Some(end), Some("claude"), None, None)?;
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].provider_id, "p-rollup");
        assert_eq!(stats[0].request_count, 8);
        assert_eq!(stats[0].total_tokens, 1200);

        Ok(())
    }

    #[test]
    fn test_get_daily_trends_respects_shorter_than_24_hours() -> Result<(), AppError> {
        let db = Database::memory()?;

        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "req-short",
                    "p1",
                    "claude",
                    "claude-3",
                    100,
                    50,
                    "0.01",
                    100,
                    200,
                    10_800
                ],
            )?;
        }

        let stats = db.get_daily_trends(Some(0), Some(15 * 60 * 60), Some("claude"), None, None)?;
        assert_eq!(stats.len(), 15);
        assert_eq!(stats[3].request_count, 1);

        Ok(())
    }

    #[test]
    fn test_get_daily_trends_groups_ranges_longer_than_24_hours_by_local_day(
    ) -> Result<(), AppError> {
        let db = Database::memory()?;
        let start = local_ts(2024, 3, 1, 12, 0, 0);
        let end = local_ts(2024, 3, 3, 12, 0, 0);

        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "day-1-detail",
                    "p1",
                    "claude",
                    "claude-3",
                    100,
                    50,
                    "0.01",
                    100,
                    200,
                    local_ts(2024, 3, 1, 13, 0, 0)
                ],
            )?;
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "day-3-detail",
                    "p1",
                    "claude",
                    "claude-3",
                    200,
                    75,
                    "0.02",
                    110,
                    200,
                    local_ts(2024, 3, 3, 10, 0, 0)
                ],
            )?;
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-03-02",
                    "claude",
                    "p1",
                    "claude-3",
                    4,
                    4,
                    400,
                    200,
                    0,
                    0,
                    "0.40",
                    120
                ],
            )?;
        }

        let stats = db.get_daily_trends(Some(start), Some(end), Some("claude"), None, None)?;
        assert_eq!(stats.len(), 3);
        assert_eq!(stats[0].request_count, 1);
        assert_eq!(stats[0].total_tokens, 150);
        assert_eq!(stats[1].request_count, 4);
        assert_eq!(stats[1].total_tokens, 600);
        assert_eq!(stats[2].request_count, 1);
        assert_eq!(stats[2].total_tokens, 275);

        Ok(())
    }

    #[test]
    fn test_get_model_stats_excludes_partial_rollup_boundary_days() -> Result<(), AppError> {
        let db = Database::memory()?;
        let start = local_ts(2024, 4, 1, 12, 0, 0);
        let end = local_ts(2024, 4, 3, 12, 0, 0);

        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-04-01",
                    "claude",
                    "p1",
                    "claude-3-haiku",
                    6,
                    6,
                    600,
                    300,
                    0,
                    0,
                    "0.60",
                    100
                ],
            )?;
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-04-02",
                    "claude",
                    "p1",
                    "claude-3-haiku",
                    9,
                    8,
                    900,
                    450,
                    0,
                    0,
                    "0.90",
                    110
                ],
            )?;
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model,
                    request_count, success_count, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, avg_latency_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "2024-04-03",
                    "claude",
                    "p1",
                    "claude-3-haiku",
                    12,
                    11,
                    1200,
                    600,
                    0,
                    0,
                    "1.20",
                    130
                ],
            )?;
        }

        let stats = db.get_model_stats(Some(start), Some(end), Some("claude"), None, None)?;
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].model, "claude-3-haiku");
        assert_eq!(stats[0].request_count, 9);
        assert_eq!(stats[0].total_tokens, 1350);

        Ok(())
    }

    #[test]
    fn test_strip_model_date_suffix_is_utf8_safe() {
        assert_eq!(
            strip_model_date_suffix("modèle-2026-05-14").as_deref(),
            Some("modèle")
        );
        assert_eq!(strip_model_date_suffix("abc🚀12345678"), None);
    }

    #[test]
    fn test_strip_model_date_suffix_handles_six_digit_yymmdd() {
        // The Volcengine Ark 6-digit YYMMDD suffix must be stripped (the whole doubao family uses it).
        assert_eq!(
            strip_model_date_suffix("doubao-seed-2-1-pro-260628").as_deref(),
            Some("doubao-seed-2-1-pro")
        );
        assert_eq!(
            strip_model_date_suffix("doubao-seed-1-6-250615").as_deref(),
            Some("doubao-seed-1-6")
        );
        // The 8-digit YYYYMMDD form is still stripped as before.
        assert_eq!(
            strip_model_date_suffix("claude-3-5-sonnet-20241022").as_deref(),
            Some("claude-3-5-sonnet")
        );
        // A 6-digit tail with an invalid month/day (a version number etc.) is not stripped, avoiding false hits.
        assert_eq!(strip_model_date_suffix("foo-bar-123456"), None); // month=34
        assert_eq!(strip_model_date_suffix("widget-209900"), None); // month=99
        assert_eq!(strip_model_date_suffix("gizmo-251200"), None); // day=00
    }

    #[test]
    fn test_pricing_resolves_volcengine_dated_model_to_bare_seed_row() -> Result<(), AppError> {
        // Regression: real Volcengine usage carries a 6-digit date suffix (doubao-seed-2-1-pro-260628)
        // and must normalize onto the bare seed row in the pricing table (doubao-seed-2-1-pro), or cost shows $0.
        let db = Database::memory()?;
        let conn = lock_conn!(db.conn);

        conn.execute(
            "INSERT OR REPLACE INTO model_pricing (
                model_id, display_name, input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
            ) VALUES ('doubao-seed-2-1-pro', 'Doubao Seed 2.1 Pro', '0.84', '4.2', '0.17', '0')",
            [],
        )?;

        let row = find_model_pricing_row(&conn, "doubao-seed-2-1-pro-260628")?;
        assert!(
            row.is_some(),
            "a dated Volcengine model must hit the bare-name pricing row via 6-digit date stripping"
        );
        let (input, output, ..) = row.unwrap();
        assert_eq!(input, "0.84");
        assert_eq!(output, "4.2");

        Ok(())
    }

    #[test]
    fn test_prefix_pricing_does_not_match_short_base_model_to_variant() -> Result<(), AppError> {
        let db = Database::memory()?;
        let conn = lock_conn!(db.conn);

        conn.execute("DELETE FROM model_pricing WHERE model_id LIKE 'gpt-5%'", [])?;
        for (model_id, display_name) in [("gpt-5-mini", "GPT-5 Mini"), ("gpt-5-pro", "GPT-5 Pro")] {
            conn.execute(
                "INSERT INTO model_pricing (
                    model_id, display_name, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
                ) VALUES (?1, ?2, '1', '2', '0', '0')",
                params![model_id, display_name],
            )?;
        }

        let result = find_model_pricing_row(&conn, "gpt-5")?;
        assert!(
            result.is_none(),
            "without base gpt-5 pricing, the prefix must not falsely match gpt-5-mini/gpt-5-pro"
        );

        Ok(())
    }

    #[test]
    fn test_model_pricing_matching() -> Result<(), AppError> {
        let db = Database::memory()?;
        let conn = lock_conn!(db.conn);

        // Prepare extra pricing data covering the prefix/suffix cleanup scenarios
        conn.execute(
            "INSERT OR REPLACE INTO model_pricing (
                model_id, display_name, input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
            ) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                "claude-haiku-4.5",
                "Claude Haiku 4.5",
                "1.0",
                "2.0",
                "0.0",
                "0.0"
            ],
        )?;

        // Test the exact match (seed_model_pricing already seeds claude-sonnet-4-5-20250929)
        let result = find_model_pricing_row(&conn, "claude-sonnet-4-5-20250929")?;
        assert!(
            result.is_some(),
            "claude-sonnet-4-5-20250929 must match exactly"
        );

        // Cleanup: strip the prefix and the colon suffix
        let result = find_model_pricing_row(&conn, "anthropic/claude-haiku-4.5")?;
        assert!(
            result.is_some(),
            "the prefixed model anthropic/claude-haiku-4.5 must match claude-haiku-4.5"
        );
        let result = find_model_pricing_row(&conn, "moonshotai/kimi-k2-0905:exa")?;
        assert!(
            result.is_some(),
            "a model with a prefix and a colon suffix must match kimi-k2-0905 after cleanup"
        );

        // Cleanup: @ becomes - (seed_model_pricing already seeds gpt-5.2-codex-low)
        let result = find_model_pricing_row(&conn, "gpt-5.2-codex@low")?;
        assert!(
            result.is_some(),
            "the @-separated model gpt-5.2-codex@low must match gpt-5.2-codex-low"
        );
        let result = find_model_pricing_row(&conn, "OpenAI/GPT-5.5@HIGH")?;
        assert!(
            result.is_some(),
            "the mixed-case GPT-5.5 model must normalize onto gpt-5.5-high"
        );
        let result = find_model_pricing_row(&conn, "OpenAI/GPT-5.5-2026-05-14")?;
        assert!(
            result.is_some(),
            "an OpenAI model with a date suffix must fall back to the base gpt-5.5 pricing"
        );
        let result = find_model_pricing_row(&conn, "google/gemini-3-pro-preview-20260514")?;
        assert!(
            result.is_some(),
            "a Gemini model with a date suffix must fall back to the base gemini-3-pro-preview pricing"
        );

        // Claude Desktop short route IDs: must prefix-match the dated pricing
        let result = find_model_pricing_row(&conn, "claude-haiku-4-5")?;
        assert!(
            result.is_some(),
            "the Claude Desktop short route claude-haiku-4-5 must match claude-haiku-4-5-20251001"
        );
        let result = find_model_pricing_row(&conn, "anthropic/claude-opus-4.8")?;
        assert!(
            result.is_some(),
            "the aggregator dotted form anthropic/claude-opus-4.8 must match claude-opus-4-8"
        );

        // Legacy/oddly wrapped non-Anthropic route from Claude Desktop: claude-gpt-5.5 -> gpt-5.5
        let result = find_model_pricing_row(&conn, "claude-gpt-5.5")?;
        assert!(
            result.is_some(),
            "a non-Anthropic model wrapped in claude- must match the real model pricing after stripping"
        );

        // Common Bedrock/Vertex shapes: provider prefix + -vN suffix + :0 modifier
        let result =
            find_model_pricing_row(&conn, "global.anthropic.claude-haiku-4-5-20251001-v1:0")?;
        assert!(
            result.is_some(),
            "a Bedrock/Vertex style Claude model ID must normalize onto the base Claude model pricing"
        );
        let result = find_model_pricing_row(&conn, "global.anthropic.claude-opus-4-8-v1:0")?;
        assert!(
            result.is_some(),
            "a Bedrock style Claude Opus 4.8 model ID must normalize onto the base Claude model pricing"
        );
        let result = find_model_pricing_row(&conn, "claude-opus-4-8@20260527")?;
        assert!(
            result.is_some(),
            "a Vertex style Claude Opus 4.8 model ID must normalize onto the base Claude model pricing"
        );

        // Reasoning effort suffix: falls back to the base model when there is no dedicated price
        let result = find_model_pricing_row(&conn, "gpt-5.4@low")?;
        assert!(
            result.is_some(),
            "without a dedicated effort price it must fall back to the base gpt-5.4 model pricing"
        );

        // Kimi Code is a subscription/quota model and must not pose as a public per-token model
        let result = find_model_pricing_row(&conn, "kimi-for-coding")?;
        assert!(
            result.is_none(),
            "kimi-for-coding has no fixed per-token price"
        );

        // Test a model that does not exist
        let result = find_model_pricing_row(&conn, "unknown-model-123")?;
        assert!(result.is_none(), "a nonexistent model must not match");

        Ok(())
    }
}
