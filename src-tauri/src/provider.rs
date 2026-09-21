use http::header::{HeaderValue, InvalidHeaderValue};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// SSOT mode: no longer writes provider copy files

/// Provider struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    #[serde(rename = "settingsConfig")]
    pub settings_config: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "websiteUrl")]
    pub website_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createdAt")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sortIndex")]
    pub sort_index: Option<usize>,
    /// Notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Provider metadata (not written to the live config; stored only in the product database / internal config)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ProviderMeta>,
    /// Icon name (e.g. "openai", "anthropic")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Icon color (hex format, e.g. "#00A67E")
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "iconColor")]
    pub icon_color: Option<String>,
    /// Whether to join the failover queue
    #[serde(default)]
    #[serde(rename = "inFailoverQueue")]
    pub in_failover_queue: bool,
}

impl Provider {
    /// Create a provider from an existing ID
    pub fn with_id(
        id: String,
        name: String,
        settings_config: Value,
        website_url: Option<String>,
    ) -> Self {
        Self {
            id,
            name,
            settings_config,
            website_url,
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

    pub fn is_codex_oauth(&self) -> bool {
        self.provider_type() == Some("codex_oauth")
    }

    pub fn is_xai_oauth(&self) -> bool {
        self.provider_type() == Some("xai_oauth")
    }

    pub fn is_github_copilot(&self) -> bool {
        self.provider_type() == Some("github_copilot")
            || self.claude_base_url_contains("githubcopilot.com")
    }

    pub fn uses_managed_account_auth(&self) -> bool {
        self.is_github_copilot()
            || self.is_codex_oauth()
            || self.is_xai_oauth()
            || self.claude_base_url_contains("chatgpt.com/backend-api/codex")
    }

    /// Third-party managed OAuth (xai_oauth, github_copilot, …): the real
    /// credential is injected per-request by the local proxy, so the card is
    /// keyless by design and its stored config is only an upstream snapshot.
    /// `codex_oauth` is deliberately excluded — the official ChatGPT login
    /// in auth.json IS its credential, so the `requires_openai_auth = true`
    /// fallback is its correct shape, never a legacy leftover.
    pub fn uses_proxy_injected_oauth(&self) -> bool {
        self.is_xai_oauth() || self.is_github_copilot()
    }

    /// Whether the provider form's "auth field" was explicitly set to
    /// ANTHROPIC_API_KEY. The form only persists `meta.apiKeyField` for the
    /// non-default choice, so `None` means the default ANTHROPIC_AUTH_TOKEN.
    pub fn claude_uses_api_key_field(&self) -> bool {
        self.meta
            .as_ref()
            .and_then(|m| m.api_key_field.as_deref())
            .map(|field| field.eq_ignore_ascii_case("ANTHROPIC_API_KEY"))
            .unwrap_or(false)
    }

    fn provider_type(&self) -> Option<&str> {
        self.meta.as_ref().and_then(|m| m.provider_type.as_deref())
    }

    fn claude_base_url_contains(&self, needle: &str) -> bool {
        self.settings_config
            .pointer("/env/ANTHROPIC_BASE_URL")
            .and_then(|value| value.as_str())
            .map(|base_url| base_url.contains(needle))
            .unwrap_or(false)
    }

    pub fn codex_fast_mode_enabled(&self) -> bool {
        self.meta
            .as_ref()
            .map(|m| m.codex_fast_mode_enabled())
            .unwrap_or(false)
    }

    pub fn has_usage_script_enabled(&self) -> bool {
        self.meta
            .as_ref()
            .and_then(|m| m.usage_script.as_ref())
            .map(|s| s.enabled)
            .unwrap_or(false)
    }

    /// Resolve `(base_url, api_key)` for usage queries (native balance /
    /// coding-plan and the JS-script `{{apiKey}}`/`{{baseUrl}}` fallback)
    /// from the stored provider config.
    ///
    /// Each app persists credentials in a different shape, so callers must pass
    /// the owning app type. This mirrors the frontend `getProviderCredentials`
    /// in `UsageScriptModal.tsx`.
    pub fn resolve_usage_credentials(
        &self,
        app_type: &crate::app_config::AppType,
    ) -> (String, String) {
        use crate::app_config::AppType;

        let settings = &self.settings_config;
        let str_at =
            |value: Option<&Value>| value.and_then(|v| v.as_str()).unwrap_or("").to_string();

        // First present, non-empty string among `keys`, mirroring the frontend's
        // `a || b || c` — JS `||` skips empty strings, and presets seed fields like
        // `ANTHROPIC_AUTH_TOKEN` as present-but-empty placeholders, so a plain
        // `.get().or_else()` chain (which only skips *absent* keys) would stop short.
        fn first_non_empty(env: Option<&Value>, keys: &[&str]) -> String {
            let Some(env) = env else {
                return String::new();
            };
            for key in keys {
                if let Some(s) = env.get(key).and_then(|v| v.as_str()) {
                    if !s.is_empty() {
                        return s.to_string();
                    }
                }
            }
            String::new()
        }

        let (base_url, api_key) = match app_type {
            // Codex keeps its key in `auth.OPENAI_API_KEY` and its base URL
            // inside a TOML `config` string, not in an `env` map.
            AppType::Codex => {
                let auth = settings.get("auth");
                let config_text = settings.get("config").and_then(|v| v.as_str());
                let api_key = crate::codex_config::extract_codex_api_key(auth, config_text)
                    .unwrap_or_default();
                let base_url = config_text
                    .and_then(crate::codex_config::extract_codex_base_url)
                    .unwrap_or_default();
                (base_url, api_key)
            }
            // Gemini uses Google-specific env keys (with a legacy GOOGLE_API_KEY fallback).
            AppType::Gemini => {
                let env = settings.get("env");
                let base_url = str_at(env.and_then(|e| e.get("GOOGLE_GEMINI_BASE_URL")));
                let api_key = first_non_empty(env, &["GEMINI_API_KEY", "GOOGLE_API_KEY"]);
                (base_url, api_key)
            }
            // GrokBuild's base_url and api_key must be resolved independently:
            // extract_credentials returns a single Option that becomes None
            // whenever credentials are missing, so a shared unwrap_or_default
            // would also blank out a base_url that IS present in the config.
            // Missing credentials are the common case (the env_key-referenced
            // variable isn't readable from the GUI process), and the endpoint
            // shouldn't be penalized for it — otherwise the usage script's
            // {{baseUrl}} becomes a relative path and the balance query just
            // reports "API key is empty", masking the real cause.
            // Kept consistent with the Codex branch above.
            AppType::GrokBuild => {
                let config_text = settings.get("config").and_then(Value::as_str);
                let base_url = config_text
                    .and_then(crate::grok_config::extract_base_url)
                    .unwrap_or_default();
                let api_key = config_text
                    .and_then(crate::grok_config::extract_credentials)
                    .map(|(_, api_key)| api_key)
                    .unwrap_or_default();
                (base_url, api_key)
            }
            // Hermes (config.yaml) flattens credentials at the top level, snake_case.
            AppType::Hermes => (
                str_at(settings.get("base_url")),
                str_at(settings.get("api_key")),
            ),
            // OpenClaw (openclaw.json) flattens credentials at the top level, camelCase.
            AppType::OpenClaw => (
                str_at(settings.get("baseUrl")),
                str_at(settings.get("apiKey")),
            ),
            // Pi custom providers use the native models.json field names.
            AppType::Pi => (
                crate::pi_config::provider_base_url(settings).unwrap_or_default(),
                str_at(settings.get("apiKey")),
            ),
            // OpenCode (OMO) nests credentials under `options` (the SDK options object).
            AppType::OpenCode => {
                let options = settings.get("options");
                (
                    str_at(options.and_then(|o| o.get("baseURL"))),
                    str_at(options.and_then(|o| o.get("apiKey"))),
                )
            }
            // Claude and Claude Desktop both use the Anthropic-style env map, keeping
            // the OpenRouter/Google key fallbacks the JS-script path relies on.
            // Listed explicitly (not `_`) so a new AppType fails to compile here.
            AppType::Claude | AppType::ClaudeDesktop => {
                let env = settings.get("env");
                let base_url = str_at(env.and_then(|e| e.get("ANTHROPIC_BASE_URL")));
                let api_key = first_non_empty(
                    env,
                    &[
                        "ANTHROPIC_AUTH_TOKEN",
                        "ANTHROPIC_API_KEY",
                        "OPENROUTER_API_KEY",
                        "GOOGLE_API_KEY",
                    ],
                );
                (base_url, api_key)
            }
        };

        // Normalize like the JS-script path (extract_base_url_from_provider) so a
        // future delegation from services/provider/usage.rs is behavior-preserving
        // and `{{baseUrl}}/path` concatenation never produces a double slash.
        (base_url.trim_end_matches('/').to_string(), api_key)
    }
}

/// Provider manager
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderManager {
    pub providers: IndexMap<String, Provider>,
    pub current: String,
}

/// Usage-query script configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageScript {
    pub enabled: bool,
    pub language: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// API key dedicated to usage queries (used by the generic template)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    /// Base URL dedicated to usage queries (used by the generic and NewAPI templates)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "baseUrl")]
    pub base_url: Option<String>,
    /// Access token (for endpoints that require login; used by the NewAPI template)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "accessToken")]
    pub access_token: Option<String>,
    /// User ID (for endpoints that require user identification; used by the NewAPI template)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
    /// Template type (used by the backend to decide validation rules)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "templateType")]
    pub template_type: Option<String>,
    /// Auto-query interval (in minutes; 0 disables auto-query)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "autoQueryInterval")]
    pub auto_query_interval: Option<u64>,
    /// Coding Plan provider identifier (e.g. "kimi", "zhipu", "minimax")
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "codingPlanProvider")]
    pub coding_plan_provider: Option<String>,
    /// Volcengine Ark control-plane OpenAPI AccessKey ID (used to sign usage queries; a separate credential from the inference key)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "accessKeyId")]
    pub access_key_id: Option<String>,
    /// Volcengine Ark control-plane OpenAPI SecretAccessKey (same as above)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "secretAccessKey")]
    pub secret_access_key: Option<String>,
    /// Zhipu Team Plan organization ID (usage-query request header bigmodel-organization)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "teamOrganizationId")]
    pub team_organization_id: Option<String>,
    /// Zhipu Team Plan project ID (usage-query request header bigmodel-project)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "teamProjectId")]
    pub team_project_id: Option<String>,
}

