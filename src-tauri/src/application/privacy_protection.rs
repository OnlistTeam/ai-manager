//! Privacy protection for proxied traffic (ADR-0049): one product switch,
//! default on, applied by the local routing proxy.

use crate::compat::ccswitch::proxy_privacy;
use crate::compat::ccswitch::settings::SettingsStore;
use crate::domain::{AppError, PrivacyProtection};
use crate::infrastructure::privacy_key;

pub struct PrivacyProtectionService;

impl PrivacyProtectionService {
    /// Hands the proxy its key and the stored switch. Failures never block
    /// startup and never turn protection off: an unreadable setting counts as
    /// on, and an unwritable key file falls back to a key for this run only.
    pub fn initialize(app_handle: &tauri::AppHandle) {
        let enabled = match SettingsStore::open(app_handle)
            .and_then(|store| store.load_privacy_protection())
        {
            Ok(enabled) => enabled,
            Err(error) => {
                log::warn!(
                    "[Privacy] could not read the privacy protection setting; keeping it on ({})",
                    error.message_key
                );
                true
            }
        };
        let key = privacy_key::load_or_create(&privacy_key::key_path()).unwrap_or_else(|error| {
            log::warn!(
                "[Privacy] could not store the placeholder key; using one for this run only ({})",
                error.kind()
            );
            privacy_key::random_key()
        });
        proxy_privacy::configure(key, enabled);
    }

    pub fn load(app_handle: &tauri::AppHandle) -> Result<PrivacyProtection, AppError> {
        let enabled = SettingsStore::open(app_handle)?.load_privacy_protection()?;
        Ok(PrivacyProtection { enabled })
    }

    /// Stores the switch, then applies the value read back to the proxy.
    pub fn save(
        app_handle: &tauri::AppHandle,
        enabled: bool,
    ) -> Result<PrivacyProtection, AppError> {
        let enabled = SettingsStore::open(app_handle)?.save_privacy_protection(enabled)?;
        proxy_privacy::set_enabled(enabled);
        Ok(PrivacyProtection { enabled })
    }
}
