//! Provider Adapters Module
//!
//! Provider adapters module, offering a unified interface that abstracts the
//! handling logic of different upstream providers.
//!
//! ## Module structure
//! - `adapter`: defines the `ProviderAdapter` trait
//! - `auth`: auth types and strategies
//! - `claude`: Claude (Anthropic) adapter
//! - `codex`: Codex (OpenAI) adapter
//! - `gemini`: Gemini (Google) adapter
//! - `models`: API data models
//! - `transform`: format transformation

mod adapter;
mod auth;
mod claude;
mod codex;
pub(crate) mod codex_chat_common;
pub mod codex_chat_history;
pub mod codex_oauth_auth;
pub(crate) mod codex_responses_sse;
pub mod copilot_auth;
pub mod copilot_model_map;
mod gemini;
pub(crate) mod gemini_schema;
pub mod gemini_shadow;
pub mod models;
pub(crate) mod reasoning_bridge;
pub mod streaming;
pub mod streaming_codex_anthropic;
pub mod streaming_codex_chat;
pub mod streaming_gemini;
pub mod streaming_responses;
pub mod transform;
pub mod transform_codex_anthropic;
pub mod transform_codex_chat;
pub mod transform_codex_chat_moonshot_schema;
pub mod transform_codex_responses_namespace;
pub mod transform_codex_responses_xai_sanitize;
pub mod transform_gemini;
pub mod transform_responses;
pub mod xai_oauth_auth;

use crate::app_config::AppType;
use crate::provider::Provider;
use serde::{Deserialize, Serialize};

pub const CHATGPT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const XAI_API_BASE_URL: &str = "https://api.x.ai/v1";

// Public exports
pub use adapter::ProviderAdapter;
pub use auth::{AuthInfo, AuthStrategy};
pub use claude::{
    claude_api_format_needs_transform, get_claude_api_format,
    normalize_anthropic_messages_for_provider, transform_claude_request_for_api_format,
    ClaudeAdapter,
};
pub use codex::CodexAdapter;
pub use codex::{
    apply_codex_chat_upstream_model, apply_codex_upstream_model, codex_provider_upstream_model,
    inject_codex_chat_prompt_cache_key, is_codex_official_provider,
    provider_needs_responses_namespace_flatten, resolve_codex_catalog_tool_profile,
    resolve_codex_chat_reasoning_config, should_convert_codex_responses_to_anthropic,
    should_convert_codex_responses_to_chat,
};
pub use gemini::GeminiAdapter;

/// Provider type enum
///
/// Distinguishes the concrete implementation of each provider, determining the
/// auth and request-handling logic. Finer-grained than AppType, supporting
/// multiple variants under the same AppType.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    /// Official Anthropic API (x-api-key + anthropic-version)
    Claude,
    /// Claude relay service (Bearer auth only, no x-api-key)
    ClaudeAuth,
    /// OpenAI Codex Response API
    Codex,
    /// Google Gemini API (x-goog-api-key)
    Gemini,
    /// Google Gemini CLI (OAuth Bearer)
    GeminiCli,
    /// OpenRouter (now supports the Claude Code-compatible endpoint, passthrough by default; legacy transform logic kept as a fallback)
    OpenRouter,
    /// GitHub Copilot (OAuth + Copilot Token, needs Anthropic <-> OpenAI transform)
    GitHubCopilot,
    /// OpenAI Codex (ChatGPT Plus/Pro OAuth, needs Anthropic <-> Responses API transform)
    CodexOAuth,
    /// xAI Grok OAuth (needs Anthropic <-> Responses API transform)
    XaiOAuth,
}

