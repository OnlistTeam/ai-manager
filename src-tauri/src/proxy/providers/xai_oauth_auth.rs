//! xAI OAuth authentication manager.
//!
//! xAI uses the OAuth 2.0 Device Authorization Grant. Endpoints are resolved
//! from xAI's OpenID Connect discovery document so authentication protocol
//! changes do not require duplicating endpoint constants across the app.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

const XAI_ISSUER: &str = "https://auth.x.ai";
const XAI_DISCOVERY_URL: &str = "https://auth.x.ai/.well-known/openid-configuration";
const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const XAI_USER_AGENT: &str = "cc-switch-xai-oauth";
const TOKEN_REFRESH_BUFFER_MS: i64 = 60_000;
const DEFAULT_TOKEN_LIFETIME_SECS: i64 = 3_600;
const MAX_OAUTH_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum XaiOAuthError {
    #[error("OAuth token fetch failed: {0}")]
    TokenFetchFailed(String),
    #[error("Refresh token invalid or expired, please log in to xAI again")]
    RefreshTokenInvalid,
    #[error("Account needs to re-authenticate: {0}")]
    ReauthRequired(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Account not found: {0}")]
    AccountNotFound(String),
}

impl From<reqwest::Error> for XaiOAuthError {
    fn from(err: reqwest::Error) -> Self {
        Self::NetworkError(err.to_string())
    }
}

impl From<std::io::Error> for XaiOAuthError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err.to_string())
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DiscoveryDocument {
    issuer: String,
    token_endpoint: String,
}

#[derive(Debug, Clone)]
struct OAuthEndpoints {
    token_endpoint: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Debug, Clone)]
struct CachedAccessToken {
    token: String,
    expires_at_ms: i64,
}

