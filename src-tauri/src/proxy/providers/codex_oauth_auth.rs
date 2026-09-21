//! Codex OAuth Authentication Module
//!
//! Implements the OAuth device-code flow for OpenAI ChatGPT Plus/Pro subscriptions.
//! Supports multiple accounts, so each provider can bind a different ChatGPT account.
//!
//! ## Auth flow
//! 1. Start the device-code flow and obtain device_auth_id and user_code
//! 2. The user completes the ChatGPT authorization in a browser
//! 3. Poll for authorization_code and code_verifier (note: the verifier comes from the server)
//! 4. Exchange code + verifier for access_token + refresh_token + id_token
//! 5. Refresh access_token automatically (60 seconds before expiry)
//!
//! ## Multi-account support
//! - Each ChatGPT account stores its own refresh_token
//! - Providers bind an account through meta.authBinding (auth_provider = "codex_oauth")
//! - chatgpt_account_id, extracted from the JWT id_token, is the unique account identifier

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};

use super::copilot_auth::{GitHubAccount, GitHubDeviceCodeResponse};

/// OpenAI OAuth client ID (used by OpenCode, identical to the official Codex CLI)
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

/// Device-code start URL
const DEVICE_AUTH_USERCODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";

/// Device-code polling URL
const DEVICE_AUTH_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";

/// OAuth token URL (used for code exchange and refresh)
const OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

/// Device-code verification URL (shown to the user)
const DEVICE_VERIFICATION_URL: &str = "https://auth.openai.com/codex/device";

/// redirect_uri of the device-code flow (as required by the OpenAI server)
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";

/// How long before expiry a token is refreshed (milliseconds)
const TOKEN_REFRESH_BUFFER_MS: i64 = 60_000;

/// Per-request timeout for the OAuth token/device endpoints. The shared HTTP client's 600s default
/// is meant for streaming model responses and is far too long for auth, which should fail fast when the network stalls.
const OAUTH_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Default device-code lifetime in seconds; the OpenAI docs specify 15 minutes
const DEVICE_CODE_DEFAULT_EXPIRES_IN: u64 = 900;

/// Safety margin on the polling interval (seconds)
const POLLING_SAFETY_MARGIN_SECS: u64 = 3;

/// User-Agent
const CODEX_USER_AGENT: &str = "cc-switch-codex-oauth";

// Shared by model discovery and generation: ChatGPT gates models by this
// client identity. gpt-6-astra requires >= 0.153.0 in the rust-v0.153.4 catalog.
// Bump together when a new model raises its minimal_client_version.
pub(crate) const CODEX_OAUTH_ORIGINATOR: &str = "codex_cli_rs";
pub(crate) const CODEX_OAUTH_CLIENT_VERSION: &str = "0.153.4";

/// Codex OAuth errors
#[derive(Debug, thiserror::Error)]
pub enum CodexOAuthError {
    #[error("Waiting for user authorization")]
    AuthorizationPending,

    #[error("User denied authorization")]
    AccessDenied,

    #[error("Device code expired")]
    ExpiredToken,

    #[error("OAuth token fetch failed: {0}")]
    TokenFetchFailed(String),

    #[error("Refresh token invalid or expired")]
    RefreshTokenInvalid,

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),
}

impl From<reqwest::Error> for CodexOAuthError {
    fn from(err: reqwest::Error) -> Self {
        CodexOAuthError::NetworkError(err.to_string())
    }
}

impl From<std::io::Error> for CodexOAuthError {
    fn from(err: std::io::Error) -> Self {
        CodexOAuthError::IoError(err.to_string())
    }
}

/// OpenAI device-code response
#[derive(Debug, Clone, Deserialize)]
struct DeviceCodeResponse {
    device_auth_id: String,
    user_code: String,
    #[serde(default)]
    interval: Option<serde_json::Value>,
    #[serde(default)]
    expires_in: Option<u64>,
}

/// OpenAI device-code poll response (success)
#[derive(Debug, Clone, Deserialize)]
struct DevicePollSuccess {
    authorization_code: String,
    code_verifier: String,
}

