//! Provider Adapter Trait
//!
//! Defines a unified provider adapter interface that abstracts the handling logic of different upstream providers.

use super::auth::AuthInfo;
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use serde_json::Value;

/// Provider adapter trait
///
/// Every provider adapter must implement this trait, giving a uniform interface for:
/// - URL construction
/// - Extracting credentials and injecting headers
/// - Request/response format conversion (optional)
pub trait ProviderAdapter: Send + Sync {
    /// Adapter name (used for logging and debugging)
    fn name(&self) -> &'static str;

    /// Extracts base_url from the provider config
    fn extract_base_url(&self, provider: &Provider) -> Result<String, ProxyError>;

    /// Extracts credentials from the provider config
    fn extract_auth(&self, provider: &Provider) -> Option<AuthInfo>;

    /// Builds the request URL
    fn build_url(&self, base_url: &str, endpoint: &str) -> String;

    /// Return auth headers as `(name, value)` pairs.
    ///
    /// The forwarder inserts these at the position of the original auth header
    /// so that header order is preserved.
    ///
    /// Returns `ProxyError::AuthError` when the credential contains characters
    /// that cannot be encoded as an HTTP header value (e.g. control chars,
    /// CR/LF), which would otherwise panic inside `HeaderValue::from_str`.
    fn get_auth_headers(
        &self,
        auth: &AuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError>;

    /// Whether format conversion is required
    fn needs_transform(&self, _provider: &Provider) -> bool {
        false
    }

    /// Converts the request body
    fn transform_request(&self, body: Value, _provider: &Provider) -> Result<Value, ProxyError> {
        Ok(body)
    }
}

/// Build an HTTP `HeaderValue` from a credential / token string.
///
/// Returns `ProxyError::AuthError` when the string contains characters that
/// cannot live in an HTTP header value (control bytes, CR/LF, non-ASCII).
/// Adapters call this for every header value derived from user-pasted
/// material so a malformed key surfaces as a 401 instead of panicking
/// the worker via `HeaderValue::from_str(...).unwrap()`.
pub fn auth_header_value(s: &str) -> Result<http::HeaderValue, ProxyError> {
    http::HeaderValue::from_str(s)
        .map_err(|e| ProxyError::AuthError(format!("invalid auth header value: {e}")))
}
