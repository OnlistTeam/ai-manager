//! What a deep link's tool-native `config` blob actually says (ADR-0029).
//!
//! Deciding whether a config carries a credential must not grow a second table
//! of secret field names. This module consults the two authorities the product
//! already has: `api_key_slots`, the write-slot table that mirrors the upstream
//! read order verbatim, and `resolve_usage_credentials`, the upstream reader
//! itself — which also covers the tools whose credential does not live in a
//! simple two-level slot (Codex's TOML text, OpenClaw, Hermes, Pi, Grok Build).
//!
//! The same pass yields the base URL and key the link did not spell out in its
//! own parameters, so the caller never has to interpret the blob itself.

use serde_json::Value;

use super::advanced::safe_base_url;
use super::{api_key_slots, app_type_for, slot_value};
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::ToolId;
use crate::provider::Provider as UpstreamProvider;

pub struct DeepLinkConfigEvidence {
    pub carries_credential: bool,
    /// A credential-free HTTP(S) base URL, when the config names one.
    pub base_url: Option<String>,
    /// The credential itself. It stays inside native code: the preview reports
    /// only which field carried one, never the value.
    pub api_key: Option<String>,
}

pub fn inspect_config(tool: ToolId, config: &Value) -> DeepLinkConfigEvidence {
    if !capabilities_for(tool).can_manage_provider {
        // A tool with no upstream service format has no reader that could prove
        // the blob is harmless, so treat it as credential-bearing.
        return DeepLinkConfigEvidence {
            carries_credential: true,
            base_url: None,
            api_key: None,
        };
    }

    let slot_filled = api_key_slots(tool)
        .iter()
        .any(|slot| slot_value(config, slot).is_some());

    let probe = UpstreamProvider::with_id(String::new(), String::new(), config.clone(), None);
    let (base_url, api_key) = probe.resolve_usage_credentials(&app_type_for(tool));
    let api_key = (!api_key.trim().is_empty()).then_some(api_key);

    DeepLinkConfigEvidence {
        carries_credential: slot_filled || api_key.is_some(),
        base_url: safe_base_url(base_url),
        api_key,
    }
}

#[cfg(test)]
mod tests {
    use super::inspect_config;
    use crate::domain::ToolId;
    use serde_json::json;

    #[test]
    fn the_upstream_read_order_decides_what_counts_as_a_credential() {
        let filled = inspect_config(
            ToolId::ClaudeCode,
            &json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "sk-ant-0123456789",
                    "ANTHROPIC_BASE_URL": "https://api.example.test"
                }
            }),
        );
        assert!(filled.carries_credential);
        assert_eq!(filled.base_url.as_deref(), Some("https://api.example.test"));
        assert_eq!(filled.api_key.as_deref(), Some("sk-ant-0123456789"));

        let placeholder = inspect_config(
            ToolId::ClaudeCode,
            &json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_BASE_URL": "https://api.example.test"
                }
            }),
        );
        assert!(!placeholder.carries_credential);
        assert!(placeholder.api_key.is_none());
        assert_eq!(
            placeholder.base_url.as_deref(),
            Some("https://api.example.test")
        );
    }

    #[test]
    fn tools_whose_credential_lives_outside_a_slot_are_still_covered() {
        let hermes = inspect_config(
            ToolId::Hermes,
            &json!({ "base_url": "https://api.example.test", "api_key": "zq83ndkwpe" }),
        );
        assert!(hermes.carries_credential, "Hermes flattens its credential");
        assert_eq!(hermes.api_key.as_deref(), Some("zq83ndkwpe"));

        let open_code = inspect_config(
            ToolId::OpenCode,
            &json!({ "options": { "baseURL": "https://api.example.test", "apiKey": "zq83ndkwpe" } }),
        );
        assert!(open_code.carries_credential);
    }

    #[test]
    fn a_tool_without_a_service_format_fails_closed() {
        let evidence = inspect_config(ToolId::KimiCode, &json!({ "anything": "at all" }));
        assert!(evidence.carries_credential);
        assert!(evidence.base_url.is_none());
        assert!(evidence.api_key.is_none());
    }

    #[test]
    fn an_endpoint_that_hides_a_credential_is_not_reported_as_a_base_url() {
        let evidence = inspect_config(
            ToolId::ClaudeCode,
            &json!({ "env": { "ANTHROPIC_BASE_URL": "https://user:secret@api.example.test" } }),
        );
        assert!(evidence.base_url.is_none());
    }
}
