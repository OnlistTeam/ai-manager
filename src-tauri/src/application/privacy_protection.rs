//! Privacy protection for proxied traffic (ADR-0049): what to hide (keys and
//! passwords, personal information, the user's own words), applied by the
//! local routing proxy.

use crate::compat::ccswitch::proxy_privacy;
use crate::compat::ccswitch::settings::SettingsStore;
use crate::domain::{AppError, PrivacyProtection, PrivacyProtectionPatch};
use crate::infrastructure::privacy_key;

pub struct PrivacyProtectionService;

impl PrivacyProtectionService {
    /// Hands the proxy its key and the stored choices. Failures never block
    /// startup and never turn protection off: unreadable settings count as
    /// the defaults, and an unwritable key file falls back to a key for this
    /// run only.
    pub fn initialize(app_handle: &tauri::AppHandle) {
        let settings = SettingsStore::open(app_handle)
            .and_then(|store| store.load_privacy_protection())
            .unwrap_or_else(|error| {
                log::warn!(
                    "[Privacy] could not read the privacy protection settings; using the defaults ({})",
                    error.message_key
                );
                PrivacyProtection::default()
            });
        let key = privacy_key::load_or_create(&privacy_key::key_path()).unwrap_or_else(|error| {
            log::warn!(
                "[Privacy] could not store the placeholder key; using one for this run only ({})",
                error.kind()
            );
            privacy_key::random_key()
        });
        proxy_privacy::configure(key, &settings);
    }

    pub fn load(app_handle: &tauri::AppHandle) -> Result<PrivacyProtection, AppError> {
        SettingsStore::open(app_handle)?.load_privacy_protection()
    }

    /// Applies `patch` to the stored settings, then hands the proxy what was
    /// read back.
    pub fn save(
        app_handle: &tauri::AppHandle,
        patch: PrivacyProtectionPatch,
    ) -> Result<PrivacyProtection, AppError> {
        let store = SettingsStore::open(app_handle)?;
        let next = store.load_privacy_protection()?.apply(patch)?;
        let saved = store.save_privacy_protection(&next)?;
        proxy_privacy::apply(&saved);
        Ok(saved)
    }
}