/// OAuth token response
#[derive(Debug, Clone, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Parsed JWT claims (only chatgpt_account_id and a few other fields matter)
#[derive(Debug, Clone, Default, Deserialize)]
struct IdTokenClaims {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    organizations: Vec<OrgClaim>,
    #[serde(default, rename = "https://api.openai.com/auth")]
    openai_auth: Option<OpenAiAuthClaim>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct OrgClaim {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct OpenAiAuthClaim {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
}

/// Cached access_token (with its expiry)
#[derive(Debug, Clone)]
struct CachedAccessToken {
    token: String,
    /// Expiry timestamp in milliseconds
    expires_at_ms: i64,
    /// Timestamp of when it was obtained or refreshed, in milliseconds. Used for `last_refresh` in the
    /// managed auth.json so it reflects when the access_token was really obtained rather than when the
    /// file was written, which would make the Codex CLI treat a stale token as freshly refreshed.
    obtained_at_ms: i64,
}

impl CachedAccessToken {
    fn is_expiring_soon(&self) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        self.expires_at_ms - now < TOKEN_REFRESH_BUFFER_MS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshTokenAdoptionMode {
    /// Normal CLI synchronization: different token material must carry a
    /// strictly newer live timestamp before it can replace manager state.
    TimestampChecked,
    /// The OAuth server has just rejected the manager refresh token. A
    /// different same-account token observed on disk is therefore the only
    /// viable recovery generation and may bypass timestamp ambiguity.
    RejectedManagerToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshTokenAdoptionOutcome {
    /// The live and manager token material already describe the same
    /// generation. `state_changed` only reflects timestamp bookkeeping.
    Synchronized { state_changed: bool },
    /// Different live token material was accepted as the newer generation.
    Adopted,
    /// Different live token material carries a timestamp strictly older than
    /// the manager generation and may therefore be overwritten or removed.
    ProvablyOlder,
    /// Different token material could not be ordered safely. Callers that are
    /// about to overwrite/delete auth.json must abort instead of guessing.
    Ambiguous,
    /// The account is not owned by this manager.
    NotManaged,
}

impl RefreshTokenAdoptionOutcome {
    fn state_changed(self) -> bool {
        matches!(
            self,
            Self::Synchronized {
                state_changed: true
            } | Self::Adopted
        )
    }
}

/// An in-flight device-code entry, with an expiry so abandoned logins can be cleaned up
#[derive(Debug, Clone)]
struct PendingDeviceCode {
    user_code: String,
    /// Unix timestamp in milliseconds; cleanable once past
    expires_at_ms: i64,
}

/// Persisted account data
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CodexAccountData {
    /// chatgpt_account_id (also the HashMap key)
    pub account_id: String,
    /// Account email, when available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Refresh token (persisted)
    pub refresh_token: String,
    /// Authentication timestamp (seconds)
    pub authenticated_at: i64,
    /// ChatGPT id_token (a JWT, persisted). Keeps the managed Codex auth.json tokens field shaped like
    /// a native browser login; updated whenever a refresh returns a new value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    /// When this OAuth token set was last obtained or adopted. Used to reject adopting an older
    /// generation from live when both the Codex CLI and cc-switch may rotate the refresh_token.
    #[serde(default)]
    pub token_updated_at_ms: i64,
}

/// Public account info returned to the frontend (reusing the GitHubAccount struct)
impl From<&CodexAccountData> for GitHubAccount {
    fn from(data: &CodexAccountData) -> Self {
        GitHubAccount {
            id: data.account_id.clone(),
            // Use email as the display name, falling back to account_id
            login: data
                .email
                .clone()
                .unwrap_or_else(|| format!("ChatGPT ({})", &data.account_id)),
            avatar_url: None,
            authenticated_at: data.authenticated_at,
            github_domain: "github.com".to_string(),
            // Legacy accounts (signed in before the upgrade) have no persisted id_token and must log in again
            reauth_required: data.id_token.is_none(),
        }
    }
}

/// Persisted storage layout (v1)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CodexOAuthStore {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    accounts: HashMap<String, CodexAccountData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_account_id: Option<String>,
}

/// The complete refreshable token bundle needed to write a managed Codex `auth.json`.
#[derive(Debug, Clone)]
pub(crate) struct ManagedTokenBundle {
    pub access_token: String,
    pub id_token: Option<String>,
    pub refresh_token: String,
    /// When the access_token was really obtained, as RFC3339 with nanosecond precision and `Z` (the
    /// same shape as `last_refresh` in a native auth.json). Reflects the real refresh time, not the write time.
    pub last_refresh: String,
}

/// Codex OAuth authentication manager (multi-account)
pub struct CodexOAuthManager {
    accounts: Arc<RwLock<HashMap<String, CodexAccountData>>>,
    default_account_id: Arc<RwLock<Option<String>>>,
    /// In-memory access_token cache (not persisted)
    access_tokens: Arc<RwLock<HashMap<String, CachedAccessToken>>>,
    /// Per-account refresh lock
    refresh_locks: Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>,
    /// Ordinary token resolution/adoption takes the read lock while account removal/clearing takes the
    /// write lock, so removal waits for in-flight refreshes and cannot create a second account lock by clearing refresh_locks too early.
    lifecycle_lock: Arc<RwLock<()>>,
    /// In-flight device-code flows: device_auth_id -> {user_code, expires_at_ms}
    /// Expired entries are pruned in start_device_flow so abandoned logins cannot grow unboundedly
    pending_device_codes: Arc<RwLock<HashMap<String, PendingDeviceCode>>>,
    /// Incremented when all auth is cleared, so a login already in flight cannot re-register.
    login_epoch: AtomicU64,
    storage_path: PathBuf,
    /// Persistence serialization lock: the snapshot-plus-write and snapshot-plus-delete of
    /// `save_to_disk` and `clear_auth` both run under it. This used to be serialized implicitly by the
    /// outer `RwLock<CodexOAuthManager>` write lock; without that, it prevents interleaved save/clear from reviving deleted accounts via a stale snapshot.
    storage_lock: Arc<Mutex<()>>,
}

impl CodexOAuthManager {
    pub fn new(data_dir: PathBuf) -> Self {
        let storage_path = data_dir.join("codex_oauth_auth.json");

        let manager = Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            default_account_id: Arc::new(RwLock::new(None)),
            access_tokens: Arc::new(RwLock::new(HashMap::new())),
            refresh_locks: Arc::new(RwLock::new(HashMap::new())),
            lifecycle_lock: Arc::new(RwLock::new(())),
            pending_device_codes: Arc::new(RwLock::new(HashMap::new())),
            login_epoch: AtomicU64::new(0),
            storage_path,
            storage_lock: Arc::new(Mutex::new(())),
        };

        if let Err(e) = manager.load_from_disk_sync() {
            log::warn!("[CodexOAuth] failed to load storage: {e}");
        }

        manager
    }

    // ==================== Device-code flow ====================

    /// Starts the device-code flow
    ///
    /// Returns a GitHubDeviceCodeResponse to reuse the existing frontend struct, with fields mapped to OpenAI's:
    /// - device_code = device_auth_id
    /// - user_code = user_code
    /// - verification_uri = https://auth.openai.com/codex/device
    pub async fn start_device_flow(&self) -> Result<GitHubDeviceCodeResponse, CodexOAuthError> {
        log::info!("[CodexOAuth] starting the device-code flow");
        let login_epoch = self.login_epoch.load(Ordering::Acquire);

        let response = crate::proxy::http_client::get()
            .post(DEVICE_AUTH_USERCODE_URL)
            .timeout(OAUTH_HTTP_TIMEOUT)
            .header("Content-Type", "application/json")
            .header("User-Agent", CODEX_USER_AGENT)
            .json(&serde_json::json!({ "client_id": CODEX_CLIENT_ID }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(CodexOAuthError::NetworkError(format!(
                "Device code request failed: {status} - {text}"
            )));
        }

        let device: DeviceCodeResponse = response
            .json()
            .await
            .map_err(|e| CodexOAuthError::ParseError(e.to_string()))?;

        let interval = parse_interval(device.interval.as_ref());
        let expires_in = device.expires_in.unwrap_or(DEVICE_CODE_DEFAULT_EXPIRES_IN);
        let expires_at_ms = chrono::Utc::now().timestamp_millis() + (expires_in as i64) * 1000;

        self.register_pending_device_code(
            device.device_auth_id.clone(),
            device.user_code.clone(),
            expires_at_ms,
            login_epoch,
        )
        .await?;

        log::info!(
            "[CodexOAuth] device code obtained, user_code: {}",
            device.user_code
        );

        Ok(GitHubDeviceCodeResponse {
            device_code: device.device_auth_id,
            user_code: device.user_code,
            verification_uri: DEVICE_VERIFICATION_URL.to_string(),
            expires_in,
            interval,
        })
    }

    async fn register_pending_device_code(
        &self,
        device_auth_id: String,
        user_code: String,
        expires_at_ms: i64,
        login_epoch: u64,
    ) -> Result<(), CodexOAuthError> {
        let mut pending = self.pending_device_codes.write().await;
        if self.login_epoch.load(Ordering::Acquire) != login_epoch {
            return Err(CodexOAuthError::ExpiredToken);
        }

        let now_ms = chrono::Utc::now().timestamp_millis();
        pending.retain(|_, entry| entry.expires_at_ms > now_ms);
        pending.insert(
            device_auth_id,
            PendingDeviceCode {
                user_code,
                expires_at_ms,
            },
        );
        Ok(())
    }

    /// Polls the device-code status
    ///
    /// Takes device_code (that is, device_auth_id); Some(account) means authorization succeeded
    pub async fn poll_for_token(
        &self,
        device_code: &str,
    ) -> Result<Option<GitHubAccount>, CodexOAuthError> {
        let entry = {
            let pending = self.pending_device_codes.read().await;
            pending.get(device_code).cloned()
        };

        let entry = entry.ok_or_else(|| {
            CodexOAuthError::TokenFetchFailed(
                "No matching user_code, please restart the login flow".to_string(),
            )
        })?;

        if entry.expires_at_ms <= chrono::Utc::now().timestamp_millis() {
            let mut pending = self.pending_device_codes.write().await;
            pending.remove(device_code);
            return Err(CodexOAuthError::ExpiredToken);
        }

        let user_code = entry.user_code;

        log::debug!("[CodexOAuth] polling the device code");

        let poll_response = crate::proxy::http_client::get()
            .post(DEVICE_AUTH_TOKEN_URL)
            .timeout(OAUTH_HTTP_TIMEOUT)
            .header("Content-Type", "application/json")
            .header("User-Agent", CODEX_USER_AGENT)
            .json(&serde_json::json!({
                "device_auth_id": device_code,
                "user_code": user_code,
            }))
            .send()
            .await?;

        let status = poll_response.status();

        // 403/404 means the user has not finished authorizing, so keep polling
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND {
            return Err(CodexOAuthError::AuthorizationPending);
        }

        if status == reqwest::StatusCode::GONE {
            return Err(CodexOAuthError::ExpiredToken);
        }

        if !status.is_success() {
            let text = poll_response.text().await.unwrap_or_default();
            return Err(CodexOAuthError::TokenFetchFailed(format!(
                "{status} - {text}"
            )));
        }

        let success: DevicePollSuccess = poll_response
            .json()
            .await
            .map_err(|e| CodexOAuthError::ParseError(e.to_string()))?;

        log::info!("[CodexOAuth] user authorized, exchanging for an OAuth token");

        // Exchange authorization_code + code_verifier for tokens
        let tokens = self
            .exchange_code_for_tokens(&success.authorization_code, &success.code_verifier)
            .await?;

        let refresh_token = tokens.refresh_token.clone().ok_or_else(|| {
            CodexOAuthError::TokenFetchFailed("Response is missing refresh_token".to_string())
        })?;

        let (account_id, email) = extract_identity_from_tokens(&tokens);
        let account_id = account_id.ok_or_else(|| {
            CodexOAuthError::ParseError("Cannot extract account_id from the token".to_string())
        })?;

        let obtained_at_ms = chrono::Utc::now().timestamp_millis();
        // The login commit shares one generation lock with this account's refresh/adopt, and accounts and
        // the access cache are written together, so an old refresh response cannot overwrite a new login chain.
        let account = self
            .add_account_internal(
                account_id.clone(),
                refresh_token,
                email,
                // Treat an empty string as missing so no empty id_token is written
                tokens.id_token.clone().filter(|t| !t.trim().is_empty()),
                Some(CachedAccessToken {
                    token: tokens.access_token.clone(),
                    expires_at_ms: compute_expires_at_ms(tokens.expires_in),
                    obtained_at_ms,
                }),
                Some(device_code),
            )
            .await?;

        Ok(Some(account))
    }

    /// Exchanges authorization_code + code_verifier for tokens
    async fn exchange_code_for_tokens(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<OAuthTokenResponse, CodexOAuthError> {
        let response = crate::proxy::http_client::get()
            .post(OAUTH_TOKEN_URL)
            .timeout(OAUTH_HTTP_TIMEOUT)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("User-Agent", CODEX_USER_AGENT)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", DEVICE_REDIRECT_URI),
                ("client_id", CODEX_CLIENT_ID),
                ("code_verifier", code_verifier),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(CodexOAuthError::TokenFetchFailed(format!(
                "Token exchange failed: {status} - {text}"
            )));
        }

        response
            .json()
            .await
            .map_err(|e| CodexOAuthError::ParseError(e.to_string()))
    }

    /// Refreshes the access_token with the refresh_token
    async fn refresh_with_token(
        &self,
        refresh_token: &str,
    ) -> Result<OAuthTokenResponse, CodexOAuthError> {
        let response = crate::proxy::http_client::get()
            .post(OAUTH_TOKEN_URL)
            .timeout(OAUTH_HTTP_TIMEOUT)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("User-Agent", CODEX_USER_AGENT)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", CODEX_CLIENT_ID),
                ("scope", "openid profile email"),
            ])
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            let refresh_error_code = extract_refresh_error_code(&text);
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
                || matches!(
                    refresh_error_code.as_deref(),
                    Some(
                        "refresh_token_expired"
                            | "refresh_token_reused"
                            | "refresh_token_invalidated"
                    )
                )
            {
                return Err(CodexOAuthError::RefreshTokenInvalid);
            }
            return Err(CodexOAuthError::TokenFetchFailed(format!(
                "Refresh failed: {status} - {text}"
            )));
        }

        response
            .json()
            .await
            .map_err(|e| CodexOAuthError::ParseError(e.to_string()))
    }

