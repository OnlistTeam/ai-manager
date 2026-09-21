//! GitHub Copilot Authentication Module
//!
//! Implements the GitHub OAuth device code flow and Copilot token management.
//! Supports multi-account authentication; each Provider can be linked to a different GitHub account.
//!
//! ## Authentication Flow
//! 1. Start the device code flow to obtain device_code and user_code
//! 2. User completes GitHub authorization in the browser
//! 3. Poll to obtain access_token
//! 4. Use the GitHub token to obtain a Copilot token
//! 5. Automatically refresh the Copilot token (60 seconds before expiry)
//!
//! ## Multi-Account Support (v3)
//! - Each GitHub account stores its token independently
//! - Providers link to an account via meta.authBinding
//! - Automatically migrates the v1 single-account format to the v3 multi-account + default-account format

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Default GitHub domain
const DEFAULT_GITHUB_DOMAIN: &str = "github.com";

fn default_github_domain() -> String {
    DEFAULT_GITHUB_DOMAIN.to_string()
}

/// GitHub API base URL (api.github.com for github.com, {domain}/api/v3 for GHES)
fn github_api_base(domain: &str) -> String {
    if domain == DEFAULT_GITHUB_DOMAIN {
        "https://api.github.com".to_string()
    } else {
        format!("https://{domain}/api/v3")
    }
}

/// Copilot Token URL
fn copilot_token_url(domain: &str) -> String {
    format!("{}/copilot_internal/v2/token", github_api_base(domain))
}

/// GitHub User API URL
fn github_user_url(domain: &str) -> String {
    format!("{}/user", github_api_base(domain))
}

/// Copilot usage API URL
fn copilot_usage_url(domain: &str) -> String {
    format!("{}/copilot_internal/user", github_api_base(domain))
}

/// Copilot API base address (api.githubcopilot.com for github.com, copilot-api.{domain} for GHES)
fn copilot_api_base(domain: &str) -> String {
    if domain == DEFAULT_GITHUB_DOMAIN {
        "https://api.githubcopilot.com".to_string()
    } else {
        format!("https://copilot-api.{domain}")
    }
}

/// Token refresh lead time (seconds)
const TOKEN_REFRESH_BUFFER_SECONDS: i64 = 60;

/// Whether this is a GitHub Enterprise Server (not github.com)
fn is_ghes(domain: &str) -> bool {
    domain != DEFAULT_GITHUB_DOMAIN
}

/// Generates a composite account ID so user IDs from different GHES instances don't collide.
/// github.com accounts keep the original format (backward compatible); GHES accounts use the `domain:user_id` format.
fn composite_account_id(domain: &str, user_id: u64) -> String {
    if domain == DEFAULT_GITHUB_DOMAIN {
        user_id.to_string()
    } else {
        format!("{}:{}", domain, user_id)
    }
}

/// Copilot API header constants
pub const COPILOT_EDITOR_VERSION: &str = "vscode/1.110.1";
pub const COPILOT_PLUGIN_VERSION: &str = "copilot-chat/0.38.2";
pub const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.38.2";
pub const COPILOT_API_VERSION: &str = "2025-10-01";
pub const COPILOT_INTEGRATION_ID: &str = "vscode-chat";

/// Copilot usage response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotUsageResponse {
    /// Copilot plan type
    pub copilot_plan: String,
    /// Quota reset date
    pub quota_reset_date: String,
    /// Quota snapshot
    pub quota_snapshots: QuotaSnapshots,
    /// API endpoint info (used to dynamically resolve the API URL)
    #[serde(default)]
    pub endpoints: Option<CopilotEndpoints>,
}

/// Copilot API endpoint info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotEndpoints {
    /// API endpoint URL
    pub api: String,
    /// Telemetry endpoint URL
    #[serde(default)]
    pub telemetry: Option<String>,
}

/// Quota snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaSnapshots {
    /// Chat quota
    pub chat: QuotaDetail,
    /// Completions quota
    pub completions: QuotaDetail,
    /// Premium interaction quota
    pub premium_interactions: QuotaDetail,
}

/// Quota detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaDetail {
    /// Total entitlement
    pub entitlement: i64,
    /// Remaining quota
    pub remaining: i64,
    /// Remaining percentage
    pub percent_remaining: f64,
    /// Whether unlimited
    pub unlimited: bool,
}

/// Copilot available model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotModel {
    /// Model ID (used for API calls)
    pub id: String,
    /// Model display name
    pub name: String,
    /// Model vendor
    pub vendor: String,
    /// Whether shown in the model picker
    pub model_picker_enabled: bool,
}

/// Copilot Models API response
#[derive(Debug, Deserialize)]
struct CopilotModelsResponse {
    data: Vec<CopilotModelsResponseItem>,
}

/// Copilot Models API response item
#[derive(Debug, Deserialize)]
struct CopilotModelsResponseItem {
    id: String,
    name: String,
    vendor: String,
    model_picker_enabled: bool,
}

/// Copilot authentication error
#[derive(Debug, thiserror::Error)]
pub enum CopilotAuthError {
    #[error("GitHub token is invalid or expired")]
    GitHubTokenInvalid,

    #[error("Failed to fetch Copilot token: {0}")]
    CopilotTokenFetchFailed(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("User is not subscribed to Copilot")]
    NoCopilotSubscription,

