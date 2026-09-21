//! Codex (OpenAI) Provider Adapter
//!
//! Pass-through only mode, supporting a direct connection to the OpenAI API
//!
//! ## Client detection
//! Detects the official Codex clients (codex_vscode, codex_cli_rs)

use super::{AuthInfo, AuthStrategy, ProviderAdapter};
use crate::provider::{CodexChatReasoningConfig, Provider};
use crate::proxy::error::ProxyError;
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use toml::Value as TomlValue;

/// Codex adapter
pub struct CodexAdapter;

/// Whether this Codex provider's real upstream should be called through
/// OpenAI Chat Completions, even if the local Codex client is talking to CC
/// Switch through the Responses API.
pub fn codex_provider_uses_chat_completions(provider: &Provider) -> bool {
    if let Some(api_format) = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.api_format.as_deref())
        .or_else(|| {
            provider
                .settings_config
                .get("api_format")
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            provider
                .settings_config
                .get("apiFormat")
                .and_then(|v| v.as_str())
        })
    {
        return is_chat_wire_api(api_format);
    }

    if let Some(wire_api) = provider
        .settings_config
        .get("config")
        .and_then(|v| v.as_str())
        .and_then(extract_codex_wire_api_from_toml)
    {
        return is_chat_wire_api(&wire_api);
    }

    if let Some(base_url) = provider
        .settings_config
        .get("base_url")
        .or_else(|| provider.settings_config.get("baseURL"))
        .and_then(|v| v.as_str())
    {
        return is_chat_completions_url(base_url);
    }

    provider
        .settings_config
        .get("config")
        .and_then(|v| v.as_str())
        .and_then(extract_codex_base_url_from_toml)
        .map(|url| is_chat_completions_url(&url))
        .unwrap_or(false)
}

pub fn should_convert_codex_responses_to_chat(provider: &Provider, endpoint: &str) -> bool {
    let path = endpoint
        .split_once('?')
        .map_or(endpoint, |(path, _query)| path);

    matches!(
        path,
        "/responses" | "/v1/responses" | "/responses/compact" | "/v1/responses/compact"
    ) && codex_provider_uses_chat_completions(provider)
}

/// Whether a converted Codex Responses request may send `prompt_cache_key` to
/// its Chat Completions upstream. Unknown OpenAI-compatible gateways default to
/// false because many reject unsupported request fields with HTTP 400.
pub fn should_send_codex_chat_prompt_cache_key(provider: &Provider) -> bool {
    match provider
        .meta
        .as_ref()
        .and_then(|meta| meta.prompt_cache_routing.as_deref())
        .unwrap_or("auto")
    {
        "enabled" => return true,
        "disabled" => return false,
        _ => {}
    }

    let base_url = provider
        .settings_config
        .get("base_url")
        .or_else(|| provider.settings_config.get("baseURL"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            provider
                .settings_config
                .get("config")
                .and_then(|value| value.as_str())
                .and_then(extract_codex_base_url_from_toml)
        });

    let Some(base_url) = base_url else {
        return false;
    };
    let Ok(url) = url::Url::parse(&base_url) else {
        return false;
    };

    match url.host_str() {
        Some("api.openai.com") => true,
        Some("api.kimi.com") => {
            let path = url.path().trim_end_matches('/');
            path == "/coding" || path.starts_with("/coding/")
        }
        _ => false,
    }
}

/// Add a stable cache-routing key after Responses -> Chat conversion. An
/// explicit client key wins; otherwise only a real client-provided session ID
/// is eligible. Generated per-request UUIDs must never be used here.
pub fn inject_codex_chat_prompt_cache_key(
    provider: &Provider,
    chat_body: &mut JsonValue,
    explicit_key: Option<&str>,
    client_session_id: Option<&str>,
) -> bool {
    if !should_send_codex_chat_prompt_cache_key(provider) {
        return false;
    }

    let key = explicit_key
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .or_else(|| {
            client_session_id
                .map(str::trim)
                .filter(|session_id| !session_id.is_empty())
        });
    let Some(key) = key else {
        return false;
    };

    chat_body["prompt_cache_key"] = JsonValue::String(key.to_string());
    true
}

/// Whether this Codex provider's real upstream speaks the native Anthropic
/// Messages protocol (`/v1/messages`). The local Codex client always talks to CC
/// Switch through the Responses API, so CC Switch bridges Responses ⇄ Anthropic.
///
/// Determined solely from explicit config (apiFormat / wire_api); no base_url
/// guessing — Anthropic gateway addresses vary widely and guessing easily misfires.
pub fn codex_provider_uses_anthropic(provider: &Provider) -> bool {
    if let Some(api_format) = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.api_format.as_deref())
        .or_else(|| {
            provider
                .settings_config
                .get("api_format")
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            provider
                .settings_config
                .get("apiFormat")
                .and_then(|v| v.as_str())
        })
    {
        return is_anthropic_wire_api(api_format);
    }

    provider
        .settings_config
        .get("config")
        .and_then(|v| v.as_str())
        .and_then(extract_codex_wire_api_from_toml)
        .map(|wire_api| is_anthropic_wire_api(&wire_api))
        .unwrap_or(false)
}

pub fn should_convert_codex_responses_to_anthropic(provider: &Provider, endpoint: &str) -> bool {
    let path = endpoint
        .split_once('?')
        .map_or(endpoint, |(path, _query)| path);

    matches!(
        path,
        "/responses" | "/v1/responses" | "/responses/compact" | "/v1/responses/compact"
    ) && codex_provider_uses_anthropic(provider)
}

/// Whether a native-Responses Codex upstream needs Codex `namespace`/plugin
/// tool declarations flattened before forwarding, plus xAI schema sanitization.
///
/// Codex 0.142+ emits ChatGPT-backend-private `{"type":"namespace",…}` tool
/// shapes that strict third-party Responses gateways reject with
/// `422 unknown variant "namespace"`. xAI also rejects root `oneOf`/`anyOf`
/// function schemas (notably `mcp__codex_app__automation_update`). The
/// Chat/Anthropic transform paths already unwrap namespaces, so this only
/// fires on native Responses passthrough.
///
/// Covers managed xAI OAuth *and* API-key providers whose live upstream is
/// `api.x.ai` with `wire_api = "responses"`. See farion1231/cc-switch#6815.
pub fn provider_needs_responses_namespace_flatten(provider: &Provider) -> bool {
    provider.is_xai_oauth() || provider_is_xai_native_responses(provider)
}

/// True when this Codex provider talks native Responses to first-party xAI
/// (`api.x.ai`), including API-key Grok cards that are not `xai_oauth`.
fn provider_is_xai_native_responses(provider: &Provider) -> bool {
    let config_text = provider
        .settings_config
        .get("config")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let Some(wire_api) = extract_codex_wire_api_from_toml(config_text) else {
        return false;
    };
    if !wire_api.eq_ignore_ascii_case("responses") {
        return false;
    }

    extract_codex_base_url_from_toml(config_text)
        .map(|url| url.to_ascii_lowercase())
        .is_some_and(|url| url.contains("api.x.ai"))
}

fn has_explicit_codex_third_party_upstream(provider: &Provider) -> bool {
    let non_empty_setting = |key: &str| {
        provider
            .settings_config
            .get(key)
            .and_then(JsonValue::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    };
    let config = provider
        .settings_config
        .get("config")
        .and_then(JsonValue::as_str)
        .map(|text| {
            crate::codex_config::strip_codex_unified_session_bucket(text)
                .unwrap_or_else(|_| text.to_string())
        });
    let config = config.as_deref();

    ["baseUrl", "baseURL", "base_url"]
        .into_iter()
        .any(non_empty_setting)
        || config
            .and_then(crate::codex_config::extract_codex_experimental_bearer_token)
            .is_some()
        || config
            .and_then(crate::codex_config::extract_codex_base_url)
            .is_some()
        || config
            .and_then(|text| text.parse::<TomlValue>().ok())
            .and_then(|doc| {
                doc.get("model_provider")
                    .and_then(TomlValue::as_str)
                    .map(str::trim)
                    .filter(|provider_id| !provider_id.is_empty())
                    .map(str::to_string)
            })
            // Exact match, mirroring upstream: the built-in lookup is
            // case-sensitive, so `OpenAI` routes to a custom table — a
            // third-party upstream, not the official provider.
            .is_some_and(|provider_id| provider_id != "openai")
}

/// Codex Official ChatGPT cards receive authentication from the calling Codex
/// client (`requires_openai_auth = true`). Unbound cards with a stored API key
/// stay on the direct OpenAI API path instead of being sent to the ChatGPT
/// backend. The fixed legacy card keeps its existing behavior.
pub fn is_codex_official_provider(provider: &Provider) -> bool {
    let is_fixed_official_id = provider.id == crate::database::CODEX_OFFICIAL_PROVIDER_ID;
    if is_fixed_official_id && provider.category.as_deref() == Some("official") {
        return true;
    }

    let has_auth_object = provider
        .settings_config
        .get("auth")
        .is_some_and(JsonValue::is_object);
    let has_valid_config_shape = provider
        .settings_config
        .get("config")
        .is_none_or(|config| config.is_null() || config.is_string());
    if !has_auth_object || !has_valid_config_shape {
        return false;
    }

    if has_explicit_codex_third_party_upstream(provider) {
        return false;
    }

    let has_managed_account = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.managed_account_id_for("codex_oauth"))
        .is_some_and(|account_id| !account_id.trim().is_empty());
    if has_managed_account {
        return true;
    }

    let has_stored_api_key = provider
        .settings_config
        .get("auth")
        .and_then(|auth| auth.get("OPENAI_API_KEY"))
        .and_then(JsonValue::as_str)
        .is_some_and(|key| !key.trim().is_empty());
    if has_stored_api_key {
        return false;
    }

    is_fixed_official_id || provider.category.as_deref() == Some("official")
}

