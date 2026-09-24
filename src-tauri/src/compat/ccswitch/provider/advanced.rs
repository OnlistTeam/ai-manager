//! Safe, capability-driven projection and mutation of common provider settings.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use http::header::{HeaderName, HeaderValue};
use serde_json::{Map, Value};
use url::{Host, Url};

use crate::domain::{
    AppError, ErrorCode, ProviderAdvancedDraft, ProviderDraft, ProviderEditCapabilities,
    ProviderEditProfile, ProviderHeaderDraft, ProviderWireProtocol, ToolId,
    MAX_PROVIDER_ENDPOINT_CANDIDATES,
};
use crate::provider::Provider as UpstreamProvider;
use crate::settings::CustomEndpoint;

use super::{app_type_for, long_tail, settings_not_object_error, write_slot};

const MAX_MODELS: usize = 64;
const MAX_MODEL_CHARS: usize = 256;
const MAX_HEADERS: usize = 32;
const MAX_HEADER_VALUE_BYTES: usize = 8 * 1024;
const MAX_HEADER_TOTAL_BYTES: usize = 32 * 1024;
const MAX_BASE_URL_BYTES: usize = 2 * 1024;

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// A Base URL is intentionally visible, but URL credentials/query tokens are
/// secrets. Fail closed instead of projecting those legacy shapes to renderer.
pub(super) fn safe_base_url(value: String) -> Option<String> {
    let value = non_empty(value)?;
    let parsed = Url::parse(&value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    Some(value)
}

pub(super) fn profile(tool: ToolId, raw: &UpstreamProvider) -> ProviderEditProfile {
    let (resolved_base_url, _) = raw.resolve_usage_credentials(&app_type_for(tool));
    let has_resolved_base_url = !resolved_base_url.trim().is_empty();
    let base_url = safe_base_url(resolved_base_url);
    let endpoint_candidates = if has_resolved_base_url && base_url.is_none() {
        None
    } else {
        endpoint_candidates(raw, base_url.as_deref())
    };
    let mut capabilities = capabilities(tool, raw);
    capabilities.can_edit_endpoints &= endpoint_candidates.is_some();
    ProviderEditProfile {
        provider_id: raw.id.clone(),
        base_url,
        endpoint_candidates: endpoint_candidates.unwrap_or_default(),
        endpoint_auto_select: raw
            .meta
            .as_ref()
            .and_then(|meta| meta.endpoint_auto_select)
            .unwrap_or(true),
        models: models(tool, raw),
        header_names: header_names(tool, raw),
        capabilities,
        base_url_takes_no_version: ProviderWireProtocol::for_tool(tool).route_carries_version(),
    }
}

pub(super) fn apply_settings(
    tool: ToolId,
    raw: &mut UpstreamProvider,
    draft: &ProviderDraft,
) -> Result<(), AppError> {
    let capabilities = capabilities(tool, raw);

    if let Some(models) = draft.models.as_ref() {
        if !capabilities.can_edit_models {
            return Err(save_failed());
        }
        let models = validate_models(models, capabilities.supports_multiple_models)?;
        write_models(tool, raw, &models)?;
    }

    if let Some(advanced) = draft.advanced.as_ref() {
        if !advanced.base_url_changed && advanced.base_url.is_some() {
            return Err(save_failed());
        }
        if advanced.base_url_changed {
            if !capabilities.can_edit_base_url {
                return Err(save_failed());
            }
            write_base_url(tool, raw, advanced)?;
        }
        if advanced.endpoint_candidates.is_some() || advanced.endpoint_auto_select.is_some() {
            if !capabilities.can_edit_endpoints {
                return Err(save_failed());
            }
            write_endpoint_preferences(tool, raw, advanced)?;
        }
        if let Some(headers) = advanced.headers.as_ref() {
            if !capabilities.can_edit_headers {
                return Err(save_failed());
            }
            write_headers(raw, headers)?;
        }
    }

    Ok(())
}

fn capabilities(tool: ToolId, raw: &UpstreamProvider) -> ProviderEditCapabilities {
    let locked = raw.category.as_deref() == Some("official")
        || matches!(raw.category.as_deref(), Some("omo") | Some("omo-slim"))
        || raw.uses_managed_account_auth();
    let shape_ok = settings_shape_supported(tool, &raw.settings_config);
    let editable = !locked && shape_ok;
    ProviderEditCapabilities {
        can_edit_base_url: editable,
        can_edit_endpoints: editable,
        can_edit_models: editable,
        can_edit_headers: editable && tool == ToolId::OpenCode && headers_shape_supported(raw),
        supports_multiple_models: matches!(
            tool,
            ToolId::OpenCode | ToolId::OpenClaw | ToolId::Hermes | ToolId::Pi
        ),
    }
}

fn endpoint_candidates(raw: &UpstreamProvider, base_url: Option<&str>) -> Option<Vec<String>> {
    let mut stored = raw
        .meta
        .as_ref()
        .map(|meta| {
            meta.custom_endpoints
                .values()
                .map(|endpoint| (endpoint.added_at, endpoint.url.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    stored.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1)));

    let mut seen = HashSet::new();
    let mut projected = Vec::with_capacity(stored.len());
    for (_, endpoint) in stored {
        let normalized = normalize_endpoint_url(endpoint).ok()?;
        if !seen.insert(normalized.clone()) {
            return None;
        }
        projected.push(normalized);
    }
    if let Some(base_url) = base_url {
        seen.insert(normalize_endpoint_url(base_url).ok()?);
    }
    (seen.len() <= MAX_PROVIDER_ENDPOINT_CANDIDATES).then_some(projected)
}

fn write_endpoint_preferences(
    tool: ToolId,
    raw: &mut UpstreamProvider,
    draft: &ProviderAdvancedDraft,
) -> Result<(), AppError> {
    let normalized = draft
        .endpoint_candidates
        .as_ref()
        .map(|candidates| validate_endpoint_candidates(candidates))
        .transpose()?;

    if let Some(candidates) = normalized.as_ref() {
        let (base_url, _) = raw.resolve_usage_credentials(&app_type_for(tool));
        if let Some(base_url) = non_empty(base_url) {
            let selected = normalize_endpoint_url(&base_url)?;
            if !candidates.iter().any(|candidate| candidate == &selected) {
                return Err(save_failed());
            }
        }

        let existing = raw
            .meta
            .as_ref()
            .map(|meta| {
                meta.custom_endpoints
                    .values()
                    .filter_map(|endpoint| {
                        normalize_endpoint_url(&endpoint.url)
                            .ok()
                            .map(|url| (url, endpoint.clone()))
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let added_at = now_millis();
        let endpoints = candidates
            .iter()
            .map(|url| {
                let endpoint = existing.get(url).cloned().unwrap_or(CustomEndpoint {
                    url: url.clone(),
                    added_at,
                    last_used: None,
                });
                (
                    url.clone(),
                    CustomEndpoint {
                        url: url.clone(),
                        ..endpoint
                    },
                )
            })
            .collect();
        raw.meta
            .get_or_insert_with(Default::default)
            .custom_endpoints = endpoints;
    }

    if let Some(auto_select) = draft.endpoint_auto_select {
        raw.meta
            .get_or_insert_with(Default::default)
            .endpoint_auto_select = Some(auto_select);
    }
    Ok(())
}

fn validate_endpoint_candidates(candidates: &[String]) -> Result<Vec<String>, AppError> {
    if candidates.len() > MAX_PROVIDER_ENDPOINT_CANDIDATES {
        return Err(save_failed());
    }
    let mut seen = HashSet::with_capacity(candidates.len());
    let mut normalized = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let candidate = normalize_endpoint_url(candidate)?;
        if !seen.insert(candidate.clone()) {
            return Err(save_failed());
        }
        normalized.push(candidate);
    }
    Ok(normalized)
}

fn normalize_endpoint_url(raw: &str) -> Result<String, AppError> {
    let raw = raw.trim();
    validate_base_url(raw)?;
    let mut normalized = Url::parse(raw).map_err(|_| save_failed())?.to_string();
    while normalized.ends_with('/') {
        normalized.pop();
    }
    (!normalized.is_empty())
        .then_some(normalized)
        .ok_or_else(save_failed)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn settings_shape_supported(tool: ToolId, settings: &Value) -> bool {
    let Some(root) = settings.as_object() else {
        return false;
    };
    match tool {
        ToolId::ClaudeCode | ToolId::GeminiCli => {
            root.get("env").is_none_or(|value| value.is_object())
        }
        ToolId::Codex => root
            .get("config")
            .is_none_or(|value| value.is_null() || value.is_string()),
        ToolId::OpenCode => {
            root.get("options").is_none_or(|value| value.is_object())
                && root.get("models").is_none_or(|value| value.is_object())
        }
        ToolId::GrokBuild | ToolId::OpenClaw | ToolId::Hermes | ToolId::Pi => {
            long_tail::settings_shape_supported(tool, settings)
        }
        ToolId::KimiCode | ToolId::DeepSeekDsh => false,
    }
}

fn headers_shape_supported(raw: &UpstreamProvider) -> bool {
    raw.settings_config
        .pointer("/options/headers")
        .is_none_or(|headers| {
            headers.as_object().is_some_and(|map| {
                map.values().all(Value::is_string)
                    && map.len() <= MAX_HEADERS
                    && map.iter().all(|(name, value)| {
                        HeaderName::from_bytes(name.as_bytes())
                            .is_ok_and(|name| !forbidden_header(name.as_str()))
                            && value
                                .as_str()
                                .is_some_and(|value| HeaderValue::from_str(value).is_ok())
                    })
            })
        })
}

fn models(tool: ToolId, raw: &UpstreamProvider) -> Vec<String> {
    let single = match tool {
        ToolId::ClaudeCode => string_at(&raw.settings_config, "/env/ANTHROPIC_MODEL"),
        ToolId::Codex => raw
            .settings_config
            .get("config")
            .and_then(Value::as_str)
            .and_then(codex_model),
        ToolId::GeminiCli => string_at(&raw.settings_config, "/env/GEMINI_MODEL"),
        ToolId::OpenCode => None,
        ToolId::GrokBuild | ToolId::OpenClaw | ToolId::Hermes | ToolId::Pi => {
            return long_tail::models(tool, raw)
        }
        ToolId::KimiCode | ToolId::DeepSeekDsh => return Vec::new(),
    };
    if let Some(model) = single {
        return vec![model];
    }
    if tool != ToolId::OpenCode {
        return Vec::new();
    }
    let mut models: Vec<String> = raw
        .settings_config
        .get("models")
        .and_then(Value::as_object)
        .map(|models| models.keys().cloned().collect())
        .unwrap_or_default();
    models.sort();
    models
}

fn string_at(settings: &Value, pointer: &str) -> Option<String> {
    settings
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn codex_model(config: &str) -> Option<String> {
    config
        .parse::<toml::Value>()
        .ok()?
        .get("model")?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn header_names(tool: ToolId, raw: &UpstreamProvider) -> Vec<String> {
    if tool != ToolId::OpenCode {
        return Vec::new();
    }
    let mut names: Vec<String> = raw
        .settings_config
        .pointer("/options/headers")
        .and_then(Value::as_object)
        .map(|headers| headers.keys().cloned().collect())
        .unwrap_or_default();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names
}

fn validate_models(
    raw_models: &[String],
    supports_multiple: bool,
) -> Result<Vec<String>, AppError> {
    if raw_models.len() > MAX_MODELS || (!supports_multiple && raw_models.len() > 1) {
        return Err(save_failed());
    }
    let mut seen = HashSet::new();
    let mut models = Vec::with_capacity(raw_models.len());
    for model in raw_models {
        let model = model.trim();
        if model.is_empty()
            || model.chars().count() > MAX_MODEL_CHARS
            || model.chars().any(char::is_control)
            || !seen.insert(model.to_string())
        {
            return Err(save_failed());
        }
        models.push(model.to_string());
    }
    Ok(models)
}

fn write_models(
    tool: ToolId,
    raw: &mut UpstreamProvider,
    models: &[String],
) -> Result<(), AppError> {
    match tool {
        ToolId::ClaudeCode => write_optional_slot(
            &mut raw.settings_config,
            &["env", "ANTHROPIC_MODEL"],
            models.first().map(String::as_str),
        ),
        ToolId::GeminiCli => write_optional_slot(
            &mut raw.settings_config,
            &["env", "GEMINI_MODEL"],
            models.first().map(String::as_str),
        ),
        ToolId::Codex => {
            let root = raw
                .settings_config
                .as_object_mut()
                .ok_or_else(settings_not_object_error)?;
            let config = root.get("config").and_then(Value::as_str).unwrap_or("");
            let updated = crate::codex_config::update_codex_toml_field(
                config,
                "model",
                models.first().map(String::as_str).unwrap_or(""),
            )
            .map_err(|_| save_failed())?;
            root.insert("config".to_string(), Value::String(updated));
            Ok(())
        }
        ToolId::OpenCode => write_opencode_models(&mut raw.settings_config, models),
        ToolId::GrokBuild | ToolId::OpenClaw | ToolId::Hermes | ToolId::Pi => {
            long_tail::write_models(tool, raw, models)
        }
        ToolId::KimiCode | ToolId::DeepSeekDsh => Err(save_failed()),
    }
}

fn write_opencode_models(settings: &mut Value, models: &[String]) -> Result<(), AppError> {
    let root = settings
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?;
    let old = root
        .get("models")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut updated = Map::new();
    for model in models {
        updated.insert(
            model.clone(),
            old.get(model)
                .cloned()
                .unwrap_or_else(|| serde_json::json!({ "name": model })),
        );
    }
    root.insert("models".to_string(), Value::Object(updated));
    Ok(())
}

fn write_base_url(
    tool: ToolId,
    raw: &mut UpstreamProvider,
    draft: &ProviderAdvancedDraft,
) -> Result<(), AppError> {
    let desired = draft
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let (current, _) = raw.resolve_usage_credentials(&app_type_for(tool));
    if desired != non_empty(current).as_deref() {
        if let Some(url) = desired {
            validate_base_url(url)?;
        }
    }
    match tool {
        ToolId::ClaudeCode => write_optional_slot(
            &mut raw.settings_config,
            &["env", "ANTHROPIC_BASE_URL"],
            desired,
        ),
        ToolId::GeminiCli => write_optional_slot(
            &mut raw.settings_config,
            &["env", "GOOGLE_GEMINI_BASE_URL"],
            desired,
        ),
        ToolId::OpenCode => {
            write_optional_slot(&mut raw.settings_config, &["options", "baseURL"], desired)
        }
        ToolId::Codex => {
            let root = raw
                .settings_config
                .as_object_mut()
                .ok_or_else(settings_not_object_error)?;
            let config = root.get("config").and_then(Value::as_str).unwrap_or("");
            let updated = crate::codex_config::update_codex_toml_field(
                config,
                "base_url",
                desired.unwrap_or(""),
            )
            .map_err(|_| save_failed())?;
            root.insert("config".to_string(), Value::String(updated));
            Ok(())
        }
        ToolId::GrokBuild | ToolId::OpenClaw | ToolId::Hermes | ToolId::Pi => {
            long_tail::write_base_url(tool, &mut raw.settings_config, desired)
        }
        ToolId::KimiCode | ToolId::DeepSeekDsh => Err(save_failed()),
    }
}

fn validate_base_url(raw: &str) -> Result<(), AppError> {
    if raw.len() > MAX_BASE_URL_BYTES {
        return Err(save_failed());
    }
    let url = Url::parse(raw).map_err(|_| save_failed())?;
    if url.cannot_be_a_base()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        return Err(save_failed());
    }
    let allowed = match url.scheme() {
        "https" => true,
        "http" => url.host().is_some_and(is_loopback),
        _ => false,
    };
    if !allowed {
        return Err(save_failed());
    }
    Ok(())
}

fn is_loopback(host: Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => {
            let domain = domain.to_ascii_lowercase();
            domain == "localhost" || domain.ends_with(".localhost")
        }
        Host::Ipv4(address) => address.is_loopback(),
        Host::Ipv6(address) => address.is_loopback(),
    }
}

fn write_optional_slot(
    settings: &mut Value,
    slot: &[&str; 2],
    value: Option<&str>,
) -> Result<(), AppError> {
    if let Some(value) = value {
        return write_slot(settings, slot, value);
    }
    let root = settings
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?;
    let Some(section) = root.get_mut(slot[0]) else {
        return Ok(());
    };
    let section = section
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?;
    section.remove(slot[1]);
    Ok(())
}

fn write_headers(
    raw: &mut UpstreamProvider,
    drafts: &[ProviderHeaderDraft],
) -> Result<(), AppError> {
    if drafts.len() > MAX_HEADERS {
        return Err(save_failed());
    }
    let old = raw
        .settings_config
        .pointer("/options/headers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut seen = HashSet::new();
    let mut total = 0usize;
    let mut updated = Map::new();
    for draft in drafts {
        let name = draft.name.trim();
        let parsed_name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| save_failed())?;
        let canonical = parsed_name.as_str().to_string();
        if forbidden_header(&canonical) || !seen.insert(canonical) {
            return Err(save_failed());
        }
        let old_entry = old
            .iter()
            .find(|(old_name, _)| old_name.eq_ignore_ascii_case(name));
        let (stored_name, value) = match draft.value.as_deref() {
            Some(value) => (name, value.trim()),
            None => old_entry
                .and_then(|(old_name, old_value)| {
                    old_value.as_str().map(|v| (old_name.as_str(), v))
                })
                .ok_or_else(save_failed)?,
        };
        if value.is_empty() || value.len() > MAX_HEADER_VALUE_BYTES {
            return Err(save_failed());
        }
        HeaderValue::from_str(value).map_err(|_| save_failed())?;
        total = total.saturating_add(stored_name.len() + value.len());
        if total > MAX_HEADER_TOTAL_BYTES {
            return Err(save_failed());
        }
        updated.insert(stored_name.to_string(), Value::String(value.to_string()));
    }

    let root = raw
        .settings_config
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?;
    let options = root
        .entry("options".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(settings_not_object_error)?;
    if updated.is_empty() {
        options.remove("headers");
    } else {
        options.insert("headers".to_string(), Value::Object(updated));
    }
    Ok(())
}

fn forbidden_header(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "content-length"
            | "host"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn save_failed() -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.saveFailed")
        .with_remediation("error.remediation.checkServiceSettings")
}
