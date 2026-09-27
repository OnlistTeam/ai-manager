//! Product-safe outbound proxy facade (ADR-0056).
//!
//! Three modes: automatic (the `*_PROXY` variables, else the system's proxy),
//! off (direct even when the system names a proxy), and a custom address. The
//! product accepts only unauthenticated loopback addresses as custom. That
//! covers the common Clash/V2Ray/Surge setup without ever returning a stored
//! credential or remote corporate proxy address to the renderer.

use std::sync::{Arc, Mutex, PoisonError};

use url::Host;

use crate::database::Database;
use crate::domain::{
    AppError, ErrorCode, NetworkProxyMode, NetworkProxySettings, NetworkProxySource,
};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::platform::system_proxy::{self, DetectedProxy, ProxySource};
use crate::proxy::http_client;
use crate::store::AppState;

static WRITE_LOCK: Mutex<()> = Mutex::new(());
const MAX_URL_BYTES: usize = 512;
/// Product setting beside the inherited `global_proxy_url`: "true" when the
/// proxy is turned off. A saved custom address wins over it.
const DIRECT_KEY: &str = "network_proxy_direct";

fn detail<E: std::fmt::Display>(error: E) -> String {
    truncate_tail(&redact_secrets(&error.to_string()), 12, 1000)
}

fn invalid(reason: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::ConfigParseFailed, "error.networkProxy.invalid")
        .with_technical(detail(reason.into()))
}

