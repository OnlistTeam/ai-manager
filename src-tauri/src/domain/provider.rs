//! The product's provider model (spec §10 / §33-35).
//!
//! This is a separate model from the upstream `crate::provider::Provider`: that one has
//! 12 fields, a free-form JSON `settings_config`, and a pile of things the product never
//! renders; this one only has the fields the frontend actually renders. The conversion
//! happens in exactly one place, `compat/ccswitch/provider.rs`.
//!
//! Secrets are projected to the renderer verbatim: this is a local desktop app, the
//! secret is already stored in plaintext in the SQLite file on this machine, and the
//! exact string is what the user wants to check, copy and edit. Masking only blocks the
//! user's own eyes, not anyone who has access to the machine. AI_RULES rule 6 constrains
//! **logs and error messages**; that boundary is unaffected here.

use serde::{Deserialize, Serialize};

use super::ToolId;

/// Official services (upstream `category == "official"`) and custom services.
/// Whether a read-only reachability probe is possible is expressed separately by
/// `Provider::testable` and is not inferred from the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Official,
    Custom,
}

/// A single service. The upstream `providers` table has an `app_type` column, so a row
/// belongs to exactly one tool (decision 4).
///
/// `Debug` is hand-written (see below): this struct carries the plaintext secret, and
/// `{:?}` is the easiest way for a secret to slip into the logs.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    /// Adding this provider does not select a default model or replace other providers.
    #[serde(default)]
    pub additive: bool,
    pub id: String,
    pub tool: ToolId,
    pub name: String,
    pub kind: ProviderKind,
    /// Whether this is the service the tool currently uses ("Now active" in §35).
    pub active: bool,
    pub base_url: Option<String>,
    /// The plaintext API key of this service; `None` when no key is configured.
    /// Empty and whitespace-only strings collapse to `None` — "no key" and "an empty key"
    /// are not the same thing.
    pub api_key: Option<String>,
    pub website_url: Option<String>,
    /// Whether there is anything to probe. The UI decides whether to render the "check"
    /// button from this, instead of from the tool or service name (AI_RULES rule 8).
    pub testable: bool,
    /// Whether the product may safely remove this row. The service currently in use and
    /// entries managed by a tool-specific config fail closed, and the UI must also explain
    /// why the action is disabled.
    pub can_remove: bool,
}

impl std::fmt::Debug for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Preserve the "has a key or not" shape (emptiness is genuine debug information), never the plaintext.
        f.debug_struct("Provider")
            .field("id", &self.id)
            .field("tool", &self.tool)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("active", &self.active)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_deref().map(|_| "***"))
            .field("website_url", &self.website_url)
            .field("testable", &self.testable)
            .field("can_remove", &self.can_remove)
            .finish()
    }
}

/// A service preset selectable when adding a connection.
///
/// The tool's native config template stays in the compatibility layer, so the
/// renderer cannot submit free-form config and pass it off as a preset. What it
/// may *not* do is hide the address: a connection whose endpoint is invisible
/// until it fails is how a Codex service saved without `/v1` went unnoticed
/// (ADR-0041 amendment). The same URLs already cross IPC for the preset speed
/// test, so projecting one here exposes nothing new.
///
/// The security rule that does still hold lives on the create path, not here:
/// creating *from a preset* sends only `preset_id` and the backend owns the
/// address, while an edited address goes through the custom-create path and is
/// recorded as custom. Showing the address never lets the renderer relabel its
/// own URL as a preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnectionPreset {
    pub id: String,
    pub service_name: String,
    pub default_name: String,
    pub default_model: String,
    pub base_url: String,
    pub website_url: String,
    pub api_key_url: String,
    pub official: bool,
}

/// The catalog of safe connections for a tool. The default preset must be one of `presets`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnectionProfile {
    pub default_preset_id: String,
    pub presets: Vec<ProviderConnectionPreset>,
    /// Whether the tool's native config must name a concrete model. Tools where this is
    /// `false` (Claude Code / Codex / Gemini CLI) have a CLI-side default when it is left
    /// empty; for the others the config itself is a model table (grok `models.default`,
    /// the model arrays of OpenCode/OpenClaw/Hermes/Pi, ...), so without a model there is
    /// no usable connection.
    pub model_required: bool,
}

/// Beginner Mode payload for creating a service (spec §34).
///
/// The API key still flows in one direction only, UI -> backend. The renderer only picks
/// a stable `preset_id`; base URL, headers, environment variables and other configuration
/// never enter the payload — the compatibility layer looks up the vetted template by
/// `(ToolId, preset_id)` and generates them.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCreateDraft {
    pub preset_id: String,
    pub name: String,
    pub api_key: String,
    pub model: String,
}