    // ==================== Token retrieval (with auto refresh) ====================

    /// Returns a valid access_token for the given account, refreshing when needed
    pub async fn get_valid_token_for_account(
        &self,
        account_id: &str,
    ) -> Result<String, CodexOAuthError> {
        let _lifecycle = self.lifecycle_lock.read().await;
        Ok(self.resolve_valid_cached_token(account_id).await?.token)
    }

    /// Resolves the account's valid cached token (with its real acquisition time), refreshing if needed.
    ///
    /// Returning the full `CachedAccessToken` keeps the token and its `obtained_at_ms` paired (used
    /// directly for `last_refresh` in the managed auth.json), avoiding a mismatch from two separate cache reads.
    ///
    /// Concurrency: the caller holds the lifecycle read lock; a refresh briefly commits accounts then
    /// access_tokens under the account refresh mutex, and persists only after releasing them. The real
    /// lock order in `save_to_disk` is storage_lock then accounts/default. remove/clear hold the
    /// lifecycle write lock, so they wait for in-flight refreshes and block ABA recreation of the same account_id.
    async fn resolve_valid_cached_token(
        &self,
        account_id: &str,
    ) -> Result<CachedAccessToken, CodexOAuthError> {
        // Fast path: confirm the account exists, then read the cache
        {
            let accounts = self.accounts.read().await;
            if !accounts.contains_key(account_id) {
                return Err(CodexOAuthError::AccountNotFound(account_id.to_string()));
            }
            let tokens = self.access_tokens.read().await;
            if let Some(cached) = tokens.get(account_id) {
                if !cached.is_expiring_soon() {
                    return Ok(cached.clone());
                }
            }
        }

        log::info!("[CodexOAuth] access_token for account {account_id} needs a refresh");

        let refresh_lock = self.get_refresh_lock(account_id).await;
        let _guard = refresh_lock.lock().await;
        self.resolve_valid_cached_token_under_lock(account_id).await
    }

    /// Resolve a token while the caller owns this account's refresh mutex.
    /// Keeping this separate lets the full auth-bundle path hold one generation
    /// lock across access/id/refresh reads without recursively locking the mutex.
    async fn resolve_valid_cached_token_under_lock(
        &self,
        account_id: &str,
    ) -> Result<CachedAccessToken, CodexOAuthError> {
        // Codex CLI may have advanced the shared refresh-token generation since
        // this manager last used the account. Reload it under the same per-account
        // lock before deciding whether a network refresh is necessary.
        if let Some((live_refresh, live_id_token, live_last_refresh_ms)) =
            crate::codex_config::read_codex_live_auth_refresh_for_account(account_id)
        {
            self.adopt_account_refresh_token_under_lock(
                account_id,
                live_refresh,
                live_id_token,
                live_last_refresh_ms,
                RefreshTokenAdoptionMode::TimestampChecked,
            )
            .await?;
        }

        // double check (still under the accounts read lock)
        {
            let accounts = self.accounts.read().await;
            if !accounts.contains_key(account_id) {
                return Err(CodexOAuthError::AccountNotFound(account_id.to_string()));
            }
            let tokens = self.access_tokens.read().await;
            if let Some(cached) = tokens.get(account_id) {
                if !cached.is_expiring_soon() {
                    return Ok(cached.clone());
                }
            }
        }

        let mut refresh_token = {
            let accounts = self.accounts.read().await;
            accounts
                .get(account_id)
                .map(|a| a.refresh_token.clone())
                .ok_or_else(|| CodexOAuthError::AccountNotFound(account_id.to_string()))?
        };

        let new_tokens = match self.refresh_with_token(&refresh_token).await {
            Err(CodexOAuthError::RefreshTokenInvalid) => {
                // If Codex CLI refreshed between our pre-read and request, reload
                // its newer generation and retry exactly once. Error-code handling
                // includes OpenAI's `refresh_token_reused` response.
                let Some((live_refresh, live_id_token, live_last_refresh_ms)) =
                    crate::codex_config::read_codex_live_auth_refresh_for_account(account_id)
                        .filter(|(token, _, _)| token.trim() != refresh_token.as_str())
                else {
                    return Err(CodexOAuthError::RefreshTokenInvalid);
                };
                let adoption = self
                    .adopt_account_refresh_token_under_lock(
                        account_id,
                        live_refresh.clone(),
                        live_id_token,
                        live_last_refresh_ms,
                        RefreshTokenAdoptionMode::RejectedManagerToken,
                    )
                    .await?;
                if !matches!(adoption, RefreshTokenAdoptionOutcome::Adopted) {
                    return Err(CodexOAuthError::RefreshTokenInvalid);
                }
                refresh_token = live_refresh;
                self.refresh_with_token(&refresh_token).await?
            }
            result => result?,
        };

        let obtained_at_ms = chrono::Utc::now().timestamp_millis();

        // If the server returned a new refresh_token or id_token, update storage
        let mut needs_save = false;
        let (stored_refresh_token, stored_id_token) = {
            let mut accounts = self.accounts.write().await;
            let account = accounts
                .get_mut(account_id)
                .ok_or_else(|| CodexOAuthError::AccountNotFound(account_id.to_string()))?;
            // Device re-login and CLI-token adoption use the same account lock,
            // but keep a generation CAS here as defense in depth: a response for
            // R0 must never overwrite a newly committed R1/N0 chain.
            if account.refresh_token != refresh_token {
                return Err(CodexOAuthError::TokenFetchFailed(
                    "Account credentials changed, stale refresh response discarded".to_string(),
                ));
            }
            if let Some(new_refresh) = new_tokens
                .refresh_token
                .clone()
                .filter(|token| !token.trim().is_empty())
            {
                if new_refresh != account.refresh_token {
                    account.refresh_token = new_refresh;
                    needs_save = true;
                }
            }
            // The refresh uses the openid scope and normally returns a new id_token; an empty value counts as
            // missing and the old one is kept rather than overwritten (its claims still drive account/plan display).
            if let Some(new_id_token) = new_tokens
                .id_token
                .clone()
                .filter(|token| !token.trim().is_empty())
            {
                if account.id_token.as_deref() != Some(new_id_token.as_str()) {
                    account.id_token = Some(new_id_token);
                    needs_save = true;
                }
            }
            if account.token_updated_at_ms != obtained_at_ms {
                account.token_updated_at_ms = obtained_at_ms;
                needs_save = true;
            }
            (account.refresh_token.clone(), account.id_token.clone())
        };
        if needs_save {
            self.save_to_disk().await?;
        }

        let cached = CachedAccessToken {
            token: new_tokens.access_token.clone(),
            expires_at_ms: compute_expires_at_ms(new_tokens.expires_in),
            obtained_at_ms,
        };

        let last_refresh = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(obtained_at_ms)
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
        let refreshed_auth = crate::codex_config::codex_managed_oauth_auth_value(
            account_id,
            &cached.token,
            stored_id_token.as_deref(),
            &stored_refresh_token,
            &last_refresh,
        );
        if let Err(err) = crate::codex_config::sync_codex_managed_oauth_live_auth_after_refresh(
            account_id,
            &refresh_token,
            &refreshed_auth,
        ) {
            // The manager token remains valid; a later provider write will
            // retry the live synchronization without rolling it back.
            log::warn!(
                "[CodexOAuth] failed to sync the refreshed Codex live auth (account={account_id}): {err}"
            );
        }

        // Confirm the account still exists under the accounts read lock before writing the cache: this is
        // mutually exclusive with remove/clear (which hold the accounts write lock and clear the cache atomically), so a deleted account's token can never be written back.
        {
            let accounts = self.accounts.read().await;
            if !accounts.contains_key(account_id) {
                return Err(CodexOAuthError::AccountNotFound(account_id.to_string()));
            }
            let mut tokens = self.access_tokens.write().await;
            tokens.insert(account_id.to_string(), cached.clone());
        }

        Ok(cached)
    }

