//! Aggregate-only compatibility facade for Advanced Usage (ADR-0006).
//!
//! This is the only product module allowed to see the upstream UsageSummary,
//! DailyStats, session parsers, Database and AppState. Raw request/session rows
//! never enter the product Domain API.

use std::sync::Arc;

use chrono::{DateTime, Duration, Local, LocalResult, NaiveDate, TimeZone};

use crate::database::Database;
use crate::domain::{
    AppError, ErrorCode, ToolId, UsageDay, UsageMetrics, UsageOverview, UsageRefreshResult,
    UsageSyncSummary, UsageToolBreakdown,
};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::services::session_usage::{self, SessionSyncResult};
use crate::services::usage_stats::{DailyStats, UsageSummary};
use crate::store::AppState;

const PERIOD_DAYS: u16 = 30;

fn detail<E: std::fmt::Display>(error: E) -> String {
    truncate_tail(&redact_secrets(&error.to_string()), 20, 2000)
}

fn read_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.usage.readFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn join_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::Internal, "error.command.joinFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn normalized_cost(raw: &str) -> String {
    raw.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| format!("{value:.6}"))
        .unwrap_or_else(|| "0.000000".to_string())
}

fn normalized_percent(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 100.0)
    } else {
        0.0
    }
}

fn metrics(summary: UsageSummary) -> UsageMetrics {
    UsageMetrics {
        requests: summary.total_requests,
        estimated_cost_usd: normalized_cost(&summary.total_cost),
        tokens: summary.real_total_tokens,
        success_rate_percent: normalized_percent(f64::from(summary.success_rate)),
        cache_hit_rate_percent: normalized_percent(summary.cache_hit_rate * 100.0),
    }
}

fn tool_for_app_type(value: &str) -> Option<ToolId> {
    match value.trim().to_ascii_lowercase().as_str() {
        "claude" | "claude-desktop" | "claude_desktop" => Some(ToolId::ClaudeCode),
        "codex" => Some(ToolId::Codex),
        "opencode" => Some(ToolId::OpenCode),
        "gemini" => Some(ToolId::GeminiCli),
        "grokbuild" | "grok-build" | "grok_build" | "grok" => Some(ToolId::GrokBuild),
        "openclaw" => Some(ToolId::OpenClaw),
        "hermes" => Some(ToolId::Hermes),
        "pi" => Some(ToolId::Pi),
        _ => None,
    }
}

struct Period {
    start_timestamp: i64,
    end_timestamp: i64,
    start_date: String,
    end_date: String,
}

fn local_midnight(day: NaiveDate) -> Result<DateTime<Local>, AppError> {
    let naive = day
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| read_failed(format!("could not construct local midnight for {day}")))?;
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(value) | LocalResult::Ambiguous(value, _) => Ok(value),
        LocalResult::None => Err(read_failed(format!(
            "local midnight is unavailable for {day}"
        ))),
    }
}

fn period_ending_at(now: DateTime<Local>) -> Result<Period, AppError> {
    let end_day = now.date_naive();
    let start_day = end_day - Duration::days(i64::from(PERIOD_DAYS - 1));
    Ok(Period {
        start_timestamp: local_midnight(start_day)?.timestamp(),
        end_timestamp: now.timestamp(),
        start_date: start_day.format("%Y-%m-%d").to_string(),
        end_date: end_day.format("%Y-%m-%d").to_string(),
    })
}

fn normalized_day(raw: &str) -> Result<String, AppError> {
    let candidate = raw.get(..10).unwrap_or(raw);
    NaiveDate::parse_from_str(candidate, "%Y-%m-%d")
        .map(|day| day.format("%Y-%m-%d").to_string())
        .map_err(read_failed)
}

fn trend_day(day: DailyStats) -> Result<UsageDay, AppError> {
    Ok(UsageDay {
        date: normalized_day(&day.date)?,
        requests: day.request_count,
        estimated_cost_usd: normalized_cost(&day.total_cost),
        tokens: day
            .total_input_tokens
            .saturating_add(day.total_output_tokens)
            .saturating_add(day.total_cache_creation_tokens)
            .saturating_add(day.total_cache_read_tokens),
    })
}

