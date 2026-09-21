//! Authentication Types
//!
//! Defines credentials and auth strategies, covering the auth schemes of several upstream providers.

/// Credentials
///
/// Holds the API key and its matching auth strategy
#[derive(Debug, Clone)]
pub struct AuthInfo {
    /// API Key
    pub api_key: String,
    /// Auth strategy
    pub strategy: AuthStrategy,
    /// OAuth access_token (used by the GoogleOAuth strategy)
    pub access_token: Option<String>,
}

impl AuthInfo {
    /// Creates new credentials
    pub fn new(api_key: String, strategy: AuthStrategy) -> Self {
        Self {
            api_key,
            strategy,
            access_token: None,
        }
    }

    /// Creates credentials carrying an access_token (for OAuth)
    pub fn with_access_token(api_key: String, access_token: String) -> Self {
        Self {
            api_key,
            strategy: AuthStrategy::GoogleOAuth,
            access_token: Some(access_token),
        }
    }
}

/// Auth strategy
///
/// Different providers use different auth schemes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStrategy {
    /// Anthropic auth scheme
    /// - Header: `x-api-key: <api_key>`
    /// - Header: `anthropic-version: 2023-06-01`
    Anthropic,

    /// Claude relay-service auth scheme (Bearer only, no x-api-key)
    ///
    /// - Header: `Authorization: Bearer <api_key>`
    ///
    /// For relay services that do not support x-api-key
    ClaudeAuth,

    /// Bearer token auth scheme (OpenAI and similar)
    ///
    /// - Header: `Authorization: Bearer <api_key>`
    Bearer,

    /// Google API key auth scheme
    ///
    /// - Header: `x-goog-api-key: <api_key>`
    Google,

    /// Google OAuth auth scheme
    ///
    /// - Header: `Authorization: Bearer <access_token>`
    ///
    /// For scenarios that require OAuth, such as the Gemini CLI
    GoogleOAuth,

    /// GitHub Copilot auth scheme
    ///
    /// - Header: `Authorization: Bearer <copilot_token>`
    ///
    /// Uses a dynamically fetched Copilot token (obtained via the GitHub OAuth device-code flow)
    GitHubCopilot,

    /// Codex OAuth auth scheme (ChatGPT Plus/Pro)
    ///
    /// - Header: `Authorization: Bearer <access_token>`
    /// - Header: `ChatGPT-Account-Id: <account_id>` (injected by the forwarder)
    /// - Header: `originator: codex_cli_rs` + `version: <codex version>` (paired; the backend routes model cohorts by these)
    ///
    /// Uses a dynamically fetched OpenAI access_token (obtained via the device-code flow)
    CodexOAuth,

    /// xAI OAuth (Grok API)
    ///
    /// - Header: `Authorization: Bearer <access_token>`
    ///
    /// The access token comes from the xAI device-code flow and is injected dynamically by the forwarder.
    XaiOAuth,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_strategy_equality() {
        assert_eq!(AuthStrategy::Anthropic, AuthStrategy::Anthropic);
        assert_ne!(AuthStrategy::Anthropic, AuthStrategy::Bearer);
        assert_ne!(AuthStrategy::Bearer, AuthStrategy::Google);
        assert_ne!(AuthStrategy::CodexOAuth, AuthStrategy::XaiOAuth);
    }

    #[test]
    fn test_auth_info_new_has_no_access_token() {
        let auth = AuthInfo::new("api-key".to_string(), AuthStrategy::Bearer);
        assert!(auth.access_token.is_none());
    }

    #[test]
    fn test_auth_info_with_access_token() {
        let auth = AuthInfo::with_access_token(
            "refresh-token".to_string(),
            "ya29.access-token-12345".to_string(),
        );
        assert_eq!(auth.api_key, "refresh-token");
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
        assert_eq!(
            auth.access_token,
            Some("ya29.access-token-12345".to_string())
        );
    }

    #[test]
    fn test_claude_auth_strategy() {
        let auth = AuthInfo::new("sk-test".to_string(), AuthStrategy::ClaudeAuth);
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
        assert_ne!(auth.strategy, AuthStrategy::Anthropic);
        assert_ne!(auth.strategy, AuthStrategy::Bearer);
    }

    #[test]
    fn test_google_oauth_strategy() {
        let auth = AuthInfo::new("refresh-token".to_string(), AuthStrategy::GoogleOAuth);
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
        assert_ne!(auth.strategy, AuthStrategy::Google);
    }

    #[test]
    fn test_all_strategies_are_distinct() {
        let strategies = [
            AuthStrategy::Anthropic,
            AuthStrategy::ClaudeAuth,
            AuthStrategy::Bearer,
            AuthStrategy::Google,
            AuthStrategy::GoogleOAuth,
            AuthStrategy::GitHubCopilot,
            AuthStrategy::CodexOAuth,
        ];

        for (i, s1) in strategies.iter().enumerate() {
            for (j, s2) in strategies.iter().enumerate() {
                if i == j {
                    assert_eq!(s1, s2);
                } else {
                    assert_ne!(s1, s2);
                }
            }
        }
    }
}
