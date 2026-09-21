//! Backend snapshot for Quick Check (spec §37 / §38).
//!
//! Tool install/version state is already provided by `Tool`; this type only carries the three
//! kinds of ready-made information that must be read from the upstream database or the live
//! config. Network connectivity is not probed automatically here — the frontend merges in the
//! results of checks the user ran during this session.

use serde::{Deserialize, Serialize};

use super::ToolId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthProviderTarget {
    pub provider_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    pub tool: ToolId,
    pub configured: bool,
    pub configured_count: u32,
    pub check_targets: Vec<HealthProviderTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigReadStatus {
    Readable,
    /// The live file does not exist yet. That is "not configured", not a fault.
    Missing,
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigHealth {
    pub tool: ToolId,
    pub status: ConfigReadStatus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHealth {
    pub total: u32,
    pub enabled: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthSnapshot {
    pub providers: Vec<ProviderHealth>,
    pub configs: Vec<ConfigHealth>,
    pub mcp: McpHealth,
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigHealth, ConfigReadStatus, HealthProviderTarget, HealthSnapshot, McpHealth,
        ProviderHealth,
    };
    use crate::domain::ToolId;

    #[test]
    fn the_health_wire_is_camel_case_and_contains_no_diagnostic_text() {
        let snapshot = HealthSnapshot {
            providers: vec![ProviderHealth {
                tool: ToolId::ClaudeCode,
                configured: true,
                configured_count: 2,
                check_targets: vec![HealthProviderTarget {
                    provider_id: "active".to_string(),
                    name: "Active service".to_string(),
                }],
            }],
            configs: vec![ConfigHealth {
                tool: ToolId::ClaudeCode,
                status: ConfigReadStatus::Readable,
            }],
            mcp: McpHealth {
                total: 8,
                enabled: 2,
            },
        };
        assert_eq!(
            serde_json::to_string(&snapshot).expect("serialize health"),
            r#"{"providers":[{"tool":"claude-code","configured":true,"configuredCount":2,"checkTargets":[{"providerId":"active","name":"Active service"}]}],"configs":[{"tool":"claude-code","status":"readable"}],"mcp":{"total":8,"enabled":2}}"#
        );
    }

    #[test]
    fn every_config_state_has_a_stable_wire_value() {
        assert_eq!(
            serde_json::to_string(&ConfigReadStatus::Readable).expect("readable"),
            "\"readable\""
        );
        assert_eq!(
            serde_json::to_string(&ConfigReadStatus::Missing).expect("missing"),
            "\"missing\""
        );
        assert_eq!(
            serde_json::to_string(&ConfigReadStatus::Unreadable).expect("unreadable"),
            "\"unreadable\""
        );
    }
}