    /// Returns a valid access_token and id_token for the account, refreshing when needed
    ///
    /// The id_token keeps the managed Codex auth.json tokens field shaped like a native browser login
    /// (used only by the managed binding path). Legacy accounts without an id_token return `None`,
    /// which the frontend uses to prompt for a new login.
    pub async fn get_valid_token_and_id_token_for_account(
        &self,
        account_id: &str,
    ) -> Result<(String, Option<String>), CodexOAuthError> {
        let bundle = self.get_valid_token_bundle_for_account(account_id).await?;
        Ok((bundle.access_token, bundle.id_token))
    }

    /// Returns the complete refreshable token bundle needed to write a managed Codex `auth.json`
    /// (access_token + id_token + refresh_token).
    ///
    /// Unlike returning only an access_token, the auth.json written for the Codex CLI must carry a
    /// refresh_token, otherwise the CLI cannot refresh itself once the access_token expires (see the
    /// managed direct-connection scenario of running codex standalone).
    pub(crate) async fn get_valid_token_bundle_for_account(
        &self,
        account_id: &str,
    ) -> Result<ManagedTokenBundle, CodexOAuthError> {
        let _lifecycle = self.lifecycle_lock.read().await;
        let refresh_lock = self.get_refresh_lock(account_id).await;
        let _refresh_guard = refresh_lock.lock().await;

        // Resolve and read every persistent token field while holding the same
        // account generation lock. Otherwise an adoption between these reads
        // can create an invalid A0 + R1/ID1 mixed bundle.
        let cached = self
            .resolve_valid_cached_token_under_lock(account_id)
            .await?;

        // A managed bundle is about to overwrite auth.json. Re-read under the
        // same manager generation lock after token resolution so an ambiguous
        // same-account disk generation can never be hidden by a valid cached
        // access token. Keeping this check after resolution also preserves the
        // RefreshTokenInvalid recovery path: the server may disprove manager R0,
        // force-adopt disk R1, and only then produce a safe bundle.
        if let Some((live_refresh, live_id_token, live_last_refresh_ms)) =
            crate::codex_config::read_codex_live_auth_refresh_for_account(account_id)
        {
            let outcome = self
                .adopt_account_refresh_token_under_lock(
                    account_id,
                    live_refresh,
                    live_id_token,
                    live_last_refresh_ms,
                    RefreshTokenAdoptionMode::TimestampChecked,
                )
                .await?;
            match outcome {
                RefreshTokenAdoptionOutcome::Synchronized { .. }
                | RefreshTokenAdoptionOutcome::ProvablyOlder => {}
                RefreshTokenAdoptionOutcome::Ambiguous => {
                    return Err(Self::ambiguous_live_refresh_error(account_id));
                }
                RefreshTokenAdoptionOutcome::Adopted => {
                    return Err(CodexOAuthError::TokenFetchFailed(format!(
                        "The on-disk credentials of Codex CLI account {account_id} were refreshed while preparing to write; the operation was cancelled to avoid writing a mixed token bundle, please retry"
                    )));
                }
                RefreshTokenAdoptionOutcome::NotManaged => {
                    return Err(CodexOAuthError::AccountNotFound(account_id.to_string()));
                }
            }
        }
        let last_refresh =
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(cached.obtained_at_ms)
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
        let (id_token, refresh_token) = {
            let accounts = self.accounts.read().await;
            let account = accounts
                .get(account_id)
                .ok_or_else(|| CodexOAuthError::AccountNotFound(account_id.to_string()))?;
            (account.id_token.clone(), account.refresh_token.clone())
        };
        Ok(ManagedTokenBundle {
            access_token: cached.token,
            id_token,
            refresh_token,
            last_refresh,
        })
    }

    /// Adopts (reads back) the refresh_token / id_token rotated by the Codex CLI.
    ///
    /// After a managed account writes a full bundle into auth.json, the Codex CLI refreshes on its own
    /// and writes the new refresh_token back. Call this before switching back to that provider so the
    /// newest on-disk refresh_token is adopted and a stale token cannot overwrite the CLI's valid login.
    ///
    /// Updates and persists only when the account really is managed here and the value really changed; returns whether it updated.
    pub async fn adopt_account_refresh_token(
        &self,
        account_id: &str,
        refresh_token: String,
        id_token: Option<String>,
        last_refresh_ms: Option<i64>,
    ) -> Result<bool, CodexOAuthError> {
        let _lifecycle = self.lifecycle_lock.read().await;
        let refresh_token = refresh_token.trim().to_string();
        if refresh_token.is_empty() {
            return Ok(false);
        }
        // Serialized with that account's refresh: if one is in flight holding the old refresh_token, this
        // stops its result from overwriting the CLI rotation we just adopted.
        let refresh_lock = self.get_refresh_lock(account_id).await;
        let _guard = refresh_lock.lock().await;
        self.adopt_account_refresh_token_under_lock(
            account_id,
            refresh_token,
            id_token,
            last_refresh_ms,
            RefreshTokenAdoptionMode::TimestampChecked,
        )
        .await
        .map(RefreshTokenAdoptionOutcome::state_changed)
    }

    fn ambiguous_live_refresh_error(account_id: &str) -> CodexOAuthError {
        CodexOAuthError::TokenFetchFailed(format!(
            "The on-disk credentials of Codex CLI account {account_id} changed, but which refresh token is newer cannot be determined safely; the operation was cancelled to avoid overwriting or deleting a valid login. Please sign in to this account again in the auth center, and if that still fails, remove it and log in again"
        ))
    }

    /// Reconcile the same-account Codex CLI refresh generation before a
    /// provider transaction overwrites or removes live auth.json.
    ///
    /// Returns the exact refresh token observed on disk. Callers must compare
    /// it again immediately before their live write/delete; the external Codex
    /// CLI does not participate in cc-switch's switch lock and may refresh in
    /// the adopt-to-write window.
    pub(crate) async fn prepare_live_auth_for_account_switch_away(
        &self,
        account_id: &str,
    ) -> Result<Option<String>, CodexOAuthError> {
        let Some((live_refresh, live_id_token, live_last_refresh_ms)) =
            crate::codex_config::read_codex_live_auth_refresh_for_account(account_id)
        else {
            return Ok(None);
        };

        let _lifecycle = self.lifecycle_lock.read().await;
        let refresh_lock = self.get_refresh_lock(account_id).await;
        let _guard = refresh_lock.lock().await;
        {
            let accounts = self.accounts.read().await;
            accounts
                .get(account_id)
                .ok_or_else(|| CodexOAuthError::AccountNotFound(account_id.to_string()))?;
        }

        let outcome = self
            .adopt_account_refresh_token_under_lock(
                account_id,
                live_refresh.clone(),
                live_id_token,
                live_last_refresh_ms,
                RefreshTokenAdoptionMode::TimestampChecked,
            )
            .await?;

        match outcome {
            RefreshTokenAdoptionOutcome::Synchronized { .. }
            | RefreshTokenAdoptionOutcome::Adopted
            | RefreshTokenAdoptionOutcome::ProvablyOlder => Ok(Some(live_refresh)),
            RefreshTokenAdoptionOutcome::Ambiguous => {
                Err(Self::ambiguous_live_refresh_error(account_id))
            }
            RefreshTokenAdoptionOutcome::NotManaged => {
                Err(CodexOAuthError::AccountNotFound(account_id.to_string()))
            }
        }
    }

