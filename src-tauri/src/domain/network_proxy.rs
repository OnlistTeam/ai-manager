//! Safe product projection of the outbound proxy setting (ADR-0056).
//!
//! A pre-existing legacy proxy can contain credentials or a remote hostname.
//! Such a value is reported as protected but is never returned to the renderer.

use serde::{Deserialize, Serialize};

/// Which proxy the app's own requests go through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkProxyMode {
    /// The `*_PROXY` variables, else the system's proxy settings.
    Auto,
    /// Direct, even when the system names a proxy.
    Off,
    /// The address saved in Settings.
    Custom,
}

/// Where the proxy requests use right now comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkProxySource {
    Custom,
    Environment,
    System,
    /// Automatic, and neither the environment nor the system names one.
    None,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProxySettings {
    pub mode: NetworkProxyMode,
    /// The saved custom address; `None` when it is protected or not custom.
    pub url: Option<String>,
    pub protected: bool,
    /// The proxy requests use now, as `scheme://host:port` with no
    /// credentials; `None` when they go direct or the address is protected.
    pub in_use: Option<String>,
    pub source: NetworkProxySource,
}

#[cfg(test)]
mod tests {
    use super::{NetworkProxyMode, NetworkProxySettings, NetworkProxySource};

    #[test]
    fn protected_values_have_no_wire_slot_for_credentials() {
        let settings = NetworkProxySettings {
            mode: NetworkProxyMode::Custom,
            url: None,
            protected: true,
            in_use: None,
            source: NetworkProxySource::Custom,
        };
        assert_eq!(
            serde_json::to_string(&settings).expect("serialize"),
            r#"{"mode":"custom","url":null,"protected":true,"inUse":null,"source":"custom"}"#
        );
    }
}