impl CachedAccessToken {
    fn is_expiring_soon(&self) -> bool {
        self.expires_at_ms - chrono::Utc::now().timestamp_millis() < TOKEN_REFRESH_BUFFER_MS
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct XaiAccountData {
    account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    login: Option<String>,
    refresh_token: String,
    authenticated_at: i64,
    #[serde(default)]
    requires_reauth: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct XaiOAuthStore {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    accounts: HashMap<String, XaiAccountData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_account_id: Option<String>,
}

pub struct XaiOAuthManager {
    accounts: Arc<RwLock<HashMap<String, XaiAccountData>>>,
    default_account_id: Arc<RwLock<Option<String>>>,
    access_tokens: Arc<RwLock<HashMap<String, CachedAccessToken>>>,
    refresh_locks: Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>,
    discovered_endpoints: Arc<RwLock<Option<OAuthEndpoints>>>,
    mutation_lock: Arc<Mutex<()>>,
    storage_path: PathBuf,
}

impl XaiOAuthManager {
    pub fn new(data_dir: PathBuf) -> Self {
        let manager = Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            default_account_id: Arc::new(RwLock::new(None)),
            access_tokens: Arc::new(RwLock::new(HashMap::new())),
            refresh_locks: Arc::new(RwLock::new(HashMap::new())),
            discovered_endpoints: Arc::new(RwLock::new(None)),
            mutation_lock: Arc::new(Mutex::new(())),
            storage_path: data_dir.join("xai_oauth_auth.json"),
        };

        if let Err(error) = manager.load_from_disk_sync() {
            log::warn!("[XaiOAuth] Failed to load storage: {error}");
        }
        manager
    }

    pub async fn get_valid_token_for_account(
        &self,
        account_id: &str,
    ) -> Result<String, XaiOAuthError> {
        if let Some(token) = self.cached_token_for_usable_account(account_id).await {
            return Ok(token);
        }

        let refresh_lock = self.get_refresh_lock(account_id).await;
        let _refresh_guard = refresh_lock.lock().await;
        if let Some(token) = self.cached_token_for_usable_account(account_id).await {
            return Ok(token);
        }

        let account = self
            .accounts
            .read()
            .await
            .get(account_id)
            .cloned()
            .ok_or_else(|| XaiOAuthError::AccountNotFound(account_id.to_string()))?;
        if account.requires_reauth {
            return Err(XaiOAuthError::ReauthRequired(account_id.to_string()));
        }

        let tokens = match self.refresh_with_token(&account.refresh_token).await {
            Ok(tokens) => tokens,
            Err(XaiOAuthError::RefreshTokenInvalid) => {
                self.mark_reauth_required(account_id).await?;
                return Err(XaiOAuthError::ReauthRequired(account_id.to_string()));
            }
            Err(error) => return Err(error),
        };

        self.commit_refreshed_tokens(account_id, &account.refresh_token, tokens)
            .await
    }

    pub async fn get_valid_token(&self) -> Result<String, XaiOAuthError> {
        match self.resolve_default_account_id().await {
            Some(account_id) => self.get_valid_token_for_account(&account_id).await,
            None => Err(XaiOAuthError::AccountNotFound(
                "No available xAI account, please log in or re-authenticate".to_string(),
            )),
        }
    }

    async fn discover_endpoints(&self) -> Result<OAuthEndpoints, XaiOAuthError> {
        if let Some(endpoints) = self.discovered_endpoints.read().await.clone() {
            return Ok(endpoints);
        }
        let response = crate::proxy::http_client::get()
            .get(XAI_DISCOVERY_URL)
            .header("User-Agent", XAI_USER_AGENT)
            .send()
            .await?;
        let status = response.status();
        let value = read_json_response(response).await?;
        if !status.is_success() {
            return Err(XaiOAuthError::NetworkError(format!(
                "xAI discovery request failed: HTTP {status}"
            )));
        }
        let document = parse_discovery_document(value)?;
        if document.issuer.trim_end_matches('/') != XAI_ISSUER {
            return Err(XaiOAuthError::ParseError(
                "xAI discovery issuer mismatch".to_string(),
            ));
        }
        validate_xai_endpoint(&document.token_endpoint)?;
        let endpoints = OAuthEndpoints {
            token_endpoint: document.token_endpoint,
        };
        *self.discovered_endpoints.write().await = Some(endpoints.clone());
        Ok(endpoints)
    }

    async fn refresh_with_token(
        &self,
        refresh_token: &str,
    ) -> Result<OAuthTokenResponse, XaiOAuthError> {
        let endpoints = self.discover_endpoints().await?;
        let response = crate::proxy::http_client::get()
            .post(&endpoints.token_endpoint)
            .header("User-Agent", XAI_USER_AGENT)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", XAI_CLIENT_ID),
                ("refresh_token", refresh_token),
                ("scope", XAI_SCOPE),
            ])
            .send()
            .await?;
        let status = response.status();
        let value_result = read_json_response(response).await;
        // Invalid credentials must transition the account to re-auth even when
        // the provider sends an empty, HTML, or otherwise malformed error body.
        if refresh_response_requires_reauth(status, value_result.is_err()) {
            return Err(XaiOAuthError::RefreshTokenInvalid);
        }
        let value = value_result?;
        let error_code = oauth_error_code(&value);
        if matches!(
            error_code.as_deref(),
            Some("invalid_grant" | "invalid_token")
        ) {
            return Err(XaiOAuthError::RefreshTokenInvalid);
        }
        if !status.is_success() || error_code.is_some() {
            return Err(XaiOAuthError::TokenFetchFailed(format_oauth_error(
                status, &value,
            )));
        }
        let tokens = parse_token_response(value)?;
        validate_access_token(&tokens.access_token)?;
        Ok(tokens)
    }

