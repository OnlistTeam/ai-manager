//! Claude (Anthropic) Provider Adapter
//!
//! Supports pass-through mode and OpenAI format conversion mode
//!
//! ## API format
//! - **anthropic** (default): Anthropic Messages API format, pass-through
//! - **openai_chat**: OpenAI Chat Completions format, requires Anthropic <-> OpenAI conversion
//! - **openai_responses**: OpenAI Responses API format, requires Anthropic <-> Responses conversion
//! - **gemini_native**: Google Gemini Native generateContent format, requires Anthropic <-> Gemini conversion
//!
//! ## Auth modes
//! - **Claude**: Official Anthropic API (x-api-key + anthropic-version)
//! - **ClaudeAuth**: Relay service (Bearer auth only, no x-api-key)
//! - **OpenRouter**: Now supports the Claude Code-compatible endpoint, pass-through by default
//! - **GitHubCopilot**: GitHub Copilot (OAuth + Copilot Token)

use super::codex_oauth_auth::{CODEX_OAUTH_CLIENT_VERSION, CODEX_OAUTH_ORIGINATOR};
use super::{AuthInfo, AuthStrategy, ProviderAdapter, ProviderType};
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use serde_json::{json, Value};

const ANTHROPIC_THINKING_PLACEHOLDER: &str = "tool call";
const ANTHROPIC_REDACTED_THINKING_PLACEHOLDER: &str = "[redacted thinking]";
// Keep hints lowercase; matching lowercases only the input value.
// Moonshot/Kimi exited on vendor request (2026-08): their endpoints no longer
// require thinking replay on tool-call turns, and injected placeholders
// disrupt the model's chain of thought. Do not re-add without re-confirming.
const REASONING_VENDOR_HINTS: &[&str] = &["deepseek", "mimo", "xiaomimimo"];

/// Returns the API format of a Claude provider
///
/// A public helper for use by the handler and forwarder.
/// Priority: meta.apiFormat > settings_config.api_format > openrouter_compat_mode > default "anthropic"
pub fn get_claude_api_format(provider: &Provider) -> &'static str {
    // 0) Managed Responses OAuth providers force their wire protocol. This is
    // an invariant, not a preset default: editable metadata must not be able to
    // send an Anthropic Messages body to a Responses-only upstream.
    if let Some(meta) = provider.meta.as_ref() {
        if matches!(
            meta.provider_type.as_deref(),
            Some("codex_oauth" | "xai_oauth")
        ) {
            return "openai_responses";
        }
    }

    // 1) Preferred: meta.apiFormat (SSOT, never written to Claude Code config)
    if let Some(meta) = provider.meta.as_ref() {
        if let Some(api_format) = meta.api_format.as_deref() {
            return match api_format {
                "openai_chat" => "openai_chat",
                "openai_responses" => "openai_responses",
                "gemini_native" => "gemini_native",
                _ => "anthropic",
            };
        }
    }

    // 2) Backward compatibility: legacy settings_config.api_format
    if let Some(api_format) = provider
        .settings_config
        .get("api_format")
        .and_then(|v| v.as_str())
    {
        return match api_format {
            "openai_chat" => "openai_chat",
            "openai_responses" => "openai_responses",
            "gemini_native" => "gemini_native",
            _ => "anthropic",
        };
    }

    // 3) Backward compatibility: legacy openrouter_compat_mode (bool/number/string)
    let raw = provider.settings_config.get("openrouter_compat_mode");
    let enabled = match raw {
        Some(serde_json::Value::Bool(v)) => *v,
        Some(serde_json::Value::Number(num)) => num.as_i64().unwrap_or(0) != 0,
        Some(serde_json::Value::String(value)) => {
            let normalized = value.trim().to_lowercase();
            normalized == "true" || normalized == "1"
        }
        _ => false,
    };

    if enabled {
        "openai_chat"
    } else {
        "anthropic"
    }
}

pub fn claude_api_format_needs_transform(api_format: &str) -> bool {
    matches!(
        api_format,
        "openai_chat" | "openai_responses" | "gemini_native"
    )
}

fn is_reasoning_vendor_identifier(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    REASONING_VENDOR_HINTS
        .iter()
        .any(|hint| value.contains(hint))
}

fn should_normalize_anthropic_tool_thinking_history(
    provider: &Provider,
    body: &Value,
    api_format: &str,
) -> bool {
    if api_format.trim() != "anthropic" {
        return false;
    }

    if body
        .get("model")
        .and_then(|m| m.as_str())
        .is_some_and(is_reasoning_vendor_identifier)
    {
        return true;
    }

    let settings = &provider.settings_config;
    [
        settings
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
            .and_then(|v| v.as_str()),
        settings.get("base_url").and_then(|v| v.as_str()),
        settings.get("baseURL").and_then(|v| v.as_str()),
        settings.get("apiEndpoint").and_then(|v| v.as_str()),
    ]
    .into_iter()
    .flatten()
    .any(is_reasoning_vendor_identifier)
}

/// DeepSeek's Anthropic-compatible endpoint requires thinking history to be
/// replayed on every assistant turn that contains tool_use. Some Anthropic SDK
/// clients keep the tool history but drop or redact the thinking block, which
/// makes DeepSeek reject the next request with `content[].thinking ... must be
/// passed back`. Normalize only the narrow tool-call history shape for
/// providers known to require plain `thinking` blocks.
pub fn normalize_anthropic_tool_thinking_history_for_provider(
    body: &mut Value,
    provider: &Provider,
    api_format: &str,
) -> bool {
    if !should_normalize_anthropic_tool_thinking_history(provider, body, api_format) {
        return false;
    }

    normalize_anthropic_tool_thinking_history(body)
}

/// DeepSeek official Anthropic-compatible endpoint URL
const DEEPSEEK_OFFICIAL_ANTHROPIC_URL: &str = "https://api.deepseek.com/anthropic";

/// Check whether the provider is configured to use DeepSeek's official
/// Anthropic-compatible endpoint.
fn is_deepseek_official_anthropic_endpoint(provider: &Provider) -> bool {
    let settings = &provider.settings_config;
    let base_url = settings
        .get("env")
        .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
        .and_then(|v| v.as_str())
        .or_else(|| settings.get("base_url").and_then(|v| v.as_str()))
        .or_else(|| settings.get("baseURL").and_then(|v| v.as_str()))
        .or_else(|| settings.get("apiEndpoint").and_then(|v| v.as_str()));

    base_url.map(|u| u.trim_end_matches('/')) == Some(DEEPSEEK_OFFICIAL_ANTHROPIC_URL)
}

/// DeepSeek's official Anthropic-compatible endpoint treats
/// `thinking: { type: "disabled" }` and effort parameters (`output_config.effort`
/// or `reasoning_effort`) as mutually exclusive, returning HTTP 400:
/// "thinking options type cannot be disabled when reasoning_effort is set".
/// This breaks Claude Code 2.1.166+ Workflow/Dynamic Workflow features.
///
/// Rather than overriding Claude Code's intentional `thinking: disabled` for
/// sub-agents, we respect that decision and remove the conflicting effort
/// parameters instead. `thinking: disabled` means "don't output thinking
/// blocks", which is the correct behavior for sub-agents that don't need
/// to display reasoning to the user.
///
/// <https://github.com/deepseek-ai/DeepSeek-V3/issues/1397>
pub fn normalize_deepseek_thinking_disabled_strip_effort(
    body: &mut Value,
    provider: &Provider,
) -> bool {
    if !is_deepseek_official_anthropic_endpoint(provider) {
        return false;
    }

    let thinking_type = body
        .get("thinking")
        .and_then(|t| t.get("type"))
        .and_then(|t| t.as_str());

    if thinking_type != Some("disabled") {
        return false;
    }

    let mut changed = false;

    // Remove output_config.effort (Anthropic format)
    if let Some(oc) = body
        .get_mut("output_config")
        .and_then(|v| v.as_object_mut())
    {
        changed |= oc.remove("effort").is_some();
        // Clean up empty output_config
        if oc.is_empty() {
            body.as_object_mut().unwrap().remove("output_config");
        }
    }

    // Remove reasoning_effort (OpenAI format, may be present in passthrough)
    if body.get("reasoning_effort").is_some() {
        body.as_object_mut().unwrap().remove("reasoning_effort");
        changed = true;
    }

    changed
}