    /// Same as `adopt_account_refresh_token`, for callers already holding the
    /// per-account refresh lock.
    async fn adopt_account_refresh_token_under_lock(
        &self,
        account_id: &str,
        refresh_token: String,
        id_token: Option<String>,
        last_refresh_ms: Option<i64>,
        mode: RefreshTokenAdoptionMode,
    ) -> Result<RefreshTokenAdoptionOutcome, CodexOAuthError> {
        let incoming_id_token = id_token.filter(|token| !token.trim().is_empty());
        let mut changed = false;
        let mut material_replaced = false;
        let mut outcome;
        {
            let mut accounts = self.accounts.write().await;
            let Some(account) = accounts.get_mut(account_id) else {
                // Not an account managed here: do not take it over and do not persist.
                return Ok(RefreshTokenAdoptionOutcome::NotManaged);
            };

            // A manager refresh may already have advanced the token generation
            // while auth.json still contains the older one. Never roll that
            // state back during the preflight/write double-build sequence.
            let refresh_changed = account.refresh_token != refresh_token;
            let id_token_changed = incoming_id_token
                .as_ref()
                .is_some_and(|token| account.id_token.as_deref() != Some(token.as_str()));
            let material_changed = refresh_changed || id_token_changed;
            let manager_was_undated = account.token_updated_at_ms <= 0;
            // Once the manager has a dated generation, any different token
            // material must carry a *strictly newer* live timestamp. Equality is
            // ambiguous at millisecond precision and therefore cannot authorize
            // replacing the manager generation either. Stores upgraded from
            // before generation timestamps existed keep a different live
            // generation ambiguous across retries; only matching material may
            // establish a timestamp. The server-rejected mode is the sole
            // exception because it has disproved the manager generation.
            let observed_order =
                last_refresh_ms.map(|observed| observed.cmp(&account.token_updated_at_ms));
            let should_adopt = material_changed
                && (matches!(mode, RefreshTokenAdoptionMode::RejectedManagerToken)
                    || (!manager_was_undated
                        && matches!(observed_order, Some(std::cmp::Ordering::Greater))));

            if !material_changed {
                outcome = RefreshTokenAdoptionOutcome::Synchronized {
                    state_changed: false,
                };
            } else if should_adopt {
                if refresh_changed {
                    account.refresh_token = refresh_token;
                    changed = true;
                    material_replaced = true;
                }
                if let Some(id_token) = incoming_id_token {
                    if account.id_token.as_deref() != Some(id_token.as_str()) {
                        account.id_token = Some(id_token);
                        changed = true;
                        material_replaced = true;
                    }
                }
                outcome = RefreshTokenAdoptionOutcome::Adopted;
            } else if !manager_was_undated
                && matches!(observed_order, Some(std::cmp::Ordering::Less))
            {
                outcome = RefreshTokenAdoptionOutcome::ProvablyOlder;
            } else {
                outcome = RefreshTokenAdoptionOutcome::Ambiguous;
            }

            if matches!(outcome, RefreshTokenAdoptionOutcome::Adopted)
                && matches!(mode, RefreshTokenAdoptionMode::RejectedManagerToken)
            {
                let adopted_at = last_refresh_ms
                    .filter(|observed| *observed > account.token_updated_at_ms)
                    .unwrap_or_else(|| {
                        chrono::Utc::now()
                            .timestamp_millis()
                            .max(account.token_updated_at_ms.saturating_add(1))
                    });
                if account.token_updated_at_ms != adopted_at {
                    account.token_updated_at_ms = adopted_at;
                    changed = true;
                }
            } else if matches!(outcome, RefreshTokenAdoptionOutcome::Adopted) {
                if let Some(observed) = last_refresh_ms {
                    if account.token_updated_at_ms != observed {
                        account.token_updated_at_ms = observed;
                        changed = true;
                    }
                }
            } else if matches!(outcome, RefreshTokenAdoptionOutcome::Synchronized { .. }) {
                if manager_was_undated {
                    // Matching material establishes one generation, so dating
                    // it cannot turn an unresolved R0/R1 conflict into a false
                    // "live is older" decision on the next retry.
                    account.token_updated_at_ms = last_refresh_ms
                        .filter(|observed| *observed > 0)
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                    changed = true;
                } else if let Some(observed) = last_refresh_ms {
                    if observed > account.token_updated_at_ms {
                        account.token_updated_at_ms = observed;
                        changed = true;
                    }
                }
            }
            // Having adopted the CLI's rotated refresh_token, the matching old access_token may already have
            // been invalidated by the server. Clear the cache inside the same accounts write lock (accounts
            // then access_tokens) so the fast path cannot read the old token after release; the next fetch uses the new refresh_token.
            if material_replaced {
                self.access_tokens.write().await.remove(account_id);
            }

            if let RefreshTokenAdoptionOutcome::Synchronized { .. } = outcome {
                outcome = RefreshTokenAdoptionOutcome::Synchronized {
                    state_changed: changed,
                };
            }
        }
        if changed {
            self.save_to_disk().await?;
        }
        Ok(outcome)
    }

    /// Returns a valid token for the default account
    pub async fn get_valid_token(&self) -> Result<String, CodexOAuthError> {
        match self.resolve_default_account_id().await {
            Some(id) => self.get_valid_token_for_account(&id).await,
            None => Err(CodexOAuthError::AccountNotFound(
                "No ChatGPT account available".to_string(),
            )),
        }
    }

    /// Returns the default account ID (for hot paths, avoiding a clone of the whole account map)
    pub async fn default_account_id(&self) -> Option<String> {
        self.resolve_default_account_id().await
    }

    // ==================== Multi-account management ====================

    pub async fn list_accounts(&self) -> Vec<GitHubAccount> {
        let accounts = self.accounts.read().await.clone();
        let default_id = self.resolve_default_account_id().await;
        Self::sorted_accounts(&accounts, default_id.as_deref())
    }

    pub async fn remove_account(&self, account_id: &str) -> Result<(), CodexOAuthError> {
        log::info!("[CodexOAuth] removing account: {account_id}");
        // Wait for all in-flight refresh/adopt operations before deleting. New
        // token work is blocked until the account, cache, lock and disk state
        // have been removed as one lifecycle transition.
        let _lifecycle = self.lifecycle_lock.write().await;

        {
            let accounts = self.accounts.read().await;
            if !accounts.contains_key(account_id) {
                return Err(CodexOAuthError::AccountNotFound(account_id.to_string()));
            }
        }

        // Explicit Auth Center removal means credentials for this managed
        // account must leave the machine. Content matching intentionally also
        // claims a native `codex login` of the same account; that is the same
        // account-scoped credential the user just chose to remove.
        crate::codex_config::clear_codex_live_auth_for_managed_account(account_id)
            .map_err(|error| CodexOAuthError::IoError(error.to_string()))?;

        {
            // Clear this account's token cache atomically inside the accounts write lock (accounts then
            // access_tokens), so there is no window where the account is gone but its cache remains.
            let mut accounts = self.accounts.write().await;
            accounts.remove(account_id);
            self.access_tokens.write().await.remove(account_id);
        }
        {
            let mut locks = self.refresh_locks.write().await;
            locks.remove(account_id);
        }

        {
            let accounts = self.accounts.read().await;
            let mut default = self.default_account_id.write().await;
            if default.as_deref() == Some(account_id) {
                *default = Self::fallback_default_account_id(&accounts);
            }
        }

        self.save_to_disk().await?;
        Ok(())
    }

    pub async fn set_default_account(&self, account_id: &str) -> Result<(), CodexOAuthError> {
        let _lifecycle = self.lifecycle_lock.read().await;
        {
            let accounts = self.accounts.read().await;
            if !accounts.contains_key(account_id) {
                return Err(CodexOAuthError::AccountNotFound(account_id.to_string()));
            }
        }

        {
            let mut default = self.default_account_id.write().await;
            *default = Some(account_id.to_string());
        }

        self.save_to_disk().await?;
        Ok(())
    }

    pub async fn clear_auth(&self) -> Result<(), CodexOAuthError> {
        log::info!("[CodexOAuth] clearing all authentication");

        // Acquire lifecycle before storage. Refresh follows lifecycle(read) ->
        // account mutex -> storage, so this fixed order cannot deadlock and the
        // write guard guarantees no refresh can recreate live/disk state after
        // the clear has committed.
        let _lifecycle = self.lifecycle_lock.write().await;

        let account_ids = self
            .accounts
            .read()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for account_id in &account_ids {
            crate::codex_config::clear_codex_live_auth_for_managed_account(account_id)
                .map_err(|error| CodexOAuthError::IoError(error.to_string()))?;
        }

        // Shares the persistence lock with save_to_disk so clearing memory and deleting the file are
        // atomic against a concurrent save, which could otherwise revive cleared accounts from a stale snapshot.
        let _persist = self.storage_lock.lock().await;

        {
            // Clear accounts and the token cache atomically inside the accounts write lock (accounts then
            // access_tokens), ruling out both a lingering cache after clearing and a concurrent refresh refilling it.
            let mut accounts = self.accounts.write().await;
            accounts.clear();
            self.access_tokens.write().await.clear();
        }
        {
            let mut default = self.default_account_id.write().await;
            *default = None;
        }
        {
            let mut locks = self.refresh_locks.write().await;
            locks.clear();
        }
        {
            let mut pending = self.pending_device_codes.write().await;
            self.login_epoch.fetch_add(1, Ordering::AcqRel);
            pending.clear();
        }

        if self.storage_path.exists() {
            std::fs::remove_file(&self.storage_path)?;
        }

        Ok(())
    }

