use serde::{Deserialize, Serialize};

/// Proxy server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Listen address
    pub listen_address: String,
    /// Listen port
    pub listen_port: u16,
    /// Maximum retry count
    pub max_retries: u8,
    /// Request timeout (seconds) - deprecated, kept for compatibility
    pub request_timeout: u64,
    /// Whether logging is enabled
    pub enable_logging: bool,
    /// Whether the live config takeover is currently active
    #[serde(default)]
    pub live_takeover_active: bool,
    /// Streaming first-byte timeout (seconds) - max time to wait for the first chunk, range 1-120s, default 60s
    #[serde(default = "default_streaming_first_byte_timeout")]
    pub streaming_first_byte_timeout: u64,
    /// Streaming idle timeout (seconds) - max interval between chunks, range 60-600s, 0 disables it (guards against mid-stream stalls)
    #[serde(default = "default_streaming_idle_timeout")]
    pub streaming_idle_timeout: u64,
    /// Non-streaming total timeout (seconds) - total timeout for non-streaming requests, range 60-1200s, default 600s (10 minutes)
    #[serde(default = "default_non_streaming_timeout")]
    pub non_streaming_timeout: u64,
}

fn default_streaming_first_byte_timeout() -> u64 {
    60
}

fn default_streaming_idle_timeout() -> u64 {
    120
}

fn default_non_streaming_timeout() -> u64 {
    600
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            listen_address: "127.0.0.1".to_string(),
            listen_port: 15721, // Use a high-numbered port that's less likely to be taken
            max_retries: 3,
            request_timeout: 600,
            enable_logging: true,
            live_takeover_active: false,
            streaming_first_byte_timeout: 60,
            streaming_idle_timeout: 120,
            non_streaming_timeout: 600,
        }
    }
}

/// Proxy server status
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyStatus {
    /// Whether it is running
    pub running: bool,
    /// Listen address
    pub address: String,
    /// Listen port
    pub port: u16,
    /// Active connection count
    pub active_connections: usize,
    /// Total request count
    pub total_requests: u64,
    /// Successful request count
    pub success_requests: u64,
    /// Failed request count
    pub failed_requests: u64,
    /// Success rate (0-100)
    pub success_rate: f32,
    /// Uptime (seconds)
    pub uptime_seconds: u64,
    /// Name of the currently used provider
    pub current_provider: Option<String>,
    /// ID of the current provider
    pub current_provider_id: Option<String>,
    /// Timestamp of the last request
    pub last_request_at: Option<String>,
    /// Last error message
    pub last_error: Option<String>,
    /// Provider failover count
    pub failover_count: u64,
    /// List of currently active proxy targets
    #[serde(default)]
    pub active_targets: Vec<ActiveTarget>,
}

/// Active proxy target info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTarget {
    pub app_type: String, // "Claude" | "Codex" | "Gemini"
    pub provider_name: String,
    pub provider_id: String,
}

/// Proxy server info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyServerInfo {
    pub address: String,
    pub port: u16,
    pub started_at: String,
}

/// Per-app takeover status (whether that app's live config has been rewritten to point at the local proxy)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyTakeoverStatus {
    pub claude: bool,
    pub codex: bool,
    pub gemini: bool,
    pub grokbuild: bool,
    pub opencode: bool,
    pub openclaw: bool,
}

/// Provider health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub app_type: String,
    pub is_healthy: bool,
    pub consecutive_failures: u32,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

/// Live config backup record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveBackup {
    /// App type (claude/codex/gemini)
    pub app_type: String,
    /// Original config JSON
    pub original_config: String,
    /// Backup timestamp
    pub backed_up_at: String,
}

/// Global proxy config (unified fields, mirrored three-way)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalProxyConfig {
    /// Master proxy switch
    pub proxy_enabled: bool,
    /// Listen address
    pub listen_address: String,
    /// Listen port
    pub listen_port: u16,
    /// Whether logging is enabled
    pub enable_logging: bool,
}

/// Per-app proxy config (independent per app)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppProxyConfig {
    /// App type (claude/codex/gemini)
    pub app_type: String,
    /// Whether the proxy is enabled for this app
    pub enabled: bool,
    /// Whether auto-failover is enabled for this app
    pub auto_failover_enabled: bool,
    /// Maximum retry count
    pub max_retries: u32,
    /// Streaming first-byte timeout (seconds)
    pub streaming_first_byte_timeout: u32,
    /// Streaming idle timeout (seconds)
    pub streaming_idle_timeout: u32,
    /// Non-streaming total timeout (seconds)
    pub non_streaming_timeout: u32,
    /// Circuit breaker failure threshold
    pub circuit_failure_threshold: u32,
    /// Circuit breaker recovery threshold
    pub circuit_success_threshold: u32,
    /// Circuit breaker recovery wait time (seconds)
    pub circuit_timeout_seconds: u32,
    /// Error rate threshold
    pub circuit_error_rate_threshold: f64,
    /// Minimum request count for computing the error rate
    pub circuit_min_requests: u32,
}