fn read_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.networkProxy.readFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn save_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.networkProxy.saveFailed",
    )
    .with_technical(detail(error))
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn safe_loopback_url(raw: &str) -> Result<String, AppError> {
    let value = raw.trim();
    if value.is_empty() || value.len() > MAX_URL_BYTES {
        return Err(invalid("proxy URL is empty or too long"));
    }
    let parsed = url::Url::parse(value).map_err(|error| invalid(error.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https" | "socks5" | "socks5h") {
        return Err(invalid("unsupported proxy scheme"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(invalid(
            "proxy credentials are not accepted by the product UI",
        ));
    }
    let loopback = match parsed.host() {
        // A socks5 URL is not a "special" scheme, so its IPv4 host arrives as
        // a domain string.
        Some(Host::Domain(host)) => {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        }
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    if !loopback {
        return Err(invalid("only a loopback proxy host is accepted"));
    }
    if parsed.port().is_none() {
        return Err(invalid("an explicit proxy port is required"));
    }
    if !matches!(parsed.path(), "" | "/") || parsed.query().is_some() || parsed.fragment().is_some()
    {
        return Err(invalid(
            "proxy URL may not contain a path, query or fragment",
        ));
    }
    Ok(value.trim_end_matches('/').to_string())
}

/// The updater's reqwest build supports HTTP(S) proxies. Return only the
/// already-normalized, credential-free loopback subset so the updater crate's
/// own debug logging can never reveal a protected legacy proxy value.
pub(crate) fn updater_proxy_url() -> Option<url::Url> {
    let raw = http_client::get_current_proxy_url()?;
    updater_proxy_from_raw(&raw)
}

/// The updater builds its own client, so "off" has to be passed on as well.
pub(crate) fn updater_goes_direct() -> bool {
    http_client::get_current_proxy_url().is_none() && http_client::is_direct()
}

/// Reads the saved "off" before the global client is first built.
pub(crate) fn restore_direct(db: &Database) {
    http_client::set_direct(direct_saved(db).unwrap_or(false));
}

/// What the installers the app starts get: a proxy to name in their
/// `*_PROXY` variables, and whether to blank the ones they inherit. An app
/// opened from the Dock or the Start menu has no such variables, so in the
/// automatic mode the system's proxy is named for them, as long as it needs
/// no credentials.
pub(crate) fn child_proxy() -> (Option<String>, bool) {
    if let Some(url) = http_client::get_current_proxy_url() {
        return (Some(url), false);
    }
    if http_client::is_direct() {
        return (None, true);
    }
    match system_proxy::detect() {
        Some(found) if found.source == ProxySource::System && !found.authenticated => {
            (Some(found.url), false)
        }
        _ => (None, false),
    }
}

fn direct_saved(db: &Database) -> Result<bool, AppError> {
    Ok(db.get_setting(DIRECT_KEY).map_err(read_failed)?.as_deref() == Some("true"))
}

/// Preserve the inherited global proxy and official-first client selection
/// behind the compatibility facade. Product platform code supplies only a
/// fixed, native-owned URL and cannot mutate proxy state through this handle.
pub(crate) fn http_client_for_fixed_url(url: &str) -> reqwest::Client {
    http_client::get_for_url(url)
}

fn updater_proxy_from_raw(raw: &str) -> Option<url::Url> {
    let normalized = safe_loopback_url(raw).ok()?;
    let parsed = url::Url::parse(&normalized).ok()?;
    matches!(parsed.scheme(), "http" | "https").then_some(parsed)
}

fn project(
    raw: Option<String>,
    direct: bool,
    detect: impl FnOnce() -> Option<DetectedProxy>,
) -> NetworkProxySettings {
    match raw {
        Some(value) => match safe_loopback_url(&value) {
            Ok(url) => NetworkProxySettings {
                mode: NetworkProxyMode::Custom,
                url: Some(url.clone()),
                protected: false,
                in_use: Some(url),
                source: NetworkProxySource::Custom,
            },
            Err(_) => NetworkProxySettings {
                mode: NetworkProxyMode::Custom,
                url: None,
                protected: true,
                in_use: None,
                source: NetworkProxySource::Custom,
            },
        },
        None if direct => NetworkProxySettings {
            mode: NetworkProxyMode::Off,
            url: None,
            protected: false,
            in_use: None,
            source: NetworkProxySource::Off,
        },
        None => {
            let found = detect();
            NetworkProxySettings {
                mode: NetworkProxyMode::Auto,
                url: None,
                protected: false,
                source: match found.as_ref().map(|found| found.source) {
                    Some(ProxySource::Environment) => NetworkProxySource::Environment,
                    Some(ProxySource::System) => NetworkProxySource::System,
                    None => NetworkProxySource::None,
                },
                in_use: found.map(|found| found.url),
            }
        }
    }
}

pub struct NetworkProxyStore {
    db: Arc<Database>,
}

impl NetworkProxyStore {
    pub fn open(app_handle: &tauri::AppHandle) -> Result<Self, AppError> {
        let state = tauri::Manager::try_state::<AppState>(app_handle).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.networkProxy.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        Ok(Self {
            db: state.db.clone(),
        })
    }

    #[cfg(test)]
    fn with_db(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn load(&self) -> Result<NetworkProxySettings, AppError> {
        let raw = self.db.get_global_proxy_url().map_err(read_failed)?;
        Ok(project(raw, direct_saved(&self.db)?, system_proxy::detect))
    }

    pub fn save(
        &self,
        mode: NetworkProxyMode,
        raw: Option<String>,
    ) -> Result<NetworkProxySettings, AppError> {
        let normalized = match mode {
            NetworkProxyMode::Custom => Some(safe_loopback_url(raw.as_deref().unwrap_or(""))?),
            NetworkProxyMode::Auto | NetworkProxyMode::Off => None,
        };
        let direct = mode == NetworkProxyMode::Off;
        // The guarded value is `()`: a panic under another writer leaves nothing
        // to distrust, so recover instead of refusing every later save (as the
        // operations, session and shell-environment guards already do).
        let _guard = WRITE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let previous = self.db.get_global_proxy_url().map_err(save_failed)?;
        let previous_direct = http_client::is_direct();

        self.db
            .set_global_proxy_url(normalized.as_deref())
            .map_err(save_failed)?;
        self.db
            .set_setting(DIRECT_KEY, if direct { "true" } else { "false" })
            .map_err(save_failed)?;
        http_client::set_direct(direct);
        if let Err(error) = http_client::apply_proxy(normalized.as_deref()) {
            let database_rollback =
                self.db
                    .set_global_proxy_url(previous.as_deref())
                    .and_then(|()| {
                        self.db
                            .set_setting(DIRECT_KEY, if previous_direct { "true" } else { "false" })
                    });
            http_client::set_direct(previous_direct);
            let runtime_rollback = http_client::apply_proxy(previous.as_deref());
            let rollback = match (database_rollback, runtime_rollback) {
                (Ok(()), Ok(())) => "previous proxy restored".to_string(),
                (db, runtime) => format!(
                    "rollback database={:?}, runtime={:?}",
                    db.err().map(detail),
                    runtime.err().map(detail)
                ),
            };
            return Err(save_failed(format!("{error}; {rollback}")));
        }
        self.load()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{project, safe_loopback_url, updater_proxy_from_raw, NetworkProxyStore};
    use crate::database::Database;
    use crate::domain::{NetworkProxyMode, NetworkProxySource};
    use crate::platform::system_proxy::{DetectedProxy, ProxySource};
    use crate::proxy::http_client;

    #[test]
    fn accepts_common_local_proxy_schemes_and_rejects_secret_or_remote_urls() {
        for value in [
            "http://127.0.0.1:7890",
            "socks5://localhost:1080",
            "socks5://127.0.0.1:1080",
            "socks5h://[::1]:1080",
        ] {
            assert_eq!(safe_loopback_url(value).expect("safe URL"), value);
        }
        for value in [
            "http://proxy.example.com:7890",
            "http://user:secret@127.0.0.1:7890",
            "ftp://127.0.0.1:21",
            "http://127.0.0.1",
            "http://127.0.0.1:7890/path",
        ] {
            assert!(safe_loopback_url(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn legacy_sensitive_values_are_reported_but_never_returned() {
        let projected = project(
            Some("http://user:secret@proxy.example.com:8080".to_string()),
            false,
            || None,
        );
        assert_eq!(projected.mode, NetworkProxyMode::Custom);
        assert!(projected.protected);
        assert_eq!(projected.url, None);
        assert_eq!(projected.in_use, None);
    }

    #[test]
    fn automatic_names_the_proxy_the_system_gives_and_off_names_none() {
        let system = || {
            Some(DetectedProxy {
                url: "http://127.0.0.1:7890".into(),
                source: ProxySource::System,
                authenticated: false,
            })
        };
        let auto = project(None, false, system);
        assert_eq!(auto.mode, NetworkProxyMode::Auto);
        assert_eq!(auto.source, NetworkProxySource::System);
        assert_eq!(auto.in_use.as_deref(), Some("http://127.0.0.1:7890"));

        let nothing = project(None, false, || None);
        assert_eq!(nothing.source, NetworkProxySource::None);
        assert_eq!(nothing.in_use, None);

        let off = project(None, true, || panic!("off does not look"));
        assert_eq!(off.mode, NetworkProxyMode::Off);
        assert_eq!(off.in_use, None);

        let custom = project(Some("socks5://127.0.0.1:1080".into()), true, || None);
        assert_eq!(custom.mode, NetworkProxyMode::Custom);
        assert_eq!(custom.in_use.as_deref(), Some("socks5://127.0.0.1:1080"));
    }

    #[test]
    fn product_updater_only_receives_a_safe_http_loopback_proxy() {
        assert_eq!(
            updater_proxy_from_raw("http://127.0.0.1:7890")
                .expect("safe updater proxy")
                .as_str(),
            "http://127.0.0.1:7890/"
        );
        for value in [
            "socks5://127.0.0.1:1080",
            "http://proxy.example.com:7890",
            "http://user:secret@127.0.0.1:7890",
        ] {
            assert!(
                updater_proxy_from_raw(value).is_none(),
                "exposed {value} to updater"
            );
        }
    }

    #[test]
    #[serial_test::serial]
    fn save_and_clear_round_trip_through_the_inherited_setting() {
        let db = Arc::new(Database::memory().expect("memory database"));
        let store = NetworkProxyStore::with_db(db.clone());
        let saved = store
            .save(
                NetworkProxyMode::Custom,
                Some("http://127.0.0.1:7890".to_string()),
            )
            .expect("save");
        assert_eq!(saved.url.as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(
            db.get_global_proxy_url().expect("stored").as_deref(),
            Some("http://127.0.0.1:7890")
        );

        let off = store.save(NetworkProxyMode::Off, None).expect("off");
        assert_eq!(off.mode, NetworkProxyMode::Off);
        assert_eq!(db.get_global_proxy_url().expect("cleared"), None);
        assert!(http_client::is_direct());
        assert_eq!(super::child_proxy(), (None, true));

        let auto = store.save(NetworkProxyMode::Auto, None).expect("auto");
        assert_eq!(auto.mode, NetworkProxyMode::Auto);
        assert!(!http_client::is_direct());
    }

    #[test]
    #[serial_test::serial]
    fn a_custom_mode_needs_a_safe_address() {
        let db = Arc::new(Database::memory().expect("memory database"));
        let store = NetworkProxyStore::with_db(db.clone());
        assert!(store.save(NetworkProxyMode::Custom, None).is_err());
        assert!(store
            .save(
                NetworkProxyMode::Custom,
                Some("http://proxy.example.com:8080".into())
            )
            .is_err());
        assert_eq!(db.get_global_proxy_url().expect("untouched"), None);
    }
}
