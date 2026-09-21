//! Use cases for the product settings (spec §9).
//!
//! **Deliberately thin**: settings are app-level and have no capability gate like
//! `ToolCapabilities` to consult. It exists so the data flow keeps a single legal path (command ->
//! application -> compatibility layer), which means the command layer never needs to know what
//! `SettingsStore` looks like. Policies such as Phase 6b's "the user was already asked whether to
//! import CC Switch" will grow here rather than inside the commands.
//!
//! This layer **must not** contain any upstream type — boundary rule R2 blocks that in CI.

use crate::compat::ccswitch::settings::SettingsStore;
use crate::domain::{AppError, ProductSettings, TerminalAppId};

pub struct ProductSettingsService;

fn enable_all_capabilities(mut settings: ProductSettings) -> ProductSettings {
    // Keep the legacy wire field for backward database compatibility, but do
    // not let an older saved value recreate a mode switch in the product.
    settings.advanced_mode = true;
    settings
}

impl ProductSettingsService {
    pub fn load(app_handle: &tauri::AppHandle) -> Result<ProductSettings, AppError> {
        let settings = SettingsStore::open(app_handle)?.load()?;
        // `advancedMode` remains on the wire and in the KV store so older
        // builds can still read this database. It is no longer a user-facing
        // mode: every capability is available and progressive disclosure is
        // handled by the UI at the point of use.
        Ok(enable_all_capabilities(settings))
    }

    /// The terminals really installed on this machine that can take over tool launching and session
    /// resumption. Empty = nothing to choose, so the UI shows no picker and the platform layer uses
    /// the system default.
    pub fn available_terminals() -> Vec<TerminalAppId> {
        crate::platform::installed_terminals()
    }

    /// Full replacement (decision 3). What comes back is the value **read back**, not an echo of the input.
    pub fn save(
        app_handle: &tauri::AppHandle,
        settings: ProductSettings,
    ) -> Result<ProductSettings, AppError> {
        SettingsStore::open(app_handle)?.save(enable_all_capabilities(settings))
    }
}

#[cfg(test)]
mod tests {
    use super::enable_all_capabilities;
    use crate::domain::ProductSettings;

    #[test]
    fn legacy_disabled_value_cannot_restore_a_product_mode_gate() {
        let settings = ProductSettings {
            advanced_mode: false,
            ..ProductSettings::default()
        };

        assert!(enable_all_capabilities(settings).advanced_mode);
    }
}
