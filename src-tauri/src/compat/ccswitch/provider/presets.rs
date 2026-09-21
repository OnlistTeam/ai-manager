//! Reviewed projection of the tracked upstream Provider preset catalog.
//!
//! The generated artifact keeps the upstream tool-native settings templates, but strips
//! partnership metadata and public referral paths. Renderer receives only the safe profile
//! below and can submit only a stable preset id; raw settings never cross product IPC.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::Value;
use url::Url;

use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{
    AppError, ProviderConnectionPreset, ProviderConnectionProfile, ProviderDraft,
    ProviderEndpointCandidate, ToolId,
};
use crate::provider::Provider as UpstreamProvider;

use super::{advanced, app_type_for, apply_draft, create::create_error};

const CATALOG_JSON: &str = include_str!("provider_presets.generated.json");
const CATALOG_VERSION: u8 = 2;
const MAX_PRESETS: usize = 1024;
const MAX_SETTINGS_BYTES: usize = 64 * 1024;
const MAX_CATALOG_BYTES: usize = 2 * 1024 * 1024;
const VALIDATION_KEY: &str = "catalog-validation-key-never-persisted";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogDocument {
    version: u8,
    presets: Vec<CatalogPreset>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CatalogPreset {
    tool: ToolId,
    id: String,
    service_name: String,
    default_name: String,
    default_model: String,
    website_url: String,
    api_key_url: String,
    official: bool,
    #[serde(rename = "default")]
    is_default: bool,
    settings_config: Value,
}

impl CatalogPreset {
    pub(super) fn raw_provider(&self, id: &str) -> UpstreamProvider {
        UpstreamProvider::with_id(
            id.to_string(),
            self.default_name.clone(),
            self.settings_config.clone(),
            Some(self.website_url.clone()),
        )
    }

    fn profile(&self) -> ProviderConnectionPreset {
        ProviderConnectionPreset {
            id: self.id.clone(),
            service_name: self.service_name.clone(),
            default_name: self.default_name.clone(),
            default_model: self.default_model.clone(),
            website_url: self.website_url.clone(),
            api_key_url: self.api_key_url.clone(),
            official: self.official,
        }
    }
}

struct Catalog {
    by_tool: HashMap<ToolId, Vec<CatalogPreset>>,
}

static CATALOG: OnceLock<Result<Catalog, String>> = OnceLock::new();

fn provider_tools() -> impl Iterator<Item = ToolId> {
    ToolId::ALL
        .into_iter()
        .filter(|tool| capabilities_for(*tool).can_manage_provider)
}

fn parse_catalog() -> Result<Catalog, String> {
    if CATALOG_JSON.len() > MAX_CATALOG_BYTES {
        return Err("provider preset catalog exceeded its byte limit".to_string());
    }
    let document: CatalogDocument = serde_json::from_str(CATALOG_JSON)
        .map_err(|error| format!("provider preset catalog is invalid: {error}"))?;
    if document.version != CATALOG_VERSION {
        return Err("provider preset catalog version is unsupported".to_string());
    }
    if document.presets.is_empty() || document.presets.len() > MAX_PRESETS {
        return Err("provider preset catalog count is outside its bounds".to_string());
    }

    let mut identities = HashSet::new();
    let mut by_tool: HashMap<ToolId, Vec<CatalogPreset>> = HashMap::new();
    for preset in document.presets {
        validate_preset(&preset)?;
        if !identities.insert((preset.tool, preset.id.clone())) {
            return Err(format!(
                "provider preset id is duplicated for {}",
                preset.tool.as_str()
            ));
        }
        by_tool.entry(preset.tool).or_default().push(preset);
    }

    for tool in provider_tools() {
        let presets = by_tool
            .get(&tool)
            .ok_or_else(|| format!("{} has no provider presets", tool.as_str()))?;
        if presets.iter().filter(|preset| preset.is_default).count() != 1 {
            return Err(format!(
                "{} must have exactly one default preset",
                tool.as_str()
            ));
        }
        if presets.iter().filter(|preset| preset.official).count() > 1 {
            return Err(format!(
                "{} has more than one official preset",
                tool.as_str()
            ));
        }
    }
    Ok(Catalog { by_tool })
}

fn catalog() -> Result<&'static Catalog, AppError> {
    match CATALOG.get_or_init(parse_catalog) {
        Ok(catalog) => Ok(catalog),
        Err(detail) => Err(create_error().with_technical(detail.clone())),
    }
}