/// Usage data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageData {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "planName")]
    pub plan_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "isValid")]
    pub is_valid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "invalidMessage")]
    pub invalid_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Usage query result (supports multiple plans)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<UsageData>>, // supports returning multiple plans
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Auth binding source
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthBindingSource {
    /// Read auth info from the provider's own config (default)
    #[default]
    ProviderConfig,
    /// Use managed-account auth (e.g. GitHub Copilot OAuth)
    ManagedAccount,
}

/// Generic auth binding
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthBinding {
    /// Auth source
    #[serde(default)]
    pub source: AuthBindingSource,
    /// Managed auth provider identifier (e.g. github_copilot)
    #[serde(rename = "authProvider", skip_serializing_if = "Option::is_none")]
    pub auth_provider: Option<String>,
    /// Managed account ID; empty means follow that auth provider's default account
    #[serde(rename = "accountId", skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

/// Claude Desktop 3P write mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClaudeDesktopMode {
    Direct,
    Proxy,
}

/// The safe model route exposed to Desktop under Claude Desktop's local routing mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopModelRoute {
    /// The real upstream model name, kept only inside CC Switch and never written to the Claude Desktop profile.
    pub model: String,
    /// Display name shown in the Claude Desktop model menu; written to the profile's `labelOverride`.
    #[serde(rename = "labelOverride", skip_serializing_if = "Option::is_none")]
    pub label_override: Option<String>,
    /// The 1M-context capability flag recognized by Claude Desktop 3P.
    #[serde(rename = "supports1m", skip_serializing_if = "Option::is_none")]
    pub supports_1m: Option<bool>,
}