    async fn commit_refreshed_tokens(
        &self,
        account_id: &str,
        expected_refresh_token: &str,
        tokens: OAuthTokenResponse,
    ) -> Result<String, XaiOAuthError> {
        let _mutation_guard = self.mutation_lock.lock().await;
        let mut accounts = self.accounts.read().await.clone();
        let account = accounts
            .get_mut(account_id)
            .ok_or_else(|| XaiOAuthError::AccountNotFound(account_id.to_string()))?;
        if account.requires_reauth {
            return Err(XaiOAuthError::ReauthRequired(account_id.to_string()));
        }
        if account.refresh_token != expected_refresh_token {
            return Err(XaiOAuthError::TokenFetchFailed(
                "Account authentication state has changed, please retry the request".to_string(),
            ));
        }

        let refresh_token_changed = tokens
            .refresh_token
            .as_deref()
            .filter(|token| !token.trim().is_empty())
            .is_some_and(|refresh_token| {
                if refresh_token == account.refresh_token {
                    false
                } else {
                    account.refresh_token = refresh_token.to_string();
                    true
                }
            });
        if refresh_token_changed {
            let default_account_id = self.default_account_id.read().await.clone();
            self.persist_and_commit(accounts, default_account_id)
                .await?;
        }

        let access_token = tokens.access_token;
        self.access_tokens.write().await.insert(
            account_id.to_string(),
            CachedAccessToken {
                token: access_token.clone(),
                expires_at_ms: compute_expires_at_ms(tokens.expires_in),
            },
        );
        Ok(access_token)
    }

    async fn mark_reauth_required(&self, account_id: &str) -> Result<(), XaiOAuthError> {
        let _mutation_guard = self.mutation_lock.lock().await;
        let mut accounts = self.accounts.read().await.clone();
        let account = accounts
            .get_mut(account_id)
            .ok_or_else(|| XaiOAuthError::AccountNotFound(account_id.to_string()))?;
        account.requires_reauth = true;
        let default_account_id = Self::fallback_default_account_id(&accounts);
        self.persist_and_commit(accounts, default_account_id)
            .await?;
        self.access_tokens.write().await.remove(account_id);
        Ok(())
    }

    async fn persist_and_commit(
        &self,
        accounts: HashMap<String, XaiAccountData>,
        default_account_id: Option<String>,
    ) -> Result<(), XaiOAuthError> {
        let store = XaiOAuthStore {
            version: 1,
            accounts: accounts.clone(),
            default_account_id: default_account_id.clone(),
        };
        let content = serde_json::to_string_pretty(&store)
            .map_err(|error| XaiOAuthError::ParseError(error.to_string()))?;
        self.write_store_atomic(&content)?;
        *self.accounts.write().await = accounts;
        *self.default_account_id.write().await = default_account_id;
        Ok(())
    }

    async fn cached_token(&self, account_id: &str) -> Option<String> {
        self.access_tokens
            .read()
            .await
            .get(account_id)
            .filter(|token| !token.is_expiring_soon())
            .map(|token| token.token.clone())
    }

    async fn cached_token_for_usable_account(&self, account_id: &str) -> Option<String> {
        let account_is_usable = {
            let accounts = self.accounts.read().await;
            Self::is_usable_account(&accounts, account_id)
        };
        if !account_is_usable {
            return None;
        }
        self.cached_token(account_id).await
    }