    pub async fn is_authenticated(&self) -> bool {
        let accounts = self.accounts.read().await;
        !accounts.is_empty()
    }

    /// Returns an auth status summary (same shape as Copilot's, so the frontend can be reused)
    pub async fn get_status(&self) -> CodexOAuthStatus {
        let accounts_map = self.accounts.read().await.clone();
        let default_id = self.resolve_default_account_id().await;
        let account_list = Self::sorted_accounts(&accounts_map, default_id.as_deref());
        let authenticated = !account_list.is_empty();
        let username = default_id
            .as_ref()
            .and_then(|id| accounts_map.get(id))
            .and_then(|a| a.email.clone())
            .or_else(|| account_list.first().map(|a| a.login.clone()));

        CodexOAuthStatus {
            accounts: account_list,
            default_account_id: default_id,
            authenticated,
            username,
        }
    }

    #[cfg(test)]
    pub(crate) async fn add_test_account_with_access_token(
        &self,
        account_id: &str,
        access_token: &str,
        id_token: Option<&str>,
    ) -> Result<(), CodexOAuthError> {
        let obtained_at_ms = chrono::Utc::now().timestamp_millis();
        self.add_account_internal(
            account_id.to_string(),
            "test-refresh-token".to_string(),
            Some(format!("{account_id}@example.test")),
            id_token.map(|token| token.to_string()),
            Some(CachedAccessToken {
                token: access_token.to_string(),
                expires_at_ms: obtained_at_ms + 3_600_000,
                obtained_at_ms,
            }),
            None,
        )
        .await?;

        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn test_refresh_token_for_account(&self, account_id: &str) -> Option<String> {
        self.accounts
            .read()
            .await
            .get(account_id)
            .map(|account| account.refresh_token.clone())
    }

    #[cfg(test)]
    pub(crate) async fn test_set_token_updated_at_ms(
        &self,
        account_id: &str,
        token_updated_at_ms: i64,
    ) {
        self.accounts
            .write()
            .await
            .get_mut(account_id)
            .expect("test account present")
            .token_updated_at_ms = token_updated_at_ms;
    }

    // ==================== Internal helpers ====================

    async fn add_account_internal(
        &self,
        account_id: String,
        refresh_token: String,
        email: Option<String>,
        id_token: Option<String>,
        initial_access_token: Option<CachedAccessToken>,
        pending_device_code: Option<&str>,
    ) -> Result<GitHubAccount, CodexOAuthError> {
        let _lifecycle = self.lifecycle_lock.read().await;
        if let Some(device_code) = pending_device_code {
            // `clear_auth` owns lifecycle(write) while clearing pending flows.
            // Re-check under lifecycle(read) at commit time so a poll that was
            // already on the network cannot recreate an account after clear.
            if self
                .pending_device_codes
                .write()
                .await
                .remove(device_code)
                .is_none()
            {
                return Err(CodexOAuthError::ExpiredToken);
            }
        }
        let refresh_lock = self.get_refresh_lock(&account_id).await;
        let _refresh_guard = refresh_lock.lock().await;
        let now = chrono::Utc::now().timestamp();
        let now_ms = chrono::Utc::now().timestamp_millis();

        let data = CodexAccountData {
            account_id: account_id.clone(),
            email,
            refresh_token,
            authenticated_at: now,
            id_token,
            token_updated_at_ms: now_ms,
        };

        let account = GitHubAccount::from(&data);

        {
            let mut accounts = self.accounts.write().await;
            accounts.insert(account_id.clone(), data);
            let mut access_tokens = self.access_tokens.write().await;
            if let Some(cached) = initial_access_token {
                access_tokens.insert(account_id.clone(), cached);
            } else {
                access_tokens.remove(&account_id);
            }
        }

        {
            let mut default = self.default_account_id.write().await;
            if default.is_none() {
                *default = Some(account_id);
            }
        }

        self.save_to_disk().await?;
        Ok(account)
    }

    fn fallback_default_account_id(accounts: &HashMap<String, CodexAccountData>) -> Option<String> {
        accounts
            .iter()
            .max_by(|(id_a, a), (id_b, b)| {
                a.authenticated_at
                    .cmp(&b.authenticated_at)
                    .then_with(|| id_b.cmp(id_a))
            })
            .map(|(id, _)| id.clone())
    }

    fn sorted_accounts(
        accounts: &HashMap<String, CodexAccountData>,
        default_account_id: Option<&str>,
    ) -> Vec<GitHubAccount> {
        let mut list: Vec<GitHubAccount> = accounts.values().map(GitHubAccount::from).collect();
        list.sort_by(|a, b| {
            let a_default = default_account_id == Some(a.id.as_str());
            let b_default = default_account_id == Some(b.id.as_str());
            b_default
                .cmp(&a_default)
                .then_with(|| b.authenticated_at.cmp(&a.authenticated_at))
                .then_with(|| a.login.cmp(&b.login))
        });
        list
    }

    async fn resolve_default_account_id(&self) -> Option<String> {
        let stored = self.default_account_id.read().await.clone();
        let accounts = self.accounts.read().await;

        if let Some(id) = stored {
            if accounts.contains_key(&id) {
                return Some(id);
            }
        }

        Self::fallback_default_account_id(&accounts)
    }

    async fn get_refresh_lock(&self, account_id: &str) -> Arc<Mutex<()>> {
        {
            let locks = self.refresh_locks.read().await;
            if let Some(lock) = locks.get(account_id) {
                return Arc::clone(lock);
            }
        }

        let mut locks = self.refresh_locks.write().await;
        Arc::clone(
            locks
                .entry(account_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    fn write_store_atomic(&self, content: &str) -> Result<(), CodexOAuthError> {
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let parent = self
            .storage_path
            .parent()
            .ok_or_else(|| CodexOAuthError::IoError("Invalid storage path".to_string()))?;
        let file_name = self
            .storage_path
            .file_name()
            .ok_or_else(|| CodexOAuthError::IoError("Invalid storage file name".to_string()))?
            .to_string_lossy()
            .to_string();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp_path = parent.join(format!("{file_name}.tmp.{ts}"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&tmp_path)?;
            file.write_all(content.as_bytes())?;
            file.flush()?;

            fs::rename(&tmp_path, &self.storage_path)?;
            fs::set_permissions(&self.storage_path, fs::Permissions::from_mode(0o600))?;
        }

        #[cfg(windows)]
        {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&tmp_path)?;
            file.write_all(content.as_bytes())?;
            file.flush()?;

            if self.storage_path.exists() {
                let _ = fs::remove_file(&self.storage_path);
            }
            fs::rename(&tmp_path, &self.storage_path)?;
        }

        Ok(())
    }

    fn load_from_disk_sync(&self) -> Result<(), CodexOAuthError> {
        if !self.storage_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.storage_path)?;
        let store: CodexOAuthStore = serde_json::from_str(&content)
            .map_err(|e| CodexOAuthError::ParseError(e.to_string()))?;

        if let Ok(mut accounts) = self.accounts.try_write() {
            *accounts = store.accounts;
            log::info!("[CodexOAuth] loaded {} accounts from disk", accounts.len());
        }
        if let Ok(mut default) = self.default_account_id.try_write() {
            *default = store.default_account_id;
            if default.is_none() {
                if let Ok(accounts) = self.accounts.try_read() {
                    *default = Self::fallback_default_account_id(&accounts);
                }
            }
        }

        Ok(())
    }

    async fn save_to_disk(&self) -> Result<(), CodexOAuthError> {
        // Serialize snapshot plus write: the snapshot is taken inside the persistence lock so a concurrent
        // save or clear cannot overwrite with a stale snapshot and revive deleted accounts.
        let _persist = self.storage_lock.lock().await;
        let accounts = self.accounts.read().await.clone();
        let default = self.resolve_default_account_id().await;

        let store = CodexOAuthStore {
            version: 1,
            accounts,
            default_account_id: default,
        };

        let content = serde_json::to_string_pretty(&store)
            .map_err(|e| CodexOAuthError::ParseError(e.to_string()))?;

        self.write_store_atomic(&content)?;

        log::info!(
            "[CodexOAuth] saved to disk ({} accounts)",
            store.accounts.len()
        );

        Ok(())
    }
}

/// Codex OAuth status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexOAuthStatus {
    pub accounts: Vec<GitHubAccount>,
    pub default_account_id: Option<String>,
    pub authenticated: bool,
    pub username: Option<String>,
}

// ==================== Utility functions ====================

/// Parses the interval field of an OpenAI device-code response
///
/// The server may return a string or a number, so both are accepted
fn parse_interval(value: Option<&serde_json::Value>) -> u64 {
    let raw = match value {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(5),
        Some(serde_json::Value::String(s)) => s.parse::<u64>().unwrap_or(5),
        _ => 5,
    };
    raw.max(1) + POLLING_SAFETY_MARGIN_SECS
}

/// Computes the expiry timestamp in milliseconds from expires_in (seconds)
fn compute_expires_at_ms(expires_in: Option<i64>) -> i64 {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let secs = expires_in.unwrap_or(3600);
    now_ms + secs * 1000
}

fn extract_refresh_error_code(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("error")
        .and_then(|error| match error {
            serde_json::Value::Object(object) => object.get("code").and_then(|code| code.as_str()),
            serde_json::Value::String(code) => Some(code.as_str()),
            _ => None,
        })
        .or_else(|| value.get("code").and_then(|code| code.as_str()))
        .map(|code| code.to_ascii_lowercase())
}

/// Parses the claims out of a JWT
fn parse_jwt_claims(token: &str) -> Option<IdTokenClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    serde_json::from_slice(&decoded).ok()
}

