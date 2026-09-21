//! Global HTTP client module
//!
//! Provides an HTTP client that honours the global proxy configuration.
//! Every module that sends HTTP requests should use the client from this module.

use once_cell::sync::OnceCell;
use reqwest::Client;
use std::env;
use std::net::IpAddr;
use std::sync::RwLock;
use std::time::Duration;

/// Global HTTP client instance
static GLOBAL_CLIENT: OnceCell<RwLock<Client>> = OnceCell::new();

/// Direct client used only for loopback provider endpoints when no explicit
/// upstream proxy is configured.
static LOOPBACK_CLIENT: OnceCell<Client> = OnceCell::new();

/// Current proxy URL (for logging and status queries)
static CURRENT_PROXY_URL: OnceCell<RwLock<Option<String>>> = OnceCell::new();

/// Port the CC Switch proxy server is currently listening on
static LOCAL_PROXY_PORT: OnceCell<RwLock<u16>> = OnceCell::new();

/// Sets the listening port of the CC Switch proxy server
///
/// Call this when the proxy server starts so system-proxy detection recognises our own port
pub fn set_proxy_port(port: u16) {
    if let Some(lock) = LOCAL_PROXY_PORT.get() {
        if let Ok(mut current_port) = lock.write() {
            *current_port = port;
            log::debug!("[GlobalProxy] Updated CC Switch proxy port to {port}");
        }
    } else {
        let _ = LOCAL_PROXY_PORT.set(RwLock::new(port));
        log::debug!("[GlobalProxy] Initialized CC Switch proxy port to {port}");
    }
}

/// Returns the listening port of the CC Switch proxy server
fn get_proxy_port() -> u16 {
    LOCAL_PROXY_PORT
        .get()
        .and_then(|lock| lock.read().ok())
        .map(|port| *port)
        .unwrap_or(15721) // default port as fallback
}

/// Initializes the global HTTP client
///
/// Should be called once at application startup.
///
/// # Arguments
/// * `proxy_url` - proxy URL, e.g. `http://127.0.0.1:7890` or `socks5://127.0.0.1:1080`
///   Pass None or an empty string for a direct connection
pub fn init(proxy_url: Option<&str>) -> Result<(), String> {
    let effective_url = proxy_url.filter(|s| !s.trim().is_empty());
    let client = build_client(effective_url)?;

    // Try to initialize the global client; if it already exists, warn and update via apply_proxy
    if GLOBAL_CLIENT.set(RwLock::new(client.clone())).is_err() {
        log::warn!(
            "[GlobalProxy] [GP-003] Already initialized, updating instead: {}",
            effective_url
                .map(mask_url)
                .unwrap_or_else(|| "direct connection".to_string())
        );
        // Already initialized, update via apply_proxy instead
        return apply_proxy(proxy_url);
    }

    // Initialize the recorded proxy URL
    let _ = CURRENT_PROXY_URL.set(RwLock::new(effective_url.map(|s| s.to_string())));

    log::info!(
        "[GlobalProxy] Initialized: {}",
        effective_url
            .map(mask_url)
            .unwrap_or_else(|| "direct connection".to_string())
    );

    Ok(())
}

/// Applies the proxy configuration (assumed already validated)
///
/// Applies the proxy configuration to the global client without extra validation.
/// Call this after validate_proxy succeeds.
///
/// # Arguments
/// * `proxy_url` - proxy URL; None or an empty string means a direct connection
pub fn apply_proxy(proxy_url: Option<&str>) -> Result<(), String> {
    let effective_url = proxy_url.filter(|s| !s.trim().is_empty());
    let new_client = build_client(effective_url)?;

    // Update the client
    if let Some(lock) = GLOBAL_CLIENT.get() {
        let mut client = lock.write().map_err(|e| {
            log::error!("[GlobalProxy] [GP-001] Failed to acquire write lock: {e}");
            "Failed to update proxy: lock poisoned".to_string()
        })?;
        *client = new_client;
    } else {
        // Initialize it if that has not happened yet
        return init(proxy_url);
    }

    // Update the recorded proxy URL
    if let Some(lock) = CURRENT_PROXY_URL.get() {
        let mut url = lock.write().map_err(|e| {
            log::error!("[GlobalProxy] [GP-002] Failed to acquire URL write lock: {e}");
            "Failed to update proxy URL record: lock poisoned".to_string()
        })?;
        *url = effective_url.map(|s| s.to_string());
    }

    log::info!(
        "[GlobalProxy] Applied: {}",
        effective_url
            .map(mask_url)
            .unwrap_or_else(|| "direct connection".to_string())
    );

    Ok(())
}