/// Reasoning capability description for Codex Responses -> Chat Completions.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CodexChatReasoningConfig {
    #[serde(rename = "supportsThinking", skip_serializing_if = "Option::is_none")]
    pub supports_thinking: Option<bool>,
    #[serde(rename = "supportsEffort", skip_serializing_if = "Option::is_none")]
    pub supports_effort: Option<bool>,
    #[serde(rename = "thinkingParam", skip_serializing_if = "Option::is_none")]
    pub thinking_param: Option<String>,
    #[serde(rename = "effortParam", skip_serializing_if = "Option::is_none")]
    pub effort_param: Option<String>,
    #[serde(rename = "effortValueMode", skip_serializing_if = "Option::is_none")]
    pub effort_value_mode: Option<String>,
    /// Declarative field: notes where the upstream reasoning is returned
    /// (reasoning_content / reasoning / reasoning_details / think_tags). The
    /// response-side `extract_reasoning_field_text` currently extracts by
    /// exhaustively checking fields and does not read this field; kept for
    /// documentation and as a hook for future format-based dispatch (e.g. think_tags).
    #[serde(rename = "outputFormat", skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    /// Runtime field (not persisted, not part of meta): the valid effort
    /// levels the platform declares for the currently requested model,
    /// filled in by `resolve` from the provider's
    /// `settings_config.modelCatalog.reasoningLevels` (declared per model,
    /// see #6228). Only consumed by the "zen" value mapping: Some -> clamp to
    /// a valid level; None -> omit the effort field (model not listed, or a toggle-type model).
    #[serde(skip)]
    pub effort_levels: Option<Vec<String>>,
}

/// Local proxy request overrides applied after route/protocol transforms.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalProxyRequestOverrides {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

impl LocalProxyRequestOverrides {
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty() && self.body.is_none()
    }
}

/// Provider metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderMeta {
    /// Custom endpoint list (deduplicated by URL)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_endpoints: HashMap<String, crate::settings::CustomEndpoint>,
    /// Whether to apply the common config fragment when writing live
    #[serde(
        rename = "commonConfigEnabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub common_config_enabled: Option<bool>,
    /// Claude Desktop 3P write mode: direct or proxy (reserved)
    #[serde(rename = "claudeDesktopMode", skip_serializing_if = "Option::is_none")]
    pub claude_desktop_mode: Option<ClaudeDesktopMode>,
    /// Model route mapping for Claude Desktop proxy mode: Claude-safe route -> upstream model.
    #[serde(
        default,
        rename = "claudeDesktopModelRoutes",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub claude_desktop_model_routes: HashMap<String, ClaudeDesktopModelRoute>,
    /// Usage-query script configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_script: Option<UsageScript>,
    /// Endpoint management: auto-select the best endpoint after speed testing
    #[serde(rename = "endpointAutoSelect", skip_serializing_if = "Option::is_none")]
    pub endpoint_auto_select: Option<bool>,
    /// Partner flag (the frontend uses isPartner; field name kept consistent)
    #[serde(rename = "isPartner", skip_serializing_if = "Option::is_none")]
    pub is_partner: Option<bool>,
    /// Partner promotion key, used to identify special providers like PackyCode
    #[serde(
        rename = "partnerPromotionKey",
        skip_serializing_if = "Option::is_none"
    )]
    pub partner_promotion_key: Option<String>,
    /// Cost multiplier (used to compute the actual cost)
    #[serde(rename = "costMultiplier", skip_serializing_if = "Option::is_none")]
    pub cost_multiplier: Option<String>,
    /// Billing mode source (response/request)
    #[serde(rename = "pricingModelSource", skip_serializing_if = "Option::is_none")]
    pub pricing_model_source: Option<String>,
    /// Daily spend limit (USD)
    #[serde(rename = "limitDailyUsd", skip_serializing_if = "Option::is_none")]
    pub limit_daily_usd: Option<String>,
    /// Monthly spend limit (USD)
    #[serde(rename = "limitMonthlyUsd", skip_serializing_if = "Option::is_none")]
    pub limit_monthly_usd: Option<String>,
    /// Claude API format (used by Claude providers only)
    /// - "anthropic": the native Anthropic Messages API, passed through directly
    /// - "openai_chat": the OpenAI Chat Completions format, needs conversion
    /// - "openai_responses": the OpenAI Responses API format, needs conversion
    #[serde(rename = "apiFormat", skip_serializing_if = "Option::is_none")]
    pub api_format: Option<String>,
    /// Generic auth binding (provider_config / managed_account)
    ///
    /// New code should write only this field; githubAccountId is kept for compatible reads.
    #[serde(rename = "authBinding", skip_serializing_if = "Option::is_none")]
    pub auth_binding: Option<AuthBinding>,
    /// Claude auth field name ("ANTHROPIC_AUTH_TOKEN" or "ANTHROPIC_API_KEY")
    #[serde(rename = "apiKeyField", skip_serializing_if = "Option::is_none")]
    pub api_key_field: Option<String>,
    /// Whether to treat base_url as the complete API endpoint (no endpoint path appended)
    #[serde(rename = "isFullUrl", skip_serializing_if = "Option::is_none")]
    pub is_full_url: Option<bool>,
    /// Prompt cache key for OpenAI Responses-compatible endpoints.
    /// When set, injected into converted Responses requests to improve cache hit rate.
    /// If not set, Claude -> Responses conversions use a client-provided session/thread
    /// identity when available; generated session IDs are not sent upstream.
    #[serde(rename = "promptCacheKey", skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    /// Session-based prompt-cache routing for Codex Responses -> Chat conversions.
    /// "auto" enables known-compatible upstreams; "enabled" / "disabled" are overrides.
    #[serde(rename = "promptCacheRouting", skip_serializing_if = "Option::is_none")]
    pub prompt_cache_routing: Option<String>,
    /// Codex OAuth FAST mode: inject `service_tier = "priority"` for ChatGPT Codex requests.
    #[serde(rename = "codexFastMode", skip_serializing_if = "Option::is_none")]
    pub codex_fast_mode: Option<bool>,
    /// Codex Responses -> Chat Completions reasoning capability metadata.
    #[serde(rename = "codexChatReasoning", skip_serializing_if = "Option::is_none")]
    pub codex_chat_reasoning: Option<CodexChatReasoningConfig>,
    /// Codex → Anthropic path: whether to emulate the Claude Code client
    /// (User-Agent / anthropic-beta / x-app + injecting the Claude Code system
    /// prompt first line). Disabled by default; only an explicit `true` enables it.
    #[serde(
        rename = "impersonateClaudeCode",
        skip_serializing_if = "Option::is_none"
    )]
    pub impersonate_claude_code: Option<bool>,
    /// Codex → Anthropic path: override the Anthropic `max_tokens` (output ceiling).
    ///
    /// Codex does not forward its `model_max_output_tokens` in the Responses
    /// request body, so without this the path falls back to a conservative
    /// default (8192), which truncates long or thinking-heavy responses
    /// (`stop_reason=max_tokens`). When set (>0), this value is injected as the
    /// request's `max_output_tokens` before conversion, taking precedence over
    /// both any request-supplied value and the default. Kept per-provider on
    /// purpose: a global large default would hard-400 on low-output-ceiling
    /// models/gateways (and that error is non-retryable).
    #[serde(rename = "maxOutputTokens", skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    /// Custom User-Agent for local proxy routing.
    #[serde(rename = "customUserAgent", skip_serializing_if = "Option::is_none")]
    pub custom_user_agent: Option<String>,
    /// Local proxy request overrides applied to the transformed upstream request.
    #[serde(
        rename = "localProxyRequestOverrides",
        skip_serializing_if = "Option::is_none"
    )]
    pub local_proxy_request_overrides: Option<LocalProxyRequestOverrides>,
    /// In additive-mode apps, whether this provider has been written into the live config.
    /// `None` means legacy data / unknown state, `Some(false)` means it explicitly exists in the database only.
    #[serde(rename = "liveConfigManaged", skip_serializing_if = "Option::is_none")]
    pub live_config_managed: Option<bool>,
    /// Provider type identifier (used for special-provider detection)
    /// - "github_copilot": GitHub Copilot provider
    #[serde(rename = "providerType", skip_serializing_if = "Option::is_none")]
    pub provider_type: Option<String>,
    /// GitHub Copilot linked account ID (only used by github_copilot providers)
    /// Supports multi-account by linking to a specific GitHub account
    #[serde(rename = "githubAccountId", skip_serializing_if = "Option::is_none")]
    pub github_account_id: Option<String>,
}

