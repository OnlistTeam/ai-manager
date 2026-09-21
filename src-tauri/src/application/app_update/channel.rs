use url::Url;

pub(super) const PRODUCT_DOWNLOAD_PAGE: &str = "https://aimanager.tools/download";

const STABLE_ENDPOINT: &str = "https://dl.aimanager.tools/ai-manager/latest.json";
const STAGING_ENDPOINT: &str = "https://dl.aimanager.tools/ai-manager/staging/latest.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateChannel {
    Stable,
    Staging,
}

#[derive(Clone)]
pub(super) struct UpdateChannelConfig {
    pub(super) endpoints: Vec<Url>,
}

pub(super) fn trusted_update_channel(app: &tauri::AppHandle) -> Option<UpdateChannelConfig> {
    let config = app
        .config()
        .plugins
        .0
        .get("updater")
        .cloned()
        .and_then(|value| serde_json::from_value::<tauri_plugin_updater::Config>(value).ok());
    resolve_update_channel(
        release_channel_enabled(),
        option_env!("AI_MANAGER_UPDATE_CHANNEL"),
        config,
    )
}

fn release_channel_enabled() -> bool {
    matches!(option_env!("AI_MANAGER_UPDATE_CHANNEL_ENABLED"), Some("1"))
}

fn resolve_update_channel(
    enabled: bool,
    channel: Option<&str>,
    config: Option<tauri_plugin_updater::Config>,
) -> Option<UpdateChannelConfig> {
    if !enabled {
        return None;
    }
    let config = config?;
    if !config_has_trusted_channel(&config) {
        return None;
    }
    let channel = match channel {
        Some("stable") => UpdateChannel::Stable,
        Some("staging") => UpdateChannel::Staging,
        _ => return None,
    };
    let endpoints = match channel {
        UpdateChannel::Stable => stable_endpoints()?,
        UpdateChannel::Staging => vec![trusted_endpoint(STAGING_ENDPOINT)?],
    };
    Some(UpdateChannelConfig { endpoints })
}

fn config_has_trusted_channel(config: &tauri_plugin_updater::Config) -> bool {
    !config.dangerous_insecure_transport_protocol
        && !config.dangerous_accept_invalid_certs
        && !config.dangerous_accept_invalid_hostnames
        && stable_endpoints().is_some_and(|endpoints| config.endpoints == endpoints)
        && !config.pubkey.trim().is_empty()
}

fn stable_endpoints() -> Option<Vec<Url>> {
    Some(vec![trusted_endpoint(STABLE_ENDPOINT)?])
}

fn trusted_endpoint(value: &str) -> Option<Url> {
    let endpoint = Url::parse(value).ok()?;
    if endpoint.scheme() != "https"
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return None;
    }
    Some(endpoint)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use url::Url;

    fn configured(channel: Option<&str>, value: serde_json::Value) -> bool {
        super::resolve_update_channel(
            true,
            channel,
            serde_json::from_value::<tauri_plugin_updater::Config>(value).ok(),
        )
        .is_some()
    }

    fn stable_config() -> serde_json::Value {
        json!({
            "pubkey": "key",
            "endpoints": [super::STABLE_ENDPOINT]
        })
    }

    #[test]
    fn updater_trust_requires_the_approved_endpoint_and_a_public_key() {
        assert!(!configured(
            Some("stable"),
            json!({ "pubkey": "key", "endpoints": [] })
        ));
        assert!(!configured(
            Some("stable"),
            json!({
                "pubkey": "",
                "endpoints": [super::STABLE_ENDPOINT]
            })
        ));
        assert!(!configured(
            Some("stable"),
            json!({
                "pubkey": "key",
                "endpoints": [super::STABLE_ENDPOINT, super::STAGING_ENDPOINT]
            })
        ));
        assert!(configured(Some("stable"), stable_config()));
    }

    #[test]
    fn insecure_update_endpoints_never_make_a_channel_ready() {
        assert!(!configured(
            Some("stable"),
            json!({
                "pubkey": "key",
                "endpoints": ["http://updates.example.com/latest.json"]
            })
        ));
        let mut config = stable_config();
        config["dangerousAcceptInvalidCerts"] = json!(true);
        assert!(!configured(Some("stable"), config));
    }

    #[test]
    fn staging_is_an_isolated_compiled_channel() {
        let channel = super::resolve_update_channel(
            true,
            Some("staging"),
            serde_json::from_value::<tauri_plugin_updater::Config>(stable_config()).ok(),
        )
        .expect("trusted staging channel");
        assert_eq!(
            channel
                .endpoints
                .iter()
                .map(Url::as_str)
                .collect::<Vec<_>>(),
            [super::STAGING_ENDPOINT]
        );
        assert!(!configured(Some("preview"), stable_config()));
        assert!(!configured(None, stable_config()));
    }

    #[test]
    fn stable_resolves_the_single_product_endpoint() {
        let channel = super::resolve_update_channel(
            true,
            Some("stable"),
            serde_json::from_value::<tauri_plugin_updater::Config>(stable_config()).ok(),
        )
        .expect("trusted stable channel");
        assert_eq!(
            channel
                .endpoints
                .iter()
                .map(Url::as_str)
                .collect::<Vec<_>>(),
            [super::STABLE_ENDPOINT]
        );
    }

    #[test]
    fn a_valid_reserved_endpoint_stays_off_without_release_opt_in() {
        assert!(super::resolve_update_channel(
            false,
            Some("stable"),
            serde_json::from_value::<tauri_plugin_updater::Config>(stable_config()).ok(),
        )
        .is_none());
    }

    #[test]
    fn endpoint_lookalikes_do_not_enter_the_trust_channel() {
        assert!(!configured(
            Some("stable"),
            json!({
                "pubkey": "key",
                "endpoints": ["https://dl.aimanager.tools.example/ai-manager/latest.json"]
            })
        ));
    }
}