/// Whether `tool`'s live config needs an explicit model entry.
///
/// Exhaustive on purpose (no `_ =>`): a new `ToolId` forces a conscious call
/// here instead of silently inheriting a neighbour's default.
pub(super) fn model_required(tool: ToolId) -> bool {
    match tool {
        // The CLI itself carries a default model when the config omits one;
        // a blank connection is a fully working connection for these three.
        ToolId::ClaudeCode | ToolId::Codex | ToolId::GeminiCli => false,
        // These tools' native config *is* a model table (grok's
        // `models.default`, OpenCode/OpenClaw/Hermes/Pi's model arrays) —
        // there is no built-in fallback to omit it to.
        ToolId::OpenCode
        | ToolId::GrokBuild
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => true,
    }
}

pub(super) fn profile_for(tool: ToolId) -> Result<ProviderConnectionProfile, AppError> {
    let presets = catalog()?.by_tool.get(&tool).ok_or_else(create_error)?;
    let default = presets
        .iter()
        .find(|preset| preset.is_default)
        .ok_or_else(create_error)?;
    Ok(ProviderConnectionProfile {
        default_preset_id: default.id.clone(),
        presets: presets.iter().map(CatalogPreset::profile).collect(),
        model_required: model_required(tool),
    })
}

pub(super) fn preset_for(tool: ToolId, id: &str) -> Result<&'static CatalogPreset, AppError> {
    catalog()?
        .by_tool
        .get(&tool)
        .and_then(|presets| presets.iter().find(|preset| preset.id == id))
        .ok_or_else(create_error)
}

/// Return the audited endpoint for the tool's official preset, when one exists.
///
/// The endpoint stays native-only: renderer receives only the `testable`
/// capability and can never replace this target or attach a credential to it.
pub(super) fn official_endpoint_for(tool: ToolId) -> Result<Option<String>, AppError> {
    let Some(preset) = catalog()?
        .by_tool
        .get(&tool)
        .and_then(|presets| presets.iter().find(|preset| preset.official))
    else {
        return Ok(None);
    };

    let raw = preset.raw_provider("official-reachability-check");
    let (endpoint, key) = raw.resolve_usage_credentials(&app_type_for(tool));
    validate_https_endpoint(&endpoint).map_err(|detail| create_error().with_technical(detail))?;
    if !key.is_empty() {
        return Err(create_error()
            .with_technical("official reachability target contained a credential".to_string()));
    }
    Ok(Some(endpoint))
}

pub(super) fn endpoint_candidates_for(
    tool: ToolId,
) -> Result<Vec<ProviderEndpointCandidate>, AppError> {
    catalog()?
        .by_tool
        .get(&tool)
        .ok_or_else(create_error)?
        .iter()
        .map(|preset| {
            let raw = preset.raw_provider("reviewed-preset-speed-test");
            let (endpoint, key) = raw.resolve_usage_credentials(&app_type_for(tool));
            validate_https_endpoint(&endpoint)
                .map_err(|detail| create_error().with_technical(detail))?;
            if !key.is_empty() {
                return Err(create_error()
                    .with_technical("reviewed speed-test preset contained a credential"));
            }
            Ok(ProviderEndpointCandidate {
                id: preset.id.clone(),
                url: endpoint,
            })
        })
        .collect()
}