impl ProviderType {
    /// Infers the provider type from AppType and Provider config
    ///
    /// Infers the concrete provider type from the config's base_url, auth_mode,
    /// api_key format, and similar signals
    pub fn from_app_type_and_config(app_type: &AppType, provider: &Provider) -> Option<Self> {
        let provider_type = match app_type {
            AppType::Claude | AppType::ClaudeDesktop => {
                if get_claude_api_format(provider) == "gemini_native" {
                    let adapter = ClaudeAdapter::new();
                    return Some(
                        match adapter.extract_auth(provider).map(|auth| auth.strategy) {
                            Some(AuthStrategy::GoogleOAuth) => ProviderType::GeminiCli,
                            _ => ProviderType::Gemini,
                        },
                    );
                }

                // Check whether this is GitHub Copilot
                if let Some(meta) = provider.meta.as_ref() {
                    if meta.provider_type.as_deref() == Some("github_copilot") {
                        return Some(ProviderType::GitHubCopilot);
                    }
                    if meta.provider_type.as_deref() == Some("codex_oauth") {
                        return Some(ProviderType::CodexOAuth);
                    }
                    if meta.provider_type.as_deref() == Some("xai_oauth") {
                        return Some(ProviderType::XaiOAuth);
                    }
                }

                // Check whether base_url is GitHub Copilot
                let adapter = ClaudeAdapter::new();
                if let Ok(base_url) = adapter.extract_base_url(provider) {
                    if base_url.contains("githubcopilot.com") {
                        return Some(ProviderType::GitHubCopilot);
                    }
                    // Check whether it's OpenRouter
                    if base_url.contains("openrouter.ai") {
                        return Some(ProviderType::OpenRouter);
                    }
                }
                // Check whether this is a relay service (Bearer auth only)
                // Note: ProviderMeta has no direct auth_mode field,
                // so we infer it from the config inside settings_config
                // Check auth_mode inside settings_config
                if let Some(auth_mode) = provider
                    .settings_config
                    .get("auth_mode")
                    .and_then(|v| v.as_str())
                {
                    if auth_mode == "bearer_only" {
                        return Some(ProviderType::ClaudeAuth);
                    }
                }
                // Check auth_mode inside env
                if let Some(env) = provider.settings_config.get("env") {
                    if let Some(auth_mode) = env.get("AUTH_MODE").and_then(|v| v.as_str()) {
                        if auth_mode == "bearer_only" {
                            return Some(ProviderType::ClaudeAuth);
                        }
                    }
                }
                ProviderType::Claude
            }
            AppType::Codex => ProviderType::Codex,
            AppType::Gemini => {
                // Check whether this is CLI mode (OAuth)
                let adapter = GeminiAdapter::new();
                if let Some(auth) = adapter.extract_auth(provider) {
                    let key = &auth.api_key;
                    // OAuth access_token starts with ya29.
                    if key.starts_with("ya29.") {
                        return Some(ProviderType::GeminiCli);
                    }
                    // JSON-formatted OAuth credentials
                    if key.starts_with('{') {
                        return Some(ProviderType::GeminiCli);
                    }
                }
                ProviderType::Gemini
            }
            AppType::GrokBuild => ProviderType::Codex,
            AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => ProviderType::Codex,
            AppType::Pi => return None,
        };
        Some(provider_type)
    }

    /// Converts to a string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::Claude => "claude",
            ProviderType::ClaudeAuth => "claude_auth",
            ProviderType::Codex => "codex",
            ProviderType::Gemini => "gemini",
            ProviderType::GeminiCli => "gemini_cli",
            ProviderType::OpenRouter => "openrouter",
            ProviderType::GitHubCopilot => "github_copilot",
            ProviderType::CodexOAuth => "codex_oauth",
            ProviderType::XaiOAuth => "xai_oauth",
        }
    }
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "claude" => Ok(ProviderType::Claude),
            "claude_auth" | "claude-auth" => Ok(ProviderType::ClaudeAuth),
            "codex" => Ok(ProviderType::Codex),
            "gemini" => Ok(ProviderType::Gemini),
            "gemini_cli" | "gemini-cli" => Ok(ProviderType::GeminiCli),
            "openrouter" => Ok(ProviderType::OpenRouter),
            "github_copilot" | "github-copilot" | "githubcopilot" => {
                Ok(ProviderType::GitHubCopilot)
            }
            "codex_oauth" | "codex-oauth" | "codexoauth" => Ok(ProviderType::CodexOAuth),
            "xai_oauth" | "xai-oauth" | "xaioauth" => Ok(ProviderType::XaiOAuth),
            _ => Err(format!("Invalid provider type: {s}")),
        }
    }
}