/// Rectifier config
///
/// Stored in the settings table
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RectifierConfig {
    /// Master switch: whether the rectifier is enabled (on by default)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Request rectification: enable the thinking signature rectifier (on by default)
    ///
    /// Handles error: Invalid 'signature' in 'thinking' block
    #[serde(default = "default_true")]
    pub request_thinking_signature: bool,
    /// Request rectification: enable the thinking budget rectifier (on by default)
    ///
    /// Handles error: budget_tokens + thinking related constraints
    #[serde(default = "default_true")]
    pub request_thinking_budget: bool,
    /// Request rectification: unsupported image downgrade (on by default)
    ///
    /// When the upstream rejects image input, replace the image block with an [Unsupported Image]
    /// marker so the conversation isn't interrupted. Master switch governing both the "explicit text-only declaration" and "upstream-error fallback" paths.
    #[serde(default = "default_true")]
    pub request_media_fallback: bool,
    /// Request rectification: pre-send downgrade via the confirmed text-only registry (on by default)
    ///
    /// When a model hasn't declared its capabilities, strip images upfront based on the built-in confirmed text-only registry.
    /// Governed by request_media_fallback; disabling this alone only turns off the proxy's registry-based pre-check —
    /// the "explicit declaration" and "upstream fallback" paths still apply, and the Codex model catalog declaration is unchanged.
    #[serde(default = "default_true")]
    pub request_media_heuristic: bool,
}

fn default_true() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for RectifierConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            request_thinking_signature: true,
            request_thinking_budget: true,
            request_media_fallback: true,
            request_media_heuristic: true,
        }
    }
}

/// Request optimizer config
///
/// Stored in the settings table, key = "optimizer_config"
/// Only takes effect for the Bedrock provider (CLAUDE_CODE_USE_BEDROCK = "1")
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerConfig {
    /// Master switch (off by default, user must enable manually)
    #[serde(default)]
    pub enabled: bool,
    /// Thinking optimization sub-switch (effective by default once the master switch is on)
    #[serde(default = "default_true")]
    pub thinking_optimizer: bool,
    /// Cache injection sub-switch (effective by default once the master switch is on)
    #[serde(default = "default_true")]
    pub cache_injection: bool,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            thinking_optimizer: true,
            cache_injection: true,
        }
    }
}

/// Copilot optimizer config
///
/// Stored in the settings table, key = "copilot_optimizer_config"
/// Addresses abnormal Copilot proxy consumption (Issue #1813)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotOptimizerConfig {
    /// Master switch (on by default — critical for Copilot users)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// x-initiator request classification (on by default, P0 priority)
    #[serde(default = "default_true")]
    pub request_classification: bool,
    /// Tool result message merging (on by default, P1 priority)
    #[serde(default = "default_true")]
    pub tool_result_merging: bool,
    /// Compact request detection (on by default, P2 priority)
    #[serde(default = "default_true")]
    pub compact_detection: bool,
    /// Deterministic request ID (on by default, P3 priority)
    #[serde(default = "default_true")]
    pub deterministic_request_id: bool,
    /// Subagent detection (on by default) — identifies Claude Code subagent requests
    /// and sets x-initiator=agent + x-interaction-type=conversation-subagent to avoid subagent billing
    #[serde(default = "default_true")]
    pub subagent_detection: bool,
    /// Warmup small-model downgrade (on by default — aligned with the reference implementation, avoids probe requests consuming premium quota)
    #[serde(default = "default_true")]
    pub warmup_downgrade: bool,
    /// Model used for warmup downgrade (default "gpt-5-mini")
    #[serde(default = "default_warmup_model")]
    pub warmup_model: String,
    /// Proactively strip thinking / redacted_thinking blocks from assistant messages before sending the request
    ///
    /// Copilot uses an OpenAI-compatible endpoint; the upstream rejects thinking blocks, triggering a reactive
    /// rectifier retry — by then the first request has already consumed one unit of premium quota. Stripping it proactively avoids that waste.
    #[serde(default = "default_true")]
    pub strip_thinking: bool,
}

fn default_warmup_model() -> String {
    "gpt-5-mini".to_string()
}

impl Default for CopilotOptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            request_classification: true,
            tool_result_merging: true,
            compact_detection: true,
            deterministic_request_id: true,
            subagent_detection: true,
            warmup_downgrade: true,
            warmup_model: "gpt-5-mini".to_string(),
            strip_thinking: true,
        }
    }
}

/// Log config
///
/// Stored in the settings table's log_config field (JSON format)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogConfig {
    /// Master switch: whether logging is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Log level: error, warn, info, debug, trace
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: "info".to_string(),
        }
    }
}