/// Vendors whose OFFICIAL Codex integration is a native `/responses` gateway that
/// rejects Codex's freeform custom tools (`apply_patch` with `type: "custom"`,
/// #6944). Same vendor set as `CODEX_WEB_SEARCH_REJECT_HOSTS` in `codex_config`
/// (kept separate: that list also gates aggregators by model brand). Matched on
/// host labels via `codex_url_host_matches_any`, never by substring.
const CODEX_NATIVE_RESPONSES_HOSTS: &[&str] = &[
    "bigmodel.cn",
    "z.ai",
    "xiaomimimo.com",
    "minimaxi.com",
    "minimax.io",
    "longcat.chat",
];

/// Path markers of a listed vendor's OpenAI *Chat Completions* endpoint, which is
/// NOT its Responses endpoint. Zhipu documents three separate base URLs per site
/// (Anthropic `/api/anthropic`, Chat `/api/coding/paas/v4` + pay-as-you-go
/// `/api/paas/v4`, Responses `/api/v1`) and warns that the wrong one cannot use
/// Coding Plan quota. A stored provider still pointing at a Chat path is a
/// pre-2026-09 Chat-route record: it keeps its `ProxyChat` catalog (the proxy
/// route converts it correctly; direct connect fails loudly with the #6944 400
/// until the preset is re-imported) instead of being silently steered onto the
/// wrong endpoint with a native catalog.
const CODEX_NATIVE_RESPONSES_CHAT_PATH_MARKERS: &[&str] = &["/paas/v4"];

/// Whether `base_url` points at a listed vendor's native Responses gateway, so a
/// provider whose stored `apiFormat` predates the preset's switch to
/// `openai_responses` still gets the `NativeResponses` catalog without a re-save.
pub fn is_codex_native_responses_url(base_url: &str) -> bool {
    if !crate::codex_config::codex_url_host_matches_any(base_url, CODEX_NATIVE_RESPONSES_HOSTS) {
        return false;
    }
    if is_chat_completions_url(base_url) {
        return false;
    }
    let lower = base_url.to_ascii_lowercase();
    !CODEX_NATIVE_RESPONSES_CHAT_PATH_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Resolve the model-catalog tool profile for a Codex provider using the SAME
/// Anthropic detection as the proxy router ([`codex_provider_uses_anthropic`]), so the
/// generated catalog never disagrees with the routed transform. A provider whose
/// Anthropic upstream is declared only via settings `apiFormat` or TOML `wire_api`
/// (not `meta.api_format`) would otherwise get a `ProxyChat` catalog and emit the
/// freeform `apply_patch` tool that the Anthropic transform then silently drops.
/// Non-Anthropic providers keep the existing `meta.api_format` classification.
pub fn resolve_codex_catalog_tool_profile(
    provider: &Provider,
) -> crate::codex_config::CodexCatalogToolProfile {
    use crate::codex_config::CodexCatalogToolProfile;
    if is_codex_official_provider(provider) {
        return CodexCatalogToolProfile::NativeResponses;
    }
    // xAI OAuth pins the native Responses profile regardless of editable
    // api_format, mirroring the Claude-side managed-provider invariant.
    if provider.is_xai_oauth() {
        return CodexCatalogToolProfile::NativeResponses;
    }
    if codex_provider_uses_anthropic(provider) {
        return CodexCatalogToolProfile::Anthropic;
    }

    // Defensive fallback for providers saved in SQLite before their preset
    // switched to `openai_responses` (the #6944 reporter reinstalled to no
    // effect precisely because the stale `apiFormat` lives in the DB row): a
    // base_url on a listed vendor's native Responses gateway forces the
    // NativeResponses catalog. Chat-endpoint paths are deliberately excluded —
    // see `CODEX_NATIVE_RESPONSES_CHAT_PATH_MARKERS`.
    if let Some(base_url) = provider
        .settings_config
        .get("config")
        .and_then(|v| v.as_str())
        .and_then(extract_codex_base_url_from_toml)
        .or_else(|| {
            provider
                .settings_config
                .get("base_url")
                .or_else(|| provider.settings_config.get("baseURL"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        })
    {
        if is_codex_native_responses_url(&base_url) {
            return CodexCatalogToolProfile::NativeResponses;
        }
    }

    let api_format = provider
        .meta
        .as_ref()
        .and_then(|m| m.api_format.as_deref())
        .or_else(|| {
            provider
                .settings_config
                .get("api_format")
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            provider
                .settings_config
                .get("apiFormat")
                .and_then(|v| v.as_str())
        });
    CodexCatalogToolProfile::from_api_format(api_format)
}

/// Extract the real upstream model configured for a Codex provider.
pub fn codex_provider_upstream_model(provider: &Provider) -> Option<String> {
    provider
        .settings_config
        .get("model")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            provider
                .settings_config
                .get("config")
                .and_then(|v| v.as_str())
                .and_then(|config| {
                    crate::grok_config::extract_model_config(config)
                        .map(|model| model.model)
                        .or_else(|| extract_codex_model_from_toml(config))
                })
        })
}

fn codex_provider_catalog_model_ids(provider: &Provider) -> HashSet<String> {
    provider
        .settings_config
        .get("modelCatalog")
        .and_then(|catalog| catalog.get("models"))
        .and_then(|models| models.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model.get("model").and_then(|value| value.as_str()))
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// For Codex Chat providers, ensure the request uses the configured upstream
/// model before converting the request to Chat Completions.
pub fn apply_codex_chat_upstream_model(
    provider: &Provider,
    body: &mut JsonValue,
) -> Option<String> {
    if !codex_provider_uses_chat_completions(provider) {
        return None;
    }
    apply_codex_upstream_model(provider, body)
}

/// Same model-substitution logic as `apply_codex_chat_upstream_model`, but without
/// the chat gating check. Reused by the anthropic conversion path (the forwarder has
/// already confirmed this provider uses anthropic).
pub fn apply_codex_upstream_model(provider: &Provider, body: &mut JsonValue) -> Option<String> {
    let catalog_model_ids = codex_provider_catalog_model_ids(provider);
    if let Some(request_model) = body
        .get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        if catalog_model_ids.contains(request_model) {
            return Some(request_model.to_string());
        }
    }

    let upstream_model = codex_provider_upstream_model(provider)?;
    body["model"] = JsonValue::String(upstream_model.clone());
    Some(upstream_model)
}

pub fn resolve_codex_chat_reasoning_config(
    provider: &Provider,
    body: &JsonValue,
) -> Option<CodexChatReasoningConfig> {
    let mut config = if let Some(config) = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.codex_chat_reasoning.clone())
    {
        normalize_codex_chat_reasoning_config(config)
    } else {
        infer_codex_chat_reasoning_config(provider, body)?
    };

    // Valid zen effort levels are per model (models.dev: glm-5.2 only high|max, kimi-k3 only max,
    // qwen/glm-5.1 and friends are toggle-style with no effort), and the opencode client also sends
    // values strictly as declared per model. Look the request model up in the modelCatalog's
    // reasoningLevels (the per-model declaration added in #6228) and attach it; a miss (model absent,
    // or the entry declares no effort) yields None and the transform layer omits reasoning_effort entirely.
    if config.effort_value_mode.as_deref() == Some("zen") {
        config.effort_levels = zen_catalog_effort_levels(provider, body);
    }

    Some(config)
}

