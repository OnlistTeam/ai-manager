//! Local-routing projection (ADR-0007, ADR-0054).
//!
//! The inherited proxy engine contains credentials, raw request errors and live
//! configuration backups. None of those belong on the product wire. This file
//! deliberately exposes only aggregate counters, provider identity and health.

use serde::{Deserialize, Serialize};

use super::ToolId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingProvider {
    pub id: String,
    pub name: String,
    /// One-based queue priority. `None` means the provider is available but is
    /// not currently part of the failover queue.
    pub priority: Option<u32>,
    pub current: bool,
    pub healthy: bool,
    pub consecutive_failures: u32,
}

/// Why a tool cannot be routed through AI Manager right now (ADR-0054).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoutingUnavailable {
    /// The tool has no endpoint chosen.
    NoService,
    /// The tool signs in with its own account: there is no address and key
    /// the local route could forward to.
    OwnLogin,
    /// The chosen endpoint lacks the address or key the local route needs.
    Incomplete,
}

/// When an already open session of the tool sees a change to its settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoutingPickup {
    /// The tool rereads its settings file while it runs.
    Live,
    /// The tool reads its settings once, when it starts.
    AtStart,
}

/// A tool routed through AI Manager, as the quit confirmation names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutedTool {
    pub tool: ToolId,
    pub pickup: RoutingPickup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingTarget {
    pub tool: ToolId,
    pub takeover_enabled: bool,
    pub auto_failover_enabled: bool,
    pub current_provider: Option<RoutingProvider>,
    pub queue: Vec<RoutingProvider>,
    pub available: Vec<RoutingProvider>,
    /// `None` when the tool can be routed; otherwise the one reason it cannot.
    pub unavailable: Option<RoutingUnavailable>,
    pub pickup: RoutingPickup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingOverview {
    pub running: bool,
    /// Present only while the local server is running.
    pub address: Option<String>,
    pub port: Option<u16>,
    pub active_connections: u64,
    pub total_requests: u64,
    pub success_requests: u64,
    pub failed_requests: u64,
    pub failover_count: u64,
    pub targets: Vec<RoutingTarget>,
}

#[cfg(test)]
mod tests {
    use super::{RoutingOverview, RoutingPickup, RoutingProvider, RoutingTarget};
    use crate::domain::ToolId;

    #[test]
    fn wire_projection_contains_no_secrets_paths_notes_or_raw_errors() {
        let overview = RoutingOverview {
            running: true,
            address: Some("127.0.0.1".to_string()),
            port: Some(15_721),
            active_connections: 1,
            total_requests: 4,
            success_requests: 3,
            failed_requests: 1,
            failover_count: 1,
            targets: vec![RoutingTarget {
                tool: ToolId::ClaudeCode,
                takeover_enabled: true,
                auto_failover_enabled: true,
                current_provider: Some(RoutingProvider {
                    id: "provider-a".to_string(),
                    name: "Provider A".to_string(),
                    priority: Some(1),
                    current: true,
                    healthy: true,
                    consecutive_failures: 0,
                }),
                queue: vec![],
                available: vec![],
                unavailable: None,
                pickup: RoutingPickup::Live,
            }],
        };

        let json = serde_json::to_value(overview).expect("serialize routing overview");
        let object = json.as_object().expect("overview object");
        assert_eq!(object.get("activeConnections"), Some(&serde_json::json!(1)));
        let encoded = json.to_string().to_ascii_lowercase();
        for forbidden in [
            "token",
            "secret",
            "api_key",
            "apikey",
            "settingsconfig",
            "provider_notes",
            "providernotes",
            "last_error",
            "lasterror",
            "backup",
            "path",
        ] {
            assert!(!encoded.contains(forbidden), "wire leaked {forbidden}");
        }
    }
}