fn validate_preset(preset: &CatalogPreset) -> Result<(), String> {
    if !capabilities_for(preset.tool).can_manage_provider {
        return Err(format!(
            "{} cannot own provider presets",
            preset.tool.as_str()
        ));
    }
    if !valid_id(&preset.id)
        || !valid_text(&preset.service_name, 100)
        || !valid_text(&preset.default_name, 100)
        || !valid_text(&preset.default_model, 256)
    {
        return Err(format!(
            "{} contains invalid provider preset text",
            preset.tool.as_str()
        ));
    }
    validate_public_origin(&preset.website_url)?;
    validate_public_origin(&preset.api_key_url)?;
    let settings_bytes = serde_json::to_vec(&preset.settings_config)
        .map_err(|_| "provider preset settings could not be measured".to_string())?;
    if settings_bytes.len() > MAX_SETTINGS_BYTES
        || contains_plain_secret(&preset.settings_config, "")
    {
        return Err(format!(
            "{}:{} contains unsafe provider settings",
            preset.tool.as_str(),
            preset.id
        ));
    }

    let mut raw = preset.raw_provider("catalog-validation");
    let (endpoint, key) = raw.resolve_usage_credentials(&app_type_for(preset.tool));
    validate_https_endpoint(&endpoint)?;
    if !key.is_empty() {
        return Err("provider preset contains a credential".to_string());
    }
    let draft = ProviderDraft {
        name: preset.default_name.clone(),
        api_key: Some(VALIDATION_KEY.to_string()),
        models: Some(vec![preset.default_model.clone()]),
        advanced: None,
    };
    apply_draft(preset.tool, &mut raw, &draft)
        .map_err(|_| "provider preset cannot accept the safe connection draft".to_string())?;
    let (_, key) = raw.resolve_usage_credentials(&app_type_for(preset.tool));
    if key != VALIDATION_KEY
        || !advanced::profile(preset.tool, &raw)
            .models
            .contains(&preset.default_model)
    {
        return Err("provider preset failed its credential/model round trip".to_string());
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed == value
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

fn validate_public_origin(value: &str) -> Result<(), String> {
    let url = Url::parse(value).map_err(|_| "provider public URL is invalid".to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.host().is_none()
    {
        return Err("provider public URL is not a clean HTTPS origin".to_string());
    }
    Ok(())
}

fn validate_https_endpoint(value: &str) -> Result<(), String> {
    let url = Url::parse(value).map_err(|_| "provider endpoint is invalid".to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host().is_none()
    {
        return Err("provider endpoint is not safe HTTPS".to_string());
    }
    Ok(())
}

fn secret_field(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect();
    normalized.contains("apikey")
        || normalized.contains("authtoken")
        || normalized.contains("accesstoken")
        || normalized.contains("bearertoken")
        || normalized.contains("secret")
        || normalized.contains("password")
        || normalized.contains("authorization")
        || normalized.contains("credential")
}

fn contains_plain_secret(value: &Value, parent: &str) -> bool {
    match value {
        Value::String(value) if secret_field(parent) => !value.trim().is_empty(),
        Value::String(value) if parent == "config" => value.lines().any(|line| {
            line.split_once('=').is_some_and(|(name, value)| {
                secret_field(name.trim()) && !value.trim().trim_matches(['\'', '"']).is_empty()
            })
        }),
        Value::Array(values) => values
            .iter()
            .any(|value| contains_plain_secret(value, parent)),
        Value::Object(values) => values
            .iter()
            .any(|(name, value)| contains_plain_secret(value, name)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{model_required, official_endpoint_for, preset_for, profile_for, provider_tools};
    use crate::domain::ToolId;

    #[test]
    fn only_the_tools_with_their_own_default_model_may_go_blank() {
        for tool in [ToolId::ClaudeCode, ToolId::Codex, ToolId::GeminiCli] {
            assert!(!model_required(tool), "{tool:?} should allow a blank model");
            assert!(!profile_for(tool).expect("valid catalog").model_required);
        }
        for tool in [ToolId::OpenCode, ToolId::GrokBuild, ToolId::Hermes] {
            assert!(model_required(tool), "{tool:?} should require a model");
            assert!(profile_for(tool).expect("valid catalog").model_required);
        }
    }

    #[test]
    fn reviewed_catalog_has_broad_coverage_and_one_default_per_tool() {
        let mut total = 0;
        for tool in provider_tools() {
            let profile = profile_for(tool).expect("valid catalog");
            assert_eq!(
                profile
                    .presets
                    .iter()
                    .filter(|preset| preset.official)
                    .count(),
                if matches!(tool, ToolId::OpenClaw | ToolId::Pi) {
                    0
                } else {
                    1
                }
            );
            assert!(profile
                .presets
                .iter()
                .any(|preset| preset.id == profile.default_preset_id));
            total += profile.presets.len();
        }
        assert!(total >= 400, "catalog unexpectedly shrank to {total}");
        assert!(profile_for(ToolId::KimiCode).is_err());
        assert!(profile_for(ToolId::DeepSeekDsh).is_err());
    }

    #[test]
    fn preset_ids_are_scoped_to_the_requested_tool() {
        assert!(preset_for(ToolId::ClaudeCode, "official").is_ok());
        let codex_only = profile_for(ToolId::Codex)
            .expect("codex profile")
            .presets
            .into_iter()
            .find(|preset| {
                !profile_for(ToolId::ClaudeCode)
                    .expect("claude profile")
                    .presets
                    .iter()
                    .any(|claude| claude.id == preset.id)
            })
            .expect("codex-only id");
        assert!(preset_for(ToolId::ClaudeCode, &codex_only.id).is_err());
    }

    #[test]
    fn official_probe_endpoints_are_audited_https_targets() {
        let cases = [
            (ToolId::ClaudeCode, Some("https://api.anthropic.com")),
            (ToolId::Codex, Some("https://api.openai.com/v1")),
            (
                ToolId::GeminiCli,
                Some("https://generativelanguage.googleapis.com"),
            ),
            (ToolId::GrokBuild, Some("https://api.x.ai/v1")),
            (ToolId::OpenCode, Some("https://api.openai.com/v1")),
            (
                ToolId::Hermes,
                Some("https://inference-api.nousresearch.com/v1"),
            ),
            (ToolId::OpenClaw, None),
            (ToolId::Pi, None),
        ];

        for (tool, expected) in cases {
            assert_eq!(
                official_endpoint_for(tool)
                    .expect("valid reviewed catalog")
                    .as_deref(),
                expected,
                "unexpected official probe target for {tool:?}"
            );
        }
    }
}