/// Returns the global HTTP client
///
/// Returns the proxy-configured client when a proxy is set, otherwise one that follows the system proxy.
pub fn get() -> Client {
    GLOBAL_CLIENT
        .get()
        .and_then(|lock| lock.read().ok())
        .map(|c| c.clone())
        .unwrap_or_else(|| {
            log::warn!("[GlobalProxy] [GP-004] Client not initialized, using fallback");
            build_client(None).unwrap_or_default()
        })
}

/// An ambient OS proxy must not intercept a user's localhost gateway. Explicit
/// product proxy settings still win and continue to use the global client.
pub fn get_for_url(target_url: &str) -> Client {
    if get_current_proxy_url().is_none() && target_url_is_loopback(target_url) {
        return LOOPBACK_CLIENT
            .get_or_init(|| {
                base_client_builder()
                    .no_proxy()
                    .build()
                    .unwrap_or_else(|error| {
                        log::error!(
                            "[GlobalProxy] Failed to build loopback-direct client: {error}"
                        );
                        get()
                    })
            })
            .clone();
    }
    get()
}

/// Returns the current proxy URL
///
/// Returns the currently configured proxy URL; None means a direct connection.
pub fn get_current_proxy_url() -> Option<String> {
    CURRENT_PROXY_URL
        .get()
        .and_then(|lock| lock.read().ok())
        .and_then(|url| url.clone())
}

/// Builds an HTTP client
fn build_client(proxy_url: Option<&str>) -> Result<Client, String> {
    let mut builder = base_client_builder();

    // Use the proxy when an address is given, otherwise follow the system proxy
    if let Some(url) = proxy_url {
        // Validate the URL format and scheme first
        let parsed = url::Url::parse(url)
            .map_err(|e| format!("Invalid proxy URL '{}': {}", mask_url(url), e))?;

        let scheme = parsed.scheme();
        if !["http", "https", "socks5", "socks5h"].contains(&scheme) {
            return Err(format!(
                "Invalid proxy scheme '{}' in URL '{}'. Supported: http, https, socks5, socks5h",
                scheme,
                mask_url(url)
            ));
        }

        let proxy = reqwest::Proxy::all(url)
            .map_err(|e| format!("Invalid proxy URL '{}': {}", mask_url(url), e))?;
        builder = builder.proxy(proxy);
        log::debug!("[GlobalProxy] Proxy configured: {}", mask_url(url));
    } else {
        // With no global proxy set, let reqwest auto-detect the system proxy (env vars)
        // If the system proxy points at this machine, disable it to avoid a loop
        if system_proxy_points_to_loopback() {
            builder = builder.no_proxy();
            log::warn!(
                "[GlobalProxy] System proxy points to localhost, bypassing to avoid recursion"
            );
        } else {
            log::debug!("[GlobalProxy] Following system proxy (no explicit proxy configured)");
        }
    }

    builder
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
}

fn base_client_builder() -> reqwest::ClientBuilder {
    Client::builder()
        .timeout(Duration::from_secs(600))
        .connect_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .tcp_keepalive(Duration::from_secs(60))
        // Disable reqwest auto-decompression so it cannot overwrite the client's original accept-encoding header.
        // response_processor decompresses responses manually based on content-encoding.
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
}

