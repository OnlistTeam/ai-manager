//! Product-safe outbound proxy facade.
//!
//! The product accepts only unauthenticated loopback proxies. That covers the
//! common Clash/V2Ray/Surge setup without ever returning a stored credential or
//! remote corporate proxy address to the renderer.

use std::sync::{Arc, Mutex, PoisonError};

use url::Host;

use crate::database::Database;
use crate::domain::{AppError, ErrorCode, NetworkProxySettings};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::store::AppState;

static WRITE_LOCK: Mutex<()> = Mutex::new(());
const MAX_URL_BYTES: usize = 512;

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
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
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
    let raw = crate::proxy::http_client::get_current_proxy_url()?;
    updater_proxy_from_raw(&raw)
}

/// Preserve the inherited global proxy and official-first client selection
/// behind the compatibility facade. Product platform code supplies only a
/// fixed, native-owned URL and cannot mutate proxy state through this handle.
pub(crate) fn http_client_for_fixed_url(url: &str) -> reqwest::Client {
    crate::proxy::http_client::get_for_url(url)
}

fn updater_proxy_from_raw(raw: &str) -> Option<url::Url> {
    let normalized = safe_loopback_url(raw).ok()?;
    let parsed = url::Url::parse(&normalized).ok()?;
    matches!(parsed.scheme(), "http" | "https").then_some(parsed)
}

fn project(raw: Option<String>) -> NetworkProxySettings {
    match raw {
        None => NetworkProxySettings {
            configured: false,
            url: None,
            protected: false,
        },
        Some(value) => match safe_loopback_url(&value) {
            Ok(url) => NetworkProxySettings {
                configured: true,
                url: Some(url),
                protected: false,
            },
            Err(_) => NetworkProxySettings {
                configured: true,
                url: None,
                protected: true,
            },
        },
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
        self.db
            .get_global_proxy_url()
            .map(project)
            .map_err(read_failed)
    }

    pub fn save(&self, raw: Option<String>) -> Result<NetworkProxySettings, AppError> {
        let normalized = raw
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(safe_loopback_url)
            .transpose()?;
        // The guarded value is `()`: a panic under another writer leaves nothing
        // to distrust, so recover instead of refusing every later save (as the
        // operations, session and shell-environment guards already do).
        let _guard = WRITE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let previous = self.db.get_global_proxy_url().map_err(save_failed)?;

        self.db
            .set_global_proxy_url(normalized.as_deref())
            .map_err(save_failed)?;
        if let Err(error) = crate::proxy::http_client::apply_proxy(normalized.as_deref()) {
            let database_rollback = self.db.set_global_proxy_url(previous.as_deref());
            let runtime_rollback = crate::proxy::http_client::apply_proxy(previous.as_deref());
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

    #[test]
    fn accepts_common_local_proxy_schemes_and_rejects_secret_or_remote_urls() {
        for value in [
            "http://127.0.0.1:7890",
            "socks5://localhost:1080",
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
        let projected = project(Some(
            "http://user:secret@proxy.example.com:8080".to_string(),
        ));
        assert!(projected.configured);
        assert!(projected.protected);
        assert_eq!(projected.url, None);
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
            .save(Some("http://127.0.0.1:7890".to_string()))
            .expect("save");
        assert_eq!(saved.url.as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(
            db.get_global_proxy_url().expect("stored").as_deref(),
            Some("http://127.0.0.1:7890")
        );

        let cleared = store.save(None).expect("clear");
        assert!(!cleared.configured);
        assert_eq!(db.get_global_proxy_url().expect("cleared"), None);
    }
}