    async fn get_refresh_lock(&self, account_id: &str) -> Arc<Mutex<()>> {
        if let Some(lock) = self.refresh_locks.read().await.get(account_id).cloned() {
            return lock;
        }
        Arc::clone(
            self.refresh_locks
                .write()
                .await
                .entry(account_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    async fn resolve_default_account_id(&self) -> Option<String> {
        let stored = self.default_account_id.read().await.clone();
        let accounts = self.accounts.read().await;
        match stored {
            Some(id) if Self::is_usable_account(&accounts, &id) => Some(id),
            _ => Self::fallback_default_account_id(&accounts),
        }
    }

    fn fallback_default_account_id(accounts: &HashMap<String, XaiAccountData>) -> Option<String> {
        accounts
            .iter()
            .filter(|(_, account)| !account.requires_reauth)
            .max_by(|(id_a, account_a), (id_b, account_b)| {
                account_a
                    .authenticated_at
                    .cmp(&account_b.authenticated_at)
                    .then_with(|| id_b.cmp(id_a))
            })
            .map(|(id, _)| id.clone())
    }

    fn is_usable_account(accounts: &HashMap<String, XaiAccountData>, id: &str) -> bool {
        accounts
            .get(id)
            .is_some_and(|account| !account.requires_reauth)
    }

    fn load_from_disk_sync(&self) -> Result<(), XaiOAuthError> {
        if !self.storage_path.exists() {
            return Ok(());
        }
        let content = fs::read_to_string(&self.storage_path)?;
        let store: XaiOAuthStore = serde_json::from_str(&content)
            .map_err(|error| XaiOAuthError::ParseError(error.to_string()))?;
        if let Ok(mut accounts) = self.accounts.try_write() {
            *accounts = store.accounts;
        }
        if let Ok(mut default_account_id) = self.default_account_id.try_write() {
            *default_account_id = store.default_account_id;
        }
        Ok(())
    }

    fn write_store_atomic(&self, content: &str) -> Result<(), XaiOAuthError> {
        let parent = self
            .storage_path
            .parent()
            .ok_or_else(|| XaiOAuthError::IoError("Invalid storage path".to_string()))?;
        fs::create_dir_all(parent)?;
        let file_name = self
            .storage_path
            .file_name()
            .ok_or_else(|| XaiOAuthError::IoError("Invalid storage file name".to_string()))?
            .to_string_lossy();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary_path = parent.join(format!("{file_name}.tmp.{nonce}"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let result = (|| -> Result<(), std::io::Error> {
                let mut file = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .mode(0o600)
                    .open(&temporary_path)?;
                file.write_all(content.as_bytes())?;
                file.flush()?;
                fs::rename(&temporary_path, &self.storage_path)?;
                fs::set_permissions(&self.storage_path, fs::Permissions::from_mode(0o600))?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temporary_path);
            }
            result?;
        }

        #[cfg(windows)]
        {
            let result = (|| -> Result<(), std::io::Error> {
                let mut file = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&temporary_path)?;
                file.write_all(content.as_bytes())?;
                file.flush()?;
                if self.storage_path.exists() {
                    fs::remove_file(&self.storage_path)?;
                }
                fs::rename(&temporary_path, &self.storage_path)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temporary_path);
            }
            result?;
        }
        Ok(())
    }
}

fn compute_expires_at_ms(expires_in: Option<i64>) -> i64 {
    chrono::Utc::now().timestamp_millis().saturating_add(
        expires_in
            .unwrap_or(DEFAULT_TOKEN_LIFETIME_SECS)
            .max(1)
            .saturating_mul(1_000),
    )
}

fn validate_access_token(access_token: &str) -> Result<(), XaiOAuthError> {
    if access_token.trim().is_empty() {
        return Err(XaiOAuthError::TokenFetchFailed(
            "Successful response is missing access_token".to_string(),
        ));
    }
    Ok(())
}

fn parse_token_response(value: serde_json::Value) -> Result<OAuthTokenResponse, XaiOAuthError> {
    serde_json::from_value(value)
        .map_err(|_| XaiOAuthError::ParseError("Invalid field in OAuth token response".to_string()))
}

fn parse_discovery_document(value: serde_json::Value) -> Result<DiscoveryDocument, XaiOAuthError> {
    serde_json::from_value(value)
        .map_err(|_| XaiOAuthError::ParseError("Invalid xAI discovery response field".to_string()))
}

fn refresh_response_requires_reauth(
    status: reqwest::StatusCode,
    response_body_is_invalid: bool,
) -> bool {
    matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ) || (status == reqwest::StatusCode::BAD_REQUEST && response_body_is_invalid)
}

fn validate_xai_endpoint(value: &str) -> Result<(), XaiOAuthError> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| XaiOAuthError::ParseError("Invalid xAI auth endpoint URL".to_string()))?;
    if url.scheme() != "https"
        || url.host_str() != Some("auth.x.ai")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(XaiOAuthError::ParseError(
            "xAI discovery returned an untrusted auth endpoint".to_string(),
        ));
    }
    Ok(())
}