impl std::fmt::Debug for ProviderCreateDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderCreateDraft")
            .field("preset_id", &self.preset_id)
            .field("name", &self.name)
            .field("api_key", &"***")
            .field("model", &self.model)
            .finish()
    }
}

/// Payload for one tool-compatible custom HTTPS endpoint.
///
/// This remains a typed, inbound-only contract. The renderer can choose the
/// common connection fields, but it cannot submit tool-native JSON, headers,
/// auth modes or environment-variable names. The compatibility layer owns
/// those shapes and derives them from `ToolId`.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCustomCreateDraft {
    pub name: String,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl std::fmt::Debug for ProviderCustomCreateDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderCustomCreateDraft")
            .field("name", &self.name)
            .field("api_key", &"***")
            .field("model", &self.model)
            .field("base_url", &"<provided>")
            .finish()
    }
}

/// The safe return value of creating a service.
///
/// `created_provider_id` lets the product layer locate the freshly created row exactly and
/// start the address check, instead of guessing by name or diffing a list that may change
/// concurrently. The returned provider still carries only a masked key; this result has no
/// field that could carry a plaintext secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCreateResult {
    pub providers: Vec<Provider>,
    pub created_provider_id: String,
}

/// The safe config view the edit dialog reads on demand.
///
/// Only endpoints, model IDs, header names and editing capabilities are returned. Header
/// values and the full `settings_config` never cross the compatibility layer boundary; the
/// value of an existing header can only be kept, replaced or deleted, never read back
/// through the product API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEditProfile {
    pub provider_id: String,
    pub base_url: Option<String>,
    /// Saved alternate routes for this service. Every value has already been
    /// reduced to a credential-free HTTP(S) base URL by the compatibility layer.
    pub endpoint_candidates: Vec<String>,
    /// Whether an explicit speed-test action should select its fastest result.
    /// No background request is implied by this preference.
    pub endpoint_auto_select: bool,
    pub models: Vec<String>,
    pub header_names: Vec<String>,
    pub capabilities: ProviderEditCapabilities,
}

/// Editing capabilities come from the config shape the compatibility layer recognized; the UI never guesses fields from `ToolId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEditCapabilities {
    pub can_edit_base_url: bool,
    pub can_edit_endpoints: bool,
    pub can_edit_models: bool,
    pub can_edit_headers: bool,
    pub supports_multiple_models: bool,
}

/// One-way payload for a single header.
///
/// `value: None` is only valid for an existing header with the same name and means "keep
/// the old value"; a new header must carry a value. Omitting an existing header deletes it.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProviderHeaderDraft {
    pub name: String,
    pub value: Option<String>,
}

impl std::fmt::Debug for ProviderHeaderDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderHeaderDraft")
            .field("name", &self.name)
            .field("value", &self.value.as_deref().map(|_| "***"))
            .finish()
    }
}

/// Payload for endpoint and header changes.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProviderAdvancedDraft {
    /// Distinguishes “preserve” from `base_url: None` (“remove”).
    pub base_url_changed: bool,
    pub base_url: Option<String>,
    /// `None` preserves membership; `Some([])` clears every alternate route.
    pub endpoint_candidates: Option<Vec<String>>,
    /// `None` preserves the saved explicit-test preference.
    pub endpoint_auto_select: Option<bool>,
    pub headers: Option<Vec<ProviderHeaderDraft>>,
}

impl std::fmt::Debug for ProviderAdvancedDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderAdvancedDraft")
            .field("base_url_changed", &self.base_url_changed)
            .field("base_url", &self.base_url.as_deref().map(|_| "<provided>"))
            .field(
                "endpoint_candidates",
                &self.endpoint_candidates.as_ref().map(Vec::len),
            )
            .field("endpoint_auto_select", &self.endpoint_auto_select)
            .field("headers", &self.headers)
            .finish()
    }
}

/// Form payload for editing an existing service.
///
/// The key is **one-way**: UI -> backend only. `api_key: None` means "leave the stored key
/// untouched", so "just rename it" does not force the user to paste the key again, and the
/// backend never has to send the plaintext out just to populate the form (decision 3).
///
/// In-only: there is no `Serialize`, and this type must never be serialized back to the
/// frontend or written into a structured log field. `Debug` is hand-written (see below)
/// and replaces `api_key` with a placeholder, so `{:?}` cannot dump the plaintext key into
/// the logs (AI_RULES rule 6).
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProviderDraft {
    pub name: String,
    pub api_key: Option<String>,
    /// `None` = this form did not read or change the model, so it must be kept as is.
    pub models: Option<Vec<String>>,
    /// `None` = Beginner Mode, which must not rewrite endpoints or headers along the way.
    pub advanced: Option<ProviderAdvancedDraft>,
}

