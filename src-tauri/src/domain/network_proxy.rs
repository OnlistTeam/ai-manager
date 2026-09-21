//! Safe product projection of the inherited outbound proxy setting.
//!
//! A pre-existing legacy proxy can contain credentials or a remote hostname.
//! Such a value is reported as protected but is never returned to the renderer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProxySettings {
    pub configured: bool,
    pub url: Option<String>,
    pub protected: bool,
}

#[cfg(test)]
mod tests {
    use super::NetworkProxySettings;

    #[test]
    fn protected_values_have_no_wire_slot_for_credentials() {
        let settings = NetworkProxySettings {
            configured: true,
            url: None,
            protected: true,
        };
        assert_eq!(
            serde_json::to_string(&settings).expect("serialize"),
            r#"{"configured":true,"url":null,"protected":true}"#
        );
    }
}
