//! Facade over the privacy filter inside the inherited routing proxy
//! (ADR-0049). The product decides the key and what to hide; the proxy only
//! applies them to the traffic it forwards.

use crate::domain::PrivacyProtection;
use crate::proxy::privacy::{self, Rules};

fn rules(settings: &PrivacyProtection) -> Rules {
    Rules::new(
        settings.mask_secrets,
        settings.mask_personal,
        &settings.words,
    )
}

/// Installs the per-install hash key and the stored choices. Called once at startup.
pub fn configure(key: [u8; 32], settings: &PrivacyProtection) {
    privacy::configure(key, rules(settings));
}

/// Applies changed choices to requests forwarded from now on.
pub fn apply(settings: &PrivacyProtection) {
    privacy::set_rules(rules(settings));
}