pub fn normalize_anthropic_messages_for_provider(
    body: &mut Value,
    provider: &Provider,
    api_format: &str,
) -> bool {
    if api_format.trim() != "anthropic" {
        return false;
    }

    let mut changed =
        normalize_anthropic_tool_thinking_history_for_provider(body, provider, api_format);
    changed |= normalize_deepseek_thinking_disabled_strip_effort(body, provider);
    changed
}

fn normalize_anthropic_tool_thinking_history(body: &mut Value) -> bool {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };

    let mut changed = false;
    for message in messages {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }

        let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        if !content
            .iter()
            .any(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        {
            continue;
        }

        let mut has_thinking = false;
        for block in content.iter_mut() {
            match block.get("type").and_then(Value::as_str) {
                Some("thinking") => {
                    let has_non_empty_thinking = block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.trim().is_empty());
                    if let Some(obj) = block.as_object_mut() {
                        if obj.remove("signature").is_some() {
                            changed = true;
                        }
                        if !has_non_empty_thinking {
                            obj.insert(
                                "thinking".to_string(),
                                json!(ANTHROPIC_THINKING_PLACEHOLDER),
                            );
                            changed = true;
                        }
                    }
                    has_thinking = true;
                }
                Some("redacted_thinking") => {
                    *block = json!({
                        "type": "thinking",
                        "thinking": ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
                    });
                    has_thinking = true;
                    changed = true;
                }
                _ => {}
            }
        }

        if !has_thinking {
            content.insert(
                0,
                json!({
                    "type": "thinking",
                    "thinking": ANTHROPIC_THINKING_PLACEHOLDER
                }),
            );
            changed = true;
        }
    }

    changed
}

fn should_preserve_reasoning_content_for_openai_chat(provider: &Provider, body: &Value) -> bool {
    if body
        .get("model")
        .and_then(|m| m.as_str())
        .is_some_and(is_reasoning_vendor_identifier)
    {
        return true;
    }

    let settings = &provider.settings_config;
    let base_urls = [
        settings
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
            .and_then(|v| v.as_str()),
        settings.get("base_url").and_then(|v| v.as_str()),
        settings.get("baseURL").and_then(|v| v.as_str()),
        settings.get("apiEndpoint").and_then(|v| v.as_str()),
    ];

    base_urls
        .into_iter()
        .flatten()
        .any(is_reasoning_vendor_identifier)
}