fn overview_with(db: &Database, now: DateTime<Local>) -> Result<UsageOverview, AppError> {
    let period = period_ending_at(now)?;
    let summary = db
        .get_usage_summary(
            Some(period.start_timestamp),
            Some(period.end_timestamp),
            None,
            None,
            None,
        )
        .map_err(read_failed)?;
    let upstream_by_tool = db
        .get_usage_summary_by_app(
            Some(period.start_timestamp),
            Some(period.end_timestamp),
            None,
            None,
        )
        .map_err(read_failed)?;
    let upstream_trend = db
        .get_daily_trends(
            Some(period.start_timestamp),
            Some(period.end_timestamp),
            None,
            None,
            None,
        )
        .map_err(read_failed)?;

    let by_tool = upstream_by_tool
        .into_iter()
        .filter_map(|entry| match tool_for_app_type(&entry.app_type) {
            Some(tool) => Some(UsageToolBreakdown {
                tool,
                metrics: metrics(entry.summary),
            }),
            None => {
                log::warn!(
                    "Advanced Usage omitted an unknown app aggregate: {}",
                    detail(entry.app_type)
                );
                None
            }
        })
        .collect();
    let trend = upstream_trend
        .into_iter()
        .map(trend_day)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(UsageOverview {
        period_days: PERIOD_DAYS,
        start_date: period.start_date,
        end_date: period.end_date,
        summary: metrics(summary),
        by_tool,
        trend,
    })
}

fn sync_summary(result: &SessionSyncResult) -> UsageSyncSummary {
    UsageSyncSummary {
        files_scanned: result.files_scanned,
        records_imported: result.imported,
        records_skipped: result.skipped,
        source_issues: u32::try_from(result.errors.len()).unwrap_or(u32::MAX),
    }
}

/// Product handle over the inherited Usage engine. The upstream Database stays
/// private to this facade.
pub struct UsageStore {
    db: Arc<Database>,
}

impl UsageStore {
    pub fn open(app_handle: &tauri::AppHandle) -> Result<Self, AppError> {
        let state = tauri::Manager::try_state::<AppState>(app_handle).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.usage.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        Ok(Self {
            db: state.db.clone(),
        })
    }

    pub fn overview(&self) -> Result<UsageOverview, AppError> {
        overview_with(&self.db, Local::now())
    }

    pub async fn refresh(self) -> Result<UsageRefreshResult, AppError> {
        let _guard = session_usage::session_sync_mutex().lock().await;
        tauri::async_runtime::spawn_blocking(move || {
            let sync = session_usage::sync_all_unlocked(&self.db);
            for error in &sync.errors {
                log::warn!("local Usage source reported: {}", detail(error));
            }
            Ok(UsageRefreshResult {
                overview: overview_with(&self.db, Local::now())?,
                sync: sync_summary(&sync),
            })
        })
        .await
        .map_err(join_failed)?
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeZone};

    use super::{metrics, normalized_cost, normalized_day, period_ending_at, tool_for_app_type};
    use crate::domain::ToolId;
    use crate::services::usage_stats::UsageSummary;

    #[test]
    fn every_product_tool_has_a_stable_usage_app_mapping() {
        let pairs = [
            ("claude", ToolId::ClaudeCode),
            ("codex", ToolId::Codex),
            ("opencode", ToolId::OpenCode),
            ("gemini", ToolId::GeminiCli),
            ("grokbuild", ToolId::GrokBuild),
            ("openclaw", ToolId::OpenClaw),
            ("hermes", ToolId::Hermes),
            ("pi", ToolId::Pi),
        ];
        for (app, tool) in pairs {
            assert_eq!(tool_for_app_type(app), Some(tool));
        }
        assert_eq!(tool_for_app_type("unknown-upstream-app"), None);
    }

    #[test]
    fn overview_metrics_are_bounded_and_money_is_normalized() {
        let projected = metrics(UsageSummary {
            total_requests: 3,
            total_cost: "1.2".to_string(),
            total_input_tokens: 1,
            total_output_tokens: 2,
            total_cache_creation_tokens: 3,
            total_cache_read_tokens: 4,
            success_rate: 140.0,
            real_total_tokens: 10,
            cache_hit_rate: -1.0,
        });
        assert_eq!(projected.estimated_cost_usd, "1.200000");
        assert_eq!(projected.tokens, 10);
        assert_eq!(projected.success_rate_percent, 100.0);
        assert_eq!(projected.cache_hit_rate_percent, 0.0);
        assert_eq!(normalized_cost("not-money"), "0.000000");
    }

    #[test]
    fn period_is_thirty_local_calendar_days_and_dates_are_normalized() {
        let now = Local
            .with_ymd_and_hms(2026, 8, 24, 15, 30, 0)
            .single()
            .expect("stable local date");
        let period = period_ending_at(now).expect("period");
        assert_eq!(period.start_date, "2026-07-26");
        assert_eq!(period.end_date, "2026-08-24");
        assert_eq!(
            normalized_day("2026-08-24T00:00:00+08:00").expect("day"),
            "2026-08-24"
        );
        assert!(normalized_day("not-a-date").is_err());
    }
}
