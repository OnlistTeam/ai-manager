//! Maps error types to HTTP status codes
//!
//! Maps ProxyError to the appropriate HTTP status code, used for logging and
//! manually building error responses

use super::ProxyError;

/// Maps ProxyError to an HTTP status code
///
/// Mapping rules:
/// - Upstream error: use the status code returned by upstream directly
/// - Timeout: 504 Gateway Timeout
/// - Connection failed: 502 Bad Gateway
/// - No provider available: 503 Service Unavailable
/// - Retries exhausted: 503 Service Unavailable
/// - Auth error: 401 Unauthorized
/// - Config/request error: 400 Bad Request
/// - Transform error: 422 Unprocessable Entity
/// - Other errors: 500 Internal Server Error
pub fn map_proxy_error_to_status(error: &ProxyError) -> u16 {
    match error {
        // Service state error: kept consistent with IntoResponse
        ProxyError::AlreadyRunning => 409,
        ProxyError::NotRunning => 503,

        // Upstream error: use the actual status code
        ProxyError::UpstreamError { status, .. } => *status,

        // Timeout error: 504 Gateway Timeout
        ProxyError::Timeout(_) => 504,

        // Forwarding/connection failed: 502 Bad Gateway
        ProxyError::ForwardFailed(_) => 502,

        // No provider available: 503 Service Unavailable
        ProxyError::NoAvailableProvider => 503,

        // All providers circuit-open: 503 Service Unavailable
        ProxyError::AllProvidersCircuitOpen => 503,

        // No provider configured: 503 Service Unavailable
        ProxyError::NoProvidersConfigured => 503,

        // Retries exhausted: 503 Service Unavailable
        ProxyError::MaxRetriesExceeded => 503,

        // Config error / invalid request: 400 Bad Request
        ProxyError::ConfigError(_) | ProxyError::InvalidRequest(_) => 400,

        // Auth error: 401 Unauthorized
        ProxyError::AuthError(_) => 401,

        // Database error: 500 Internal Server Error
        ProxyError::DatabaseError(_) => 500,

        // Transform error: 422 Unprocessable Entity
        ProxyError::TransformError(_) => 422,

        // Other unknown error: 500 Internal Server Error
        _ => 500,
    }
}

/// Converts ProxyError into a user-friendly error message
pub fn get_error_message(error: &ProxyError) -> String {
    match error {
        ProxyError::UpstreamError { status, body } => {
            if let Some(body) = body {
                format!("Upstream error ({status}): {body}")
            } else {
                format!("Upstream error ({status})")
            }
        }
        ProxyError::Timeout(msg) => format!("Request timeout: {msg}"),
        ProxyError::ForwardFailed(msg) => format!("Forwarding failed: {msg}"),
        ProxyError::NoAvailableProvider => "No provider available".to_string(),
        ProxyError::AllProvidersCircuitOpen => {
            "All providers are circuit-open, no channel available".to_string()
        }
        ProxyError::NoProvidersConfigured => "No provider configured".to_string(),
        ProxyError::MaxRetriesExceeded => "All providers failed, retries exhausted".to_string(),
        ProxyError::DatabaseError(msg) => format!("Database error: {msg}"),
        ProxyError::TransformError(msg) => format!("Request/response transform error: {msg}"),
        _ => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_upstream_error() {
        let error = ProxyError::UpstreamError {
            status: 401,
            body: Some("Unauthorized".to_string()),
        };
        assert_eq!(map_proxy_error_to_status(&error), 401);
    }

    #[test]
    fn test_map_timeout_error() {
        let error = ProxyError::Timeout("Request timeout".to_string());
        assert_eq!(map_proxy_error_to_status(&error), 504);
    }

    #[test]
    fn test_map_connection_error() {
        let error = ProxyError::ForwardFailed("Connection refused".to_string());
        assert_eq!(map_proxy_error_to_status(&error), 502);
    }

    #[test]
    fn test_map_no_provider_error() {
        let error = ProxyError::NoAvailableProvider;
        assert_eq!(map_proxy_error_to_status(&error), 503);
    }

    #[test]
    fn test_get_error_message() {
        let error = ProxyError::UpstreamError {
            status: 500,
            body: Some("Internal Server Error".to_string()),
        };
        let msg = get_error_message(&error);
        assert!(msg.contains("Upstream error"));
        assert!(msg.contains("500"));
        assert!(msg.contains("Internal Server Error"));
    }
}
