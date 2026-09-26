//! Facade over the privacy filter inside the inherited routing proxy
//! (ADR-0049). The product decides the key and the switch; the proxy only
//! applies them to the traffic it forwards.

/// Installs the per-install hash key and the stored switch. Called once at startup.
pub fn configure(key: [u8; 32], enabled: bool) {
    crate::proxy::privacy::configure(key, enabled);
}

/// Applies a changed switch to requests forwarded from now on.
pub fn set_enabled(enabled: bool) {
    crate::proxy::privacy::set_enabled(enabled);
}
