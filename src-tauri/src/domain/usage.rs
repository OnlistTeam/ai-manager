use serde::{Deserialize, Serialize};

use super::ToolId;

/// Small, aggregate-only Usage metric set. Costs are decimal USD strings so
/// the wire format never introduces binary-float rounding into money values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetrics {
    pub requests: u64,
    pub estimated_cost_usd: String,
    pub tokens: u64,
    pub success_rate_percent: f64,
    pub cache_hit_rate_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageToolBreakdown {
    pub tool: ToolId,
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageDay {
    /// Local calendar date in `YYYY-MM-DD` form.
    pub date: String,
    pub requests: u64,
    pub estimated_cost_usd: String,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageOverview {
    pub period_days: u16,
    pub start_date: String,
    pub end_date: String,
    pub summary: UsageMetrics,
    pub by_tool: Vec<UsageToolBreakdown>,
    pub trend: Vec<UsageDay>,
}

/// Sanitized result of an explicit local-session sync. Parser error strings,
/// session paths and request identities deliberately do not cross the API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageSyncSummary {
    pub files_scanned: u32,
    pub records_imported: u32,
    pub records_skipped: u32,
    pub source_issues: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageRefreshResult {
    pub overview: UsageOverview,
    pub sync: UsageSyncSummary,
}

#[cfg(test)]
mod tests {
    use super::{UsageDay, UsageMetrics, UsageOverview, UsageToolBreakdown};
    use crate::domain::ToolId;

    #[test]
    fn overview_wire_format_is_aggregate_only_and_camel_case() {
        let overview = UsageOverview {
            period_days: 30,
            start_date: "2026-07-26".to_string(),
            end_date: "2026-08-24".to_string(),
            summary: UsageMetrics {
                requests: 12,
                estimated_cost_usd: "1.250000".to_string(),
                tokens: 42_000,
                success_rate_percent: 91.5,
                cache_hit_rate_percent: 72.0,
            },
            by_tool: vec![UsageToolBreakdown {
                tool: ToolId::Codex,
                metrics: UsageMetrics {
                    requests: 12,
                    estimated_cost_usd: "1.250000".to_string(),
                    tokens: 42_000,
                    success_rate_percent: 91.5,
                    cache_hit_rate_percent: 72.0,
                },
            }],
            trend: vec![UsageDay {
                date: "2026-08-24".to_string(),
                requests: 12,
                estimated_cost_usd: "1.250000".to_string(),
                tokens: 42_000,
            }],
        };

        let value = serde_json::to_value(overview).expect("serialize Usage overview");
        assert_eq!(value["periodDays"], 30);
        assert_eq!(value["byTool"][0]["tool"], "codex");
        assert_eq!(value["summary"]["estimatedCostUsd"], "1.250000");
        for forbidden in [
            "requestId",
            "sessionId",
            "provider",
            "model",
            "prompt",
            "errorMessage",
            "path",
            "credential",
        ] {
            assert!(
                !value.to_string().contains(forbidden),
                "aggregate Usage payload leaked {forbidden}"
            );
        }
    }
}
