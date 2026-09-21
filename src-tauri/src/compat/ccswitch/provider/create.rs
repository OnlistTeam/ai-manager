//! Beginner Mode preset selection and upstream Provider construction.
//!
//! The renderer submits only a reviewed preset id plus the user's name, secret and model.
//! Tool-native templates come from `presets`; the completed record still goes through the
//! upstream `ProviderService::add` database/live-config transaction.

use crate::domain::{
    AppError, ErrorCode, ProviderAdvancedDraft, ProviderConnectionProfile, ProviderCreateDraft,
    ProviderCustomCreateDraft, ProviderDraft, ToolId,
};
use crate::provider::Provider as UpstreamProvider;
use url::Url;

use super::{apply_draft, presets};

const CONNECTION_ATTEMPT_NOTE: &str = "ai-manager:provider-connect:v1";
const LEGACY_CONNECTION_ATTEMPT_NOTE: &str = "ai-manager:beginner-connect:v1";
const MAX_CUSTOM_NAME_CHARS: usize = 100;
const MAX_CUSTOM_KEY_BYTES: usize = 32 * 1024;
const MAX_CUSTOM_MODEL_CHARS: usize = 256;
const MAX_CUSTOM_BASE_URL_BYTES: usize = 2 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnectionCreateMode {
    Activate,
    StoreOnly,
}

impl ConnectionCreateMode {
    fn code(self) -> &'static str {
        match self {
            Self::Activate => "a",
            Self::StoreOnly => "s",
        }
    }
}

pub fn connection_profile_for(tool: ToolId) -> Result<ProviderConnectionProfile, AppError> {
    if !crate::compat::ccswitch::tools::capabilities_for(tool).can_manage_provider {
        return Err(create_error());
    }
    presets::profile_for(tool)
}

pub(super) fn create_error() -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.createFailed")
        .with_remediation("error.remediation.checkConnectionSettings")
}

pub(super) fn provider_id_for_attempt(
    request_id: &str,
    mode: ConnectionCreateMode,
) -> Result<String, AppError> {
    let canonical = uuid::Uuid::parse_str(request_id)
        .map_err(|_| create_error())?
        .hyphenated()
        .to_string();
    Ok(format!("aimgr-connect-{}-{canonical}", mode.code()))
}

pub(super) fn is_connection_attempt_provider(provider: &UpstreamProvider) -> bool {
    matches!(
        provider.notes.as_deref(),
        Some(CONNECTION_ATTEMPT_NOTE | LEGACY_CONNECTION_ATTEMPT_NOTE)
    )
}

fn validated_fields(
    tool: ToolId,
    draft: &ProviderCreateDraft,
) -> Result<(&str, &str, &str), AppError> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err(
            AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.nameRequired")
                .with_remediation("error.remediation.checkConnectionSettings"),
        );
    }
    let api_key = draft.api_key.trim();
    let model = draft.model.trim();
    if draft.preset_id.is_empty() || api_key.is_empty() {
        return Err(create_error());
    }
    // Tools with a built-in default model (Claude Code / Codex / Gemini CLI) may leave it empty;
    // for the others the native config is itself a model table, so an empty model means no usable
    // connection.
    if model.is_empty() && presets::model_required(tool) {
        return Err(create_error());
    }
    Ok((name, api_key, model))
}

pub(super) fn provider_for_create(
    tool: ToolId,
    id: &str,
    draft: &ProviderCreateDraft,
) -> Result<UpstreamProvider, AppError> {
    let (name, api_key, model) = validated_fields(tool, draft)?;
    let preset = presets::preset_for(tool, &draft.preset_id)?;
    let mut provider = preset.raw_provider(id);
    let models = if model.is_empty() {
        Vec::new()
    } else {
        vec![model.to_string()]
    };
    apply_draft(
        tool,
        &mut provider,
        &ProviderDraft {
            name: name.to_string(),
            api_key: Some(api_key.to_string()),
            models: Some(models),
            advanced: None,
        },
    )?;
    provider.notes = Some(CONNECTION_ATTEMPT_NOTE.to_string());
    provider.icon = Some(
        match tool {
            ToolId::ClaudeCode => "anthropic",
            ToolId::Codex | ToolId::OpenCode => "openai",
            ToolId::GeminiCli => "gemini",
            ToolId::GrokBuild => "grok",
            ToolId::OpenClaw => "openclaw",
            ToolId::Hermes => "hermes",
            ToolId::Pi => "pi",
            ToolId::KimiCode | ToolId::DeepSeekDsh => {
                unreachable!("lifecycle-only tool reached provider creation")
            }
        }
        .to_string(),
    );
    Ok(provider)
}