async fn read_json_response(
    response: reqwest::Response,
) -> Result<serde_json::Value, XaiOAuthError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_OAUTH_RESPONSE_BYTES as u64)
    {
        return Err(XaiOAuthError::ParseError(
            "OAuth response exceeds the size limit".to_string(),
        ));
    }
    let bytes = response.bytes().await?;
    if bytes.len() > MAX_OAUTH_RESPONSE_BYTES {
        return Err(XaiOAuthError::ParseError(
            "OAuth response exceeds the size limit".to_string(),
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| XaiOAuthError::ParseError("OAuth response is not valid JSON".to_string()))
}

fn oauth_error_code(value: &serde_json::Value) -> Option<String> {
    value
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map(sanitize_oauth_error_code)
        .filter(|value| !value.is_empty())
}

fn sanitize_oauth_error_code(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || "_.-".contains(*character))
        .take(64)
        .collect()
}

fn format_oauth_error(status: reqwest::StatusCode, value: &serde_json::Value) -> String {
    match oauth_error_code(value) {
        Some(code) => format!("HTTP {status} ({code})"),
        None => format!("HTTP {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_error_never_embeds_upstream_body() {
        let value = serde_json::json!({
            "error": "invalid_grant<script>",
            "error_description": "refresh_token=super-secret"
        });
        let message = format_oauth_error(reqwest::StatusCode::BAD_REQUEST, &value);
        assert_eq!(message, "HTTP 400 Bad Request (invalid_grantscript)");
        assert!(!message.contains("super-secret"));
        assert!(!message.contains("refresh_token"));
    }

    #[test]
    fn malformed_token_response_never_embeds_upstream_values() {
        let result = parse_token_response(serde_json::json!({
            "access_token": ["upstream-secret"],
            "refresh_token": "another-secret",
            "expires_in": "refresh_token=third-secret"
        }));
        let error = result.unwrap_err().to_string();
        assert_eq!(error, "Parse error: Invalid field in OAuth token response");
        assert!(!error.contains("secret"));
        assert!(validate_access_token("  ").is_err());
    }

    #[test]
    fn refresh_auth_status_is_classified_before_body_parsing() {
        assert!(refresh_response_requires_reauth(
            reqwest::StatusCode::UNAUTHORIZED,
            true,
        ));
        assert!(refresh_response_requires_reauth(
            reqwest::StatusCode::FORBIDDEN,
            true,
        ));
        assert!(refresh_response_requires_reauth(
            reqwest::StatusCode::BAD_REQUEST,
            true,
        ));
        assert!(!refresh_response_requires_reauth(
            reqwest::StatusCode::BAD_REQUEST,
            false,
        ));
        assert!(!refresh_response_requires_reauth(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            true,
        ));
        assert!(!refresh_response_requires_reauth(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            true,
        ));
    }

    #[test]
    fn fallback_default_skips_accounts_requiring_reauth() {
        let mut accounts = HashMap::new();
        accounts.insert(
            "invalid".to_string(),
            XaiAccountData {
                account_id: "invalid".to_string(),
                login: None,
                refresh_token: "r1".to_string(),
                authenticated_at: 20,
                requires_reauth: true,
            },
        );
        accounts.insert(
            "valid".to_string(),
            XaiAccountData {
                account_id: "valid".to_string(),
                login: None,
                refresh_token: "r2".to_string(),
                authenticated_at: 10,
                requires_reauth: false,
            },
        );
        assert_eq!(
            XaiOAuthManager::fallback_default_account_id(&accounts),
            Some("valid".to_string())
        );
    }

    #[test]
    fn discovery_endpoints_must_stay_on_xai_origin() {
        assert!(validate_xai_endpoint("https://auth.x.ai/oauth2/token").is_ok());
        assert!(validate_xai_endpoint("http://auth.x.ai/oauth2/token").is_err());
        assert!(validate_xai_endpoint("https://auth.x.ai:8443/oauth2/token").is_err());
        assert!(validate_xai_endpoint("https://user@auth.x.ai/oauth2/token").is_err());
        assert!(validate_xai_endpoint("https://attacker.example/token").is_err());
    }
}
