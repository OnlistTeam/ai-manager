//! The proxy the environment or the operating system names (ADR-0056).
//!
//! The HTTP client follows the same sources on its own; this reads them only
//! to say which proxy is in use, and to hand it to the installers the app
//! starts, which see only `*_PROXY` variables.

use hyper_util::client::proxy::matcher::Matcher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxySource {
    Environment,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedProxy {
    /// `scheme://host:port`, never with credentials.
    pub url: String,
    pub source: ProxySource,
    /// The source names credentials, which are not carried in `url`.
    pub authenticated: bool,
}

/// Any HTTPS destination outside a bypass list answers the same.
const SAMPLE: &str = "https://api.anthropic.com/";

/// The `*_PROXY` variables first, then the system's settings (macOS network
/// settings, Windows Internet Options), as the HTTP client reads them.
pub fn detect() -> Option<DetectedProxy> {
    let destination: http::Uri = SAMPLE.parse().ok()?;
    if let Some(found) = intercept(&Matcher::from_env(), &destination, ProxySource::Environment) {
        return Some(found);
    }
    intercept(&Matcher::from_system(), &destination, ProxySource::System)
}

fn intercept(
    matcher: &Matcher,
    destination: &http::Uri,
    source: ProxySource,
) -> Option<DetectedProxy> {
    let found = matcher.intercept(destination)?;
    Some(DetectedProxy {
        url: display(found.uri())?,
        source,
        authenticated: found.raw_auth().is_some() || found.basic_auth().is_some(),
    })
}

fn display(uri: &http::Uri) -> Option<String> {
    let host = uri.host()?;
    let scheme = uri.scheme_str().unwrap_or("http");
    Some(match uri.port_u16() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    })
}

#[cfg(test)]
mod tests {
    use super::display;

    #[test]
    fn the_address_is_shown_without_credentials() {
        let uri: http::Uri = "http://user:secret@127.0.0.1:7890/".parse().expect("uri");
        assert_eq!(display(&uri).as_deref(), Some("http://127.0.0.1:7890"));
        let uri: http::Uri = "socks5://[::1]:1080".parse().expect("uri");
        assert_eq!(display(&uri).as_deref(), Some("socks5://[::1]:1080"));
    }
}