fn valid_custom_text(value: &str, max_chars: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= max_chars
        && !trimmed.chars().any(char::is_control)
}

fn normalize_custom_https_base_url(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_CUSTOM_BASE_URL_BYTES {
        return Err(create_error());
    }
    let url = Url::parse(value).map_err(|_| create_error())?;
    if url.scheme() != "https"
        || url.cannot_be_a_base()
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(create_error());
    }
    let mut normalized = url.to_string();
    while normalized.ends_with('/') {
        normalized.pop();
    }
    (!normalized.is_empty())
        .then_some(normalized)
        .ok_or_else(create_error)
}

/// Builds a custom service from the tool's already-audited canonical shape.
/// Only the four common fields are replaced; no renderer-owned raw config is
/// accepted and the reviewed preset's public identity is deliberately removed.
pub(super) fn provider_for_custom_create(
    tool: ToolId,
    id: &str,
    draft: &ProviderCustomCreateDraft,
) -> Result<UpstreamProvider, AppError> {
    let name = draft.name.trim();
    let api_key = draft.api_key.trim();
    let model = draft.model.trim();
    // An empty model is only valid input when the tool allows it; a non-empty model still has to pass the character/length validation.
    let model_ok = if model.is_empty() {
        !presets::model_required(tool)
    } else {
        valid_custom_text(model, MAX_CUSTOM_MODEL_CHARS)
    };
    if !valid_custom_text(name, MAX_CUSTOM_NAME_CHARS)
        || api_key.is_empty()
        || api_key.len() > MAX_CUSTOM_KEY_BYTES
        || api_key.chars().any(char::is_control)
        || !model_ok
    {
        return Err(create_error());
    }
    let base_url = normalize_custom_https_base_url(&draft.base_url)?;
    let profile = connection_profile_for(tool)?;
    let mut provider = provider_for_create(
        tool,
        id,
        &ProviderCreateDraft {
            preset_id: profile.default_preset_id,
            name: name.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        },
    )?;
    apply_draft(
        tool,
        &mut provider,
        &ProviderDraft {
            name: name.to_string(),
            api_key: None,
            models: None,
            advanced: Some(ProviderAdvancedDraft {
                base_url_changed: true,
                base_url: Some(base_url),
                endpoint_candidates: None,
                endpoint_auto_select: None,
                headers: None,
            }),
        },
    )?;
    provider.website_url = None;
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use super::{
        connection_profile_for, is_connection_attempt_provider, provider_for_create,
        provider_for_custom_create, provider_id_for_attempt, ConnectionCreateMode,
    };
    use crate::domain::{ProviderCreateDraft, ProviderCustomCreateDraft, ToolId};

    fn draft() -> ProviderCreateDraft {
        ProviderCreateDraft {
            preset_id: "official".to_string(),
            name: "  My service  ".to_string(),
            api_key: "  sk-secret-value  ".to_string(),
            model: "  model-a  ".to_string(),
        }
    }

    fn default_draft(tool: ToolId) -> ProviderCreateDraft {
        let mut draft = draft();
        draft.preset_id = connection_profile_for(tool)
            .expect("connection profile")
            .default_preset_id;
        draft
    }

    fn custom_draft(base_url: &str) -> ProviderCustomCreateDraft {
        ProviderCustomCreateDraft {
            name: "  Private relay  ".to_string(),
            api_key: "  sk-custom-secret-value  ".to_string(),
            model: "  model-custom  ".to_string(),
            base_url: base_url.to_string(),
        }
    }

    #[test]
    fn every_tool_builds_a_custom_https_service_in_its_native_shape() {
        for tool in ToolId::ALL {
            if !crate::compat::ccswitch::tools::capabilities_for(tool).can_manage_provider {
                continue;
            }
            let raw = provider_for_custom_create(
                tool,
                tool.as_str(),
                &custom_draft("  https://relay.example.test/v1/  "),
            )
            .expect("custom provider");
            let (base_url, api_key) =
                raw.resolve_usage_credentials(&super::super::app_type_for(tool));
            assert_eq!(base_url, "https://relay.example.test/v1", "{tool:?}");
            assert_eq!(api_key, "sk-custom-secret-value", "{tool:?}");
            assert_eq!(raw.name, "Private relay");
            assert_eq!(raw.website_url, None);
            assert_eq!(raw.category, None);
            assert!(is_connection_attempt_provider(&raw));
            assert!(super::super::advanced::profile(tool, &raw)
                .models
                .contains(&"model-custom".to_string()));
        }
    }

    #[test]
    fn custom_create_rejects_non_https_and_secret_bearing_urls() {
        for base_url in [
            "http://relay.example.test/v1",
            "http://localhost:11434/v1",
            "https://user:pass@relay.example.test/v1",
            "https://relay.example.test/v1?token=secret",
            "https://relay.example.test/v1#secret",
            "file:///tmp/provider",
            "not a URL",
        ] {
            let error =
                provider_for_custom_create(ToolId::ClaudeCode, "rejected", &custom_draft(base_url))
                    .expect_err("unsafe custom endpoint");
            assert_eq!(error.message_key, "error.provider.createFailed");
            assert!(error.technical_message.is_none());
        }
    }

    #[test]
    fn every_default_builds_credentials_the_upstream_reader_can_find() {
        for tool in ToolId::ALL {
            if !crate::compat::ccswitch::tools::capabilities_for(tool).can_manage_provider {
                continue;
            }
            let raw = provider_for_create(tool, "provider-id", &default_draft(tool))
                .expect("valid beginner draft");
            let (base_url, api_key) =
                raw.resolve_usage_credentials(&super::super::app_type_for(tool));
            assert_eq!(api_key, "sk-secret-value", "{tool:?} lost the key");
            assert!(
                base_url.starts_with("https://"),
                "{tool:?} lost its endpoint"
            );
            assert_eq!(raw.name, "My service");
            assert!(is_connection_attempt_provider(&raw));
            let profile = connection_profile_for(tool).expect("supported connection profile");
            let selected = profile
                .presets
                .iter()
                .find(|preset| preset.id == profile.default_preset_id)
                .expect("default preset");
            assert_eq!(
                raw.website_url.as_deref(),
                Some(selected.website_url.as_str())
            );
        }
    }

    #[test]
    fn every_default_places_the_model_in_the_tools_native_shape() {
        let claude = provider_for_create(
            ToolId::ClaudeCode,
            "claude",
            &default_draft(ToolId::ClaudeCode),
        )
        .expect("claude");
        assert_eq!(claude.settings_config["env"]["ANTHROPIC_MODEL"], "model-a");

        let codex = provider_for_create(ToolId::Codex, "codex", &default_draft(ToolId::Codex))
            .expect("codex");
        let doc = codex.settings_config["config"]
            .as_str()
            .expect("config string")
            .parse::<toml_edit::DocumentMut>()
            .expect("valid TOML");
        assert_eq!(doc["model"].as_str(), Some("model-a"));
        assert_eq!(doc["model_provider"].as_str(), Some("custom"));

        let opencode = provider_for_create(
            ToolId::OpenCode,
            "opencode",
            &default_draft(ToolId::OpenCode),
        )
        .expect("opencode");
        assert!(opencode.settings_config["models"].get("model-a").is_some());

        let gemini = provider_for_create(
            ToolId::GeminiCli,
            "gemini",
            &default_draft(ToolId::GeminiCli),
        )
        .expect("gemini");
        assert_eq!(gemini.settings_config["env"]["GEMINI_MODEL"], "model-a");

        let grok =
            provider_for_create(ToolId::GrokBuild, "grok", &default_draft(ToolId::GrokBuild))
                .expect("grok");
        assert_eq!(
            crate::grok_config::extract_model_config(
                grok.settings_config["config"].as_str().expect("Grok TOML")
            )
            .expect("Grok model")
            .model,
            "model-a"
        );

        for tool in [ToolId::OpenClaw, ToolId::Hermes, ToolId::Pi] {
            let provider =
                provider_for_create(tool, tool.as_str(), &default_draft(tool)).expect("provider");
            assert_eq!(provider.settings_config["models"][0]["id"], "model-a");
        }
    }

    #[test]
    fn a_compatible_preset_reuses_its_upstream_endpoint_and_config_shape() {
        let profile = connection_profile_for(ToolId::Codex).expect("codex profile");
        let deepseek = profile
            .presets
            .iter()
            .find(|preset| preset.service_name == "DeepSeek")
            .expect("reviewed DeepSeek preset");
        let mut compatible = draft();
        compatible.preset_id = deepseek.id.clone();
        let raw = provider_for_create(ToolId::Codex, "deepseek", &compatible)
            .expect("compatible provider");
        let (endpoint, key) =
            raw.resolve_usage_credentials(&super::super::app_type_for(ToolId::Codex));
        assert_eq!(endpoint, "https://api.deepseek.com");
        assert_eq!(key, "sk-secret-value");
        assert!(raw.settings_config["config"]
            .as_str()
            .expect("config")
            .contains("wire_api"));
    }

    #[test]
    fn blank_required_fields_and_unknown_presets_are_rejected() {
        // Blank model only belongs on this list for a tool that actually
        // requires one (Grok Build) — Claude Code/Codex/Gemini CLI accept it,
        // see the dedicated tests below.
        let grok_default = connection_profile_for(ToolId::GrokBuild)
            .expect("grok profile")
            .default_preset_id;
        for (tool, preset_id, name, key, model, expected) in [
            (
                ToolId::ClaudeCode,
                "official",
                " ",
                "sk",
                "model",
                "error.provider.nameRequired",
            ),
            (
                ToolId::ClaudeCode,
                "official",
                "name",
                " ",
                "model",
                "error.provider.createFailed",
            ),
            (
                ToolId::GrokBuild,
                grok_default.as_str(),
                "name",
                "sk",
                " ",
                "error.provider.createFailed",
            ),
            (
                ToolId::ClaudeCode,
                "missing",
                "name",
                "sk",
                "model",
                "error.provider.createFailed",
            ),
        ] {
            let error = provider_for_create(
                tool,
                "rejected",
                &ProviderCreateDraft {
                    preset_id: preset_id.to_string(),
                    name: name.to_string(),
                    api_key: key.to_string(),
                    model: model.to_string(),
                },
            )
            .expect_err("invalid create draft must be refused");
            assert_eq!(error.message_key, expected);
            assert_eq!(
                error.remediation.as_deref(),
                Some("error.remediation.checkConnectionSettings")
            );
        }
    }

    #[test]
    fn a_blank_model_on_claude_code_omits_every_model_key() {
        let mut draft = default_draft(ToolId::ClaudeCode);
        draft.model = "  ".to_string();
        let raw = provider_for_create(ToolId::ClaudeCode, "claude-blank", &draft)
            .expect("Claude Code's own default model makes a blank model valid");
        let env = raw.settings_config["env"].as_object().expect("env object");
        for key in [
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
        ] {
            assert!(!env.contains_key(key), "unexpected {key} in {env:?}");
        }
    }

    #[test]
    fn a_blank_model_on_a_custom_claude_code_endpoint_omits_every_model_key() {
        let mut draft = custom_draft("https://relay.example.test/v1");
        draft.model = "  ".to_string();
        let raw = provider_for_custom_create(ToolId::ClaudeCode, "claude-custom-blank", &draft)
            .expect("Claude Code's own default model makes a blank custom model valid");
        assert_eq!(
            raw.settings_config["env"]["ANTHROPIC_BASE_URL"],
            "https://relay.example.test/v1"
        );
        let env = raw.settings_config["env"].as_object().expect("env object");
        for key in [
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
        ] {
            assert!(!env.contains_key(key), "unexpected {key} in {env:?}");
        }
    }

    #[test]
    fn a_blank_model_on_grok_build_is_still_rejected() {
        let mut draft = default_draft(ToolId::GrokBuild);
        draft.model = "  ".to_string();
        let error = provider_for_create(ToolId::GrokBuild, "grok-blank", &draft)
            .expect_err("Grok Build's native config is itself a model table");
        assert_eq!(error.message_key, "error.provider.createFailed");
    }

    #[test]
    fn one_request_has_distinct_stable_ids_for_both_original_intents() {
        let request_id = "2ef45bc4-2918-4af7-a87f-8efc9d852116";
        assert_eq!(
            provider_id_for_attempt(request_id, ConnectionCreateMode::Activate).expect("valid id"),
            "aimgr-connect-a-2ef45bc4-2918-4af7-a87f-8efc9d852116"
        );
        assert_eq!(
            provider_id_for_attempt(request_id, ConnectionCreateMode::StoreOnly).expect("valid id"),
            "aimgr-connect-s-2ef45bc4-2918-4af7-a87f-8efc9d852116"
        );
        assert!(provider_id_for_attempt("not-a-uuid", ConnectionCreateMode::Activate).is_err());
    }

    #[test]
    fn profiles_cover_eight_tools_and_the_full_reviewed_directory() {
        let profiles: Vec<_> = ToolId::ALL
            .into_iter()
            .filter(|tool| {
                crate::compat::ccswitch::tools::capabilities_for(*tool).can_manage_provider
            })
            .map(connection_profile_for)
            .collect::<Result<Vec<_>, _>>()
            .expect("supported profiles");
        assert_eq!(profiles.len(), 8);
        assert!(
            profiles
                .iter()
                .map(|profile| profile.presets.len())
                .sum::<usize>()
                >= 400
        );
    }
}