/// Parse a provider-level custom User-Agent string (the single source of truth).
///
/// Forwarding (forwarder), stream detection (stream_check) and model-list fetching
/// (model_fetch) all share this definition, so no path ends up "using the UA while another
/// does not, or errors out".
///
/// Validity is decided **byte-wise** by `http::HeaderValue::from_str`
/// (`b >= 32 && b != 127 || b == '\t'`), exactly matching the frontend
/// `src/lib/userAgent.ts::isValidUserAgentHeader`:
/// - `Ok(None)`: not set, or pure whitespace (empty after trim).
/// - `Ok(Some(hv))`: valid. Tabs, printable ASCII (0x20-0x7E) and any non-ASCII character
///   (all UTF-8 bytes >= 0x80) are valid.
/// - `Err(_)`: only when it contains control characters — 0x00-0x1F except `\t` (including
///   newline) and 0x7F (DEL).
///
/// Handling of invalid values: all three runtime paths **silently ignore** them
/// (`.ok().flatten()`, so one path can never error while another lets it through); the
/// frontend shows a non-blocking hint next to the input. Saving is currently **not** blocked
/// — non-form paths such as deeplink imports should be lenient, and silently ignoring the
/// value at runtime is the safety net.
pub fn parse_custom_user_agent(
    raw: Option<&str>,
) -> Result<Option<HeaderValue>, InvalidHeaderValue> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        Some(ua) => HeaderValue::from_str(ua).map(Some),
        None => Ok(None),
    }
}

impl ProviderMeta {
    /// Whether Codex OAuth FAST mode is enabled. Off by default, because
    /// `service_tier="priority"` burns the ChatGPT subscription quota at a higher rate, so
    /// the user must opt in to trade quota for lower latency.
    pub fn codex_fast_mode_enabled(&self) -> bool {
        self.codex_fast_mode.unwrap_or(false)
    }

    /// The validated provider-level custom User-Agent. See [`parse_custom_user_agent`].
    pub fn custom_user_agent_header(&self) -> Result<Option<HeaderValue>, InvalidHeaderValue> {
        parse_custom_user_agent(self.custom_user_agent.as_deref())
    }

    /// Resolve the account ID bound to the given managed auth provider.
    ///
    /// Newer data reads authBinding first; older data still falls back to githubAccountId.
    pub fn managed_account_id_for(&self, auth_provider: &str) -> Option<String> {
        if let Some(binding) = self.auth_binding.as_ref() {
            if binding.source == AuthBindingSource::ManagedAccount
                && binding.auth_provider.as_deref() == Some(auth_provider)
            {
                return binding.account_id.clone();
            }
        }

        if auth_provider == "github_copilot" {
            return self.github_account_id.clone();
        }

        None
    }
}

impl ProviderManager {
    /// Get all providers
    pub fn get_all_providers(&self) -> &IndexMap<String, Provider> {
        &self.providers
    }
}

// ============================================================================
// Universal provider - configuration shared across apps
// ============================================================================

/// Per-app enabled state for a universal provider
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UniversalProviderApps {
    #[serde(default)]
    pub claude: bool,
    #[serde(default)]
    pub codex: bool,
    #[serde(default)]
    pub gemini: bool,
}

/// Claude model config
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeModelConfig {
    /// Primary model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Default Haiku model
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "haikuModel")]
    pub haiku_model: Option<String>,
    /// Default Sonnet model
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sonnetModel")]
    pub sonnet_model: Option<String>,
    /// Default Opus model
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "opusModel")]
    pub opus_model: Option<String>,
}

/// Codex model config
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodexModelConfig {
    /// Model name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Reasoning effort
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "reasoningEffort")]
    pub reasoning_effort: Option<String>,
}

/// Gemini model config
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeminiModelConfig {
    /// Model name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Per-app model configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UniversalProviderModels {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude: Option<ClaudeModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex: Option<CodexModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gemini: Option<GeminiModelConfig>,
}

/// Universal provider (config shared across apps)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalProvider {
    /// Unique identifier
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type (e.g. "newapi", "custom")
    #[serde(rename = "providerType")]
    pub provider_type: String,
    /// Per-app enabled state
    pub apps: UniversalProviderApps,
    /// API base URL
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    /// API key
    #[serde(rename = "apiKey")]
    pub api_key: String,
    /// Per-app model configuration
    #[serde(default)]
    pub models: UniversalProviderModels,
    /// Website link
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "websiteUrl")]
    pub website_url: Option<String>,
    /// Notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Icon name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Icon color
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "iconColor")]
    pub icon_color: Option<String>,
    /// Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ProviderMeta>,
    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createdAt")]
    pub created_at: Option<i64>,
    /// Sort index
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sortIndex")]
    pub sort_index: Option<usize>,
}

impl UniversalProvider {
    /// Create a new universal provider
    pub fn new(
        id: String,
        name: String,
        provider_type: String,
        base_url: String,
        api_key: String,
    ) -> Self {
        Self {
            id,
            name,
            provider_type,
            apps: UniversalProviderApps::default(),
            base_url,
            api_key,
            models: UniversalProviderModels::default(),
            website_url: None,
            notes: None,
            icon: None,
            icon_color: None,
            meta: None,
            created_at: Some(chrono::Utc::now().timestamp_millis()),
            sort_index: None,
        }
    }

