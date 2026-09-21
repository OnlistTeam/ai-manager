//! The product wire format of the CC Switch import (spec §17).
//!
//! Only the counts the user needs to see may appear here. Provider settings, API keys, MCP
//! server_config and skill paths from the source database all stop in the backend and never enter
//! the serialized model (§19).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub services: u32,
    pub mcp_servers: u32,
    pub skills: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub available: bool,
    pub summary: ImportSummary,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub imported: ImportSummary,
}

#[cfg(test)]
mod tests {
    use super::{ImportOutcome, ImportPreview, ImportSummary};

    #[test]
    fn preview_and_outcome_expose_counts_but_no_source_payload() {
        let summary = ImportSummary {
            services: 12,
            mcp_servers: 8,
            skills: 15,
        };
        let preview = ImportPreview {
            available: true,
            summary,
        };
        assert_eq!(
            serde_json::to_string(&preview).expect("serialize preview"),
            r#"{"available":true,"summary":{"services":12,"mcpServers":8,"skills":15}}"#
        );
        assert_eq!(
            serde_json::to_string(&ImportOutcome { imported: summary }).expect("serialize outcome"),
            r#"{"imported":{"services":12,"mcpServers":8,"skills":15}}"#
        );

        let wire = serde_json::to_value(preview).expect("preview value");
        let text = wire.to_string().to_ascii_lowercase();
        for forbidden in ["settingsconfig", "serverconfig", "apikey", "path"] {
            assert!(!text.contains(forbidden), "wire leaked {forbidden}");
        }
    }

    #[test]
    fn missing_source_is_an_available_false_zero_summary() {
        assert_eq!(
            ImportPreview::default(),
            ImportPreview {
                available: false,
                summary: ImportSummary::default(),
            }
        );
    }
}
