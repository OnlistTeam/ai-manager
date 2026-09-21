//! Official subscription quota query service
//!
//! Reads the CLI tool's existing OAuth credentials to query official subscription quota.
//! First layer: only reads credentials, does not implement login/refresh.

use serde::{Deserialize, Serialize};

// ── Data types ──────────────────────────────────────────────

/// Credential status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStatus {
    Valid,
    Expired,
    NotFound,
    ParseError,
}

/// A single rate-limit window (e.g. 5-hour session, 7-day cycle)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaTier {
    /// Window identifier: five_hour, seven_day, seven_day_fable, seven_day_opus, etc.
    pub name: String,
    /// Usage percentage 0-100
    pub utilization: f64,
    /// ISO 8601 reset time
    pub resets_at: Option<String>,
    /// ZenMux: quota used (USD)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_value_usd: Option<f64>,
    /// ZenMux: window cap (USD)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value_usd: Option<f64>,
}

/// Overage usage info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraUsage {
    pub is_enabled: bool,
    pub monthly_limit: Option<f64>,
    pub used_credits: Option<f64>,
    pub utilization: Option<f64>,
    pub currency: Option<String>,
}

/// Subscription quota query result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionQuota {
    pub tool: String,
    pub credential_status: CredentialStatus,
    pub credential_message: Option<String>,
    pub success: bool,
    pub tiers: Vec<QuotaTier>,
    pub extra_usage: Option<ExtraUsage>,
    pub error: Option<String>,
    pub queried_at: Option<i64>,
}

impl SubscriptionQuota {}

// ── Claude credential reading ──────────────────────────────────────

// ── Claude API query ──────────────────────────────────────

// ── Codex credential reading ──────────────────────────────────────

// ── Codex API query ──────────────────────────────────────

// ── Gemini credential reading ──────────────────────────────────────

// ── Gemini token refresh ──────────────────────────────────────

// ── Gemini API query ──────────────────────────────────────

// ── Entry functions ──────────────────────────────────────────────

// ── Helper functions ──────────────────────────────────────────────

#[cfg(test)]
mod tests {}