impl std::fmt::Debug for ProviderDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `Some("***")` / `None` — preserve the "has a value or not" shape, but never the plaintext.
        f.debug_struct("ProviderDraft")
            .field("name", &self.name)
            .field("api_key", &self.api_key.as_deref().map(|_| "***"))
            .field("models", &self.models)
            .field("advanced", &self.advanced)
            .finish()
    }
}

/// Tri-state connectivity. Upstream counts any HTTP response as "reachable" and only DNS /
/// connection refused / TLS / timeout as a failure — this is "is the address reachable",
/// not "is the key correct", and the copy must say so honestly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderReachability {
    Operational,
    Degraded,
    Failed,
}

/// A batch remains useful for ordinary service inventories while a damaged or
/// hostile local database cannot make one click schedule unbounded requests or
/// emit an unbounded operation payload.
pub const MAX_PROVIDER_TEST_ALL_RESULTS: usize = 512;
pub const PROVIDER_TEST_ALL_CONCURRENCY: usize = 8;

/// Probe result. **No message field** — the upstream message is a bare localized string
/// that may contain the address; the tri-state enum lets the frontend pick the copy and the
/// backend emits no display strings (decision 6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTestResult {
    pub provider_id: String,
    pub reachability: ProviderReachability,
    pub response_time_ms: Option<u64>,
    pub http_status: Option<u16>,
}

/// User-action-bound reachability gate for switching a service or opening a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderPreflightStatus {
    /// The checked address answered and the requested action may continue.
    Ready,
    /// The address did not answer and no configuration was changed.
    Unreachable,
    /// A saved compatible service answered and became active.
    FailedOver,
    /// No safe probe target exists; the original action may continue without a claim of health.
    NotChecked,
}

/// Safe result of a preflight/failover action. It contains no endpoint or credential fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreflightOutcome {
    pub status: ProviderPreflightStatus,
    pub origin_provider_id: Option<String>,
    pub active_provider_id: Option<String>,
    pub providers: Vec<Provider>,
    pub checks: Vec<ProviderTestResult>,
}

#[cfg(test)]
mod tests {
    use super::{
        Provider, ProviderAdvancedDraft, ProviderConnectionPreset, ProviderConnectionProfile,
        ProviderCreateDraft, ProviderCreateResult, ProviderCustomCreateDraft, ProviderDraft,
        ProviderEditCapabilities, ProviderEditProfile, ProviderKind, ProviderPreflightOutcome,
        ProviderPreflightStatus, ProviderReachability, ProviderTestResult,
    };
    use crate::domain::ToolId;

    fn sample() -> Provider {
        Provider {
            additive: false,
            id: "anthropic".to_string(),
            tool: ToolId::ClaudeCode,
            name: "Anthropic".to_string(),
            kind: ProviderKind::Custom,
            active: true,
            base_url: Some("https://api.anthropic.com".to_string()),
            api_key: Some("sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F".to_string()),
            website_url: None,
            testable: true,
            can_remove: false,
        }
    }

    #[test]
    fn the_wire_format_is_camel_case_and_carries_the_key_but_never_the_raw_config() {
        let json = serde_json::to_string(&sample()).expect("serialize provider");
        assert_eq!(
            json,
            r#"{"additive":false,"id":"anthropic","tool":"claude-code","name":"Anthropic","kind":"custom","active":true,"baseUrl":"https://api.anthropic.com","apiKey":"sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F","websiteUrl":null,"testable":true,"canRemove":false}"#
        );
        // That free-form JSON blob from upstream still never leaves the backend: the
        // renderer receives projected fields, not the raw config.
        assert!(!json.contains("settingsConfig"));
    }