pub fn transform_claude_request_for_api_format(
    body: serde_json::Value,
    provider: &Provider,
    api_format: &str,
    session_id: Option<&str>,
    shadow_store: Option<&super::gemini_shadow::GeminiShadowStore>,
) -> Result<serde_json::Value, ProxyError> {
    let is_codex_oauth = provider.is_codex_oauth();

    // Copilot case: prefer the session ID from metadata.user_id as the cache key
    // Format: "uuid_sessionId", so the part after "_" is the session identifier
    // Requests of the same conversation share a cache key, improving the Copilot cache hit rate
    let is_copilot = provider
        .meta
        .as_ref()
        .and_then(|m| m.provider_type.as_deref())
        == Some("github_copilot")
        || provider
            .settings_config
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .is_some_and(|u| u.contains("githubcopilot.com"));
    let session_cache_key: Option<String> = if is_copilot {
        let metadata = body.get("metadata");
        // Session extraction priority (identical to forwarder and session.rs):
        //   1. the _session_ suffix inside metadata.user_id
        //   2. metadata.session_id (the direct field)
        metadata
            .and_then(|m| m.get("user_id"))
            .and_then(|v| v.as_str())
            .and_then(super::super::session::parse_session_from_user_id)
            .or_else(|| {
                metadata
                    .and_then(|m| m.get("session_id"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            })
    } else {
        session_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
    };

    let explicit_cache_key = provider
        .meta
        .as_ref()
        .and_then(|m| m.prompt_cache_key.as_deref());
    let (cache_key, cache_key_source) = if let Some(key) = explicit_cache_key {
        (Some(key), "explicit")
    } else if let Some(key) = session_cache_key.as_deref() {
        (Some(key), "session")
    } else {
        (None, "none")
    };
    match api_format {
        "openai_responses" => {
            log::debug!(
                "[Cache] OpenAI Responses prompt_cache_key source={cache_key_source}, provider={}, codex_oauth={is_codex_oauth}, has_key={}",
                provider.id,
                cache_key.is_some()
            );
            // Codex OAuth (the ChatGPT Plus/Pro reverse proxy) needs store: false forced in the body
            // plus include: ["reasoning.encrypted_content"], both handled in the transform layer.
            let codex_fast_mode = provider.codex_fast_mode_enabled();
            let mut result = super::transform_responses::anthropic_to_responses(
                body,
                cache_key,
                is_codex_oauth,
                codex_fast_mode,
            )?;
            if provider.is_xai_oauth() {
                const REASONING_MARKER: &str = "reasoning.encrypted_content";
                let mut include = result
                    .get("include")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if !include
                    .iter()
                    .any(|item| item.as_str() == Some(REASONING_MARKER))
                {
                    include.push(json!(REASONING_MARKER));
                }
                result["include"] = json!(include);
            }
            Ok(result)
        }
        "openai_chat" => {
            let preserve_reasoning_content =
                should_preserve_reasoning_content_for_openai_chat(provider, &body);
            let mut result = super::transform::anthropic_to_openai_with_reasoning_content(
                body,
                preserve_reasoning_content,
            )?;
            // Inject prompt_cache_key only if explicitly configured in meta
            if let Some(key) = provider
                .meta
                .as_ref()
                .and_then(|m| m.prompt_cache_key.as_deref())
            {
                result["prompt_cache_key"] = serde_json::json!(key);
            }
            // Streaming requests must get stream_options.include_usage injected, otherwise OpenAI-compatible
            // upstreams emit no usage at the end of the SSE, the converted Anthropic message_delta is all
            // zeros, and the whole input/output/cache is lost (same root cause as the Codex Responses -> Chat path).
            super::transform::inject_openai_stream_include_usage(&mut result);
            Ok(result)
        }
        "gemini_native" => super::transform_gemini::anthropic_to_gemini_with_shadow(
            body,
            shadow_store,
            Some(&provider.id),
            session_id,
        ),
        _ => Ok(body),
    }
}

/// Claude adapter
pub struct ClaudeAdapter;

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Returns the provider type
    ///
    /// Detected from base_url and auth_mode:
    /// - GitHubCopilot: meta.provider_type is github_copilot, or base_url contains githubcopilot.com
    /// - CodexOAuth: meta.provider_type is codex_oauth
    /// - XaiOAuth: meta.provider_type is xai_oauth
    /// - OpenRouter: base_url contains openrouter.ai
    /// - ClaudeAuth: auth_mode is bearer_only
    /// - Claude: the default, Anthropic's own API
    pub fn provider_type(&self, provider: &Provider) -> ProviderType {
        // Detect the Gemini Native format
        if self.get_api_format(provider) == "gemini_native" {
            return match self.extract_key(provider) {
                Some(key) if key.starts_with("ya29.") || key.starts_with('{') => {
                    ProviderType::GeminiCli
                }
                _ => ProviderType::Gemini,
            };
        }

        // Detect Codex OAuth (ChatGPT Plus/Pro)
        if self.is_codex_oauth(provider) {
            return ProviderType::CodexOAuth;
        }

        if self.is_xai_oauth(provider) {
            return ProviderType::XaiOAuth;
        }

        // Detect GitHub Copilot
        if self.is_github_copilot(provider) {
            return ProviderType::GitHubCopilot;
        }

        // Detect OpenRouter
        if self.is_openrouter(provider) {
            return ProviderType::OpenRouter;
        }

        // Detect ClaudeAuth (Bearer-only auth)
        if self.is_bearer_only_mode(provider) {
            return ProviderType::ClaudeAuth;
        }

        ProviderType::Claude
    }

    /// Detects whether this is a Codex OAuth provider (the ChatGPT Plus/Pro reverse proxy)
    fn is_codex_oauth(&self, provider: &Provider) -> bool {
        if let Some(meta) = provider.meta.as_ref() {
            if meta.provider_type.as_deref() == Some("codex_oauth") {
                return true;
            }
        }
        false
    }

    fn is_xai_oauth(&self, provider: &Provider) -> bool {
        provider.is_xai_oauth()
    }

    /// Detects whether this is a GitHub Copilot provider
    fn is_github_copilot(&self, provider: &Provider) -> bool {
        // Way 1: check meta.provider_type
        if let Some(meta) = provider.meta.as_ref() {
            if meta.provider_type.as_deref() == Some("github_copilot") {
                return true;
            }
        }

        // Way 2: check base_url (a fallback for legacy data; prefer providerType going forward)
        if let Ok(base_url) = self.extract_base_url(provider) {
            if base_url.contains("githubcopilot.com") {
                return true;
            }
        }

        false
    }

    /// Detects whether OpenRouter is in use
    fn is_openrouter(&self, provider: &Provider) -> bool {
        if let Ok(base_url) = self.extract_base_url(provider) {
            return base_url.contains("openrouter.ai");
        }
        false
    }

    /// Returns the API format
    ///
    /// Read from provider.meta.api_format:
    /// - "anthropic" (default): Anthropic Messages API format, passed straight through
    /// - "openai_chat": OpenAI Chat Completions format, needs conversion
    /// - "openai_responses": OpenAI Responses API format, needs conversion
    fn get_api_format(&self, provider: &Provider) -> &'static str {
        get_claude_api_format(provider)
    }

    /// Detects whether Bearer-only auth mode is in use
    fn is_bearer_only_mode(&self, provider: &Provider) -> bool {
        // Check auth_mode in settings_config
        if let Some(auth_mode) = provider
            .settings_config
            .get("auth_mode")
            .and_then(|v| v.as_str())
        {
            if auth_mode == "bearer_only" {
                return true;
            }
        }

        // Check AUTH_MODE in env
        if let Some(env) = provider.settings_config.get("env") {
            if let Some(auth_mode) = env.get("AUTH_MODE").and_then(|v| v.as_str()) {
                if auth_mode == "bearer_only" {
                    return true;
                }
            }
        }

        false
    }

    /// Extracts the API key from the provider config
    fn extract_key(&self, provider: &Provider) -> Option<String> {
        if let Some(env) = provider.settings_config.get("env") {
            // The standard Anthropic key
            if let Some(key) = env
                .get("ANTHROPIC_AUTH_TOKEN")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                log::debug!("[Claude] using ANTHROPIC_AUTH_TOKEN");
                return Some(key.to_string());
            }
            if let Some(key) = env
                .get("ANTHROPIC_API_KEY")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                log::debug!("[Claude] using ANTHROPIC_API_KEY");
                return Some(key.to_string());
            }
            // OpenRouter key
            if let Some(key) = env
                .get("OPENROUTER_API_KEY")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                log::debug!("[Claude] using OPENROUTER_API_KEY");
                return Some(key.to_string());
            }
            // Alternative OpenAI key (used for OpenRouter)
            if let Some(key) = env
                .get("OPENAI_API_KEY")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                log::debug!("[Claude] using OPENAI_API_KEY");
                return Some(key.to_string());
            }
            // Gemini Native key
            if let Some(key) = env
                .get("GEMINI_API_KEY")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                log::debug!("[Claude] using GEMINI_API_KEY");
                return Some(key.to_string());
            }
        }

        // Try a direct lookup
        if let Some(key) = provider
            .settings_config
            .get("apiKey")
            .or_else(|| provider.settings_config.get("api_key"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            log::debug!("[Claude] using apiKey/api_key");
            return Some(key.to_string());
        }

        log::warn!("[Claude] no valid API key found");
        None
    }

    /// Infers the default Anthropic auth strategy from which env variable name was filled in.
    ///
    /// Matches the native Anthropic SDK semantics:
    /// - `ANTHROPIC_AUTH_TOKEN` -> `ClaudeAuth` (sends `Authorization: Bearer`)
    /// - `ANTHROPIC_API_KEY`    -> `Anthropic` (sends `x-api-key`)
    ///
    /// Priority matches [`extract_key`]; when both are absent it returns `None` and the caller picks the fallback.
    fn infer_anthropic_auth_strategy(&self, provider: &Provider) -> Option<AuthStrategy> {
        let env = provider.settings_config.get("env")?;

        let has_value = |key: &str| -> bool {
            env.get(key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_some()
        };

        if has_value("ANTHROPIC_AUTH_TOKEN") {
            return Some(AuthStrategy::ClaudeAuth);
        }
        if has_value("ANTHROPIC_API_KEY") {
            return Some(AuthStrategy::Anthropic);
        }
        None
    }
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAdapter for ClaudeAdapter {
    fn name(&self) -> &'static str {
        "Claude"
    }

    fn extract_base_url(&self, provider: &Provider) -> Result<String, ProxyError> {
        // Codex OAuth: force the ChatGPT backend API endpoint (ignoring the configured base_url)
        if self.is_codex_oauth(provider) {
            return Ok(super::CHATGPT_CODEX_BASE_URL.to_string());
        }

        // xAI OAuth: ignore editable provider base URLs and always use the xAI
        // API origin associated with the managed token.
        if self.is_xai_oauth(provider) {
            return Ok(super::XAI_API_BASE_URL.to_string());
        }

        // 1. Read from env
        if let Some(env) = provider.settings_config.get("env") {
            if let Some(url) = env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()) {
                return Ok(url.trim_end_matches('/').to_string());
            }
        }

        // 2. Try a direct lookup
        if let Some(url) = provider
            .settings_config
            .get("base_url")
            .and_then(|v| v.as_str())
        {
            return Ok(url.trim_end_matches('/').to_string());
        }

        if let Some(url) = provider
            .settings_config
            .get("baseURL")
            .and_then(|v| v.as_str())
        {
            return Ok(url.trim_end_matches('/').to_string());
        }

        if let Some(url) = provider
            .settings_config
            .get("apiEndpoint")
            .and_then(|v| v.as_str())
        {
            return Ok(url.trim_end_matches('/').to_string());
        }

        Err(ProxyError::ConfigError(
            "Claude provider is missing the base_url setting".to_string(),
        ))
    }

    fn extract_auth(&self, provider: &Provider) -> Option<AuthInfo> {
        let provider_type = self.provider_type(provider);

        // GitHub Copilot uses a special auth strategy
        // The real token is fetched dynamically when the request is proxied
        if provider_type == ProviderType::GitHubCopilot {
            // Return a placeholder; the real token comes from CopilotAuthManager at runtime
            return Some(AuthInfo::new(
                "copilot_placeholder".to_string(),
                AuthStrategy::GitHubCopilot,
            ));
        }

        // Codex OAuth (ChatGPT Plus/Pro) also uses a placeholder
        // The real access_token comes from CodexOAuthManager at runtime
        if provider_type == ProviderType::CodexOAuth {
            return Some(AuthInfo::new(
                "codex_oauth_placeholder".to_string(),
                AuthStrategy::CodexOAuth,
            ));
        }

        if provider_type == ProviderType::XaiOAuth {
            return Some(AuthInfo::new(
                "xai_oauth_placeholder".to_string(),
                AuthStrategy::XaiOAuth,
            ));
        }

        let key = self.extract_key(provider)?;

        match provider_type {
            ProviderType::GeminiCli => {
                // Parse stored OAuth JSON and only attach access_token when
                // it's actually usable. `parse_oauth_credentials` accepts
                // refresh-token-only JSON (which is legitimate before the
                // first refresh) and also surfaces `{"access_token": "", ...}`
                // for expired credentials. In both cases we would otherwise
                // send `Authorization: Bearer ` to upstream and get a 401.
                //
                // CC Switch does not currently exchange the refresh_token for
                // a fresh access_token. Until that path exists, degrade to
                // plain GoogleOAuth strategy (which still sends the raw key
                // as a fallback) and log loudly so users know to refresh
                // their `~/.gemini/oauth_creds.json`.
                match super::gemini::GeminiAdapter::new().parse_oauth_credentials(&key) {
                    Some(creds) if !creds.access_token.is_empty() => {
                        Some(AuthInfo::with_access_token(key, creds.access_token))
                    }
                    Some(_) => {
                        log::warn!(
                            "[Gemini OAuth] access_token missing or empty for provider `{}`; \
                             bearer auth will likely fail with 401. Refresh \
                             ~/.gemini/oauth_creds.json via the gemini CLI to obtain a new token.",
                            provider.id
                        );
                        Some(AuthInfo::new(key, AuthStrategy::GoogleOAuth))
                    }
                    None => Some(AuthInfo::new(key, AuthStrategy::GoogleOAuth)),
                }
            }
            ProviderType::Gemini => Some(AuthInfo::new(key, AuthStrategy::Google)),
            ProviderType::OpenRouter => Some(AuthInfo::new(key, AuthStrategy::Bearer)),
            ProviderType::ClaudeAuth => Some(AuthInfo::new(key, AuthStrategy::ClaudeAuth)),
            _ => {
                // Infer the auth strategy from the env variable name, matching Anthropic SDK semantics:
                // ANTHROPIC_AUTH_TOKEN → Authorization: Bearer
                // ANTHROPIC_API_KEY    → x-api-key
                // Other sources (a directly filled apiKey and similar) default to x-api-key (Anthropic's own protocol).
                let strategy = self
                    .infer_anthropic_auth_strategy(provider)
                    .unwrap_or(AuthStrategy::Anthropic);
                Some(AuthInfo::new(key, strategy))
            }
        }
    }

    fn build_url(&self, base_url: &str, endpoint: &str) -> String {
        // Codex OAuth: every request goes to the /responses endpoint
        if base_url == super::CHATGPT_CODEX_BASE_URL {
            let _ = endpoint; // ignore the original endpoint
            return format!("{}/responses", super::CHATGPT_CODEX_BASE_URL);
        }

        // Defense in depth for callers that bypass endpoint rewriting.
        if base_url == super::XAI_API_BASE_URL {
            let query = endpoint.split_once('?').map(|(_, query)| query);
            return match query {
                Some(query) if !query.is_empty() => {
                    format!("{}/responses?{query}", super::XAI_API_BASE_URL)
                }
                _ => format!("{}/responses", super::XAI_API_BASE_URL),
            };
        }

        // NOTE:
        // OpenRouter used to offer only an OpenAI Chat Completions compatible API, so Claude's
        // `/v1/messages` had to be mapped to `/v1/chat/completions` with Anthropic/OpenAI conversion.
        //
        // OpenRouter now ships a Claude Code compatible API, so the endpoint passes through by default.
        // To fall back to the old behaviour, rewrite the endpoint in the forwarder based on needs_transform.
        //
        let mut base = format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            endpoint.trim_start_matches('/')
        );

        // Collapse a duplicated /v1/v1 (possible when both base_url and endpoint carry the version)
        while base.contains("/v1/v1") {
            base = base.replace("/v1/v1", "/v1");
        }

        base
    }

    fn get_auth_headers(
        &self,
        auth: &AuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        use super::adapter::auth_header_value as hv;
        use http::{HeaderName, HeaderValue};
        // Note: anthropic-version is handled in forwarder.rs (pass the client value through or set a default)
        let bearer = format!("Bearer {}", auth.api_key);
        Ok(match auth.strategy {
            AuthStrategy::Anthropic => {
                vec![(HeaderName::from_static("x-api-key"), hv(&auth.api_key)?)]
            }
            AuthStrategy::ClaudeAuth | AuthStrategy::Bearer => {
                vec![(HeaderName::from_static("authorization"), hv(&bearer)?)]
            }
            AuthStrategy::Google => vec![(
                HeaderName::from_static("x-goog-api-key"),
                hv(&auth.api_key)?,
            )],
            AuthStrategy::GoogleOAuth => {
                let token = auth.access_token.as_ref().unwrap_or(&auth.api_key);
                vec![
                    (
                        HeaderName::from_static("authorization"),
                        hv(&format!("Bearer {token}"))?,
                    ),
                    (
                        HeaderName::from_static("x-goog-api-client"),
                        HeaderValue::from_static("GeminiCLI/1.0"),
                    ),
                ]
            }
            AuthStrategy::CodexOAuth => {
                // Note: the bearer token is injected into auth.api_key by the forwarder
                // ChatGPT-Account-Id is injected as an extra header by the forwarder
                vec![
                    (HeaderName::from_static("authorization"), hv(&bearer)?),
                    (
                        HeaderName::from_static("originator"),
                        HeaderValue::from_static(CODEX_OAUTH_ORIGINATOR),
                    ),
                    (
                        HeaderName::from_static("version"),
                        HeaderValue::from_static(CODEX_OAUTH_CLIENT_VERSION),
                    ),
                ]
            }
            AuthStrategy::XaiOAuth => {
                vec![(HeaderName::from_static("authorization"), hv(&bearer)?)]
            }
            AuthStrategy::GitHubCopilot => {
                // Generate a request trace ID
                let request_id = uuid::Uuid::new_v4().to_string();
                vec![
                    (HeaderName::from_static("authorization"), hv(&bearer)?),
                    (
                        HeaderName::from_static("editor-version"),
                        HeaderValue::from_static(super::copilot_auth::COPILOT_EDITOR_VERSION),
                    ),
                    (
                        HeaderName::from_static("editor-plugin-version"),
                        HeaderValue::from_static(super::copilot_auth::COPILOT_PLUGIN_VERSION),
                    ),
                    (
                        HeaderName::from_static("copilot-integration-id"),
                        HeaderValue::from_static(super::copilot_auth::COPILOT_INTEGRATION_ID),
                    ),
                    (
                        HeaderName::from_static("user-agent"),
                        HeaderValue::from_static(super::copilot_auth::COPILOT_USER_AGENT),
                    ),
                    (
                        HeaderName::from_static("x-github-api-version"),
                        HeaderValue::from_static(super::copilot_auth::COPILOT_API_VERSION),
                    ),
                    // Key Copilot headers added on 2026-04-01
                    (
                        HeaderName::from_static("openai-intent"),
                        HeaderValue::from_static("conversation-agent"),
                    ),
                    (
                        HeaderName::from_static("x-initiator"),
                        HeaderValue::from_static("user"),
                    ),
                    (
                        HeaderName::from_static("x-interaction-type"),
                        HeaderValue::from_static("conversation-agent"),
                    ),
                    // x-interaction-id is injected by the forwarder on demand (only when a session exists)
                    (
                        HeaderName::from_static("x-vscode-user-agent-library-version"),
                        HeaderValue::from_static("electron-fetch"),
                    ),
                    (HeaderName::from_static("x-request-id"), hv(&request_id)?),
                    (HeaderName::from_static("x-agent-task-id"), hv(&request_id)?),
                ]
            }
        })
    }

    fn needs_transform(&self, provider: &Provider) -> bool {
        // GitHub Copilot always needs conversion (Anthropic -> OpenAI)
        if self.is_github_copilot(provider) {
            return true;
        }

        // Codex OAuth always needs conversion (Anthropic -> OpenAI Responses API)
        if self.is_codex_oauth(provider) {
            return true;
        }

        if self.is_xai_oauth(provider) {
            return true;
        }

        // api_format decides whether conversion is needed
        // - "anthropic" (default): pass straight through, no conversion
        // - "openai_chat": needs Anthropic/OpenAI Chat Completions conversion
        // - "openai_responses": needs Anthropic/OpenAI Responses API conversion
        matches!(
            self.get_api_format(provider),
            "openai_chat" | "openai_responses" | "gemini_native"
        )
    }

    fn transform_request(
        &self,
        body: serde_json::Value,
        provider: &Provider,
    ) -> Result<serde_json::Value, ProxyError> {
        transform_claude_request_for_api_format(
            body,
            provider,
            self.get_api_format(provider),
            None,
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderMeta;
    use serde_json::json;

    fn create_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Claude".to_string(),
            settings_config: config,
            website_url: None,
            category: Some("claude".to_string()),
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    fn create_provider_with_meta(config: serde_json::Value, meta: ProviderMeta) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Claude".to_string(),
            settings_config: config,
            website_url: None,
            category: Some("claude".to_string()),
            created_at: None,
            sort_index: None,
            notes: None,
            meta: Some(meta),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn test_extract_base_url_from_env() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }));

        let url = adapter.extract_base_url(&provider).unwrap();
        assert_eq!(url, "https://api.anthropic.com");
    }

    #[test]
    fn test_extract_auth_anthropic_auth_token_uses_claude_auth_strategy() {
        // In the Anthropic SDK, ANTHROPIC_AUTH_TOKEN means Authorization: Bearer,
        // so it uses the ClaudeAuth strategy rather than Anthropic (x-api-key).
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-ant-test-key");
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
    }

    #[test]
    fn test_extract_auth_anthropic_api_key() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_API_KEY": "sk-ant-test-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-ant-test-key");
        assert_eq!(auth.strategy, AuthStrategy::Anthropic);
    }

    #[test]
    fn test_extract_auth_both_env_vars_prefer_auth_token() {
        // When both variables are set, extract_key picks AUTH_TOKEN and the inferred strategy must agree.
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-from-auth-token",
                "ANTHROPIC_API_KEY": "sk-from-api-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-from-auth-token");
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
    }

    #[test]
    fn test_extract_auth_apikey_field_fallback_uses_anthropic_strategy() {
        // When no ANTHROPIC_* env var is set and the apiKey field is used directly, there is no
        // explicit preference, so the default is Anthropic's own protocol (x-api-key).
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "apiKey": "sk-direct",
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-direct");
        assert_eq!(auth.strategy, AuthStrategy::Anthropic);
    }

    #[test]
    fn codex_oauth_generation_uses_gpt6_compatible_identity() {
        let headers: http::HeaderMap = ClaudeAdapter::new()
            .get_auth_headers(&AuthInfo::new(
                "test-token".into(),
                AuthStrategy::CodexOAuth,
            ))
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(headers["authorization"], "Bearer test-token");
        assert_eq!(headers["originator"], "codex_cli_rs");
        let version: Vec<u32> = headers["version"]
            .to_str()
            .unwrap()
            .split('.')
            .map(|part| part.parse().unwrap())
            .collect();
        // Official rust-v0.153.4 catalog: gpt-6-astra requires 0.153.0.
        assert!(
            version.as_slice() >= [0, 153, 0].as_slice(),
            "gpt-6-astra requires Codex >= 0.153.0; sent {version:?}"
        );
    }

    #[test]
    fn test_get_auth_headers_anthropic_emits_x_api_key() {
        let adapter = ClaudeAdapter::new();
        let auth = AuthInfo::new("sk-ant-test".to_string(), AuthStrategy::Anthropic);

        let headers = adapter.get_auth_headers(&auth).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "x-api-key");
        assert_eq!(headers[0].1.to_str().unwrap(), "sk-ant-test");
    }

    #[test]
    fn test_get_auth_headers_claude_auth_emits_authorization_bearer() {
        let adapter = ClaudeAdapter::new();
        let auth = AuthInfo::new("sk-relay-test".to_string(), AuthStrategy::ClaudeAuth);

        let headers = adapter.get_auth_headers(&auth).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(headers[0].1.to_str().unwrap(), "Bearer sk-relay-test");
    }

    #[test]
    fn test_get_auth_headers_bearer_emits_authorization_bearer() {
        let adapter = ClaudeAdapter::new();
        let auth = AuthInfo::new("sk-or-test".to_string(), AuthStrategy::Bearer);

        let headers = adapter.get_auth_headers(&auth).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(headers[0].1.to_str().unwrap(), "Bearer sk-or-test");
    }

    #[test]
    fn test_get_auth_headers_rejects_illegal_header_chars() {
        // A pasted key containing \r\n must never panic the process
        let adapter = ClaudeAdapter::new();
        let auth = AuthInfo::new(
            "sk-ant-bad\r\nX-Inject: 1".to_string(),
            AuthStrategy::Anthropic,
        );

        let result = adapter.get_auth_headers(&auth);
        assert!(result.is_err(), "expected AuthError, got Ok");
        assert!(matches!(result, Err(ProxyError::AuthError(_))));
    }

    #[test]
    fn test_extract_auth_openrouter() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "OPENROUTER_API_KEY": "sk-or-test-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-or-test-key");
        assert_eq!(auth.strategy, AuthStrategy::Bearer);
    }

    #[test]
    fn test_extract_auth_gemini_api_key() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com/v1beta",
                    "GEMINI_API_KEY": "gemini-test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "gemini-test-key");
        assert_eq!(auth.strategy, AuthStrategy::Google);
    }

    #[test]
    fn test_extract_auth_claude_auth_mode() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-proxy-key"
            },
            "auth_mode": "bearer_only"
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-proxy-key");
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
    }

    #[test]
    fn test_extract_auth_claude_auth_env_mode() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-proxy-key",
                "AUTH_MODE": "bearer_only"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-proxy-key");
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
    }

    /// Regression: a Gemini OAuth credential JSON that carries only a
    /// refresh_token (no active access_token) must not be surfaced as an
    /// `AuthInfo` whose bearer would be empty. Without the guard, downstream
    /// header injection produces `Authorization: Bearer ` and a deterministic
    /// 401 from upstream.
    #[test]
    fn test_extract_auth_gemini_cli_refresh_only_json_does_not_expose_empty_bearer() {
        let adapter = ClaudeAdapter::new();
        let refresh_only_json =
            r#"{"refresh_token":"rt-abc","client_id":"cid","client_secret":"cs"}"#;
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": refresh_only_json
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        let auth = adapter.extract_auth(&provider).unwrap();
        // access_token must not be surfaced as `Some("")` — the OAuth header
        // builder uses `access_token.as_ref().unwrap_or(&api_key)`, so a
        // `Some("")` would win over the raw key and emit `Bearer `.
        assert!(
            auth.access_token.as_deref().is_none_or(|t| !t.is_empty()),
            "empty access_token leaked into AuthInfo"
        );
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
    }

    /// Companion case: a JSON credential with an empty-string `access_token`
    /// field (the shape an expired credential can take after partial writes)
    /// must degrade the same way.
    #[test]
    fn test_extract_auth_gemini_cli_empty_access_token_degrades_to_raw_key() {
        let adapter = ClaudeAdapter::new();
        let expired_json = r#"{"access_token":"","refresh_token":"rt-abc","client_id":"cid","client_secret":"cs"}"#;
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": expired_json
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        let auth = adapter.extract_auth(&provider).unwrap();
        assert!(
            auth.access_token.as_deref().is_none_or(|t| !t.is_empty()),
            "empty access_token leaked into AuthInfo"
        );
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
    }

    /// Counter-case: a well-formed JSON credential with a non-empty
    /// access_token must still flow through the OAuth path unchanged.
    #[test]
    fn test_extract_auth_gemini_cli_valid_json_keeps_access_token() {
        let adapter = ClaudeAdapter::new();
        let valid_json = r#"{"access_token":"ya29.valid","refresh_token":"rt"}"#;
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": valid_json
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.access_token.as_deref(), Some("ya29.valid"));
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
    }

    /// Regression: copying from oauth_creds.json often brings a leading newline or space. Without a
    /// trim, `starts_with('{')` misses, the value is misclassified as `ProviderType::Gemini`, and the
    /// raw JSON goes out as `x-goog-api-key`, causing a 401. The trim must happen before both the
    /// provider type decision and OAuth parsing.
    #[test]
    fn test_extract_auth_gemini_cli_json_with_leading_whitespace_classifies_correctly() {
        let adapter = ClaudeAdapter::new();
        let valid_json = r#"{"access_token":"ya29.valid","refresh_token":"rt"}"#;
        let key_with_whitespace = format!("\n  {valid_json}\n");
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": key_with_whitespace
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        assert_eq!(adapter.provider_type(&provider), ProviderType::GeminiCli);

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.access_token.as_deref(), Some("ya29.valid"));
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
    }

    /// Regression: a bare `ya29.` access_token with a leading newline must still be trimmed and
    /// recognised as Gemini CLI OAuth, so leading whitespace cannot defeat the `starts_with("ya29.")` check.
    #[test]
    fn test_extract_auth_gemini_cli_access_token_with_leading_newline_classifies_correctly() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": "\nya29.raw-token-value\n"
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        assert_eq!(adapter.provider_type(&provider), ProviderType::GeminiCli);

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.access_token.as_deref(), Some("ya29.raw-token-value"));
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
    }

    #[test]
    fn test_provider_type_detection() {
        let adapter = ClaudeAdapter::new();

        // Anthropic's own API
        let anthropic = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test"
            }
        }));
        assert_eq!(adapter.provider_type(&anthropic), ProviderType::Claude);

        // OpenRouter
        let openrouter = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "OPENROUTER_API_KEY": "sk-or-test"
            }
        }));
        assert_eq!(adapter.provider_type(&openrouter), ProviderType::OpenRouter);

        // ClaudeAuth
        let claude_auth = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-test"
            },
            "auth_mode": "bearer_only"
        }));
        assert_eq!(
            adapter.provider_type(&claude_auth),
            ProviderType::ClaudeAuth
        );
    }

    #[test]
    fn test_build_url_anthropic() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://api.anthropic.com", "/v1/messages");
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn xai_oauth_invariants_ignore_editable_format_and_base_url() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://attacker.example/anthropic",
                    "ANTHROPIC_API_KEY": "user-edited"
                }
            }),
            ProviderMeta {
                provider_type: Some("xai_oauth".to_string()),
                api_format: Some("anthropic".to_string()),
                is_full_url: Some(true),
                ..Default::default()
            },
        );

        assert_eq!(get_claude_api_format(&provider), "openai_responses");
        assert_eq!(adapter.provider_type(&provider), ProviderType::XaiOAuth);
        assert_eq!(
            adapter.extract_base_url(&provider).unwrap(),
            super::super::XAI_API_BASE_URL
        );
        assert!(adapter.needs_transform(&provider));
        assert_eq!(
            adapter
                .extract_auth(&provider)
                .expect("managed auth placeholder")
                .strategy,
            AuthStrategy::XaiOAuth
        );
        assert_eq!(
            adapter.build_url(super::super::XAI_API_BASE_URL, "/v1/messages?beta=1"),
            "https://api.x.ai/v1/responses?beta=1"
        );

        let transformed = transform_claude_request_for_api_format(
            json!({
                "model": "grok-4.5",
                "max_tokens": 2048,
                "thinking": { "type": "enabled", "budget_tokens": 20000 },
                "messages": [{ "role": "user", "content": "hello" }]
            }),
            &provider,
            "openai_responses",
            None,
            None,
        )
        .unwrap();
        assert_eq!(transformed["reasoning"]["effort"], json!("high"));
        assert_eq!(
            transformed["include"],
            json!(["reasoning.encrypted_content"])
        );
        assert!(transformed.get("store").is_none());
    }

    #[test]
    fn test_build_url_openrouter() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://openrouter.ai/api", "/v1/messages");
        assert_eq!(url, "https://openrouter.ai/api/v1/messages");
    }

    #[test]
    fn test_build_url_no_beta_for_other_endpoints() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://api.anthropic.com", "/v1/complete");
        assert_eq!(url, "https://api.anthropic.com/v1/complete");
    }

    #[test]
    fn test_build_url_preserve_existing_query() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://api.anthropic.com", "/v1/messages?foo=bar");
        assert_eq!(url, "https://api.anthropic.com/v1/messages?foo=bar");
    }

    #[test]
    fn test_build_url_no_beta_for_github_copilot() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://api.githubcopilot.com", "/v1/messages");
        assert_eq!(url, "https://api.githubcopilot.com/v1/messages");
    }

    #[test]
    fn test_build_url_no_beta_for_openai_chat_completions() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://integrate.api.nvidia.com", "/v1/chat/completions");
        assert_eq!(url, "https://integrate.api.nvidia.com/v1/chat/completions");
    }

    #[test]
    fn test_needs_transform() {
        let adapter = ClaudeAdapter::new();

        // Default: no transform (anthropic format) - no meta
        let anthropic_provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }));
        assert!(!adapter.needs_transform(&anthropic_provider));

        // Explicit anthropic format in meta: no transform
        let explicit_anthropic = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("anthropic".to_string()),
                ..Default::default()
            },
        );
        assert!(!adapter.needs_transform(&explicit_anthropic));

        // Legacy settings_config.api_format: openai_chat should enable transform
        let legacy_settings_api_format = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "api_format": "openai_chat"
        }));
        assert!(adapter.needs_transform(&legacy_settings_api_format));

        // Legacy openrouter_compat_mode: bool/number/string should enable transform
        let legacy_openrouter_bool = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "openrouter_compat_mode": true
        }));
        assert!(adapter.needs_transform(&legacy_openrouter_bool));

        let legacy_openrouter_num = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "openrouter_compat_mode": 1
        }));
        assert!(adapter.needs_transform(&legacy_openrouter_num));

        let legacy_openrouter_str = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "openrouter_compat_mode": "true"
        }));
        assert!(adapter.needs_transform(&legacy_openrouter_str));

        // OpenAI Chat format in meta: needs transform
        let openai_chat_provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        assert!(adapter.needs_transform(&openai_chat_provider));

        // OpenAI Responses format in meta: needs transform
        let openai_responses_provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                ..Default::default()
            },
        );
        assert!(adapter.needs_transform(&openai_responses_provider));

        let gemini_native_provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );
        assert!(adapter.needs_transform(&gemini_native_provider));
        assert_eq!(
            adapter.provider_type(&gemini_native_provider),
            ProviderType::Gemini
        );

        // meta takes precedence over legacy settings_config fields
        let meta_precedence_over_settings = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                },
                "api_format": "openai_chat",
                "openrouter_compat_mode": true
            }),
            ProviderMeta {
                api_format: Some("anthropic".to_string()),
                ..Default::default()
            },
        );
        assert!(!adapter.needs_transform(&meta_precedence_over_settings));

        // Unknown format in meta: default to anthropic (no transform)
        let unknown_format = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("unknown".to_string()),
                ..Default::default()
            },
        );
        assert!(!adapter.needs_transform(&unknown_format));
    }

    #[test]
    fn test_github_copilot_detection_by_url() {
        let adapter = ClaudeAdapter::new();

        // GitHub Copilot by base_url
        let copilot = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
            }
        }));
        assert_eq!(adapter.provider_type(&copilot), ProviderType::GitHubCopilot);
    }

    #[test]
    fn test_github_copilot_detection_by_meta() {
        let adapter = ClaudeAdapter::new();

        // GitHub Copilot by meta.provider_type
        let copilot_meta = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            ProviderMeta {
                provider_type: Some("github_copilot".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(
            adapter.provider_type(&copilot_meta),
            ProviderType::GitHubCopilot
        );
    }

    #[test]
    fn test_github_copilot_auth() {
        let adapter = ClaudeAdapter::new();

        let copilot = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
            }
        }));

        let auth = adapter.extract_auth(&copilot).unwrap();
        assert_eq!(auth.strategy, AuthStrategy::GitHubCopilot);
    }

    #[test]
    fn test_github_copilot_needs_transform() {
        let adapter = ClaudeAdapter::new();

        let copilot = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
            }
        }));

        // GitHub Copilot always needs transform
        assert!(adapter.needs_transform(&copilot));
    }

    #[test]
    fn test_transform_claude_request_for_api_format_responses() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
            }
        }));
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            None,
            None,
        )
        .unwrap();

        assert_eq!(transformed["model"], "gpt-5.4");
        assert!(transformed.get("input").is_some());
        assert!(transformed.get("max_output_tokens").is_some());
    }

    #[test]
    fn test_transform_claude_request_openai_chat_streaming_injects_include_usage() {
        let provider = create_provider(json!({
            "env": { "ANTHROPIC_BASE_URL": "https://openrouter.ai/api/v1" }
        }));
        // Streaming requests must get stream_options.include_usage, otherwise OpenAI-compatible upstreams
        // emit no usage at the end of the SSE, the converted Anthropic message_delta is all zeros, and the whole usage is lost.
        let body = json!({
            "model": "moonshotai/kimi-k2",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128,
            "stream": true
        });
        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();
        assert_eq!(transformed["stream"], true);
        assert_eq!(transformed["stream_options"]["include_usage"], true);
    }

    #[test]
    fn test_transform_claude_request_openai_chat_non_streaming_omits_stream_options() {
        let provider = create_provider(json!({
            "env": { "ANTHROPIC_BASE_URL": "https://openrouter.ai/api/v1" }
        }));
        // Non-streaming requests must not get stream_options (usage is always in a non-streaming body).
        let body = json!({
            "model": "moonshotai/kimi-k2",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });
        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();
        assert!(transformed.get("stream_options").is_none());
    }

    #[test]
    fn test_transform_claude_request_for_codex_oauth_uses_session_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                provider_type: Some("codex_oauth".to_string()),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            Some("session-123"),
            None,
        )
        .unwrap();

        assert_eq!(transformed["prompt_cache_key"], "session-123");
    }

    #[test]
    fn test_transform_claude_request_for_codex_oauth_without_session_omits_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                provider_type: Some("codex_oauth".to_string()),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            None,
            None,
        )
        .unwrap();

        assert!(transformed.get("prompt_cache_key").is_none());
    }

    #[test]
    fn test_transform_claude_request_for_responses_uses_session_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.openai.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            Some("claude-session-123"),
            None,
        )
        .unwrap();

        assert_eq!(transformed["prompt_cache_key"], "claude-session-123");
    }

    #[test]
    fn test_transform_claude_request_for_responses_without_session_omits_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.openai.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            None,
            None,
        )
        .unwrap();

        assert!(transformed.get("prompt_cache_key").is_none());
    }

    #[test]
    fn test_transform_claude_request_for_codex_oauth_keeps_explicit_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                provider_type: Some("codex_oauth".to_string()),
                prompt_cache_key: Some("explicit-cache-key".to_string()),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            Some("session-123"),
            None,
        )
        .unwrap();

        assert_eq!(transformed["prompt_cache_key"], "explicit-cache-key");
    }

    #[test]
    fn test_transform_claude_request_for_api_format_codex_oauth_fast_mode_off() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            ProviderMeta {
                provider_type: Some("codex_oauth".to_string()),
                codex_fast_mode: Some(false),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            None,
            None,
        )
        .unwrap();

        assert_eq!(transformed["store"], json!(false));
        assert!(transformed.get("service_tier").is_none());
        assert_eq!(
            transformed["include"],
            json!(["reasoning.encrypted_content"])
        );
    }

    #[test]
    fn test_transform_claude_request_for_api_format_gemini_native() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "gemini-2.5-pro",
            "system": "You are helpful.",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 64
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "gemini_native", None, None)
                .unwrap();

        assert!(transformed.get("contents").is_some());
        assert_eq!(
            transformed["systemInstruction"]["parts"][0]["text"],
            "You are helpful."
        );
        assert_eq!(transformed["generationConfig"]["maxOutputTokens"], 64);
    }

    #[test]
    fn test_transform_claude_request_for_api_format_openai_chat_skips_prompt_cache_key_by_default()
    {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 64
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        assert!(transformed.get("prompt_cache_key").is_none());
    }

    #[test]
    fn test_transform_claude_request_for_api_format_openai_chat_keeps_explicit_prompt_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                prompt_cache_key: Some("claude-cache-route".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 64
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        assert_eq!(transformed["prompt_cache_key"], "claude-cache-route");
    }

    #[test]
    fn test_transform_openai_chat_skips_reasoning_content_for_generic_provider() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "max_tokens": 64,
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "I should call the tool."},
                    {"type": "tool_use", "id": "call_123", "name": "get_weather", "input": {"location": "Tokyo"}}
                ]
            }]
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        let msg = &transformed["messages"][0];
        assert!(msg.get("tool_calls").is_some());
        assert!(msg.get("reasoning_content").is_none());
    }

    #[test]
    fn test_transform_openai_chat_skips_reasoning_content_for_kimi_provider() {
        // Kimi feedback 2026-08: reasoning_content replay is no longer needed and injecting it disturbs the chain of thought.
        // Kimi/Moonshot was removed from REASONING_VENDOR_HINTS and must behave like a generic provider.
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.moonshot.cn/v1",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "kimi-k2.6",
            "max_tokens": 64,
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "I should call the tool."},
                    {"type": "tool_use", "id": "call_123", "name": "get_weather", "input": {"location": "Tokyo"}}
                ]
            }]
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        let msg = &transformed["messages"][0];
        assert!(msg.get("tool_calls").is_some());
        assert!(msg.get("reasoning_content").is_none());
    }

    #[test]
    fn test_transform_openai_chat_preserves_reasoning_content_for_deepseek_provider() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.deepseek.com/v1",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "deepseek-v4-flash",
            "max_tokens": 64,
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "I should call the tool."},
                    {"type": "tool_use", "id": "call_123", "name": "get_weather", "input": {"location": "Tokyo"}}
                ]
            }]
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        let msg = &transformed["messages"][0];
        assert_eq!(msg["reasoning_content"], "I should call the tool.");
        assert!(msg.get("tool_calls").is_some());
    }

    #[test]
    fn test_transform_openai_chat_preserves_reasoning_content_for_mimo_provider() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.xiaomimimo.com/v1",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "mimo-v2.5-pro",
            "max_tokens": 64,
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "I should call the tool."},
                    {"type": "tool_use", "id": "call_123", "name": "get_weather", "input": {"location": "Tokyo"}}
                ]
            }]
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        let msg = &transformed["messages"][0];
        assert_eq!(msg["reasoning_content"], "I should call the tool.");
        assert!(msg.get("tool_calls").is_some());
    }

    #[test]
    fn test_deepseek_anthropic_tool_history_injects_missing_thinking() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "I will inspect the repo."},
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });

        let changed = normalize_anthropic_tool_thinking_history_for_provider(
            &mut body,
            &provider,
            "anthropic",
        );

        assert!(changed);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], ANTHROPIC_THINKING_PLACEHOLDER);
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[2]["type"], "tool_use");
    }

    #[test]
    fn test_anthropic_messages_no_longer_hoists_system_role_messages() {
        // After reverting #3775, role=system messages are left in `messages[]`
        // (DeepSeek's endpoint accepts them natively) and the top-level `system`
        // field is untouched, preserving the request prefix.
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "system": "Existing top-level system.",
            "model": "deepseek-v4-pro",
            "messages": [
                { "role": "system", "content": "Message system one." },
                { "role": "user", "content": "hello" },
                {
                    "role": "system",
                    "content": [{ "type": "text", "text": "Message system two." }]
                }
            ]
        });

        let changed = normalize_anthropic_messages_for_provider(&mut body, &provider, "anthropic");

        assert!(!changed);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "system");
        assert_eq!(body["system"], "Existing top-level system.");
    }

    #[test]
    fn test_anthropic_system_role_messages_skip_non_anthropic_format() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/v1",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [
                { "role": "system", "content": "Keep in messages." },
                { "role": "user", "content": "hello" }
            ]
        });

        let changed =
            normalize_anthropic_messages_for_provider(&mut body, &provider, "openai_chat");

        assert!(!changed);
        assert!(body.get("system").is_none());
        assert_eq!(body["messages"][0]["role"], "system");
    }

    #[test]
    fn test_kimi_anthropic_tool_history_not_modified() {
        // Kimi feedback 2026-08: the Anthropic-compatible endpoint no longer requires replaying thinking
        // on tool_use turns, and a placeholder disturbs the chain of thought. Kimi takes the generic pass-through with the body untouched.
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.kimi.com/coding",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "kimi-for-coding",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });
        let original = body.clone();

        let changed = normalize_anthropic_tool_thinking_history_for_provider(
            &mut body,
            &provider,
            "anthropic",
        );

        assert!(!changed);
        assert_eq!(body, original);
    }

    #[test]
    fn test_deepseek_anthropic_tool_history_rewrites_redacted_thinking() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "redacted_thinking", "data": "opaque"},
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });

        let changed = normalize_anthropic_tool_thinking_history_for_provider(
            &mut body,
            &provider,
            "anthropic",
        );

        assert!(changed);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(
            content[0]["thinking"],
            ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
        );
        assert!(content[0].get("data").is_none());
    }

    #[test]
    fn test_deepseek_anthropic_tool_history_keeps_thinking_text_but_drops_signature() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Need to inspect the file.", "signature": "anthropic-signature"},
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });

        let changed = normalize_anthropic_tool_thinking_history_for_provider(
            &mut body,
            &provider,
            "anthropic",
        );

        assert!(changed);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], "Need to inspect the file.");
        assert!(content[0].get("signature").is_none());
    }

    #[test]
    fn test_generic_anthropic_tool_history_is_not_modified() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "claude-sonnet-4.6",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });
        let original = body.clone();

        let changed = normalize_anthropic_tool_thinking_history_for_provider(
            &mut body,
            &provider,
            "anthropic",
        );

        assert!(!changed);
        assert_eq!(body, original);
    }

    // ==================== normalize_deepseek_thinking_disabled_strip_effort tests ====================

    fn deepseek_official_provider() -> Provider {
        create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }))
    }

    #[test]
    fn test_deepseek_official_strips_output_config_effort() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider(),
        );

        assert!(changed);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("output_config").is_none());
    }

    #[test]
    fn test_deepseek_official_strips_reasoning_effort() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "reasoning_effort": "high",
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider(),
        );

        assert!(changed);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_deepseek_official_strips_both_effort_fields() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "reasoning_effort": "high",
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider(),
        );

        assert!(changed);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("output_config").is_none());
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_deepseek_official_no_effort_no_change() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "max_tokens": 100000
        });
        let original = body.clone();

        let changed = normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider(),
        );

        assert!(!changed);
        assert_eq!(body, original);
    }

    #[test]
    fn test_deepseek_official_preserves_output_config_other_fields() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max", "temperature": 0.5 },
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider(),
        );

        assert!(changed);
        assert_eq!(body["output_config"]["temperature"], 0.5);
        assert!(body["output_config"].get("effort").is_none());
    }

    #[test]
    fn test_deepseek_official_non_disabled_not_modified() {
        let cases = vec![
            (
                "enabled",
                json!({ "type": "enabled", "budget_tokens": 16000 }),
            ),
            ("adaptive", json!({ "type": "adaptive" })),
        ];

        for (label, thinking_value) in cases {
            let mut body = json!({
                "model": "deepseek-v4-pro",
                "thinking": thinking_value,
                "output_config": { "effort": "max" },
                "max_tokens": 100000
            });
            let original = body.clone();

            let changed = normalize_deepseek_thinking_disabled_strip_effort(
                &mut body,
                &deepseek_official_provider(),
            );

            assert!(!changed, "should not modify thinking.type={label}");
            assert_eq!(body, original);
        }

        // missing thinking field entirely
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "output_config": { "effort": "max" },
            "max_tokens": 100000
        });
        let original = body.clone();
        assert!(!normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider()
        ));
        assert_eq!(body, original);
    }

    #[test]
    fn test_deepseek_official_url_with_trailing_slash() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic/",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(&mut body, &provider);

        assert!(changed);
        assert!(body.get("output_config").is_none());
    }

    #[test]
    fn test_deepseek_official_detected_via_base_url_fallback() {
        let provider = create_provider(json!({
            "base_url": "https://api.deepseek.com/anthropic",
            "ANTHROPIC_API_KEY": "test-key"
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "reasoning_effort": "high",
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(&mut body, &provider);

        assert!(changed);
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_non_deepseek_endpoint_not_modified() {
        let providers = vec![
            create_provider(json!({
                "env": { "ANTHROPIC_BASE_URL": "https://other-api.com/anthropic", "ANTHROPIC_API_KEY": "test-key" }
            })),
            create_provider(json!({
                "env": { "ANTHROPIC_BASE_URL": "https://api.anthropic.com", "ANTHROPIC_API_KEY": "test-key" }
            })),
        ];

        for provider in providers {
            let mut body = json!({
                "model": "deepseek-v4-pro",
                "thinking": { "type": "disabled" },
                "output_config": { "effort": "max" },
                "max_tokens": 100000
            });
            let original = body.clone();

            let changed = normalize_deepseek_thinking_disabled_strip_effort(&mut body, &provider);

            assert!(
                !changed,
                "should not modify for {}",
                provider.settings_config["env"]["ANTHROPIC_BASE_URL"]
            );
            assert_eq!(body, original);
        }
    }

    #[test]
    fn test_normalize_messages_pipeline_strips_effort_for_deepseek() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "max_tokens": 100000,
            "messages": [{ "role": "user", "content": "hello" }]
        });

        let changed = normalize_anthropic_messages_for_provider(
            &mut body,
            &deepseek_official_provider(),
            "anthropic",
        );

        assert!(changed);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("output_config").is_none());
    }
}