    /// Generate a Claude provider config
    pub fn to_claude_provider(&self) -> Option<Provider> {
        if !self.apps.claude {
            return None;
        }

        let models = self.models.claude.as_ref();
        let model = models
            .and_then(|m| m.model.clone())
            .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());
        let haiku = models
            .and_then(|m| m.haiku_model.clone())
            .unwrap_or_else(|| model.clone());
        let sonnet = models
            .and_then(|m| m.sonnet_model.clone())
            .unwrap_or_else(|| model.clone());
        let opus = models
            .and_then(|m| m.opus_model.clone())
            .unwrap_or_else(|| model.clone());

        let settings_config = serde_json::json!({
            "env": {
                "ANTHROPIC_BASE_URL": self.base_url,
                "ANTHROPIC_AUTH_TOKEN": self.api_key,
                "ANTHROPIC_MODEL": model,
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": haiku,
                "ANTHROPIC_DEFAULT_SONNET_MODEL": sonnet,
                "ANTHROPIC_DEFAULT_OPUS_MODEL": opus,
            }
        });

        Some(Provider {
            id: format!("universal-claude-{}", self.id),
            name: self.name.clone(),
            settings_config,
            website_url: self.website_url.clone(),
            category: Some("aggregator".to_string()),
            created_at: self.created_at,
            sort_index: self.sort_index,
            notes: self.notes.clone(),
            meta: self.meta.clone(),
            icon: self.icon.clone(),
            icon_color: self.icon_color.clone(),
            in_failover_queue: false,
        })
    }

    /// Generate a Codex provider config
    pub fn to_codex_provider(&self) -> Option<Provider> {
        if !self.apps.codex {
            return None;
        }

        let models = self.models.codex.as_ref();
        let model = models
            .and_then(|m| m.model.clone())
            .unwrap_or_else(|| "gpt-4o".to_string());
        let reasoning_effort = models
            .and_then(|m| m.reasoning_effort.clone())
            .unwrap_or_else(|| "high".to_string());

        // Codex/OpenAI's base_url may be a bare origin (needs /v1 appended) or may already include a custom prefix (should not force-append a version)
        let base_trimmed = self.base_url.trim_end_matches('/');
        let origin_only = match base_trimmed.split_once("://") {
            Some((_scheme, rest)) => !rest.contains('/'),
            None => !base_trimmed.contains('/'),
        };
        let codex_base_url = if base_trimmed.ends_with("/v1") {
            base_trimmed.to_string()
        } else if origin_only {
            format!("{base_trimmed}/v1")
        } else {
            base_trimmed.to_string()
        };

        // Generate the Codex config.toml content
        let config_toml = format!(
            r#"model_provider = "custom"
model = "{model}"
model_reasoning_effort = "{reasoning_effort}"
disable_response_storage = true

[model_providers.custom]
name = "NewAPI"
base_url = "{codex_base_url}"
wire_api = "responses"
requires_openai_auth = true"#
        );

        let settings_config = serde_json::json!({
            "auth": {
                "OPENAI_API_KEY": self.api_key
            },
            "config": config_toml
        });

        Some(Provider {
            id: format!("universal-codex-{}", self.id),
            name: self.name.clone(),
            settings_config,
            website_url: self.website_url.clone(),
            category: Some("aggregator".to_string()),
            created_at: self.created_at,
            sort_index: self.sort_index,
            notes: self.notes.clone(),
            meta: self.meta.clone(),
            icon: self.icon.clone(),
            icon_color: self.icon_color.clone(),
            in_failover_queue: false,
        })
    }

    /// Generate a Gemini provider config
    pub fn to_gemini_provider(&self) -> Option<Provider> {
        if !self.apps.gemini {
            return None;
        }

        let models = self.models.gemini.as_ref();
        let model = models
            .and_then(|m| m.model.clone())
            .unwrap_or_else(|| "gemini-2.5-pro".to_string());

        let settings_config = serde_json::json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": self.base_url,
                "GEMINI_API_KEY": self.api_key,
                "GEMINI_MODEL": model,
            }
        });

        Some(Provider {
            id: format!("universal-gemini-{}", self.id),
            name: self.name.clone(),
            settings_config,
            website_url: self.website_url.clone(),
            category: Some("aggregator".to_string()),
            created_at: self.created_at,
            sort_index: self.sort_index,
            notes: self.notes.clone(),
            meta: self.meta.clone(),
            icon: self.icon.clone(),
            icon_color: self.icon_color.clone(),
            in_failover_queue: false,
        })
    }
}

// ============================================================================
// OpenCode provider config structures
// ============================================================================

/// settings_config structure of an OpenCode provider
///
/// OpenCode uses AI SDK package names to specify the provider type, which differs from the
/// config format of the other apps.
/// Example config:
/// ```json
/// {
///   "npm": "@ai-sdk/openai-compatible",
///   "options": { "baseURL": "https://api.example.com/v1", "apiKey": "sk-xxx" },
///   "models": { "gpt-4o": { "name": "GPT-4o" } }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeProviderConfig {
    /// AI SDK package name, e.g. "@ai-sdk/openai-compatible", "@ai-sdk/anthropic"
    pub npm: String,

    /// Provider name (optional, for display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Provider options (API key, base URL, etc.)
    #[serde(default)]
    pub options: OpenCodeProviderOptions,

    /// Model definition map
    #[serde(default)]
    pub models: HashMap<String, OpenCodeModel>,
}

impl Default for OpenCodeProviderConfig {
    fn default() -> Self {
        Self {
            npm: "@ai-sdk/openai-compatible".to_string(),
            name: None,
            options: OpenCodeProviderOptions::default(),
            models: HashMap::new(),
        }
    }
}

/// OpenCode provider options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenCodeProviderOptions {
    /// API base URL
    #[serde(rename = "baseURL", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// API key (supports env var references, e.g. "{env:API_KEY}")
    #[serde(rename = "apiKey", skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Custom request headers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,

    /// Extra options (timeout, setCacheKey, ...)
    /// flatten captures every field not defined explicitly
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, Value>,
}

