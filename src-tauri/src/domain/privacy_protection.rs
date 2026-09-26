//! The privacy protection switch for proxied traffic (ADR-0049).
//!
//! Only the user's choice crosses the wire: no masked value, placeholder,
//! count per value or hash key ever does.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyProtection {
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::PrivacyProtection;

    #[test]
    fn the_wire_carries_only_the_switch() {
        assert_eq!(
            serde_json::to_string(&PrivacyProtection { enabled: true }).expect("serialize"),
            r#"{"enabled":true}"#
        );
    }
}