/// Looks up the valid Zen effort levels for the request model in the provider modelCatalog (the
/// per-model data mirrors models.dev reasoning_options effort values). This only looks up levels
/// and never decides the platform; platform identity still comes only from name/base_url (see infer_aggregator_platform_config).
/// The DB SSOT is camelCase while hand-written or legacy data may be snake_case, so both are accepted (matching the form loader).
fn zen_catalog_effort_levels(provider: &Provider, body: &JsonValue) -> Option<Vec<String>> {
    let model = body.get("model")?.as_str()?.trim();
    if model.is_empty() {
        return None;
    }
    let entries = provider
        .settings_config
        .get("modelCatalog")?
        .get("models")?
        .as_array()?;
    let entry = entries.iter().find(|entry| {
        entry
            .get("model")
            .and_then(|value| value.as_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(model))
    })?;
    let levels_value = entry
        .get("reasoningLevels")
        .or_else(|| entry.get("reasoning_levels"))?;
    let levels: Vec<String> = levels_value
        .as_array()?
        .iter()
        .filter_map(|level| level.as_str().map(str::to_string))
        .collect();
    (!levels.is_empty()).then_some(levels)
}

fn normalize_codex_chat_reasoning_config(
    mut config: CodexChatReasoningConfig,
) -> CodexChatReasoningConfig {
    if config.supports_effort.unwrap_or(false) && config.supports_thinking.is_none() {
        config.supports_thinking = Some(true);
    }
    config
}

fn infer_codex_chat_reasoning_config(
    provider: &Provider,
    body: &JsonValue,
) -> Option<CodexChatReasoningConfig> {
    let model = body
        .get("model")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .or_else(|| codex_provider_upstream_model(provider))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let base_url = provider
        .settings_config
        .get("base_url")
        .or_else(|| provider.settings_config.get("baseURL"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            provider
                .settings_config
                .get("config")
                .and_then(|v| v.as_str())
                .and_then(extract_codex_base_url_from_toml)
        })
        .unwrap_or_default()
        .to_ascii_lowercase();
    let name = provider.name.to_ascii_lowercase();

    // Platform first: on aggregator/hosting platforms the reasoning API is defined by the platform's
    // inference framework, not the model vendor, so platform identity (name + base_url only, never the model name) is matched first and overrides model rules.
    if let Some(config) = infer_aggregator_platform_config(&name, &base_url) {
        return Some(config);
    }

    let haystack = format!("{name} {base_url} {model}");

    if haystack.contains("deepseek") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("deepseek".to_string()),
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        });
    }

    // StepFun: per the official reasoning guide and both model pages (surveyed 2026-08-15),
    // step-3.5-flash-2603 supports low/high, step-3.7-flash supports low/medium/high
    // (medium by default), and the remaining step models (including the suffix-less
    // step-3.5-flash) expose no effort. 2603 keeps the low_high collapsing map, while
    // 3.7-flash must pass through: low_high would collapse medium into high and invent a
    // fake level indistinguishable on the wire. No model in the family has a thinking switch (thinking_param is always none).
    // The second OR branch covers running the model through a relay/aggregator whose name/base_url does not contain stepfun.
    if haystack.contains("stepfun") || haystack.contains("step-3.5-flash-2603") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(model.contains("2603") || model.contains("step-3.7-flash")),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some(
                if model.contains("2603") {
                    "low_high"
                } else {
                    "passthrough"
                }
                .to_string(),
            ),
            output_format: Some("reasoning".to_string()),
            effort_levels: None,
        });
    }

    if haystack.contains("kimi") || haystack.contains("moonshot") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        });
    }

    if haystack.contains("glm") || haystack.contains("zhipu") || haystack.contains("z.ai") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        });
    }

    if haystack.contains("qwen") || haystack.contains("dashscope") || haystack.contains("bailian") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("enable_thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        });
    }

    if haystack.contains("minimax") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("reasoning_split".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_details".to_string()),
            effort_levels: None,
        });
    }

    if haystack.contains("mimo") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        });
    }

    None
}

/// On aggregator/hosting platforms the reasoning API is defined by the platform: the same model
/// can take completely different parameters per platform (DeepSeek's own API uses
/// `thinking:{type}`, SiliconFlow uses `enable_thinking`, OpenRouter uses a native
/// `reasoning:{effort}` object). Match on platform identity (name / base_url) only, never the model name, which belongs to the model vendor and would mistake a hosting platform for the vendor API.
fn infer_aggregator_platform_config(
    name: &str,
    base_url: &str,
) -> Option<CodexChatReasoningConfig> {
    let platform = format!("{name} {base_url}");

    // OpenRouter: use the native normalized object `reasoning: { effort }` (OpenRouter translates it
    // into the right reasoning parameter for each underlying model, covering more than the top-level
    // OpenAI alias reasoning_effort). effort goes through the "openrouter" value map, whose enum is
    // xhigh|high|medium|low|minimal with no max, since max triggers `400 reasoning_effort: Invalid option` (see openclaw#77350), so it is clamped to xhigh.
    // Safe degradation: do not send `thinking:{type}` (OpenRouter rejects that field) so a misconfiguration cannot get the request refused.
    if platform.contains("openrouter") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(false),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning.effort".to_string()),
            effort_value_mode: Some("openrouter".to_string()),
            output_format: Some("auto".to_string()),
            effort_levels: None,
        });
    }

    // SiliconFlow: one platform-wide `enable_thinking`, with thinking returned in reasoning_content.
    // Safe degradation: do not send effort via reasoning_effort (the platform controls depth with
    // thinking_budget, and reasoning_effort may simply be rejected).
    if platform.contains("siliconflow") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("enable_thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        });
    }

    // ModelScope API-Inference: structurally the same as SiliconFlow, with one platform-wide
    // `enable_thinking` boolean (the official model page shows extra_body {"enable_thinking": bool},
    // and the OpenAI SDK merges extra_body into the top level), with thinking in reasoning_content.
    // The Zhipu-style thinking:{type} is a vendor dialect absent from the platform docs; without this
    // branch a ModelScope provider serving GLM would wrongly get that shape from the glm model rule below.
    if platform.contains("modelscope") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("enable_thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        });
    }

    // OpenCode Zen (the opencode.ai gateway, issue #6112): its own client sends a top-level
    // `reasoning_effort` on this transport (provider/transform.ts) as the platform-normalized
    // parameter; the vendor-native thinking shape is not sent (the gateway rejects Zhipu's thinking:{type} for glm models on zen).
    // Valid levels are per model (models.dev reasoning_options; the opencode client likewise sends
    // values strictly as declared). The level table lives in reasoningLevels on each provider
    // modelCatalog entry, and the proxy clamps by request model from it (effort_levels is attached at resolve time); with no table the field is omitted.
    // Match the domain rather than a bare "opencode" so unrelated providers with opencode in the name are not caught.
    if platform.contains("opencode.ai") {
        return Some(CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("zen".to_string()),
            output_format: Some("reasoning_content".to_string()),
            effort_levels: None,
        });
    }

    None
}

fn is_chat_wire_api(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "chat"
            | "chat_completions"
            | "chat-completions"
            | "openai_chat"
            | "openai-chat"
            | "openai_chat_completions"
    )
}

fn is_anthropic_wire_api(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "anthropic" | "anthropic_messages" | "anthropic-messages" | "claude" | "messages"
    )
}

fn is_chat_completions_url(value: &str) -> bool {
    value
        .trim_end_matches('/')
        .to_ascii_lowercase()
        .ends_with("/chat/completions")
}

/// A bare origin form with no path segment after `scheme://host`. In that case `build_url`
/// appends `/v1` automatically, and paths such as Stream Check need the same check.
pub fn is_origin_only_url(value: &str) -> bool {
    let trimmed = value.trim_end_matches('/');
    match trimmed.split_once("://") {
        Some((_scheme, rest)) => !rest.contains('/'),
        None => !trimmed.contains('/'),
    }
}

fn extract_codex_wire_api_from_toml(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<TomlValue>().ok()?;

    if let Some(active_provider) = doc.get("model_provider").and_then(|v| v.as_str()) {
        if let Some(wire_api) = doc
            .get("model_providers")
            .and_then(|providers| providers.get(active_provider))
            .and_then(|provider| provider.get("wire_api"))
            .and_then(|v| v.as_str())
        {
            return Some(wire_api.to_string());
        }
    }

    doc.get("wire_api")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}