    #[error("Account not found: {0}")]
    AccountNotFound(String),
}

impl From<reqwest::Error> for CopilotAuthError {
    fn from(err: reqwest::Error) -> Self {
        CopilotAuthError::NetworkError(err.to_string())
    }
}

impl From<std::io::Error> for CopilotAuthError {
    fn from(err: std::io::Error) -> Self {
        CopilotAuthError::IoError(err.to_string())
    }
}

/// GitHub device code response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubDeviceCodeResponse {
    /// Device code (used for polling)
    pub device_code: String,
    /// User code (shown to the user)
    pub user_code: String,
    /// Verification URL
    pub verification_uri: String,
    /// Expiry time (seconds)
    pub expires_in: u64,
    /// Polling interval (seconds)
    pub interval: u64,
}

/// Copilot Token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotToken {
    /// JWT Token
    pub token: String,
    /// Expiry timestamp (Unix seconds)
    pub expires_at: i64,
}

impl CopilotToken {
    /// Checks whether the token is about to expire (60 seconds ahead)
    pub fn is_expiring_soon(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.expires_at - now < TOKEN_REFRESH_BUFFER_SECONDS
    }
}

/// Copilot Token API response
#[derive(Debug, Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: i64,
}

/// GitHub user info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUser {
    pub login: String,
    pub id: u64,
    pub avatar_url: Option<String>,
}

/// GitHub account (public info returned to the frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubAccount {
    /// GitHub user ID (string form, used as the unique identifier)
    pub id: String,
    /// GitHub username
    pub login: String,
    /// Avatar URL
    pub avatar_url: Option<String>,
    /// Authentication timestamp
    pub authenticated_at: i64,
    /// GitHub domain (github.com or a GHES domain)
    #[serde(default = "default_github_domain")]
    pub github_domain: String,
    /// Whether the hosted account needs to re-authenticate to fill in missing credentials.
    /// Codex: true for legacy accounts missing a persisted id_token; always false for Copilot.
    #[serde(default)]
    pub reauth_required: bool,
}

impl From<&GitHubAccountData> for GitHubAccount {
    fn from(data: &GitHubAccountData) -> Self {
        GitHubAccount {
            id: composite_account_id(&data.github_domain, data.user.id),
            login: data.user.login.clone(),
            avatar_url: data.user.avatar_url.clone(),
            authenticated_at: data.authenticated_at,
            github_domain: data.github_domain.clone(),
            reauth_required: false,
        }
    }
}

/// Account data (internal storage structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitHubAccountData {
    /// GitHub OAuth Token
    ///
    /// Security note: this token is persisted locally to reuse the login session.
    /// The current implementation does not use the system keychain; it relies on private file permissions (0600 on Unix) for protection.
    pub github_token: String,
    /// User info
    pub user: GitHubUser,
    /// Authentication timestamp
    pub authenticated_at: i64,
    /// GitHub domain (github.com or a GHES domain)
    #[serde(default = "default_github_domain")]
    pub github_domain: String,
}

/// Persisted storage structure (v3 multi-account + default-account format)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CopilotAuthStore {
    /// Storage format version (3 = multi-account + default-account format)
    #[serde(default)]
    version: u32,
    /// Multi-account data (key = GitHub user ID)
    #[serde(default)]
    accounts: HashMap<String, GitHubAccountData>,
    /// Default account ID
    #[serde(skip_serializing_if = "Option::is_none")]
    default_account_id: Option<String>,
    /// Fields kept for compatibility with the v1 single-account format
    #[serde(skip_serializing_if = "Option::is_none")]
    github_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authenticated_at: Option<i64>,
}

/// Copilot authentication manager (supports multiple accounts)
pub struct CopilotAuthManager {
    /// All GitHub accounts (key = GitHub user ID)
    accounts: Arc<RwLock<HashMap<String, GitHubAccountData>>>,
    /// Default account ID
    default_account_id: Arc<RwLock<Option<String>>>,
    /// Per-account refresh lock, avoids concurrent refreshes hammering the GitHub API
    refresh_locks: Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>,
    /// Copilot Token cache (key = GitHub user ID, in-memory, auto-refreshed)
    copilot_tokens: Arc<RwLock<HashMap<String, CopilotToken>>>,
    /// Copilot Models cache (key = GitHub user ID, reused within the process only)
    copilot_models: Arc<RwLock<HashMap<String, Vec<CopilotModel>>>>,
    /// Copilot API endpoint cache (key = GitHub user ID, fetched from /copilot_internal/user)
    api_endpoints: Arc<RwLock<HashMap<String, String>>>,
    /// Per-account endpoint-fetch lock, avoids concurrent fetches hammering the GitHub API
    endpoint_locks: Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>,
    /// Storage path
    storage_path: PathBuf,
    /// Legacy-format token pending migration
    pending_migration: Arc<RwLock<Option<String>>>,
    /// Status message when legacy auth data migration fails
    migration_error: Arc<RwLock<Option<String>>>,
}