fn target_url_is_loopback(target_url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(target_url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn system_proxy_points_to_loopback() -> bool {
    const KEYS: [&str; 6] = [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ];

    KEYS.iter()
        .filter_map(|key| env::var(key).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .any(|value| proxy_points_to_loopback(&value))
}

fn proxy_points_to_loopback(value: &str) -> bool {
    fn host_is_loopback(host: &str) -> bool {
        if host.eq_ignore_ascii_case("localhost") {
            return true;
        }
        host.parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
    }

    // Check whether it points at CC Switch's own proxy port
    // Only a proxy pointing at ourselves needs skipping, to avoid recursion
    fn is_cc_switch_proxy_port(port: Option<u16>) -> bool {
        let cc_switch_port = get_proxy_port();
        port == Some(cc_switch_port)
    }

    if let Ok(parsed) = url::Url::parse(value) {
        if let Some(host) = parsed.host_str() {
            // Return true only when the host is loopback and the port is CC Switch's own
            return host_is_loopback(host) && is_cc_switch_proxy_port(parsed.port());
        }
        return false;
    }

    let with_scheme = format!("http://{value}");
    if let Ok(parsed) = url::Url::parse(&with_scheme) {
        if let Some(host) = parsed.host_str() {
            return host_is_loopback(host) && is_cc_switch_proxy_port(parsed.port());
        }
    }

    false
}

/// Masks sensitive information in a URL (for logging)
pub fn mask_url(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        // Hide username and password, keep scheme, host, and port
        let host = parsed.host_str().unwrap_or("?");
        match parsed.port() {
            Some(port) => format!("{}://{}:{}", parsed.scheme(), host, port),
            None => format!("{}://{}", parsed.scheme(), host),
        }
    } else {
        // URL parsing failed, return a partial value. The cut point falls back to the nearest char
        // boundary so it never splits a multi-byte UTF-8 char and panics.
        if url.len() > 20 {
            let cut = (0..=20)
                .rev()
                .find(|&i| url.is_char_boundary(i))
                .unwrap_or(0);
            format!("{}...", &url[..cut])
        } else {
            url.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_mask_url() {
        assert_eq!(mask_url("http://127.0.0.1:7890"), "http://127.0.0.1:7890");
        assert_eq!(
            mask_url("http://user:pass@127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(
            mask_url("socks5://admin:secret@proxy.example.com:1080"),
            "socks5://proxy.example.com:1080"
        );
        // A URL without a port must not render ":?"
        assert_eq!(
            mask_url("http://proxy.example.com"),
            "http://proxy.example.com"
        );
        assert_eq!(
            mask_url("https://user:pass@proxy.example.com"),
            "https://proxy.example.com"
        );
    }

    #[test]
    fn test_mask_url_does_not_panic_on_multibyte_boundary() {
        // A string that Url::parse cannot handle and whose byte 20 lands inside a multi-byte char.
        // Regression for the mask_url out-of-bounds panic in https://github.com/farion1231/cc-switch.
        let bad = "€€€€€€€";
        assert!(bad.len() > 20 && !bad.is_char_boundary(20));
        let masked = mask_url(bad);
        assert!(masked.ends_with("..."));
    }

    #[test]
    fn test_build_client_direct() {
        let result = build_client(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_client_with_http_proxy() {
        let result = build_client(Some("http://127.0.0.1:7890"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_client_with_socks5_proxy() {
        let result = build_client(Some("socks5://127.0.0.1:1080"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_client_invalid_url() {
        // reqwest::Proxy::all does not fail immediately on some invalid URLs
        // Use a clearly invalid scheme to force the error
        let result = build_client(Some("invalid-scheme://127.0.0.1:7890"));
        assert!(result.is_err(), "Should reject invalid proxy scheme");
    }

    #[test]
    fn test_proxy_points_to_loopback() {
        // Set the CC Switch proxy port to 15721 (the default)
        set_proxy_port(15721);

        // Only a loopback address on CC Switch's own port returns true
        assert!(proxy_points_to_loopback("http://127.0.0.1:15721"));
        assert!(proxy_points_to_loopback("socks5://localhost:15721"));
        assert!(proxy_points_to_loopback("127.0.0.1:15721"));

        // Other loopback ports must not be skipped (other local proxy tools are allowed)
        assert!(!proxy_points_to_loopback("http://127.0.0.1:7890"));
        assert!(!proxy_points_to_loopback("socks5://localhost:1080"));

        // Non-loopback addresses must not be skipped
        assert!(!proxy_points_to_loopback("http://192.168.1.10:7890"));
        assert!(!proxy_points_to_loopback("http://192.168.1.10:15721"));
    }

    #[test]
    fn target_url_loopback_detection_is_destination_scoped() {
        assert!(target_url_is_loopback("http://127.0.0.1:4317/v1"));
        assert!(target_url_is_loopback("http://[::1]:4317/v1"));
        assert!(target_url_is_loopback("https://localhost:8443/v1"));
        assert!(!target_url_is_loopback("https://api.openai.com/v1"));
        assert!(!target_url_is_loopback("not a URL"));
    }

    #[test]
    fn test_system_proxy_points_to_loopback() {
        let _guard = env_lock().lock().unwrap();

        // Set the CC Switch proxy port
        set_proxy_port(15721);

        let keys = [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ];

        for key in &keys {
            std::env::remove_var(key);
        }

        // A proxy pointing at the CC Switch port must be skipped
        std::env::set_var("HTTP_PROXY", "http://127.0.0.1:15721");
        assert!(system_proxy_points_to_loopback());

        // A local proxy on a different port must not be skipped
        std::env::set_var("HTTP_PROXY", "http://127.0.0.1:7890");
        assert!(!system_proxy_points_to_loopback());

        // Non-loopback addresses must not be skipped
        std::env::set_var("HTTP_PROXY", "http://10.0.0.2:7890");
        assert!(!system_proxy_points_to_loopback());

        for key in &keys {
            std::env::remove_var(key);
        }
    }
}