    #[test]
    fn a_provider_round_trips_through_json() {
        let json = serde_json::to_string(&sample()).expect("serialize");
        let parsed: Provider = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, sample());
    }

    #[test]
    fn an_absent_api_key_in_a_draft_means_keep_the_stored_one() {
        let keep: ProviderDraft =
            serde_json::from_str(r#"{"name":"Anthropic","apiKey":null}"#).expect("draft");
        assert_eq!(keep.api_key, None);
        let replace: ProviderDraft =
            serde_json::from_str(r#"{"name":"Anthropic","apiKey":"sk-new"}"#).expect("draft");
        assert_eq!(replace.api_key.as_deref(), Some("sk-new"));
    }

    #[test]
    fn a_drafts_debug_output_never_contains_the_plaintext_key() {
        let with_key = ProviderDraft {
            name: "Anthropic".to_string(),
            api_key: Some("sk-ant-super-secret-value".to_string()),
            models: None,
            advanced: None,
        };
        let debugged = format!("{with_key:?}");
        assert!(!debugged.contains("sk-ant-super-secret-value"));
        assert!(debugged.contains("***"));

        let without_key = ProviderDraft {
            name: "Anthropic".to_string(),
            api_key: None,
            models: None,
            advanced: None,
        };
        assert_eq!(
            format!("{without_key:?}"),
            r#"ProviderDraft { name: "Anthropic", api_key: None, models: None, advanced: None }"#
        );
    }

    #[test]
    fn edit_profiles_have_no_field_that_can_carry_keys_or_header_values() {
        let profile = ProviderEditProfile {
            provider_id: "relay".to_string(),
            base_url: Some("https://relay.example".to_string()),
            endpoint_candidates: vec!["https://backup.example".to_string()],
            endpoint_auto_select: true,
            models: vec!["model-a".to_string()],
            header_names: vec!["Authorization".to_string()],
            capabilities: ProviderEditCapabilities {
                can_edit_base_url: true,
                can_edit_endpoints: true,
                can_edit_models: true,
                can_edit_headers: true,
                supports_multiple_models: false,
            },
        };
        let json = serde_json::to_string(&profile).expect("serialize edit profile");
        assert_eq!(
            json,
            r#"{"providerId":"relay","baseUrl":"https://relay.example","endpointCandidates":["https://backup.example"],"endpointAutoSelect":true,"models":["model-a"],"headerNames":["Authorization"],"capabilities":{"canEditBaseUrl":true,"canEditEndpoints":true,"canEditModels":true,"canEditHeaders":true,"supportsMultipleModels":false}}"#
        );
        for forbidden in ["apiKey", "headerValues", "settingsConfig"] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn provider_drafts_reject_raw_upstream_configuration_fields() {
        let error = serde_json::from_str::<ProviderDraft>(
            r#"{"name":"Relay","apiKey":null,"settingsConfig":{"auth":"secret"}}"#,
        )
        .expect_err("raw provider config must not cross the product boundary");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn advanced_draft_debug_output_does_not_log_a_credentialed_url() {
        let draft = ProviderAdvancedDraft {
            base_url_changed: true,
            base_url: Some("https://user:password@example.test/v1?token=query-secret".to_string()),
            endpoint_candidates: Some(vec![
                "https://user:password@example.test/v1?token=route-secret".to_string(),
            ]),
            endpoint_auto_select: Some(true),
            headers: None,
        };
        let debug = format!("{draft:?}");
        assert!(!debug.contains("password"));
        assert!(!debug.contains("query-secret"));
        assert!(!debug.contains("route-secret"));
        assert!(debug.contains("endpoint_candidates: Some(1)"));
        assert!(debug.contains("<provided>"));
    }

    #[test]
    fn reachability_and_test_results_use_stable_camel_case_wire_values() {
        for (value, expected) in [
            (ProviderReachability::Operational, "\"operational\""),
            (ProviderReachability::Degraded, "\"degraded\""),
            (ProviderReachability::Failed, "\"failed\""),
        ] {
            assert_eq!(
                serde_json::to_string(&value).expect("serialize reachability"),
                expected
            );
        }

        let result = ProviderTestResult {
            provider_id: "anthropic".to_string(),
            reachability: ProviderReachability::Degraded,
            response_time_ms: Some(7100),
            http_status: Some(200),
        };
        assert_eq!(
            serde_json::to_string(&result).expect("serialize result"),
            r#"{"providerId":"anthropic","reachability":"degraded","responseTimeMs":7100,"httpStatus":200}"#
        );
    }

    #[test]
    fn preflight_outcomes_use_safe_camel_case_fields() {
        let outcome = ProviderPreflightOutcome {
            status: ProviderPreflightStatus::FailedOver,
            origin_provider_id: Some("previous".to_string()),
            active_provider_id: Some("anthropic".to_string()),
            providers: vec![sample()],
            checks: vec![ProviderTestResult {
                provider_id: "anthropic".to_string(),
                reachability: ProviderReachability::Operational,
                response_time_ms: Some(42),
                http_status: Some(200),
            }],
        };

        let json = serde_json::to_string(&outcome).expect("serialize preflight outcome");
        assert!(json.contains(r#""status":"failedOver""#));
        assert!(json.contains(r#""originProviderId":"previous""#));
        assert!(json.contains(r#""activeProviderId":"anthropic""#));
        // `providers` is exactly the list the UI renders, and the key travelling with it is
        // deliberate; what is guarded here are the upstream fields that must not cross the
        // product boundary.
        for forbidden in ["settingsConfig", "targetUrl", "automaticProviderFailover"] {
            assert!(
                !json.contains(forbidden),
                "unsafe preflight field in {json}"
            );
        }
    }

    #[test]
    fn provider_kind_serializes_lower_case() {
        assert_eq!(
            serde_json::to_string(&ProviderKind::Official).expect("serialize"),
            "\"official\""
        );
    }

    #[test]
    fn a_connection_profile_uses_the_beginner_safe_wire_shape() {
        let profile = ProviderConnectionProfile {
            default_preset_id: "official".to_string(),
            presets: vec![ProviderConnectionPreset {
                id: "official".to_string(),
                service_name: "Anthropic API".to_string(),
                default_name: "Anthropic".to_string(),
                default_model: "claude-sonnet-5".to_string(),
                base_url: "https://api.anthropic.com".to_string(),
                website_url: "https://www.anthropic.com".to_string(),
                api_key_url: "https://console.anthropic.com".to_string(),
                official: true,
            }],
            model_required: false,
        };
        assert_eq!(
            serde_json::to_string(&profile).expect("serialize profile"),
            r#"{"defaultPresetId":"official","presets":[{"id":"official","serviceName":"Anthropic API","defaultName":"Anthropic","defaultModel":"claude-sonnet-5","baseUrl":"https://api.anthropic.com","websiteUrl":"https://www.anthropic.com","apiKeyUrl":"https://console.anthropic.com","official":true}],"modelRequired":false}"#
        );
    }

    #[test]
    fn a_create_drafts_debug_output_never_contains_the_plaintext_key() {
        let draft = ProviderCreateDraft {
            preset_id: "official".to_string(),
            name: "Anthropic".to_string(),
            api_key: "sk-ant-super-secret-value".to_string(),
            model: "claude-sonnet-5".to_string(),
        };
        let debugged = format!("{draft:?}");
        assert!(!debugged.contains("sk-ant-super-secret-value"));
        assert!(debugged.contains("***"));
        assert!(debugged.contains("claude-sonnet-5"));
    }

    #[test]
    fn create_drafts_require_a_reviewed_preset_and_reject_raw_configuration() {
        assert!(serde_json::from_str::<ProviderCreateDraft>(
            r#"{"name":"Anthropic","apiKey":"sk-secret","model":"claude-sonnet-5"}"#,
        )
        .is_err());
        let error = serde_json::from_str::<ProviderCreateDraft>(
            r#"{"presetId":"official","name":"Anthropic","apiKey":"sk-secret","model":"claude-sonnet-5","settingsConfig":{"env":{"ANTHROPIC_API_KEY":"sk-secret"}}}"#,
        )
        .expect_err("raw provider config must not cross the create boundary");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn custom_create_drafts_are_inbound_only_and_reject_raw_configuration() {
        let draft = ProviderCustomCreateDraft {
            name: "Private relay".to_string(),
            api_key: "sk-custom-super-secret".to_string(),
            model: "model-a".to_string(),
            base_url: "https://private.example.test/v1".to_string(),
        };
        let debugged = format!("{draft:?}");
        assert!(!debugged.contains("sk-custom-super-secret"));
        assert!(!debugged.contains("private.example.test"));
        assert!(debugged.contains("***"));

        let error = serde_json::from_str::<ProviderCustomCreateDraft>(
            r#"{"name":"Relay","apiKey":"sk-secret","model":"model-a","baseUrl":"https://relay.example.test/v1","settingsConfig":{"auth":"sk-secret"}}"#,
        )
        .expect_err("raw provider config must not cross the custom boundary");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn a_create_result_identifies_the_new_safe_provider_without_the_raw_config() {
        let result = ProviderCreateResult {
            providers: vec![sample()],
            created_provider_id: "anthropic".to_string(),
        };
        let json = serde_json::to_string(&result).expect("serialize create result");
        assert_eq!(
            json,
            r#"{"providers":[{"additive":false,"id":"anthropic","tool":"claude-code","name":"Anthropic","kind":"custom","active":true,"baseUrl":"https://api.anthropic.com","apiKey":"sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F","websiteUrl":null,"testable":true,"canRemove":false}],"createdProviderId":"anthropic"}"#
        );
        assert!(!json.contains("settingsConfig"));
    }
}