/// Extracts (account_id, email) from a token response
fn extract_identity_from_tokens(tokens: &OAuthTokenResponse) -> (Option<String>, Option<String>) {
    let mut account_id: Option<String> = None;
    let mut email: Option<String> = None;

    if let Some(id_token) = tokens.id_token.as_deref() {
        if let Some(claims) = parse_jwt_claims(id_token) {
            account_id = claims
                .chatgpt_account_id
                .clone()
                .or_else(|| {
                    claims
                        .openai_auth
                        .as_ref()
                        .and_then(|a| a.chatgpt_account_id.clone())
                })
                .or_else(|| claims.organizations.first().and_then(|o| o.id.clone()));
            email = claims.email.clone();
        }
    }

    if account_id.is_none() {
        if let Some(claims) = parse_jwt_claims(&tokens.access_token) {
            account_id = claims
                .chatgpt_account_id
                .clone()
                .or_else(|| {
                    claims
                        .openai_auth
                        .as_ref()
                        .and_then(|a| a.chatgpt_account_id.clone())
                })
                .or_else(|| claims.organizations.first().and_then(|o| o.id.clone()));
            if email.is_none() {
                email = claims.email.clone();
            }
        }
    }

    (account_id, email)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_interval_number() {
        let v = serde_json::Value::Number(serde_json::Number::from(5));
        assert_eq!(parse_interval(Some(&v)), 5 + POLLING_SAFETY_MARGIN_SECS);
    }

    #[test]
    fn test_parse_interval_string() {
        let v = serde_json::Value::String("10".to_string());
        assert_eq!(parse_interval(Some(&v)), 10 + POLLING_SAFETY_MARGIN_SECS);
    }

    #[test]
    fn test_parse_interval_default() {
        assert_eq!(parse_interval(None), 5 + POLLING_SAFETY_MARGIN_SECS);
    }

    #[test]
    fn test_parse_interval_min() {
        let v = serde_json::Value::Number(serde_json::Number::from(0));
        // 0 must be raised to 1
        assert_eq!(parse_interval(Some(&v)), 1 + POLLING_SAFETY_MARGIN_SECS);
    }

    #[test]
    fn test_compute_expires_at_ms() {
        let result = compute_expires_at_ms(Some(3600));
        let now = chrono::Utc::now().timestamp_millis();
        // Must land roughly 3600 seconds in the future (a small margin is allowed)
        assert!(result > now + 3500 * 1000);
        assert!(result < now + 3700 * 1000);
    }

    #[test]
    fn test_compute_expires_at_ms_default() {
        let result = compute_expires_at_ms(None);
        let now = chrono::Utc::now().timestamp_millis();
        assert!(result > now);
    }

    #[test]
    fn test_cached_token_expiring_soon() {
        let now = chrono::Utc::now().timestamp_millis();
        // Expires in 30 seconds - inside the refresh buffer
        let expiring = CachedAccessToken {
            token: "t".to_string(),
            expires_at_ms: now + 30_000,
            obtained_at_ms: now,
        };
        assert!(expiring.is_expiring_soon());

        // Expires in 1 hour - outside the refresh buffer
        let valid = CachedAccessToken {
            token: "t".to_string(),
            expires_at_ms: now + 3_600_000,
            obtained_at_ms: now,
        };
        assert!(!valid.is_expiring_soon());
    }

    #[test]
    fn test_parse_jwt_claims_invalid() {
        assert!(parse_jwt_claims("not-a-jwt").is_none());
        assert!(parse_jwt_claims("only.two").is_none());
    }

    #[test]
    fn test_parse_jwt_claims_valid() {
        // Header: {"alg":"none"}
        // Payload: {"chatgpt_account_id":"acc-123","email":"test@example.com"}
        // Signature: empty
        let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\"}");
        let payload = URL_SAFE_NO_PAD
            .encode(b"{\"chatgpt_account_id\":\"acc-123\",\"email\":\"test@example.com\"}");
        let jwt = format!("{header}.{payload}.");
        let claims = parse_jwt_claims(&jwt).unwrap();
        assert_eq!(claims.chatgpt_account_id.as_deref(), Some("acc-123"));
        assert_eq!(claims.email.as_deref(), Some("test@example.com"));
    }

    #[test]
    fn test_parse_jwt_claims_organizations_fallback() {
        let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\"}");
        let payload = URL_SAFE_NO_PAD.encode(b"{\"organizations\":[{\"id\":\"org-456\"}]}");
        let jwt = format!("{header}.{payload}.");
        let claims = parse_jwt_claims(&jwt).unwrap();
        assert_eq!(
            claims
                .organizations
                .first()
                .and_then(|o| o.id.clone())
                .as_deref(),
            Some("org-456")
        );
    }

    #[tokio::test]
    async fn test_manager_initial_state() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());
        assert!(!manager.is_authenticated().await);
        assert!(manager.list_accounts().await.is_empty());
    }

    #[tokio::test]
    async fn test_manager_save_and_load() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_path_buf();

        // Manually inject an account through internal methods
        {
            let manager = CodexOAuthManager::new(path.clone());
            manager
                .add_account_internal(
                    "acc-123".to_string(),
                    "rt-secret".to_string(),
                    Some("user@example.com".to_string()),
                    None,
                    None,
                    None,
                )
                .await
                .unwrap();
        }

        // New manager should load from disk
        let manager2 = CodexOAuthManager::new(path);
        let accounts = manager2.list_accounts().await;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "acc-123");
    }

    #[tokio::test]
    async fn test_remove_account() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());

        manager
            .add_account_internal(
                "acc-123".to_string(),
                "rt".to_string(),
                Some("a@example.com".to_string()),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        manager
            .add_account_internal(
                "acc-456".to_string(),
                "rt2".to_string(),
                Some("b@example.com".to_string()),
                None,
                None,
                None,
            )
            .await
            .unwrap();

        manager.remove_account("acc-123").await.unwrap();
        let accounts = manager.list_accounts().await;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "acc-456");
    }

    #[tokio::test]
    async fn adopt_account_refresh_token_syncs_rotated_value() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());
        manager
            .add_test_account_with_access_token("acc-1", "access-cached", Some("id-1"))
            .await
            .unwrap();

        // Adopt the Codex CLI's rotated refresh_token / id_token carrying a newer last_refresh.
        let manager_updated_at = manager
            .accounts
            .read()
            .await
            .get("acc-1")
            .expect("account present")
            .token_updated_at_ms;
        let changed = manager
            .adopt_account_refresh_token(
                "acc-1",
                "rotated-rt".to_string(),
                Some("id-2".to_string()),
                Some(manager_updated_at.saturating_add(1)),
            )
            .await
            .unwrap();
        assert!(changed, "rotated refresh_token should be adopted");

        // Storage now holds the on-disk values rotated by the CLI.
        {
            let accounts = manager.accounts.read().await;
            let account = accounts.get("acc-1").expect("account present");
            assert_eq!(account.refresh_token, "rotated-rt");
            assert_eq!(account.id_token.as_deref(), Some("id-2"));
        }
        // Adoption clears the cached access_token so the next fetch uses the new refresh_token
        // (so get_valid_token_bundle_for_account is no longer asserted here; it would hit the network).
        assert!(
            !manager.access_tokens.read().await.contains_key("acc-1"),
            "adopt should invalidate the cached access token"
        );

        // Unknown accounts are not taken over.
        assert!(!manager
            .adopt_account_refresh_token("acc-unknown", "x".to_string(), None, None)
            .await
            .unwrap());

        // Identical values do not count as a change.
        assert!(!manager
            .adopt_account_refresh_token("acc-1", "rotated-rt".to_string(), None, None)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn adopt_account_refresh_token_rejects_older_live_generation() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());
        manager
            .add_test_account_with_access_token("acc-1", "access-cached", Some("id-1"))
            .await
            .unwrap();

        let manager_updated_at = manager
            .accounts
            .read()
            .await
            .get("acc-1")
            .expect("account present")
            .token_updated_at_ms;
        let changed = manager
            .adopt_account_refresh_token(
                "acc-1",
                "stale-live-refresh".to_string(),
                None,
                Some(manager_updated_at.saturating_sub(1)),
            )
            .await
            .unwrap();

        assert!(!changed, "older live state must not roll the manager back");
        assert_eq!(
            manager
                .accounts
                .read()
                .await
                .get("acc-1")
                .expect("account present")
                .refresh_token,
            "test-refresh-token"
        );
    }

    #[tokio::test]
    async fn adopt_account_refresh_token_rejects_undated_live_generation() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());
        manager
            .add_test_account_with_access_token("acc-1", "access-cached", Some("id-1"))
            .await
            .unwrap();

        let changed = manager
            .adopt_account_refresh_token("acc-1", "ambiguous-live-refresh".to_string(), None, None)
            .await
            .unwrap();

        assert!(
            !changed,
            "an undated live token must not roll back a timestamped manager generation"
        );
        assert_eq!(
            manager
                .accounts
                .read()
                .await
                .get("acc-1")
                .expect("account present")
                .refresh_token,
            "test-refresh-token"
        );
    }

    #[tokio::test]
    async fn adopt_account_refresh_token_rejects_stale_id_token_with_same_refresh() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());
        manager
            .add_test_account_with_access_token("acc-1", "access-cached", Some("id-new"))
            .await
            .unwrap();
        let manager_updated_at = manager
            .accounts
            .read()
            .await
            .get("acc-1")
            .expect("account present")
            .token_updated_at_ms;

        let changed = manager
            .adopt_account_refresh_token(
                "acc-1",
                "test-refresh-token".to_string(),
                Some("id-stale".to_string()),
                Some(manager_updated_at.saturating_sub(1)),
            )
            .await
            .unwrap();

        assert!(!changed);
        assert_eq!(
            manager
                .accounts
                .read()
                .await
                .get("acc-1")
                .expect("account present")
                .id_token
                .as_deref(),
            Some("id-new")
        );
    }

    #[tokio::test]
    async fn adopt_account_refresh_token_rejects_equal_timestamp_generation() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());
        manager
            .add_test_account_with_access_token("acc-1", "access-cached", Some("id-1"))
            .await
            .unwrap();
        let manager_updated_at = manager
            .accounts
            .read()
            .await
            .get("acc-1")
            .expect("account present")
            .token_updated_at_ms;

        let changed = manager
            .adopt_account_refresh_token(
                "acc-1",
                "same-millisecond-refresh".to_string(),
                None,
                Some(manager_updated_at),
            )
            .await
            .unwrap();

        assert!(!changed);
        assert_eq!(
            manager
                .accounts
                .read()
                .await
                .get("acc-1")
                .expect("account present")
                .refresh_token,
            "test-refresh-token"
        );
    }

    #[tokio::test]
    async fn adopt_account_refresh_token_keeps_legacy_conflict_ambiguous_across_retries() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());
        manager
            .add_test_account_with_access_token("acc-1", "access-cached", Some("id-manager"))
            .await
            .unwrap();
        manager
            .accounts
            .write()
            .await
            .get_mut("acc-1")
            .expect("account present")
            .token_updated_at_ms = 0;

        for attempt in 1..=2 {
            let changed = manager
                .adopt_account_refresh_token(
                    "acc-1",
                    "ambiguous-live-refresh".to_string(),
                    Some("id-live".to_string()),
                    Some(1_700_000_000_000),
                )
                .await
                .unwrap();
            assert!(
                !changed,
                "legacy conflict must remain unresolved on attempt {attempt}"
            );
        }

        let accounts = manager.accounts.read().await;
        let account = accounts.get("acc-1").expect("account present");
        assert_eq!(account.refresh_token, "test-refresh-token");
        assert_eq!(account.id_token.as_deref(), Some("id-manager"));
        assert_eq!(
            account.token_updated_at_ms, 0,
            "dating old manager material would make the next retry falsely classify the live token as older"
        );
        drop(accounts);
        assert!(
            manager.access_tokens.read().await.contains_key("acc-1"),
            "an unresolved conflict must not invalidate a valid access token"
        );
    }

    #[tokio::test]
    async fn rejected_manager_token_adopts_different_disk_token_without_timestamp() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());
        manager
            .add_test_account_with_access_token("acc-1", "access-cached", Some("id-manager"))
            .await
            .unwrap();

        let outcome = manager
            .adopt_account_refresh_token_under_lock(
                "acc-1",
                "recovered-live-refresh".to_string(),
                Some("id-live".to_string()),
                None,
                RefreshTokenAdoptionMode::RejectedManagerToken,
            )
            .await
            .unwrap();

        assert_eq!(outcome, RefreshTokenAdoptionOutcome::Adopted);
        let accounts = manager.accounts.read().await;
        let account = accounts.get("acc-1").expect("account present");
        assert_eq!(account.refresh_token, "recovered-live-refresh");
        assert_eq!(account.id_token.as_deref(), Some("id-live"));
        assert!(account.token_updated_at_ms > 0);
        drop(accounts);
        assert!(
            !manager.access_tokens.read().await.contains_key("acc-1"),
            "forced recovery must invalidate the cached access token"
        );
    }

    #[tokio::test]
    async fn device_commit_rejects_flow_cleared_during_network_poll() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());
        manager.pending_device_codes.write().await.insert(
            "device-auth-id".to_string(),
            PendingDeviceCode {
                user_code: "ABCD-EFGH".to_string(),
                expires_at_ms: chrono::Utc::now().timestamp_millis() + 60_000,
            },
        );

        manager.clear_auth().await.unwrap();
        let result = manager
            .add_account_internal(
                "acc-after-clear".to_string(),
                "refresh-after-clear".to_string(),
                None,
                None,
                None,
                Some("device-auth-id"),
            )
            .await;

        assert!(matches!(result, Err(CodexOAuthError::ExpiredToken)));
        assert!(manager.list_accounts().await.is_empty());
        assert!(!manager.storage_path.exists());
    }

    #[tokio::test]
    async fn device_start_rejects_flow_cleared_during_network_request() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(temp.path().to_path_buf());
        let login_epoch = manager.login_epoch.load(Ordering::Acquire);

        manager.clear_auth().await.unwrap();
        let result = manager
            .register_pending_device_code(
                "stale-device-auth-id".to_string(),
                "ABCD-EFGH".to_string(),
                chrono::Utc::now().timestamp_millis() + 60_000,
                login_epoch,
            )
            .await;

        assert!(matches!(result, Err(CodexOAuthError::ExpiredToken)));
        assert!(manager.pending_device_codes.read().await.is_empty());
    }

    #[test]
    fn refresh_error_code_accepts_openai_error_shapes() {
        assert_eq!(
            extract_refresh_error_code(r#"{"error":{"code":"refresh_token_reused"}}"#).as_deref(),
            Some("refresh_token_reused")
        );
        assert_eq!(
            extract_refresh_error_code(r#"{"error":"refresh_token_expired"}"#).as_deref(),
            Some("refresh_token_expired")
        );
        assert_eq!(
            extract_refresh_error_code(r#"{"code":"REFRESH_TOKEN_INVALIDATED"}"#).as_deref(),
            Some("refresh_token_invalidated")
        );
        assert_eq!(extract_refresh_error_code("not json"), None);
    }
}