impl LogConfig {
    /// Convert the config into a log::LevelFilter
    pub fn to_level_filter(&self) -> log::LevelFilter {
        if !self.enabled {
            return log::LevelFilter::Off;
        }
        match self.level.to_lowercase().as_str() {
            "error" => log::LevelFilter::Error,
            "warn" => log::LevelFilter::Warn,
            "info" => log::LevelFilter::Info,
            "debug" => log::LevelFilter::Debug,
            "trace" => log::LevelFilter::Trace,
            _ => log::LevelFilter::Info,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rectifier_config_default_enabled() {
        // Verify that RectifierConfig::default() returns all switches enabled
        let config = RectifierConfig::default();
        assert!(
            config.enabled,
            "rectifier master switch should default to true"
        );
        assert!(
            config.request_thinking_signature,
            "thinking signature rectifier should default to true"
        );
        assert!(
            config.request_thinking_budget,
            "thinking budget rectifier should default to true"
        );
        assert!(
            config.request_media_fallback,
            "media downgrade master switch should default to true"
        );
        assert!(
            config.request_media_heuristic,
            "heuristic text-only model detection should default to true"
        );
    }

    #[test]
    fn test_rectifier_config_serde_default() {
        // Verify that missing fields deserialize to the default value true
        let json = "{}";
        let config: RectifierConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert!(config.request_thinking_signature);
        assert!(config.request_thinking_budget);
        assert!(
            config.request_media_fallback,
            "missing requestMediaFallback should fall back to default true"
        );
        assert!(
            config.request_media_heuristic,
            "missing requestMediaHeuristic should fall back to default true"
        );
    }

    #[test]
    fn test_rectifier_config_serde_explicit_true() {
        // Verify correct deserialization when explicitly set to true
        let json =
            r#"{"enabled": true, "requestThinkingSignature": true, "requestThinkingBudget": true}"#;
        let config: RectifierConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert!(config.request_thinking_signature);
        assert!(config.request_thinking_budget);
    }

    #[test]
    fn test_rectifier_config_serde_partial_fields() {
        // Verify that when only some fields are set, missing fields use the default value true
        let json = r#"{"enabled": true, "requestThinkingSignature": false}"#;
        let config: RectifierConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert!(!config.request_thinking_signature);
        assert!(config.request_thinking_budget);
    }

    #[test]
    fn test_rectifier_config_serde_media_explicit_false() {
        // Verify the two media fields deserialize faithfully when explicitly false (a user's deliberate opt-out must take effect and not be overridden by the default)
        let json = r#"{"requestMediaFallback": false, "requestMediaHeuristic": false}"#;
        let config: RectifierConfig = serde_json::from_str(json).unwrap();
        assert!(!config.request_media_fallback);
        assert!(!config.request_media_heuristic);
        // The remaining fields still use the default true
        assert!(config.enabled);
        assert!(config.request_thinking_signature);
        assert!(config.request_thinking_budget);
    }

    #[test]
    fn test_log_config_default() {
        let config = LogConfig::default();
        assert!(config.enabled);
        assert_eq!(config.level, "info");
    }

    #[test]
    fn test_log_config_serde_default() {
        let json = "{}";
        let config: LogConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, "info");
    }

    #[test]
    fn test_log_config_to_level_filter() {
        let config = LogConfig {
            level: "error".to_string(),
            ..Default::default()
        };
        assert_eq!(config.to_level_filter(), log::LevelFilter::Error);

        let config = LogConfig {
            level: "warn".to_string(),
            ..Default::default()
        };
        assert_eq!(config.to_level_filter(), log::LevelFilter::Warn);

        let config = LogConfig {
            level: "info".to_string(),
            ..Default::default()
        };
        assert_eq!(config.to_level_filter(), log::LevelFilter::Info);

        let config = LogConfig {
            level: "debug".to_string(),
            ..Default::default()
        };
        assert_eq!(config.to_level_filter(), log::LevelFilter::Debug);

        let config = LogConfig {
            level: "trace".to_string(),
            ..Default::default()
        };
        assert_eq!(config.to_level_filter(), log::LevelFilter::Trace);

        // Invalid level falls back to info
        let config = LogConfig {
            level: "invalid".to_string(),
            ..Default::default()
        };
        assert_eq!(config.to_level_filter(), log::LevelFilter::Info);

        // Returns Off when disabled
        let config = LogConfig {
            enabled: false,
            level: "debug".to_string(),
        };
        assert_eq!(config.to_level_filter(), log::LevelFilter::Off);
    }

    #[test]
    fn test_log_config_serde_roundtrip() {
        let config = LogConfig {
            enabled: true,
            level: "debug".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: LogConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.level, "debug");
    }
}