fn extract_codex_model_from_toml(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<TomlValue>().ok()?;

    doc.get("model")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

fn extract_codex_base_url_from_toml(config_text: &str) -> Option<String> {
    // Canonical parser lives in codex_config; keep this thin alias so the
    // proxy hot path and the usage-credential resolver share one implementation.
    crate::codex_config::extract_codex_base_url(config_text)
}

impl CodexAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Extracts the API key from the provider config
    fn extract_key(&self, provider: &Provider) -> Option<String> {
        // 1. Try env
        if let Some(env) = provider.settings_config.get("env") {
            if let Some(key) = env
                .get("OPENAI_API_KEY")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|key| !key.is_empty())
            {
                return Some(key.to_string());
            }
        }

        // 2. Try auth (Codex CLI form)
        if let Some(auth) = provider.settings_config.get("auth") {
            if let Some(key) = crate::codex_config::extract_codex_auth_api_key(auth) {
                return Some(key.to_string());
            }
        }

        // 3. Try a direct lookup
        if let Some(key) = provider
            .settings_config
            .get("apiKey")
            .or_else(|| provider.settings_config.get("api_key"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            return Some(key.to_string());
        }

        // 4. Try the config object
        if let Some(config) = provider.settings_config.get("config") {
            if let Some(key) = config
                .get("api_key")
                .or_else(|| config.get("apiKey"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|key| !key.is_empty())
            {
                return Some(key.to_string());
            }

            if let Some(config_str) = config.as_str() {
                if let Some((_, key)) = crate::grok_config::extract_credentials(config_str) {
                    return Some(key);
                }
                if let Some(key) =
                    crate::codex_config::extract_codex_experimental_bearer_token(config_str)
                {
                    return Some(key);
                }
            }
        }

        None
    }
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAdapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "Codex"
    }

    fn extract_base_url(&self, provider: &Provider) -> Result<String, ProxyError> {
        if is_codex_official_provider(provider) {
            return Ok(super::CHATGPT_CODEX_BASE_URL.to_string());
        }

        // xAI OAuth: ignore editable provider base URLs and always use the xAI
        // API origin associated with the managed token.
        if provider.is_xai_oauth() {
            return Ok(super::XAI_API_BASE_URL.to_string());
        }

        // 1. Try the base_url field directly
        if let Some(url) = provider
            .settings_config
            .get("base_url")
            .and_then(|v| v.as_str())
        {
            return Ok(url.trim_end_matches('/').to_string());
        }

        // 2. Try baseURL
        if let Some(url) = provider
            .settings_config
            .get("baseURL")
            .and_then(|v| v.as_str())
        {
            return Ok(url.trim_end_matches('/').to_string());
        }

        // 3. Try the config object
        if let Some(config) = provider.settings_config.get("config") {
            if let Some(url) = config.get("base_url").and_then(|v| v.as_str()) {
                return Ok(url.trim_end_matches('/').to_string());
            }

            // Try parsing the TOML string form
            if let Some(config_str) = config.as_str() {
                if let Some(url) = crate::grok_config::extract_base_url(config_str) {
                    return Ok(url.trim_end_matches('/').to_string());
                }
                if let Some(start) = config_str.find("base_url = \"") {
                    let rest = &config_str[start + 12..];
                    if let Some(end) = rest.find('"') {
                        return Ok(rest[..end].trim_end_matches('/').to_string());
                    }
                }
                if let Some(start) = config_str.find("base_url = '") {
                    let rest = &config_str[start + 12..];
                    if let Some(end) = rest.find('\'') {
                        return Ok(rest[..end].trim_end_matches('/').to_string());
                    }
                }
            }
        }

        Err(ProxyError::ConfigError(
            "Codex provider is missing the base_url setting".to_string(),
        ))
    }

    fn extract_auth(&self, provider: &Provider) -> Option<AuthInfo> {
        // xAI OAuth (Grok subscription): placeholder credentials only; the real
        // access_token is resolved per-request by the forwarder via XaiOAuthManager.
        if provider.is_xai_oauth() {
            return Some(AuthInfo::new(
                "xai_oauth_placeholder".to_string(),
                AuthStrategy::XaiOAuth,
            ));
        }

        // Anthropic upstream: the auth field is chosen by the user in the UI (meta.apiKeyField).
        //   ANTHROPIC_API_KEY    → x-api-key (AuthStrategy::Anthropic)
        //   ANTHROPIC_AUTH_TOKEN → Authorization: Bearer (default, AuthStrategy::Bearer)
        // The two are mutually exclusive to avoid a 401 from the gateway receiving
        // both auth headers at once. All other Codex upstreams stay pure Bearer.
        let strategy = if codex_provider_uses_anthropic(provider) {
            let uses_x_api_key = provider
                .meta
                .as_ref()
                .and_then(|meta| meta.api_key_field.as_deref())
                .map(|field| field.eq_ignore_ascii_case("ANTHROPIC_API_KEY"))
                .unwrap_or(false);
            if uses_x_api_key {
                AuthStrategy::Anthropic
            } else {
                AuthStrategy::Bearer
            }
        } else {
            AuthStrategy::Bearer
        };
        self.extract_key(provider)
            .map(|key| AuthInfo::new(key, strategy))
    }

    fn build_url(&self, base_url: &str, endpoint: &str) -> String {
        let base_trimmed = base_url.trim_end_matches('/');
        let endpoint_trimmed = endpoint.trim_start_matches('/');

        // An OpenAI/Codex base_url may be:
        // - a bare origin: https://api.openai.com  (needs /v1 appended)
        // - already with /v1: https://api.openai.com/v1 (just concatenate)
        // - a custom prefix: https://xxx/openai (do not add /v1, just concatenate)

        // Check whether base_url already contains /v1
        let already_has_v1 = base_trimmed.ends_with("/v1");
        let origin_only = is_origin_only_url(base_trimmed);

        let mut url = if already_has_v1 {
            // Already has /v1, just concatenate
            format!("{base_trimmed}/{endpoint_trimmed}")
        } else if origin_only {
            // Bare origin, append /v1
            format!("{base_trimmed}/v1/{endpoint_trimmed}")
        } else {
            // Custom prefix: do not add /v1, just concatenate
            format!("{base_trimmed}/{endpoint_trimmed}")
        };

        // Collapse a duplicated /v1/v1 (possible when both base_url and endpoint carry the version)
        while url.contains("/v1/v1") {
            url = url.replace("/v1/v1", "/v1");
        }

        url
    }

    fn get_auth_headers(
        &self,
        auth: &AuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        use super::adapter::auth_header_value;
        let bearer = format!("Bearer {}", auth.api_key);
        // Anthropic gateway: send only x-api-key (anthropic-version is filled in by
        // the forwarder). Mutually exclusive with Bearer to avoid a 401 from the
        // gateway receiving both auth headers at once.
        if auth.strategy == AuthStrategy::Anthropic {
            return Ok(vec![(
                http::HeaderName::from_static("x-api-key"),
                auth_header_value(&auth.api_key)?,
            )]);
        }
        Ok(vec![(
            http::HeaderName::from_static("authorization"),
            auth_header_value(&bearer)?,
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Codex".to_string(),
            settings_config: config,
            website_url: None,
            category: Some("codex".to_string()),
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn grok_build_toml_exposes_upstream_credentials_and_model() {
        let adapter = CodexAdapter::new();
        let provider = create_provider(json!({
            "config": r#"
[models]
default = "grok-4.5"

[model."grok-4.5"]
model = "upstream-grok-model"
base_url = "https://relay.example.com/v1/"
name = "Example Relay"
api_key = "grok-secret"
api_backend = "responses"
context_window = 500000
"#
        }));

        assert_eq!(
            adapter.extract_base_url(&provider).unwrap(),
            "https://relay.example.com/v1"
        );
        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "grok-secret");
        assert_eq!(auth.strategy, AuthStrategy::Bearer);
        assert_eq!(
            codex_provider_upstream_model(&provider).as_deref(),
            Some("upstream-grok-model")
        );
    }

    #[test]
    fn explicit_codex_official_cards_use_chatgpt_backend() {
        let mut provider = create_provider(json!({
            "auth": {
                "auth_mode": "chatgpt",
                "OPENAI_API_KEY": null,
                "tokens": { "refresh_token": "legacy-live-only-token" }
            },
            "config": ""
        }));
        provider.id = "unbound-official-account".to_string();
        provider.category = Some("official".to_string());
        assert!(is_codex_official_provider(&provider));

        let mut native = provider.clone();
        native.id = crate::database::CODEX_OFFICIAL_PROVIDER_ID.to_string();
        assert!(is_codex_official_provider(&native));

        provider.id = "managed-official-account".to_string();
        provider.meta = Some(crate::provider::ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            auth_binding: Some(crate::provider::AuthBinding {
                source: crate::provider::AuthBindingSource::ManagedAccount,
                auth_provider: Some("codex_oauth".to_string()),
                account_id: Some("acct-managed".to_string()),
            }),
            ..Default::default()
        });
        let adapter = CodexAdapter::new();

        assert!(is_codex_official_provider(&provider));
        assert_eq!(
            adapter
                .extract_base_url(&provider)
                .expect("official base url"),
            "https://chatgpt.com/backend-api/codex"
        );
        assert!(adapter.extract_auth(&provider).is_none());
        assert_eq!(
            adapter.build_url(
                "https://chatgpt.com/backend-api/codex",
                "/responses/compact"
            ),
            "https://chatgpt.com/backend-api/codex/responses/compact"
        );

        let mut official_api_key = create_provider(json!({
            "auth": { "OPENAI_API_KEY": "sk-official" },
            "config": ""
        }));
        official_api_key.category = Some("official".to_string());
        assert!(!is_codex_official_provider(&official_api_key));

        let mut stored_bearer = create_provider(json!({
            "auth": {},
            "config": "experimental_bearer_token = \"sk-legacy\""
        }));
        stored_bearer.category = Some("official".to_string());
        assert!(!is_codex_official_provider(&stored_bearer));

        let mut unmarked_custom = create_provider(json!({
            "auth": {},
            "config": "model_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://example.com/v1\""
        }));
        unmarked_custom.category = Some("official".to_string());
        assert!(!is_codex_official_provider(&unmarked_custom));

        let mut category_less_fixed_custom = unmarked_custom.clone();
        category_less_fixed_custom.id = crate::database::CODEX_OFFICIAL_PROVIDER_ID.to_string();
        category_less_fixed_custom.category = None;
        assert!(!is_codex_official_provider(&category_less_fixed_custom));

        let mut category_less_fixed_api_key = official_api_key.clone();
        category_less_fixed_api_key.id = crate::database::CODEX_OFFICIAL_PROVIDER_ID.to_string();
        category_less_fixed_api_key.category = None;
        assert!(!is_codex_official_provider(&category_less_fixed_api_key));

        let mut explicit_openai = create_provider(json!({
            "auth": { "OPENAI_API_KEY": "sk-official" },
            "config": "model_provider = \"openai\""
        }));
        explicit_openai.category = Some("official".to_string());
        assert!(!is_codex_official_provider(&explicit_openai));

        let mut managed_with_null_config = provider.clone();
        managed_with_null_config.category = None;
        managed_with_null_config.settings_config["config"] = JsonValue::Null;
        assert!(is_codex_official_provider(&managed_with_null_config));

        let mut unified_session = create_provider(json!({
            "auth": {},
            "config": crate::codex_config::inject_codex_unified_session_bucket("")
                .expect("inject unified session route")
        }));
        unified_session.category = Some("official".to_string());
        assert!(is_codex_official_provider(&unified_session));

        let mut implicit_custom = create_provider(json!({
            "auth": {},
            "config": "model_provider = \"ollama\""
        }));
        implicit_custom.category = Some("official".to_string());
        assert!(!is_codex_official_provider(&implicit_custom));

        let mut grok_official = create_provider(json!({ "config": "" }));
        grok_official.id = crate::database::GROKBUILD_OFFICIAL_PROVIDER_ID.to_string();
        grok_official.category = Some("official".to_string());
        assert!(!is_codex_official_provider(&grok_official));
    }

    #[test]
    fn prompt_cache_routing_auto_enables_known_upstreams_only() {
        let kimi = create_provider(json!({
            "config": r#"
model_provider = "custom"
[model_providers.custom]
base_url = "https://api.kimi.com/coding/v1"
wire_api = "responses"
"#
        }));
        let openai = create_provider(json!({
            "base_url": "https://api.openai.com/v1"
        }));
        let unknown = create_provider(json!({
            "base_url": "https://strict.example.com/v1"
        }));

        assert!(should_send_codex_chat_prompt_cache_key(&kimi));
        assert!(should_send_codex_chat_prompt_cache_key(&openai));
        assert!(!should_send_codex_chat_prompt_cache_key(&unknown));
    }

    #[test]
    fn prompt_cache_routing_user_override_wins_over_auto_detection() {
        let mut kimi = create_provider(json!({
            "base_url": "https://api.kimi.com/coding/v1"
        }));
        kimi.meta = Some(crate::provider::ProviderMeta {
            prompt_cache_routing: Some("disabled".to_string()),
            ..Default::default()
        });
        assert!(!should_send_codex_chat_prompt_cache_key(&kimi));

        let mut unknown = create_provider(json!({
            "base_url": "https://strict.example.com/v1"
        }));
        unknown.meta = Some(crate::provider::ProviderMeta {
            prompt_cache_routing: Some("enabled".to_string()),
            ..Default::default()
        });
        assert!(should_send_codex_chat_prompt_cache_key(&unknown));
    }

    #[test]
    fn prompt_cache_key_prefers_explicit_key_then_real_session() {
        let provider = create_provider(json!({
            "base_url": "https://api.kimi.com/coding/v1"
        }));
        let mut explicit_body = json!({ "model": "kimi-for-coding" });
        assert!(inject_codex_chat_prompt_cache_key(
            &provider,
            &mut explicit_body,
            Some("request-key"),
            Some("session-key"),
        ));
        assert_eq!(explicit_body["prompt_cache_key"], "request-key");

        let mut session_body = json!({ "model": "kimi-for-coding" });
        assert!(inject_codex_chat_prompt_cache_key(
            &provider,
            &mut session_body,
            None,
            Some("session-key"),
        ));
        assert_eq!(session_body["prompt_cache_key"], "session-key");
    }

    #[test]
    fn prompt_cache_key_is_not_injected_without_real_session_or_support() {
        let kimi = create_provider(json!({
            "base_url": "https://api.kimi.com/coding/v1"
        }));
        let mut no_session_body = json!({ "model": "kimi-for-coding" });
        assert!(!inject_codex_chat_prompt_cache_key(
            &kimi,
            &mut no_session_body,
            None,
            None,
        ));
        assert!(no_session_body.get("prompt_cache_key").is_none());

        let unknown = create_provider(json!({
            "base_url": "https://strict.example.com/v1"
        }));
        let mut unsupported_body = json!({ "model": "other" });
        assert!(!inject_codex_chat_prompt_cache_key(
            &unknown,
            &mut unsupported_body,
            Some("request-key"),
            Some("session-key"),
        ));
        assert!(unsupported_body.get("prompt_cache_key").is_none());
    }

    #[test]
    fn test_extract_base_url_direct() {
        let adapter = CodexAdapter::new();
        let provider = create_provider(json!({
            "base_url": "https://api.openai.com/v1"
        }));

        let url = adapter.extract_base_url(&provider).unwrap();
        assert_eq!(url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_extract_auth_from_auth_field() {
        let adapter = CodexAdapter::new();
        let provider = create_provider(json!({
            "auth": {
                "OPENAI_API_KEY": "sk-test-key-12345678"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-test-key-12345678");
        assert_eq!(auth.strategy, AuthStrategy::Bearer);
    }

    #[test]
    fn test_extract_auth_falls_back_to_config_bearer_when_auth_key_empty() {
        let adapter = CodexAdapter::new();
        let provider = create_provider(json!({
            "auth": {
                "OPENAI_API_KEY": ""
            },
            "config": r#"model_provider = "custom"

[model_providers.custom]
experimental_bearer_token = "sk-config-key"
"#
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-config-key");
        assert_eq!(auth.strategy, AuthStrategy::Bearer);
    }

    #[test]
    fn test_extract_auth_from_env() {
        let adapter = CodexAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "OPENAI_API_KEY": "sk-env-key-12345678"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-env-key-12345678");
    }

    #[test]
    fn test_build_url() {
        let adapter = CodexAdapter::new();
        let url = adapter.build_url("https://api.openai.com/v1", "/responses");
        assert_eq!(url, "https://api.openai.com/v1/responses");
    }

    // ==================== anthropic upstream detection ====================

    #[test]
    fn test_uses_anthropic_from_settings_api_format() {
        let provider = create_provider(json!({ "apiFormat": "anthropic" }));
        assert!(codex_provider_uses_anthropic(&provider));

        let provider = create_provider(json!({ "api_format": "anthropic_messages" }));
        assert!(codex_provider_uses_anthropic(&provider));
    }

    #[test]
    fn test_uses_anthropic_from_meta_api_format() {
        let mut provider = create_provider(json!({}));
        provider.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });
        assert!(codex_provider_uses_anthropic(&provider));
    }

    #[test]
    fn test_uses_anthropic_from_toml_wire_api() {
        let provider = create_provider(json!({
            "config": r#"model_provider = "custom"

[model_providers.custom]
wire_api = "anthropic"
"#
        }));
        assert!(codex_provider_uses_anthropic(&provider));
    }

    #[test]
    fn test_anthropic_false_for_chat_and_responses() {
        let chat = create_provider(json!({ "apiFormat": "openai_chat" }));
        assert!(!codex_provider_uses_anthropic(&chat));
        let responses = create_provider(json!({ "apiFormat": "openai_responses" }));
        assert!(!codex_provider_uses_anthropic(&responses));
    }

    #[test]
    fn test_anthropic_and_chat_are_mutually_exclusive() {
        let anth = create_provider(json!({ "apiFormat": "anthropic" }));
        assert!(codex_provider_uses_anthropic(&anth));
        assert!(!codex_provider_uses_chat_completions(&anth));

        let chat = create_provider(json!({ "apiFormat": "openai_chat" }));
        assert!(codex_provider_uses_chat_completions(&chat));
        assert!(!codex_provider_uses_anthropic(&chat));
    }

    #[test]
    fn test_should_convert_responses_to_anthropic_path_guard() {
        let provider = create_provider(json!({ "apiFormat": "anthropic" }));
        assert!(should_convert_codex_responses_to_anthropic(
            &provider,
            "/responses"
        ));
        assert!(should_convert_codex_responses_to_anthropic(
            &provider,
            "/v1/responses/compact"
        ));
        assert!(should_convert_codex_responses_to_anthropic(
            &provider,
            "/responses?x=1"
        ));
        assert!(!should_convert_codex_responses_to_anthropic(
            &provider,
            "/chat/completions"
        ));
    }

    #[test]
    fn test_resolve_catalog_profile_matches_router() {
        use crate::codex_config::CodexCatalogToolProfile;

        // Anthropic declared only via TOML wire_api (no meta.api_format) must still
        // resolve to the Anthropic catalog profile — this is the routing/catalog
        // divergence that let apply_patch leak through.
        let toml_anthropic = create_provider(json!({
            "config": r#"model_provider = "custom"

[model_providers.custom]
wire_api = "anthropic"
"#
        }));
        assert_eq!(
            resolve_codex_catalog_tool_profile(&toml_anthropic),
            CodexCatalogToolProfile::Anthropic
        );

        // Anthropic via settings apiFormat.
        let settings_anthropic = create_provider(json!({ "apiFormat": "anthropic" }));
        assert_eq!(
            resolve_codex_catalog_tool_profile(&settings_anthropic),
            CodexCatalogToolProfile::Anthropic
        );

        // Native openai_responses (meta) → NativeResponses; chat → ProxyChat.
        let mut native = create_provider(json!({}));
        native.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("openai_responses".to_string()),
            ..Default::default()
        });
        assert_eq!(
            resolve_codex_catalog_tool_profile(&native),
            CodexCatalogToolProfile::NativeResponses
        );

        let chat = create_provider(json!({ "apiFormat": "openai_chat" }));
        assert_eq!(
            resolve_codex_catalog_tool_profile(&chat),
            CodexCatalogToolProfile::ProxyChat
        );

        // Host fallback (#6944): a DB row saved while the Zhipu preset was still
        // `openai_chat` but whose base_url is the vendor's native Responses
        // gateway (`/api/v1`) resolves to NativeResponses without a re-save —
        // whether the URL lives in the TOML or in settings `baseURL`.
        let legacy_chat_toml = |base_url: &str| {
            create_provider(json!({
                "apiFormat": "openai_chat",
                "config": format!(
                    "model_provider = \"custom\"\n\n[model_providers.custom]\nname = \"zhipu_glm\"\nbase_url = \"{base_url}\"\nwire_api = \"responses\"\n"
                )
            }))
        };
        for base_url in [
            "https://open.bigmodel.cn/api/v1",
            "https://api.z.ai/api/v1",
            "https://Open.BigModel.cn/api/v1/",
        ] {
            assert_eq!(
                resolve_codex_catalog_tool_profile(&legacy_chat_toml(base_url)),
                CodexCatalogToolProfile::NativeResponses,
                "{base_url}"
            );
        }
        let legacy_chat_settings = create_provider(json!({
            "apiFormat": "openai_chat",
            "baseURL": "https://api.z.ai/api/v1"
        }));
        assert_eq!(
            resolve_codex_catalog_tool_profile(&legacy_chat_settings),
            CodexCatalogToolProfile::NativeResponses
        );

        // The vendor's Chat Completions endpoints are NOT its Responses gateway
        // (Zhipu: `/api/coding/paas/v4` Coding Plan, `/api/paas/v4` pay-as-you-go)
        // — a stale row there keeps ProxyChat so it is never steered onto the
        // endpoint Zhipu documents as unable to use Coding Plan quota.
        for base_url in [
            "https://open.bigmodel.cn/api/coding/paas/v4",
            "https://api.z.ai/api/coding/paas/v4",
            "https://open.bigmodel.cn/api/paas/v4",
            "https://open.bigmodel.cn/api/paas/v4/chat/completions",
        ] {
            assert_eq!(
                resolve_codex_catalog_tool_profile(&legacy_chat_toml(base_url)),
                CodexCatalogToolProfile::ProxyChat,
                "{base_url}"
            );
        }

        // Host matching is on DNS labels: `xyz.ai` must not be captured by the
        // 4-char `z.ai` entry (it would lose apply_patch and web_search).
        for base_url in [
            "https://api.xyz.ai/v1",
            "https://viz.ai/v1",
            "https://z.ai.example.com/v1",
        ] {
            assert_eq!(
                resolve_codex_catalog_tool_profile(&legacy_chat_toml(base_url)),
                CodexCatalogToolProfile::ProxyChat,
                "{base_url}"
            );
        }

        // An explicit `openai_responses` on an unlisted host is untouched by the
        // fallback (it only ever widens toward NativeResponses for listed hosts).
        let explicit_native = create_provider(json!({
            "apiFormat": "openai_responses",
            "baseURL": "https://api.xyz.ai/v1"
        }));
        assert_eq!(
            resolve_codex_catalog_tool_profile(&explicit_native),
            CodexCatalogToolProfile::NativeResponses
        );
    }

    #[test]
    fn native_responses_url_guard_matches_gateway_not_chat_paths() {
        for url in [
            "https://open.bigmodel.cn/api/v1",
            "https://api.z.ai/api/v1",
            "https://api.xiaomimimo.com/v1",
            "https://token-plan-cn.xiaomimimo.com/v1",
            "https://api.minimaxi.com/v1",
            "https://api.minimax.io/v1",
            "https://api.longcat.chat/openai/v1",
        ] {
            assert!(is_codex_native_responses_url(url), "{url}");
        }
        for url in [
            "https://open.bigmodel.cn/api/coding/paas/v4",
            "https://api.z.ai/api/coding/paas/v4",
            "https://open.bigmodel.cn/api/paas/v4",
            "https://api.minimaxi.com/v1/chat/completions",
            "https://api.xyz.ai/v1",
            "https://api.deepseek.com",
            "",
        ] {
            assert!(!is_codex_native_responses_url(url), "{url}");
        }
    }

    #[test]
    fn test_apply_codex_upstream_model_preserves_one_m_catalog_model() {
        // Regression for the [1m] path: a request model carrying the [1m] marker must
        // match its catalog entry and be preserved (not overridden by the provider
        // default) so the transform can later strip [1m] and emit the context-1m beta.
        // This only works because the forwarder no longer strips [1m] before this call
        // on the Anthropic path.
        let provider = create_provider(json!({
            "config": r#"model_provider = "custom"
model = "claude-opus-4-1"

[model_providers.custom]
wire_api = "anthropic"
"#,
            "modelCatalog": {
                "models": [
                    { "model": "claude-opus-4-1[1m]" }
                ]
            }
        }));
        let mut body = json!({ "model": "claude-opus-4-1[1m]", "input": "hi" });
        let result = apply_codex_upstream_model(&provider, &mut body);
        assert_eq!(result.as_deref(), Some("claude-opus-4-1[1m]"));
        assert_eq!(
            body.get("model").and_then(|v| v.as_str()),
            Some("claude-opus-4-1[1m]")
        );
    }

    #[test]
    fn test_anthropic_auth_defaults_to_bearer() {
        // No meta.apiKeyField (defaults to ANTHROPIC_AUTH_TOKEN) → Authorization: Bearer only
        let adapter = CodexAdapter::new();
        let provider = create_provider(json!({
            "apiFormat": "anthropic",
            "auth": { "OPENAI_API_KEY": "sk-anthropic-key-123" }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.strategy, AuthStrategy::Bearer);

        let headers = adapter.get_auth_headers(&auth).unwrap();
        let names: Vec<String> = headers
            .iter()
            .map(|(name, _)| name.as_str().to_string())
            .collect();
        assert_eq!(names, vec!["authorization".to_string()]);
    }

    #[test]
    fn test_anthropic_auth_x_api_key_when_selected() {
        // meta.apiKeyField = ANTHROPIC_API_KEY → x-api-key only
        let adapter = CodexAdapter::new();
        let mut provider = create_provider(json!({
            "apiFormat": "anthropic",
            "auth": { "OPENAI_API_KEY": "sk-anthropic-key-123" }
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("anthropic".to_string()),
            api_key_field: Some("ANTHROPIC_API_KEY".to_string()),
            ..Default::default()
        });

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.strategy, AuthStrategy::Anthropic);

        let headers = adapter.get_auth_headers(&auth).unwrap();
        let names: Vec<String> = headers
            .iter()
            .map(|(name, _)| name.as_str().to_string())
            .collect();
        assert_eq!(names, vec!["x-api-key".to_string()]);
    }

    #[test]
    fn test_build_url_origin_adds_v1() {
        let adapter = CodexAdapter::new();
        let url = adapter.build_url("https://api.openai.com", "/responses");
        assert_eq!(url, "https://api.openai.com/v1/responses");
    }

    #[test]
    fn test_build_url_custom_prefix_no_v1() {
        let adapter = CodexAdapter::new();
        let url = adapter.build_url("https://example.com/openai", "/responses");
        assert_eq!(url, "https://example.com/openai/responses");
    }

    #[test]
    fn test_build_url_dedup_v1() {
        let adapter = CodexAdapter::new();
        // base_url already contains /v1 and so does the endpoint
        let url = adapter.build_url("https://www.packyapi.com/v1", "/v1/responses");
        assert_eq!(url, "https://www.packyapi.com/v1/responses");
    }

    #[test]
    fn test_codex_provider_uses_chat_completions_from_active_wire_api() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "chat_only"
model = "gpt-5"

[model_providers.chat_only]
name = "Chat Only"
base_url = "https://example.com/v1"
wire_api = "chat"
"#
        }));

        assert!(codex_provider_uses_chat_completions(&provider));
        assert!(should_convert_codex_responses_to_chat(
            &provider,
            "/responses?stream=true"
        ));
        assert!(!should_convert_codex_responses_to_chat(
            &provider,
            "/chat/completions"
        ));
    }

    #[test]
    fn test_codex_provider_uses_chat_completions_from_full_chat_url() {
        let provider = create_provider(json!({
            "base_url": "https://example.com/v1/chat/completions"
        }));

        assert!(codex_provider_uses_chat_completions(&provider));
        assert!(should_convert_codex_responses_to_chat(
            &provider,
            "/v1/responses/compact"
        ));
    }

    #[test]
    fn test_codex_provider_uses_chat_completions_from_meta_api_format_for_compact() {
        let mut provider = create_provider(json!({
            "base_url": "https://example.com/v1"
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });

        assert!(codex_provider_uses_chat_completions(&provider));
        assert!(should_convert_codex_responses_to_chat(
            &provider,
            "/responses/compact?stream=true"
        ));
    }

    #[test]
    fn test_codex_provider_uses_chat_completions_from_meta_api_format_for_responses() {
        let mut provider = create_provider(json!({
            "base_url": "https://api.deepseek.com/v1"
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });

        assert!(should_convert_codex_responses_to_chat(
            &provider,
            "/v1/responses"
        ));
    }

    #[test]
    fn test_apply_codex_chat_upstream_model_uses_provider_config_model() {
        let mut provider = create_provider(json!({
            "config": r#"
model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        let mut body = json!({
            "model": "placeholder-client-model",
            "input": "ping"
        });

        let upstream_model = apply_codex_chat_upstream_model(&provider, &mut body);

        assert_eq!(upstream_model.as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(
            body.get("model").and_then(|v| v.as_str()),
            Some("deepseek-v4-flash")
        );
    }

    #[test]
    fn test_apply_codex_chat_upstream_model_preserves_catalog_model_selection() {
        let mut provider = create_provider(json!({
            "config": r#"
model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#,
            "modelCatalog": {
                "models": [
                    { "model": "deepseek-v4-flash" },
                    { "model": "kimi-k2" }
                ]
            }
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        let mut body = json!({
            "model": "kimi-k2",
            "input": "ping"
        });

        let upstream_model = apply_codex_chat_upstream_model(&provider, &mut body);

        assert_eq!(upstream_model.as_deref(), Some("kimi-k2"));
        assert_eq!(body.get("model").and_then(|v| v.as_str()), Some("kimi-k2"));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_infers_deepseek_effort_support() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "deepseek"
model = "deepseek-v4-pro"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
wire_api = "chat"
"#
        }));

        let config =
            resolve_codex_chat_reasoning_config(&provider, &json!({ "model": "deepseek-v4-pro" }))
                .unwrap();

        assert_eq!(config.supports_thinking, Some(true));
        assert_eq!(config.supports_effort, Some(true));
        assert_eq!(config.effort_value_mode.as_deref(), Some("deepseek"));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_explicit_meta_overrides_inference() {
        let mut provider = create_provider(json!({
            "config": r#"
model_provider = "deepseek"
model = "deepseek-v4-pro"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
wire_api = "chat"
"#
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            codex_chat_reasoning: Some(CodexChatReasoningConfig {
                supports_thinking: Some(false),
                supports_effort: Some(false),
                thinking_param: Some("none".to_string()),
                effort_param: Some("none".to_string()),
                effort_value_mode: None,
                output_format: Some("auto".to_string()),
                effort_levels: None,
            }),
            ..Default::default()
        });

        let config =
            resolve_codex_chat_reasoning_config(&provider, &json!({ "model": "deepseek-v4-pro" }))
                .unwrap();

        assert_eq!(config.supports_thinking, Some(false));
        assert_eq!(config.supports_effort, Some(false));
        assert_eq!(config.thinking_param.as_deref(), Some("none"));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_openrouter_platform_overrides_model() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "openrouter"
model = "deepseek/deepseek-chat-v3.1"

[model_providers.openrouter]
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
wire_api = "chat"
"#
        }));

        // The model name contains "deepseek" but the platform is OpenRouter, so the platform rule must win.
        let config = resolve_codex_chat_reasoning_config(
            &provider,
            &json!({ "model": "deepseek/deepseek-chat-v3.1" }),
        )
        .unwrap();

        assert_eq!(config.thinking_param.as_deref(), Some("none"));
        assert_eq!(config.effort_param.as_deref(), Some("reasoning.effort"));
        assert_eq!(config.effort_value_mode.as_deref(), Some("openrouter"));
        assert_eq!(config.supports_effort, Some(true));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_siliconflow_platform_overrides_minimax() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "siliconflow"
model = "MiniMaxAI/MiniMax-M2.5"

[model_providers.siliconflow]
name = "SiliconFlow"
base_url = "https://api.siliconflow.cn/v1"
wire_api = "chat"
"#
        }));

        // The model is MiniMax (whose own API uses reasoning_split) but the platform is SiliconFlow, so enable_thinking applies.
        let config = resolve_codex_chat_reasoning_config(
            &provider,
            &json!({ "model": "MiniMaxAI/MiniMax-M2.5" }),
        )
        .unwrap();

        assert_eq!(config.thinking_param.as_deref(), Some("enable_thinking"));
        assert_eq!(config.supports_effort, Some(false));
        assert_eq!(config.output_format.as_deref(), Some("reasoning_content"));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_modelscope_platform_overrides_glm() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "modelscope"
model = "ZhipuAI/GLM-5.2"

[model_providers.modelscope]
name = "ModelScope"
base_url = "https://api-inference.modelscope.cn/v1"
wire_api = "chat"
"#
        }));

        // The model is GLM (Zhipu's own API uses thinking:{type}) but the platform is ModelScope, so
        // the platform-level enable_thinking applies instead of the Zhipu dialect from the glm model rule.
        let config =
            resolve_codex_chat_reasoning_config(&provider, &json!({ "model": "ZhipuAI/GLM-5.2" }))
                .unwrap();

        assert_eq!(config.thinking_param.as_deref(), Some("enable_thinking"));
        assert_eq!(config.supports_effort, Some(false));
        assert_eq!(config.output_format.as_deref(), Some("reasoning_content"));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_opencode_zen_platform_overrides_model_vendor() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "opencode"
model = "glm-5.2"

[model_providers.opencode]
name = "OpenCode Go"
base_url = "https://opencode.ai/zen/go/v1"
wire_api = "chat"
"#
        }));

        // The model is GLM (Zhipu's own thinking:{type} dialect) but the platform is OpenCode Zen, so
        // the platform config must win: top-level reasoning_effort plus reasoning_content,
        // and the Zhipu thinking shape must not be injected.
        let config =
            resolve_codex_chat_reasoning_config(&provider, &json!({ "model": "glm-5.2" })).unwrap();

        assert_eq!(config.supports_thinking, Some(true));
        assert_eq!(config.supports_effort, Some(true));
        assert_eq!(config.thinking_param.as_deref(), Some("none"));
        assert_eq!(config.effort_param.as_deref(), Some("reasoning_effort"));
        assert_eq!(config.effort_value_mode.as_deref(), Some("zen"));
        assert_eq!(config.output_format.as_deref(), Some("reasoning_content"));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_zen_attaches_per_model_effort_levels() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "opencode"
model = "glm-5.2"

[model_providers.opencode]
name = "OpenCode Go"
base_url = "https://opencode.ai/zen/go/v1"
wire_api = "chat"
"#,
            "modelCatalog": {
                "models": [
                    { "model": "glm-5.2", "reasoningLevels": ["high", "max"] },
                    { "model": "deepseek-v4-flash", "reasoningLevels": ["low", "high", "max"] },
                    { "model": "glm-5.1" }
                ]
            }
        }));

        // Per-model lookup: models declaring reasoningLevels get their own levels attached (model name is case-insensitive).
        let config =
            resolve_codex_chat_reasoning_config(&provider, &json!({ "model": "GLM-5.2" })).unwrap();
        assert_eq!(
            config.effort_levels,
            Some(vec!["high".to_string(), "max".to_string()])
        );

        let config = resolve_codex_chat_reasoning_config(
            &provider,
            &json!({ "model": "deepseek-v4-flash" }),
        )
        .unwrap();
        assert_eq!(
            config.effort_levels,
            Some(vec![
                "low".to_string(),
                "high".to_string(),
                "max".to_string()
            ])
        );

        // Toggle-style models (catalog entry declares no reasoningLevels) yield None, so the transform layer omits the effort field.
        let config =
            resolve_codex_chat_reasoning_config(&provider, &json!({ "model": "glm-5.1" })).unwrap();
        assert_eq!(config.effort_value_mode.as_deref(), Some("zen"));
        assert!(config.effort_levels.is_none());

        // Models absent from the catalog likewise yield None.
        let config =
            resolve_codex_chat_reasoning_config(&provider, &json!({ "model": "kimi-k3" })).unwrap();
        assert!(config.effort_levels.is_none());
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_zen_levels_attach_on_explicit_meta_too() {
        let mut provider = create_provider(json!({
            "config": r#"
model_provider = "opencode"
model = "deepseek-v4-flash"

[model_providers.opencode]
name = "OpenCode Go"
base_url = "https://opencode.ai/zen/go/v1"
wire_api = "chat"
"#,
            "modelCatalog": {
                "models": [
                    { "model": "deepseek-v4-flash", "reasoning_levels": ["low", "high", "max"] }
                ]
            }
        }));
        // Explicit meta (zen mode picked by hand in the form) must also attach the table by request model
        // at the end of resolve, and hand-written or legacy data may use snake_case reasoning_levels (the loader accepts both).
        provider.meta = Some(crate::provider::ProviderMeta {
            codex_chat_reasoning: Some(CodexChatReasoningConfig {
                supports_thinking: Some(true),
                supports_effort: Some(true),
                thinking_param: Some("none".to_string()),
                effort_param: Some("reasoning_effort".to_string()),
                effort_value_mode: Some("zen".to_string()),
                output_format: Some("reasoning_content".to_string()),
                effort_levels: None,
            }),
            ..Default::default()
        });

        let config = resolve_codex_chat_reasoning_config(
            &provider,
            &json!({ "model": "deepseek-v4-flash" }),
        )
        .unwrap();

        assert_eq!(config.effort_value_mode.as_deref(), Some("zen"));
        assert_eq!(
            config.effort_levels,
            Some(vec![
                "low".to_string(),
                "high".to_string(),
                "max".to_string()
            ])
        );
    }

    #[test]
    fn test_infer_codex_chat_reasoning_stepfun_per_model_effort() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "stepfun"