/// Gets the corresponding adapter for an AppType
pub fn get_adapter(app_type: &AppType) -> Option<Box<dyn ProviderAdapter>> {
    Some(match app_type {
        AppType::Claude | AppType::ClaudeDesktop => Box::new(ClaudeAdapter::new()),
        AppType::Codex => Box::new(CodexAdapter::new()),
        AppType::Gemini => Box::new(GeminiAdapter::new()),
        AppType::GrokBuild => Box::new(CodexAdapter::new()),
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => Box::new(CodexAdapter::new()),
        AppType::Pi => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Provider".to_string(),
            settings_config: config,
            website_url: None,
            category: None,
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
    fn test_provider_type_from_str() {
        assert_eq!(
            "claude".parse::<ProviderType>().unwrap(),
            ProviderType::Claude
        );
        assert_eq!(
            "claude_auth".parse::<ProviderType>().unwrap(),
            ProviderType::ClaudeAuth
        );
        assert_eq!(
            "claude-auth".parse::<ProviderType>().unwrap(),
            ProviderType::ClaudeAuth
        );
        assert_eq!(
            "codex".parse::<ProviderType>().unwrap(),
            ProviderType::Codex
        );
        assert_eq!(
            "gemini".parse::<ProviderType>().unwrap(),
            ProviderType::Gemini
        );
        assert_eq!(
            "gemini_cli".parse::<ProviderType>().unwrap(),
            ProviderType::GeminiCli
        );
        assert_eq!(
            "gemini-cli".parse::<ProviderType>().unwrap(),
            ProviderType::GeminiCli
        );
        assert_eq!(
            "openrouter".parse::<ProviderType>().unwrap(),
            ProviderType::OpenRouter
        );
        assert_eq!(
            "github_copilot".parse::<ProviderType>().unwrap(),
            ProviderType::GitHubCopilot
        );
        assert_eq!(
            "github-copilot".parse::<ProviderType>().unwrap(),
            ProviderType::GitHubCopilot
        );
        assert_eq!(
            "githubcopilot".parse::<ProviderType>().unwrap(),
            ProviderType::GitHubCopilot
        );
        assert_eq!(
            "xai_oauth".parse::<ProviderType>().unwrap(),
            ProviderType::XaiOAuth
        );
        assert!("invalid".parse::<ProviderType>().is_err());
    }

    #[test]
    fn test_provider_type_as_str() {
        assert_eq!(ProviderType::Claude.as_str(), "claude");
        assert_eq!(ProviderType::ClaudeAuth.as_str(), "claude_auth");
        assert_eq!(ProviderType::Codex.as_str(), "codex");
        assert_eq!(ProviderType::Gemini.as_str(), "gemini");
        assert_eq!(ProviderType::GeminiCli.as_str(), "gemini_cli");
        assert_eq!(ProviderType::OpenRouter.as_str(), "openrouter");
        assert_eq!(ProviderType::GitHubCopilot.as_str(), "github_copilot");
        assert_eq!(ProviderType::XaiOAuth.as_str(), "xai_oauth");
    }

    #[test]
    fn test_provider_type_serde() {
        // Test serialization
        let claude = ProviderType::Claude;
        let serialized = serde_json::to_string(&claude).unwrap();
        assert_eq!(serialized, "\"claude\"");

        let claude_auth = ProviderType::ClaudeAuth;
        let serialized = serde_json::to_string(&claude_auth).unwrap();
        assert_eq!(serialized, "\"claude_auth\"");

        // Test deserialization
        let deserialized: ProviderType = serde_json::from_str("\"claude\"").unwrap();
        assert_eq!(deserialized, ProviderType::Claude);

        let deserialized: ProviderType = serde_json::from_str("\"gemini_cli\"").unwrap();
        assert_eq!(deserialized, ProviderType::GeminiCli);
    }

    #[test]
    fn test_from_app_type_claude_direct() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test"
            }
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Claude, &provider);
        assert_eq!(provider_type, Some(ProviderType::Claude));
    }

    #[test]
    fn test_from_app_type_claude_openrouter() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "OPENROUTER_API_KEY": "sk-or-test"
            }
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Claude, &provider);
        assert_eq!(provider_type, Some(ProviderType::OpenRouter));
    }

    #[test]
    fn test_from_app_type_claude_auth() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-test"
            },
            "auth_mode": "bearer_only"
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Claude, &provider);
        assert_eq!(provider_type, Some(ProviderType::ClaudeAuth));
    }

    #[test]
    fn test_from_app_type_codex() {
        let provider = create_provider(json!({
            "env": {
                "OPENAI_API_KEY": "sk-test"
            }
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Codex, &provider);
        assert_eq!(provider_type, Some(ProviderType::Codex));
    }

    #[test]
    fn test_from_app_type_gemini_api_key() {
        let provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "AIza-test-key"
            }
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Gemini, &provider);
        assert_eq!(provider_type, Some(ProviderType::Gemini));
    }

    #[test]
    fn test_from_app_type_gemini_cli_oauth() {
        let provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "ya29.test-access-token"
            }
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Gemini, &provider);
        assert_eq!(provider_type, Some(ProviderType::GeminiCli));
    }

    #[test]
    fn test_from_app_type_gemini_cli_json() {
        let provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "{\"access_token\":\"ya29.test\",\"refresh_token\":\"1//test\"}"
            }
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Gemini, &provider);
        assert_eq!(provider_type, Some(ProviderType::GeminiCli));
    }
}