impl CopilotAuthManager {
    /// Creates a new authentication manager
    pub fn new(data_dir: PathBuf) -> Self {
        let storage_path = data_dir.join("copilot_auth.json");

        let manager = Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            default_account_id: Arc::new(RwLock::new(None)),
            refresh_locks: Arc::new(RwLock::new(HashMap::new())),
            copilot_tokens: Arc::new(RwLock::new(HashMap::new())),
            copilot_models: Arc::new(RwLock::new(HashMap::new())),
            api_endpoints: Arc::new(RwLock::new(HashMap::new())),
            endpoint_locks: Arc::new(RwLock::new(HashMap::new())),
            storage_path,
            pending_migration: Arc::new(RwLock::new(None)),
            migration_error: Arc::new(RwLock::new(None)),
        };

        // Try loading from disk (synchronous, no network request)
        if let Err(e) = manager.load_from_disk_sync() {
            log::warn!("[CopilotAuth] Failed to load storage: {e}");
        }

        manager
    }

    // ==================== Multi-account management methods ====================

    /// Adds a new account (internal method, called after OAuth completes)
    async fn add_account_internal(
        &self,
        github_token: String,
        user: GitHubUser,
        github_domain: String,
    ) -> Result<GitHubAccount, CopilotAuthError> {
        let account_id = composite_account_id(&github_domain, user.id);
        let now = chrono::Utc::now().timestamp();

        let account_data = GitHubAccountData {
            github_token,
            user: user.clone(),
            authenticated_at: now,
            github_domain: github_domain.clone(),
        };

        let account = GitHubAccount {
            id: account_id.clone(),
            login: user.login.clone(),
            avatar_url: user.avatar_url.clone(),
            authenticated_at: now,
            github_domain,
            reauth_required: false,
        };

        {
            let mut accounts = self.accounts.write().await;
            accounts.insert(account_id, account_data);
        }

        {
            let mut default_account_id = self.default_account_id.write().await;
            if default_account_id.is_none() {
                *default_account_id = Some(account.id.clone());
            }
        }

        self.set_migration_error(None).await;

        // Persist
        self.save_to_disk().await?;

        log::info!("[CopilotAuth] Account added successfully: {}", user.login);

        Ok(account)
    }

    // ==================== Device code flow ====================

    // ==================== Token retrieval methods ====================

    /// Gets a valid Copilot Token for the given account (auto-refreshed)
    pub async fn get_valid_token_for_account(
        &self,
        account_id: &str,
    ) -> Result<String, CopilotAuthError> {
        // Ensure migration is complete
        self.ensure_migration_complete().await?;

        // GHES accounts use the GitHub OAuth token directly, no Copilot token exchange needed
        let domain = self.get_account_domain(account_id).await;
        if is_ghes(&domain) {
            let accounts = self.accounts.read().await;
            return accounts
                .get(account_id)
                .map(|a| a.github_token.clone())
                .ok_or_else(|| CopilotAuthError::AccountNotFound(account_id.to_string()));
        }

        // Check the cached token
        {
            let tokens = self.copilot_tokens.read().await;
            if let Some(copilot_token) = tokens.get(account_id) {
                if !copilot_token.is_expiring_soon() {
                    return Ok(copilot_token.token.clone());
                }
            }
        }

        // Needs refresh
        log::info!("[CopilotAuth] Copilot Token for account {account_id} needs refreshing");

        let refresh_lock = self.get_refresh_lock(account_id).await;
        let _refresh_guard = refresh_lock.lock().await;

        // double-check: another request may have completed the refresh while we waited for the lock
        {
            let tokens = self.copilot_tokens.read().await;
            if let Some(copilot_token) = tokens.get(account_id) {
                if !copilot_token.is_expiring_soon() {
                    return Ok(copilot_token.token.clone());
                }
            }
        }

        // Get the account's GitHub token
        let (github_token, domain) = {
            let accounts = self.accounts.read().await;
            let account = accounts
                .get(account_id)
                .ok_or_else(|| CopilotAuthError::AccountNotFound(account_id.to_string()))?;
            (account.github_token.clone(), account.github_domain.clone())
        };

        // Refresh the Copilot token
        self.fetch_copilot_token_with_github_token(&github_token, account_id, &domain)
            .await?;

        // Return the new token
        let tokens = self.copilot_tokens.read().await;
        tokens.get(account_id).map(|t| t.token.clone()).ok_or(
            CopilotAuthError::CopilotTokenFetchFailed("Still no token after refresh".to_string()),
        )
    }

    /// Gets a valid Copilot Token (backward compatible: uses the first account)
    pub async fn get_valid_token(&self) -> Result<String, CopilotAuthError> {
        // Ensure migration is complete
        self.ensure_migration_complete().await?;

        match self.resolve_default_account_id().await {
            Some(id) => self.get_valid_token_for_account(&id).await,
            None => Err(CopilotAuthError::GitHubTokenInvalid),
        }
    }

    // ==================== Models and usage ====================

    /// Gets the list of Copilot models available to the given account
    pub async fn fetch_models_for_account(
        &self,
        account_id: &str,
    ) -> Result<Vec<CopilotModel>, CopilotAuthError> {
        self.ensure_migration_complete().await?;

        {
            let models = self.copilot_models.read().await;
            if let Some(cached) = models.get(account_id) {
                return Ok(cached.clone());
            }
        }

        let models = self.fetch_models_for_account_uncached(account_id).await?;
        {
            let mut cache = self.copilot_models.write().await;
            cache.insert(account_id.to_string(), models.clone());
        }
        Ok(models)
    }

    async fn fetch_models_for_account_uncached(
        &self,
        account_id: &str,
    ) -> Result<Vec<CopilotModel>, CopilotAuthError> {
        let copilot_token = self.get_valid_token_for_account(account_id).await?;

        // Uses get_api_endpoint() to dynamically resolve the Copilot API base URL.
        // For github.com accounts, this queries /copilot_internal/user for the endpoints.api field.
        // For GHES accounts, /copilot_internal/user may not return endpoints — in that case
        // get_api_endpoint() falls back to copilot_api_base(&domain), matching the previous
        // static URL concatenation. This fallback behavior is safe and expected.
        let api_base = self.get_api_endpoint(account_id).await;
        let models_url = format!("{}/models", api_base);

        log::info!("[CopilotAuth] Fetching Copilot models available to account {account_id}");

        let response = crate::proxy::http_client::get()
            .get(&models_url)
            .header("Authorization", format!("Bearer {copilot_token}"))
            .header("Content-Type", "application/json")
            .header("copilot-integration-id", "vscode-chat")
            .header("editor-version", COPILOT_EDITOR_VERSION)
            .header("editor-plugin-version", COPILOT_PLUGIN_VERSION)
            .header("user-agent", COPILOT_USER_AGENT)
            .header("x-github-api-version", COPILOT_API_VERSION)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(CopilotAuthError::CopilotTokenFetchFailed(format!(
                "Failed to fetch model list: {status} - {text}"
            )));
        }

        let models_response: CopilotModelsResponse = response
            .json()
            .await
            .map_err(|e| CopilotAuthError::ParseError(e.to_string()))?;

        let models: Vec<CopilotModel> = models_response
            .data
            .into_iter()
            .filter(|m| m.model_picker_enabled)
            .map(|m| CopilotModel {
                id: m.id,
                name: m.name,
                vendor: m.vendor,
                model_picker_enabled: m.model_picker_enabled,
            })
            .collect();

        log::info!("[CopilotAuth] Fetched {} available models", models.len());

        Ok(models)
    }

    pub async fn get_model_vendor_for_account(
        &self,
        account_id: &str,
        model_id: &str,
    ) -> Result<Option<String>, CopilotAuthError> {
        let models = self.fetch_models_for_account(account_id).await?;
        Ok(models
            .into_iter()
            .find(|model| model.id == model_id)
            .map(|model| model.vendor))
    }

    /// Gets the list of available Copilot models (backward compatible: uses the first account)
    pub async fn fetch_models(&self) -> Result<Vec<CopilotModel>, CopilotAuthError> {
        match self.resolve_default_account_id().await {
            Some(id) => self.fetch_models_for_account(&id).await,
            None => Err(CopilotAuthError::GitHubTokenInvalid),
        }
    }

    pub async fn get_model_vendor(
        &self,
        model_id: &str,
    ) -> Result<Option<String>, CopilotAuthError> {
        match self.resolve_default_account_id().await {
            Some(id) => self.get_model_vendor_for_account(&id, model_id).await,
            None => Err(CopilotAuthError::GitHubTokenInvalid),
        }
    }

    // ==================== Status queries ====================

    /// Gets the API endpoint for the given account (returns directly on cache hit, otherwise lazily fetches from the API)
    pub async fn get_api_endpoint(&self, account_id: &str) -> String {
        let _ = self.ensure_migration_complete().await;

        {
            let endpoints = self.api_endpoints.read().await;
            if let Some(endpoint) = endpoints.get(account_id) {
                return endpoint.clone();
            }
        }

        // Serialize concurrent fetches for the same account with a lock, avoiding duplicate requests to the GitHub API
        let lock = self.get_endpoint_lock(account_id).await;
        let _guard = lock.lock().await;

        // Double-check after acquiring the lock: another request may have already filled it in
        {
            let endpoints = self.api_endpoints.read().await;
            if let Some(endpoint) = endpoints.get(account_id) {
                return endpoint.clone();
            }
        }

        match self.fetch_and_cache_endpoint(account_id).await {
            Ok(endpoint) => endpoint,
            Err(e) => {
                log::debug!(
                    "[CopilotAuth] Failed to fetch dynamic API endpoint for account {account_id}: {e}, using the default"
                );
                let domain = self.get_account_domain(account_id).await;
                copilot_api_base(&domain)
            }
        }
    }

    /// Gets the API endpoint for the default account
    pub async fn get_default_api_endpoint(&self) -> String {
        let _ = self.ensure_migration_complete().await;

        match self.resolve_default_account_id().await {
            Some(id) => self.get_api_endpoint(&id).await,
            None => {
                // Fall back to the default github.com endpoint when there's no account
                copilot_api_base(DEFAULT_GITHUB_DOMAIN)
            }
        }
    }

    async fn fetch_and_cache_endpoint(&self, account_id: &str) -> Result<String, CopilotAuthError> {
        let (github_token, domain) = {
            let accounts = self.accounts.read().await;
            let account = accounts
                .get(account_id)
                .ok_or_else(|| CopilotAuthError::AccountNotFound(account_id.to_string()))?;
            (account.github_token.clone(), account.github_domain.clone())
        };

        log::debug!("[CopilotAuth] Lazily fetching dynamic API endpoint for account {account_id}");

        let response = crate::proxy::http_client::get()
            .get(copilot_usage_url(&domain))
            .header("Authorization", format!("token {github_token}"))
            .header("Content-Type", "application/json")
            .header("editor-version", COPILOT_EDITOR_VERSION)
            .header("editor-plugin-version", COPILOT_PLUGIN_VERSION)
            .header("user-agent", COPILOT_USER_AGENT)
            .header("x-github-api-version", COPILOT_API_VERSION)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CopilotAuthError::GitHubTokenInvalid);
        }

        if !response.status().is_success() {
            return Err(CopilotAuthError::CopilotTokenFetchFailed(format!(
                "Failed to fetch API endpoint: {}",
                response.status()
            )));
        }

        let usage: CopilotUsageResponse = response
            .json()
            .await
            .map_err(|e| CopilotAuthError::ParseError(e.to_string()))?;

        let endpoint = match usage.endpoints {
            Some(endpoints) => endpoints.api.clone(),
            None => copilot_api_base(&domain),
        };

        // Cache the endpoint (including the default value) to avoid duplicate requests
        let mut api_endpoints = self.api_endpoints.write().await;
        api_endpoints.insert(account_id.to_string(), endpoint.clone());
        log::debug!("[CopilotAuth] Cached API endpoint for account {account_id}");

        Ok(endpoint)
    }

    async fn get_endpoint_lock(&self, account_id: &str) -> Arc<Mutex<()>> {
        {
            let locks = self.endpoint_locks.read().await;
            if let Some(lock) = locks.get(account_id) {
                return Arc::clone(lock);
            }
        }

        let mut locks = self.endpoint_locks.write().await;
        Arc::clone(
            locks
                .entry(account_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    // ==================== Internal methods ====================

    fn fallback_default_account_id(
        accounts: &HashMap<String, GitHubAccountData>,
    ) -> Option<String> {
        accounts
            .iter()
            .max_by(|(id_a, a), (id_b, b)| {
                a.authenticated_at
                    .cmp(&b.authenticated_at)
                    .then_with(|| id_b.cmp(id_a))
            })
            .map(|(id, _)| id.clone())
    }

    async fn resolve_default_account_id(&self) -> Option<String> {
        let stored_default = self.default_account_id.read().await.clone();
        let accounts = self.accounts.read().await;

        if let Some(default_id) = stored_default {
            if accounts.contains_key(&default_id) {
                return Some(default_id);
            }
        }

        Self::fallback_default_account_id(&accounts)
    }

    /// Gets the GitHub domain for the given account
    async fn get_account_domain(&self, account_id: &str) -> String {
        let accounts = self.accounts.read().await;
        accounts
            .get(account_id)
            .map(|a| a.github_domain.clone())
            .unwrap_or_else(|| DEFAULT_GITHUB_DOMAIN.to_string())
    }

    async fn get_refresh_lock(&self, account_id: &str) -> Arc<Mutex<()>> {
        {
            let refresh_locks = self.refresh_locks.read().await;
            if let Some(lock) = refresh_locks.get(account_id) {
                return Arc::clone(lock);
            }
        }

        let mut refresh_locks = self.refresh_locks.write().await;
        Arc::clone(
            refresh_locks
                .entry(account_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    async fn set_migration_error(&self, message: Option<String>) {
        let mut migration_error = self.migration_error.write().await;
        *migration_error = message;
    }

    fn write_store_atomic(&self, content: &str) -> Result<(), CopilotAuthError> {
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let parent = self
            .storage_path
            .parent()
            .ok_or_else(|| CopilotAuthError::IoError("Invalid storage path".to_string()))?;
        let file_name = self
            .storage_path
            .file_name()
            .ok_or_else(|| CopilotAuthError::IoError("Invalid storage file name".to_string()))?
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

    /// Fetches GitHub user info using the given token
    async fn fetch_user_info_with_token(
        &self,
        github_token: &str,
        domain: &str,
    ) -> Result<GitHubUser, CopilotAuthError> {
        let response = crate::proxy::http_client::get()
            .get(github_user_url(domain))
            .header("Authorization", format!("token {github_token}"))
            .header("User-Agent", COPILOT_USER_AGENT)
            .header("Editor-Version", COPILOT_EDITOR_VERSION)
            .header("Editor-Plugin-Version", COPILOT_PLUGIN_VERSION)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(CopilotAuthError::GitHubTokenInvalid);
        }

        let user: GitHubUser = response
            .json()
            .await
            .map_err(|e| CopilotAuthError::ParseError(e.to_string()))?;

        log::info!(
            "[CopilotAuth] Fetched user info successfully: {}",
            user.login
        );

        Ok(user)
    }

    /// Fetches a Copilot Token using the GitHub token
    async fn fetch_copilot_token_with_github_token(
        &self,
        github_token: &str,
        account_id: &str,
        domain: &str,
    ) -> Result<(), CopilotAuthError> {
        log::debug!(
            "[CopilotAuth] Fetching Copilot Token for account {account_id} (domain: {domain})"
        );

        let response = crate::proxy::http_client::get()
            .get(copilot_token_url(domain))
            .header("Authorization", format!("token {github_token}"))
            .header("User-Agent", COPILOT_USER_AGENT)
            .header("Editor-Version", COPILOT_EDITOR_VERSION)
            .header("Editor-Plugin-Version", COPILOT_PLUGIN_VERSION)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CopilotAuthError::GitHubTokenInvalid);
        }

        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(CopilotAuthError::NoCopilotSubscription);
        }

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(CopilotAuthError::CopilotTokenFetchFailed(format!(
                "{status}: {text}"
            )));
        }

        let token_response: CopilotTokenResponse = response
            .json()
            .await
            .map_err(|e| CopilotAuthError::ParseError(e.to_string()))?;

        log::info!(
            "[CopilotAuth] Copilot Token fetched successfully for account {}, expires at: {}",
            account_id,
            token_response.expires_at
        );

        let copilot_token = CopilotToken {
            token: token_response.token,
            expires_at: token_response.expires_at,
        };

        let mut tokens = self.copilot_tokens.write().await;
        tokens.insert(account_id.to_string(), copilot_token);

        Ok(())
    }

    // ==================== Storage and migration ====================

    /// Loads from disk (loads the token only, no network request)
    fn load_from_disk_sync(&self) -> Result<(), CopilotAuthError> {
        if !self.storage_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.storage_path)?;
        let store: CopilotAuthStore = serde_json::from_str(&content)
            .map_err(|e| CopilotAuthError::ParseError(e.to_string()))?;

        if store.version >= 2 {
            // v2 multi-account format
            if let Ok(mut accounts) = self.accounts.try_write() {
                *accounts = store.accounts;
                log::info!("[CopilotAuth] Loaded {} accounts from disk", accounts.len());
            }
            if let Ok(mut default_account_id) = self.default_account_id.try_write() {
                *default_account_id = store.default_account_id;
                if default_account_id.is_none() {
                    if let Ok(accounts) = self.accounts.try_read() {
                        *default_account_id = Self::fallback_default_account_id(&accounts);
                    }
                }
            }
        } else if store.github_token.is_some() {
            // v1 single-account format, mark as pending migration
            log::info!("[CopilotAuth] Detected legacy format, will migrate on first access");
            if let Ok(mut pending) = self.pending_migration.try_write() {
                *pending = store.github_token;
            }
        }

        Ok(())
    }

    /// Ensures migration is complete
    async fn ensure_migration_complete(&self) -> Result<(), CopilotAuthError> {
        let pending = {
            let guard = self.pending_migration.read().await;
            guard.clone()
        };

        if let Some(legacy_token) = pending {
            log::info!("[CopilotAuth] Running legacy format migration");

            // Fetch user info
            match self
                .fetch_user_info_with_token(&legacy_token, DEFAULT_GITHUB_DOMAIN)
                .await
            {
                Ok(user) => {
                    let account_id = composite_account_id(DEFAULT_GITHUB_DOMAIN, user.id);

                    // Try fetching a Copilot token to verify the subscription
                    if let Err(e) = self
                        .fetch_copilot_token_with_github_token(
                            &legacy_token,
                            &account_id,
                            DEFAULT_GITHUB_DOMAIN,
                        )
                        .await
                    {
                        log::warn!("[CopilotAuth] Failed to verify Copilot subscription during migration: {e}");
                    }

                    // Add the account
                    self.add_account_internal(
                        legacy_token,
                        user,
                        DEFAULT_GITHUB_DOMAIN.to_string(),
                    )
                    .await?;
                    self.set_migration_error(None).await;

                    log::info!("[CopilotAuth] Legacy format migration complete");
                }
                Err(e) => {
                    self.set_migration_error(Some(format!(
                        "Legacy Copilot auth migration failed: {e}"
                    )))
                    .await;
                    log::warn!("[CopilotAuth] Migration failed, the legacy token may no longer be valid: {e}");
                }
            }

            // Clear the pending-migration marker
            {
                let mut pending = self.pending_migration.write().await;
                *pending = None;
            }
        }

        Ok(())
    }

    /// Saves to disk
    async fn save_to_disk(&self) -> Result<(), CopilotAuthError> {
        let accounts = self.accounts.read().await.clone();
        let default_account_id = self.resolve_default_account_id().await;

        let store = CopilotAuthStore {
            version: 3,
            accounts,
            default_account_id,
            github_token: None,
            authenticated_at: None,
        };

        let content = serde_json::to_string_pretty(&store)
            .map_err(|e| CopilotAuthError::ParseError(e.to_string()))?;

        self.write_store_atomic(&content)?;

        log::info!(
            "[CopilotAuth] Saved to disk successfully ({} accounts)",
            store.accounts.len()
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_copilot_token_expiry() {
        let now = chrono::Utc::now().timestamp();

        // Not-yet-expired token (expires in 1 hour, outside the 60-second buffer)
        let token = CopilotToken {
            token: "test".to_string(),
            expires_at: now + 3600,
        };
        assert!(!token.is_expiring_soon());

        // Soon-to-expire token (expires in 30 seconds, within the 60-second buffer)
        let token = CopilotToken {
            token: "test".to_string(),
            expires_at: now + 30,
        };
        assert!(token.is_expiring_soon());

        // Already-expired token (also within the buffer)
        let token = CopilotToken {
            token: "test".to_string(),
            expires_at: now - 100,
        };
        assert!(token.is_expiring_soon());
    }

    #[test]
    fn test_multi_account_store_serialization() {
        let mut accounts = HashMap::new();
        accounts.insert(
            "12345".to_string(),
            GitHubAccountData {
                github_token: "gho_test_token".to_string(),
                user: GitHubUser {
                    login: "alice".to_string(),
                    id: 12345,
                    avatar_url: Some("https://example.com/alice.png".to_string()),
                },
                authenticated_at: 1700000000,
                github_domain: DEFAULT_GITHUB_DOMAIN.to_string(),
            },
        );
        accounts.insert(
            "67890".to_string(),
            GitHubAccountData {
                github_token: "gho_test_token_2".to_string(),
                user: GitHubUser {
                    login: "bob".to_string(),
                    id: 67890,
                    avatar_url: None,
                },
                authenticated_at: 1700000001,
                github_domain: DEFAULT_GITHUB_DOMAIN.to_string(),
            },
        );

        let store = CopilotAuthStore {
            version: 3,
            accounts,
            default_account_id: Some("67890".to_string()),
            github_token: None,
            authenticated_at: None,
        };

        let json = serde_json::to_string_pretty(&store).unwrap();
        let parsed: CopilotAuthStore = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 3);
        assert_eq!(parsed.default_account_id, Some("67890".to_string()));
        assert_eq!(parsed.accounts.len(), 2);
        assert!(parsed.accounts.contains_key("12345"));
        assert!(parsed.accounts.contains_key("67890"));
        assert_eq!(parsed.accounts["12345"].user.login, "alice");
        assert_eq!(parsed.accounts["67890"].user.login, "bob");
    }

    #[test]
    fn test_legacy_format_detection() {
        // Legacy format (v1)
        let legacy_json = r#"{
            "github_token": "gho_legacy_token",
            "authenticated_at": 1700000000
        }"#;

        let store: CopilotAuthStore = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(store.version, 0); // default value
        assert!(store.github_token.is_some());
        assert!(store.accounts.is_empty());
    }

    #[test]
    fn test_github_account_from_data() {
        let data = GitHubAccountData {
            github_token: "gho_test".to_string(),
            user: GitHubUser {
                login: "testuser".to_string(),
                id: 99999,
                avatar_url: Some("https://example.com/avatar.png".to_string()),
            },
            authenticated_at: 1700000000,
            github_domain: DEFAULT_GITHUB_DOMAIN.to_string(),
        };

        let account = GitHubAccount::from(&data);
        assert_eq!(account.id, "99999");
        assert_eq!(account.login, "testuser");
        assert_eq!(
            account.avatar_url,
            Some("https://example.com/avatar.png".to_string())
        );
        assert_eq!(account.authenticated_at, 1700000000);
    }

    #[test]
    fn test_fallback_default_account_prefers_latest_authenticated() {
        let mut accounts = HashMap::new();
        accounts.insert(
            "12345".to_string(),
            GitHubAccountData {
                github_token: "gho_test_token".to_string(),
                user: GitHubUser {
                    login: "alice".to_string(),
                    id: 12345,
                    avatar_url: None,
                },
                authenticated_at: 1700000000,
                github_domain: DEFAULT_GITHUB_DOMAIN.to_string(),
            },
        );
        accounts.insert(
            "67890".to_string(),
            GitHubAccountData {
                github_token: "gho_test_token_2".to_string(),
                user: GitHubUser {
                    login: "bob".to_string(),
                    id: 67890,
                    avatar_url: None,
                },
                authenticated_at: 1700000001,
                github_domain: DEFAULT_GITHUB_DOMAIN.to_string(),
            },
        );

        assert_eq!(
            CopilotAuthManager::fallback_default_account_id(&accounts),
            Some("67890".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_model_vendor_from_cache() {
        let temp_dir = tempdir().unwrap();
        let manager = CopilotAuthManager::new(temp_dir.path().to_path_buf());

        {
            let mut default_account_id = manager.default_account_id.write().await;
            *default_account_id = Some("12345".to_string());
        }
        {
            let mut accounts = manager.accounts.write().await;
            accounts.insert(
                "12345".to_string(),
                GitHubAccountData {
                    github_token: "gho_test".to_string(),
                    user: GitHubUser {
                        login: "alice".to_string(),
                        id: 12345,
                        avatar_url: None,
                    },
                    authenticated_at: 1700000000,
                    github_domain: DEFAULT_GITHUB_DOMAIN.to_string(),
                },
            );
        }
        {
            let mut models = manager.copilot_models.write().await;
            models.insert(
                "12345".to_string(),
                vec![
                    CopilotModel {
                        id: "gpt-5.4".to_string(),
                        name: "GPT-5.4".to_string(),
                        vendor: "OpenAI".to_string(),
                        model_picker_enabled: true,
                    },
                    CopilotModel {
                        id: "claude-sonnet-4".to_string(),
                        name: "Claude Sonnet 4".to_string(),
                        vendor: "Anthropic".to_string(),
                        model_picker_enabled: true,
                    },
                ],
            );
        }

        let vendor = manager
            .get_model_vendor_for_account("12345", "gpt-5.4")
            .await
            .unwrap();
        assert_eq!(vendor.as_deref(), Some("OpenAI"));

        let default_vendor = manager.get_model_vendor("claude-sonnet-4").await.unwrap();
        assert_eq!(default_vendor.as_deref(), Some("Anthropic"));
    }

    #[tokio::test]
    async fn test_get_api_endpoint_returns_cached_value() {
        let temp_dir = tempdir().unwrap();
        let manager = CopilotAuthManager::new(temp_dir.path().to_path_buf());

        // Manually set the api_endpoints cache
        {
            let mut api_endpoints = manager.api_endpoints.write().await;
            api_endpoints.insert(
                "12345".to_string(),
                "https://copilot-api.enterprise.example.com".to_string(),
            );
        }

        let endpoint = manager.get_api_endpoint("12345").await;
        assert_eq!(endpoint, "https://copilot-api.enterprise.example.com");
    }

    #[tokio::test]
    async fn test_get_api_endpoint_returns_default_when_not_cached() {
        let temp_dir = tempdir().unwrap();
        let manager = CopilotAuthManager::new(temp_dir.path().to_path_buf());

        let endpoint = manager.get_api_endpoint("99999").await;
        assert_eq!(endpoint, "https://api.githubcopilot.com");
    }

    #[tokio::test]
    async fn test_get_default_api_endpoint_uses_default_account() {
        let temp_dir = tempdir().unwrap();
        let manager = CopilotAuthManager::new(temp_dir.path().to_path_buf());

        // Set the default account
        {
            let mut default_account_id = manager.default_account_id.write().await;
            *default_account_id = Some("12345".to_string());
        }
        // Add account data
        {
            let mut accounts = manager.accounts.write().await;
            accounts.insert(
                "12345".to_string(),
                GitHubAccountData {
                    github_token: "gho_test".to_string(),
                    user: GitHubUser {
                        login: "alice".to_string(),
                        id: 12345,
                        avatar_url: None,
                    },
                    authenticated_at: 1700000000,
                    github_domain: DEFAULT_GITHUB_DOMAIN.to_string(),
                },
            );
        }
        // Set the API endpoint cache
        {
            let mut api_endpoints = manager.api_endpoints.write().await;
            api_endpoints.insert(
                "12345".to_string(),
                "https://copilot-api.corp.example.com".to_string(),
            );
        }

        let endpoint = manager.get_default_api_endpoint().await;
        assert_eq!(endpoint, "https://copilot-api.corp.example.com");
    }

    #[tokio::test]
    async fn test_get_api_endpoint_cache_hit_skips_fetch() {
        // On cache hit, should return directly without making a network request
        let temp_dir = tempdir().unwrap();
        let manager = CopilotAuthManager::new(temp_dir.path().to_path_buf());

        let enterprise_endpoint = "https://copilot-api.enterprise.example.com".to_string();
        {
            let mut api_endpoints = manager.api_endpoints.write().await;
            api_endpoints.insert("12345".to_string(), enterprise_endpoint.clone());
        }

        // Even without account data, a cache hit should return directly
        let endpoint = manager.get_api_endpoint("12345").await;
        assert_eq!(endpoint, enterprise_endpoint);
    }

    #[tokio::test]
    async fn test_get_api_endpoint_returns_default_for_unknown_account() {
        let temp_dir = tempdir().unwrap();
        let manager = CopilotAuthManager::new(temp_dir.path().to_path_buf());

        let endpoint = manager.get_api_endpoint("12345").await;
        assert_eq!(endpoint, copilot_api_base(DEFAULT_GITHUB_DOMAIN));
    }

    #[tokio::test]
    async fn test_fetch_and_cache_endpoint_requires_account() {
        // fetch_and_cache_endpoint should return an AccountNotFound error when the account doesn't exist
        let temp_dir = tempdir().unwrap();
        let manager = CopilotAuthManager::new(temp_dir.path().to_path_buf());

        let result = manager.fetch_and_cache_endpoint("nonexistent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CopilotAuthError::AccountNotFound(id) => assert_eq!(id, "nonexistent"),
            other => panic!("Expected AccountNotFound error, got: {other:?}"),
        }
    }

    #[test]
    fn test_composite_account_id() {
        // github.com keeps the original format (backward compatible)
        assert_eq!(composite_account_id("github.com", 12345), "12345");

        // GHES uses the composite format
        assert_eq!(
            composite_account_id("company.ghe.com", 12345),
            "company.ghe.com:12345"
        );

        // Different GHES instances, same user ID, no collision
        assert_ne!(
            composite_account_id("a.ghe.com", 1),
            composite_account_id("b.ghe.com", 1)
        );
    }

    #[test]
    fn test_github_account_from_data_ghes_uses_composite_id() {
        let data = GitHubAccountData {
            github_token: "gho_test".to_string(),
            user: GitHubUser {
                login: "testuser".to_string(),
                id: 99999,
                avatar_url: None,
            },
            authenticated_at: 1700000000,
            github_domain: "company.ghe.com".to_string(),
        };

        let account = GitHubAccount::from(&data);
        assert_eq!(account.id, "company.ghe.com:99999");
    }
}