/// OpenCode model definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeModel {
    /// Model display name
    pub name: String,

    /// Model limits (context and output token counts)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<OpenCodeModelLimit>,

    /// Extra model options (provider routing, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<HashMap<String, Value>>,

    /// Extra fields (cost, modalities, thinking, variants, ...)
    /// flatten captures every field not defined explicitly
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, Value>,
}

/// OpenCode model limits
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenCodeModelLimit {
    /// Context token limit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<u64>,

    /// Output token limit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::{
        ClaudeModelConfig, CodexModelConfig, GeminiModelConfig, LocalProxyRequestOverrides,
        OpenCodeProviderConfig, Provider, ProviderManager, ProviderMeta, UniversalProvider,
    };
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn proxy_injected_oauth_excludes_codex_oauth() {
        let mut provider = Provider::with_id("p".to_string(), "P".to_string(), json!({}), None);
        assert!(!provider.uses_proxy_injected_oauth());

        for (provider_type, expected) in [
            ("xai_oauth", true),
            ("github_copilot", true),
            // the official ChatGPT login IS this card's credential — its
            // auth.json fallback shape must never be neutralized
            ("codex_oauth", false),
        ] {
            provider.meta = Some(ProviderMeta {
                provider_type: Some(provider_type.to_string()),
                ..ProviderMeta::default()
            });
            assert_eq!(
                provider.uses_proxy_injected_oauth(),
                expected,
                "{provider_type}"
            );
        }
    }

    #[test]
    fn provider_meta_serializes_pricing_model_source() {
        let meta = ProviderMeta {
            pricing_model_source: Some("response".to_string()),
            ..ProviderMeta::default()
        };

        let value = serde_json::to_value(&meta).expect("serialize ProviderMeta");

        assert_eq!(
            value
                .get("pricingModelSource")
                .and_then(|item| item.as_str()),
            Some("response")
        );
        assert!(value.get("pricing_model_source").is_none());
    }

    #[test]
    fn provider_meta_omits_pricing_model_source_when_none() {
        let meta = ProviderMeta::default();
        let value = serde_json::to_value(&meta).expect("serialize ProviderMeta");

        assert!(value.get("pricingModelSource").is_none());
    }

    #[test]
    fn provider_meta_roundtrips_max_output_tokens() {
        let meta = ProviderMeta {
            max_output_tokens: Some(64000),
            ..ProviderMeta::default()
        };

        let value = serde_json::to_value(&meta).expect("serialize ProviderMeta");
        assert_eq!(
            value.get("maxOutputTokens").and_then(|v| v.as_u64()),
            Some(64000)
        );
        assert!(value.get("max_output_tokens").is_none());

        let parsed: ProviderMeta = serde_json::from_value(value).expect("deserialize ProviderMeta");
        assert_eq!(parsed.max_output_tokens, Some(64000));
    }

    #[test]
    fn provider_meta_omits_max_output_tokens_when_none() {
        let value = serde_json::to_value(ProviderMeta::default()).expect("serialize ProviderMeta");
        assert!(value.get("maxOutputTokens").is_none());
    }

    #[test]
    fn provider_meta_roundtrips_local_proxy_request_overrides() {
        let meta = ProviderMeta {
            local_proxy_request_overrides: Some(LocalProxyRequestOverrides {
                headers: HashMap::from([("X-Test".to_string(), "yes".to_string())]),
                body: Some(json!({ "temperature": 0.2 })),
            }),
            ..ProviderMeta::default()
        };

        let value = serde_json::to_value(&meta).expect("serialize ProviderMeta");
        assert_eq!(
            value["localProxyRequestOverrides"]["headers"]["X-Test"],
            "yes"
        );
        assert_eq!(
            value["localProxyRequestOverrides"]["body"]["temperature"],
            0.2
        );

        let decoded: ProviderMeta =
            serde_json::from_value(value).expect("deserialize ProviderMeta");
        let overrides = decoded.local_proxy_request_overrides.unwrap();
        assert_eq!(overrides.headers.get("X-Test"), Some(&"yes".to_string()));
        assert_eq!(overrides.body.unwrap()["temperature"], 0.2);
    }

    #[test]
    fn provider_with_id_populates_defaults() {
        let settings_config = json!({
            "env": { "API_KEY": "test" }
        });
        let provider = Provider::with_id(
            "provider-1".to_string(),
            "Provider".to_string(),
            settings_config.clone(),
            Some("https://example.com".to_string()),
        );

        assert_eq!(provider.id, "provider-1");
        assert_eq!(provider.name, "Provider");
        assert_eq!(provider.settings_config, settings_config);
        assert_eq!(provider.website_url.as_deref(), Some("https://example.com"));
        assert!(provider.category.is_none());
        assert!(provider.created_at.is_none());
        assert!(provider.sort_index.is_none());
        assert!(provider.notes.is_none());
        assert!(provider.meta.is_none());
        assert!(provider.icon.is_none());
        assert!(provider.icon_color.is_none());
        assert!(!provider.in_failover_queue);
    }

    #[test]
    fn provider_managed_account_auth_detection_uses_type_or_known_endpoint() {
        let mut copilot = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
                }
            }),
            None,
        );
        assert!(copilot.is_github_copilot());
        assert!(copilot.uses_managed_account_auth());

        let mut codex = Provider::with_id(
            "codex".to_string(),
            "Codex".to_string(),
            json!({ "env": {} }),
            None,
        );
        codex.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            ..Default::default()
        });
        assert!(codex.is_codex_oauth());
        assert!(codex.uses_managed_account_auth());

        let codex_endpoint = Provider::with_id(
            "codex-endpoint".to_string(),
            "Codex Endpoint".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            None,
        );
        assert!(codex_endpoint.uses_managed_account_auth());

        copilot.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..Default::default()
        });
        assert!(copilot.is_github_copilot());
    }

    #[test]
    fn provider_manager_get_all_providers_returns_map() {
        let mut manager = ProviderManager::default();
        let provider = Provider::with_id(
            "provider-1".to_string(),
            "Provider".to_string(),
            json!({ "env": {} }),
            None,
        );
        manager.providers.insert("provider-1".to_string(), provider);

        assert_eq!(manager.get_all_providers().len(), 1);
        assert!(manager.get_all_providers().contains_key("provider-1"));
    }

    #[test]
    fn universal_provider_to_claude_provider_uses_models() {
        let mut universal = UniversalProvider::new(
            "u1".to_string(),
            "Universal".to_string(),
            "newapi".to_string(),
            "https://api.example.com".to_string(),
            "api-key".to_string(),
        );
        universal.apps.claude = true;
        universal.models.claude = Some(ClaudeModelConfig {
            model: Some("claude-main".to_string()),
            haiku_model: Some("claude-haiku".to_string()),
            sonnet_model: Some("claude-sonnet".to_string()),
            opus_model: Some("claude-opus".to_string()),
        });

        let provider = universal.to_claude_provider().expect("claude provider");

        assert_eq!(provider.id, "universal-claude-u1");
        assert_eq!(provider.name, "Universal");
        assert_eq!(provider.category.as_deref(), Some("aggregator"));
        assert_eq!(
            provider
                .settings_config
                .pointer("/env/ANTHROPIC_MODEL")
                .and_then(|item| item.as_str()),
            Some("claude-main")
        );
        assert_eq!(
            provider
                .settings_config
                .pointer("/env/ANTHROPIC_DEFAULT_HAIKU_MODEL")
                .and_then(|item| item.as_str()),
            Some("claude-haiku")
        );
        assert_eq!(
            provider
                .settings_config
                .pointer("/env/ANTHROPIC_DEFAULT_SONNET_MODEL")
                .and_then(|item| item.as_str()),
            Some("claude-sonnet")
        );
        assert_eq!(
            provider
                .settings_config
                .pointer("/env/ANTHROPIC_DEFAULT_OPUS_MODEL")
                .and_then(|item| item.as_str()),
            Some("claude-opus")
        );
    }

    #[test]
    fn universal_provider_to_claude_provider_disabled_returns_none() {
        let universal = UniversalProvider::new(
            "u1".to_string(),
            "Universal".to_string(),
            "newapi".to_string(),
            "https://api.example.com".to_string(),
            "api-key".to_string(),
        );

        assert!(universal.to_claude_provider().is_none());
    }

    #[test]
    fn universal_provider_to_codex_provider_appends_v1() {
        let mut universal = UniversalProvider::new(
            "u1".to_string(),
            "Universal".to_string(),
            "newapi".to_string(),
            "https://api.example.com".to_string(),
            "api-key".to_string(),
        );
        universal.apps.codex = true;
        universal.models.codex = Some(CodexModelConfig {
            model: Some("gpt-4o-mini".to_string()),
            reasoning_effort: Some("low".to_string()),
        });

        let provider = universal.to_codex_provider().expect("codex provider");
        let config = provider
            .settings_config
            .get("config")
            .and_then(|item| item.as_str())
            .expect("config toml");

        assert!(config.contains("base_url = \"https://api.example.com/v1\""));
        assert_eq!(
            provider
                .settings_config
                .pointer("/auth/OPENAI_API_KEY")
                .and_then(|item| item.as_str()),
            Some("api-key")
        );
    }

    #[test]
    fn universal_provider_to_codex_provider_keeps_v1_suffix() {
        let mut universal = UniversalProvider::new(
            "u1".to_string(),
            "Universal".to_string(),
            "newapi".to_string(),
            "https://api.example.com/v1".to_string(),
            "api-key".to_string(),
        );
        universal.apps.codex = true;

        let provider = universal.to_codex_provider().expect("codex provider");
        let config = provider
            .settings_config
            .get("config")
            .and_then(|item| item.as_str())
            .expect("config toml");

        assert!(config.contains("base_url = \"https://api.example.com/v1\""));
    }

    #[test]
    fn universal_provider_to_codex_provider_disabled_returns_none() {
        let universal = UniversalProvider::new(
            "u1".to_string(),
            "Universal".to_string(),
            "newapi".to_string(),
            "https://api.example.com".to_string(),
            "api-key".to_string(),
        );

        assert!(universal.to_codex_provider().is_none());
    }

    #[test]
    fn universal_provider_to_gemini_provider_defaults_model() {
        let mut universal = UniversalProvider::new(
            "u1".to_string(),
            "Universal".to_string(),
            "newapi".to_string(),
            "https://api.example.com".to_string(),
            "api-key".to_string(),
        );
        universal.apps.gemini = true;

        let provider = universal.to_gemini_provider().expect("gemini provider");

        assert_eq!(
            provider
                .settings_config
                .pointer("/env/GEMINI_MODEL")
                .and_then(|item| item.as_str()),
            Some("gemini-2.5-pro")
        );
    }

    #[test]
    fn universal_provider_to_gemini_provider_uses_model() {
        let mut universal = UniversalProvider::new(
            "u1".to_string(),
            "Universal".to_string(),
            "newapi".to_string(),
            "https://api.example.com".to_string(),
            "api-key".to_string(),
        );
        universal.apps.gemini = true;
        universal.models.gemini = Some(GeminiModelConfig {
            model: Some("gemini-custom".to_string()),
        });

        let provider = universal.to_gemini_provider().expect("gemini provider");

        assert_eq!(
            provider
                .settings_config
                .pointer("/env/GEMINI_MODEL")
                .and_then(|item| item.as_str()),
            Some("gemini-custom")
        );
    }

    #[test]
    fn opencode_provider_config_defaults() {
        let config = OpenCodeProviderConfig::default();
        assert_eq!(config.npm, "@ai-sdk/openai-compatible");
        assert!(config.name.is_none());
        assert!(config.models.is_empty());
        assert!(config.options.base_url.is_none());
        assert!(config.options.api_key.is_none());
        assert!(config.options.headers.is_none());
        assert!(config.options.extra.is_empty());
    }

    #[test]
    fn universal_codex_provider_origin_base_url_adds_v1() {
        let mut p = UniversalProvider::new(
            "id".to_string(),
            "Test".to_string(),
            "custom".to_string(),
            "https://api.openai.com".to_string(),
            "sk-test".to_string(),
        );
        p.apps.codex = true;

        let provider = p.to_codex_provider().expect("should build codex provider");
        let toml = provider
            .settings_config
            .get("config")
            .and_then(|v| v.as_str())
            .expect("config should be a toml string");

        assert!(toml.contains("base_url = \"https://api.openai.com/v1\""));
    }

    #[test]
    fn universal_codex_provider_custom_prefix_does_not_force_v1() {
        let mut p = UniversalProvider::new(
            "id".to_string(),
            "Test".to_string(),
            "custom".to_string(),
            "https://example.com/openai".to_string(),
            "sk-test".to_string(),
        );
        p.apps.codex = true;

        let provider = p.to_codex_provider().expect("should build codex provider");
        let toml = provider
            .settings_config
            .get("config")
            .and_then(|v| v.as_str())
            .expect("config should be a toml string");

        assert!(toml.contains("base_url = \"https://example.com/openai\""));
        assert!(!toml.contains("https://example.com/openai/v1"));
    }

    // ── resolve_usage_credentials (per-app credential extraction) ──

    use crate::app_config::AppType;

    fn provider_with(settings_config: serde_json::Value) -> Provider {
        Provider::with_id("p".to_string(), "P".to_string(), settings_config, None)
    }

    #[test]
    fn resolve_credentials_claude_env() {
        let p = provider_with(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "sk-claude",
            }
        }));
        assert_eq!(
            p.resolve_usage_credentials(&AppType::Claude),
            (
                "https://api.deepseek.com/anthropic".to_string(),
                "sk-claude".to_string()
            )
        );
    }

    #[test]
    fn resolve_credentials_claude_openrouter_fallback() {
        // OpenRouter-on-Claude keeps its key in OPENROUTER_API_KEY; the superset
        // fallback must still find it (regression guard for the per-app refactor).
        let p = provider_with(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api/v1",
                "OPENROUTER_API_KEY": "sk-or",
            }
        }));
        let (base_url, api_key) = p.resolve_usage_credentials(&AppType::Claude);
        assert_eq!(base_url, "https://openrouter.ai/api/v1");
        assert_eq!(api_key, "sk-or");
    }

    #[test]
    fn resolve_credentials_codex_auth_and_toml() {
        let p = provider_with(json!({
            "auth": { "OPENAI_API_KEY": "sk-codex" },
            "config": "model_provider = \"deepseek\"\n\
                       [model_providers.deepseek]\n\
                       base_url = \"https://api.deepseek.com\"\n",
        }));
        assert_eq!(
            p.resolve_usage_credentials(&AppType::Codex),
            (
                "https://api.deepseek.com".to_string(),
                "sk-codex".to_string()
            )
        );
    }

    #[test]
    fn resolve_credentials_gemini_env_with_google_fallback() {
        let p = provider_with(json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com",
                "GOOGLE_API_KEY": "g-legacy",
            }
        }));
        let (base_url, api_key) = p.resolve_usage_credentials(&AppType::Gemini);
        assert_eq!(base_url, "https://generativelanguage.googleapis.com");
        assert_eq!(api_key, "g-legacy");
    }

    #[test]
    fn resolve_credentials_claude_skips_empty_primary_key() {
        // Presets seed ANTHROPIC_AUTH_TOKEN as a present-but-empty placeholder.
        // The fallback chain must skip empty values (matching the frontend's
        // `a || b` semantics), not just absent keys.
        let p = provider_with(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api/v1",
                "ANTHROPIC_AUTH_TOKEN": "",
                "ANTHROPIC_API_KEY": "",
                "OPENROUTER_API_KEY": "sk-or",
            }
        }));
        let (_, api_key) = p.resolve_usage_credentials(&AppType::Claude);
        assert_eq!(api_key, "sk-or");
    }

    #[test]
    fn resolve_credentials_gemini_skips_empty_primary_key() {
        let p = provider_with(json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com",
                "GEMINI_API_KEY": "",
                "GOOGLE_API_KEY": "g-real",
            }
        }));
        let (_, api_key) = p.resolve_usage_credentials(&AppType::Gemini);
        assert_eq!(api_key, "g-real");
    }

    #[test]
    fn resolve_credentials_hermes_snake_case() {
        let p = provider_with(json!({
            "base_url": "https://api.deepseek.com",
            "api_key": "sk-hermes",
        }));
        assert_eq!(
            p.resolve_usage_credentials(&AppType::Hermes),
            (
                "https://api.deepseek.com".to_string(),
                "sk-hermes".to_string()
            )
        );
    }

    #[test]
    fn resolve_credentials_openclaw_camel_case() {
        let p = provider_with(json!({
            "baseUrl": "https://api.deepseek.com",
            "apiKey": "sk-openclaw",
        }));
        assert_eq!(
            p.resolve_usage_credentials(&AppType::OpenClaw),
            (
                "https://api.deepseek.com".to_string(),
                "sk-openclaw".to_string()
            )
        );
    }

    #[test]
    fn resolve_credentials_pi_uses_native_model_level_base_url() {
        let p = provider_with(json!({
            "apiKey": "sk-pi",
            "models": [{
                "id": "model-a",
                "api": "openai-completions",
                "baseUrl": "https://api.example.com/v1/"
            }]
        }));
        assert_eq!(
            p.resolve_usage_credentials(&AppType::Pi),
            (
                "https://api.example.com/v1".to_string(),
                "sk-pi".to_string()
            )
        );
    }

    #[test]
    fn resolve_credentials_opencode_options() {
        // OpenCode (OMO) nests creds under options.{baseURL,apiKey}; useOpencodeFormState
        // writes config.options.apiKey, so the stored provider keeps them there.
        let p = provider_with(json!({
            "npm": "@ai-sdk/openai-compatible",
            "options": {
                "baseURL": "https://api.deepseek.com/v1",
                "apiKey": "sk-opencode",
                "setCacheKey": true,
            }
        }));
        assert_eq!(
            p.resolve_usage_credentials(&AppType::OpenCode),
            (
                "https://api.deepseek.com/v1".to_string(),
                "sk-opencode".to_string()
            )
        );
    }

    #[test]
    fn resolve_credentials_claude_desktop_uses_env() {
        // ClaudeDesktop persists the Anthropic env shape (ClaudeDesktopProviderForm
        // reads env.ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN), so it resolves via
        // the default env branch — it is NOT unsupported.
        let p = provider_with(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "sk-desktop",
            }
        }));
        assert_eq!(
            p.resolve_usage_credentials(&AppType::ClaudeDesktop),
            (
                "https://api.deepseek.com/anthropic".to_string(),
                "sk-desktop".to_string()
            )
        );
    }

    #[test]
    fn resolve_credentials_trims_trailing_slash_on_base_url() {
        let p = provider_with(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic/",
                "ANTHROPIC_AUTH_TOKEN": "sk-claude",
            }
        }));
        let (base_url, _) = p.resolve_usage_credentials(&AppType::Claude);
        assert_eq!(base_url, "https://api.deepseek.com/anthropic");
    }

    #[test]
    fn resolve_credentials_missing_fields_yield_empty() {
        let p = provider_with(json!({}));
        assert_eq!(
            p.resolve_usage_credentials(&AppType::Claude),
            (String::new(), String::new())
        );
    }
}