model = "step-3.7-flash"

[model_providers.stepfun]
name = "StepFun"
base_url = "https://api.stepfun.com/step_plan/v1"
wire_api = "chat"
"#
        }));

        // step-3.7-flash: three official levels low/medium/high, so it must pass through;
        // low_high would collapse medium into high (a fake distinct level)
        let config =
            resolve_codex_chat_reasoning_config(&provider, &json!({ "model": "step-3.7-flash" }))
                .unwrap();
        assert_eq!(config.supports_effort, Some(true));
        assert_eq!(config.effort_value_mode.as_deref(), Some("passthrough"));

        // step-3.5-flash-2603: two official levels low/high, keeping the collapsing map
        let config = resolve_codex_chat_reasoning_config(
            &provider,
            &json!({ "model": "step-3.5-flash-2603" }),
        )
        .unwrap();
        assert_eq!(config.supports_effort, Some(true));
        assert_eq!(config.effort_value_mode.as_deref(), Some("low_high"));

        // Suffix-less step-3.5-flash: no official effort exposed, so nothing is sent
        let config =
            resolve_codex_chat_reasoning_config(&provider, &json!({ "model": "step-3.5-flash" }))
                .unwrap();
        assert_eq!(config.supports_effort, Some(false));
        assert_eq!(config.thinking_param.as_deref(), Some("none"));
    }

    #[test]
    fn xai_oauth_invariants_ignore_editable_base_url_and_auth() {
        let adapter = CodexAdapter::new();
        let mut provider = create_provider(json!({
            "auth": { "OPENAI_API_KEY": "user-edited" },
            "config": r#"
model = "grok-4.5"

[model_providers.custom]
name = "xai"
base_url = "https://attacker.example/v1"
wire_api = "responses"
"#
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            provider_type: Some("xai_oauth".to_string()),
            ..Default::default()
        });

        // Editable fields (base_url / auth key) must not affect hosted routing:
        // the endpoint is pinned to api.x.ai and the credential is a placeholder (the real token is injected by the forwarder).
        assert_eq!(
            adapter.extract_base_url(&provider).unwrap(),
            super::super::XAI_API_BASE_URL
        );
        let auth = adapter
            .extract_auth(&provider)
            .expect("managed auth placeholder");
        assert_eq!(auth.api_key, "xai_oauth_placeholder");
        assert_eq!(auth.strategy, AuthStrategy::XaiOAuth);
    }

    #[test]
    fn xai_oauth_pins_native_responses_catalog_profile() {
        let mut provider = create_provider(json!({ "auth": {}, "config": "" }));
        provider.meta = Some(crate::provider::ProviderMeta {
            provider_type: Some("xai_oauth".to_string()),
            // Even if api_format is changed to anthropic, the catalog profile must stay pinned to native Responses
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });

        assert!(matches!(
            resolve_codex_catalog_tool_profile(&provider),
            crate::codex_config::CodexCatalogToolProfile::NativeResponses
        ));
    }

    #[test]
    fn namespace_flatten_gate_fires_for_xai_oauth_and_api_xai_responses() {
        let mut xai = create_provider(json!({ "auth": {}, "config": "" }));
        xai.meta = Some(crate::provider::ProviderMeta {
            provider_type: Some("xai_oauth".to_string()),
            ..Default::default()
        });
        assert!(provider_needs_responses_namespace_flatten(&xai));

        // API-key Grok cards (no xai_oauth meta) still talk to api.x.ai Responses.
        let grok_key = create_provider(json!({
            "auth": { "OPENAI_API_KEY": "sk-x" },
            "config": r#"
model_provider = "custom"
model = "grok-4.6"

[model_providers.custom]
name = "xai"
base_url = "https://api.x.ai/v1"
wire_api = "responses"
"#
        }));
        assert!(provider_needs_responses_namespace_flatten(&grok_key));

        // A non-xAI Responses provider must not be flattened.
        let other = create_provider(json!({
            "auth": { "OPENAI_API_KEY": "sk-x" },
            "config": r#"
[model_providers.custom]
base_url = "https://api.deepseek.com"
wire_api = "responses"
"#
        }));
        assert!(!provider_needs_responses_namespace_flatten(&other));
    }
}
