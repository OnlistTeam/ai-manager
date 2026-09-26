//! Compatibility layer for the product settings (ADR-0003 / decision 1 / decision 2).
//!
//! This is the **only** place that knows two things: the product settings live in the
//! upstream `settings` KV table, and what the product keys are called. The signatures above
//! contain only `crate::domain::ProductSettings`.
//!
//! Upstream files are unmodified by this layer: `mod database;` / `mod store;` are modules
//! private to the crate root and this module is a descendant of the crate root, so they are
//! visible naturally (the same reasoning as in Phases 4/5).

use std::sync::Arc;

use crate::database::Database;
use crate::domain::{
    normalize_privacy_words, AppError, DesktopAppId, DownloadStrategy, ErrorCode, ExtensionKind,
    ExtensionScope, PrivacyProtection, ProductSettings, TerminalAppId, ToolId,
};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::store::AppState;

/// The product setting key names. All 16 keys upstream writes into this table today are
/// snake_case with **not a single dot** (decision 1 lists them all), so a dotted prefix
/// cannot structurally collide with them.
const ADVANCED_MODE_KEY: &str = "aimgr.advancedMode";
const IMPORT_PROMPT_SEEN_KEY: &str = "aimgr.importPromptSeen";
const TOOL_SCOPE_KEY: &str = "aimgr.toolScope";
const EXTENSION_SCOPE_KEY: &str = "aimgr.extensionScope";
const EXTENSION_KIND_KEY: &str = "aimgr.extensionKind";
const DOWNLOAD_STRATEGY_KEY: &str = "aimgr.downloadStrategy";
const TERMINAL_APP_KEY: &str = "aimgr.terminalApp";
/// ADR-0049. Kept out of `ProductSettings` so they have exactly one write path.
const PRIVACY_MASK_SECRETS_KEY: &str = "aimgr.privacy.maskSecrets";
const PRIVACY_MASK_PERSONAL_KEY: &str = "aimgr.privacy.maskPersonal";
/// A JSON array of strings.
const PRIVACY_WORDS_KEY: &str = "aimgr.privacy.words";

/// All product keys. For the guard test and manual inspection; the production path never iterates it.
pub const PRODUCT_SETTING_KEYS: [&str; 10] = [
    ADVANCED_MODE_KEY,
    IMPORT_PROMPT_SEEN_KEY,
    TOOL_SCOPE_KEY,
    EXTENSION_SCOPE_KEY,
    EXTENSION_KIND_KEY,
    DOWNLOAD_STRATEGY_KEY,
    TERMINAL_APP_KEY,
    PRIVACY_MASK_SECRETS_KEY,
    PRIVACY_MASK_PERSONAL_KEY,
    PRIVACY_WORDS_KEY,
];

/// An empty string = never chosen. The upstream DAO exposes no "delete a key" interface, and
/// writing an empty string is far cheaper than editing upstream files just to delete a row;
/// on read, an empty string takes the same fallback as "the key does not exist at all".
const UNSET: &str = "";

/// Upstream errors may be bare localized strings and may carry file paths. Never hand them
/// over as is: redact and truncate first, and only put them into `technical_message` (View
/// Details in §42).
fn detail<E: std::fmt::Display>(error: E) -> String {
    truncate_tail(&redact_secrets(&error.to_string()), 20, 2000)
}

fn load_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.settings.loadFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn save_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.settings.saveFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

/// Written to match the upstream `get_bool_flag`, which only accepts `"true"` and `"1"`.
fn bool_value(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn encode_tool(value: Option<ToolId>) -> &'static str {
    value.map(|id| id.as_str()).unwrap_or(UNSET)
}

fn encode_terminal(value: Option<TerminalAppId>) -> &'static str {
    value.map(|id| id.as_str()).unwrap_or(UNSET)
}

fn decode_terminal(raw: Option<String>) -> Option<TerminalAppId> {
    raw.as_deref().and_then(TerminalAppId::from_str_id)
}

fn encode_kind(value: Option<ExtensionKind>) -> &'static str {
    value.map(|kind| kind.as_str()).unwrap_or(UNSET)
}

fn encode_scope(value: Option<ExtensionScope>) -> String {
    value.map(|scope| scope.stable_key()).unwrap_or_default()
}

/// An unrecognized string (written by an older version, hand-edited by the user, or restored
/// from another machine's database) counts as "never chosen" rather than an error — one
/// forgotten tab is not worth making the whole settings read fail.
fn decode_tool(raw: Option<String>) -> Option<ToolId> {
    raw.as_deref().and_then(ToolId::from_str_id)
}

fn decode_kind(raw: Option<String>) -> Option<ExtensionKind> {
    raw.as_deref().and_then(ExtensionKind::from_str_id)
}

fn decode_scope(raw: Option<String>) -> Option<ExtensionScope> {
    let raw = raw.as_deref()?;
    if let Some(id) = raw.strip_prefix("tool:") {
        return ToolId::from_str_id(id).map(ExtensionScope::tool);
    }
    if let Some(id) = raw.strip_prefix("desktop-app:") {
        return DesktopAppId::from_str_id(id).map(ExtensionScope::desktop_app);
    }
    None
}

fn encode_download_strategy(value: DownloadStrategy) -> &'static str {
    match value {
        DownloadStrategy::OfficialOnly => "officialOnly",
        DownloadStrategy::Automatic => "automatic",
    }
}

fn decode_download_strategy(raw: Option<String>) -> DownloadStrategy {
    // `officialOnly` was exposed by an earlier test build. Normalize both old
    // values and a missing key to the product's region-neutral automatic
    // policy: official first, then a bounded npm-only network fallback.
    let _legacy = raw;
    DownloadStrategy::Automatic
}

/// A missing, empty or unrecognized value reads as `default`.
fn decode_flag(raw: Option<String>, default: bool) -> bool {
    match raw.as_deref() {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}

/// A missing or unreadable list reads as no words. The stored list was
/// normalized when it was saved; normalizing again keeps a hand-edited value
/// inside the same limits.
fn decode_privacy_words(raw: Option<String>) -> Vec<String> {
    raw.as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .and_then(|words| normalize_privacy_words(&words).ok())
        .unwrap_or_default()
}

/// Everything a backup restore or archive import must keep from this machine.
pub struct PreservedPreferences {
    settings: ProductSettings,
    privacy_protection: PrivacyProtection,
}

/// Handle to the upstream KV store. The fields are private, so the layers above can never reach `Database`.
pub struct SettingsStore {
    db: Arc<Database>,
}

impl SettingsStore {
    /// Constructor shared by the production path, the tests and `backup.rs` (which uses it to
    /// preserve preferences during a restore).
    /// `pub(super)` rather than private: `ccswitch::backup` is a sibling module and cannot see
    /// a private function.
    /// **Deliberately not `#[cfg(test)]`**: the scanner in `message_keys.rs` stops at the first
    /// `#[cfg(test)]` in a file, and one in the middle would hide every message_key after it
    /// from the guard.
    pub(super) fn with_db(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn open(app_handle: &tauri::AppHandle) -> Result<Self, AppError> {
        let state = tauri::Manager::try_state::<AppState>(app_handle).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.settings.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        Ok(Self::with_db(state.db.clone()))
    }

    pub fn load(&self) -> Result<ProductSettings, AppError> {
        let tool_scope = decode_tool(self.db.get_setting(TOOL_SCOPE_KEY).map_err(load_failed)?);
        let raw_extension_scope = self
            .db
            .get_setting(EXTENSION_SCOPE_KEY)
            .map_err(load_failed)?;
        // Before R3.1 only the shared CLI tool selection existed. Migrate it
        // only when the new key is absent; an explicitly stored empty value
        // remains a deliberate "no remembered extension scope".
        let extension_scope = match raw_extension_scope {
            Some(raw) => decode_scope(Some(raw)),
            None => tool_scope.map(ExtensionScope::tool),
        };
        Ok(ProductSettings {
            advanced_mode: self
                .db
                .get_bool_flag(ADVANCED_MODE_KEY)
                .map_err(load_failed)?,
            import_prompt_seen: self
                .db
                .get_bool_flag(IMPORT_PROMPT_SEEN_KEY)
                .map_err(load_failed)?,
            tool_scope,
            extension_scope,
            extension_kind: decode_kind(
                self.db
                    .get_setting(EXTENSION_KIND_KEY)
                    .map_err(load_failed)?,
            ),
            download_strategy: decode_download_strategy(
                self.db
                    .get_setting(DOWNLOAD_STRATEGY_KEY)
                    .map_err(load_failed)?,
            ),
            terminal_app: decode_terminal(
                self.db.get_setting(TERMINAL_APP_KEY).map_err(load_failed)?,
            ),
        })
    }

    /// Unset values read as the defaults: keys and passwords hidden,
    /// personal information not, no words.
    pub fn load_privacy_protection(&self) -> Result<PrivacyProtection, AppError> {
        let defaults = PrivacyProtection::default();
        let read = |key: &str| self.db.get_setting(key).map_err(load_failed);
        Ok(PrivacyProtection {
            mask_secrets: decode_flag(read(PRIVACY_MASK_SECRETS_KEY)?, defaults.mask_secrets),
            mask_personal: decode_flag(read(PRIVACY_MASK_PERSONAL_KEY)?, defaults.mask_personal),
            words: decode_privacy_words(read(PRIVACY_WORDS_KEY)?),
        })
    }

    /// Writes all three values and returns what is read back.
    pub fn save_privacy_protection(
        &self,
        settings: &PrivacyProtection,
    ) -> Result<PrivacyProtection, AppError> {
        let words = serde_json::to_string(&settings.words).map_err(save_failed)?;
        self.db
            .set_setting(PRIVACY_MASK_SECRETS_KEY, bool_value(settings.mask_secrets))
            .map_err(save_failed)?;
        self.db
            .set_setting(
                PRIVACY_MASK_PERSONAL_KEY,
                bool_value(settings.mask_personal),
            )
            .map_err(save_failed)?;
        self.db
            .set_setting(PRIVACY_WORDS_KEY, &words)
            .map_err(save_failed)?;
        self.load_privacy_protection()
    }

    /// Captures the per-machine preferences before the database is replaced.
    pub fn preserve(&self) -> Result<PreservedPreferences, AppError> {
        Ok(PreservedPreferences {
            settings: self.load()?,
            privacy_protection: self.load_privacy_protection()?,
        })
    }

    /// Writes captured preferences back after the database was replaced.
    pub fn reinstate(&self, preserved: PreservedPreferences) -> Result<(), AppError> {
        self.save(preserved.settings)?;
        self.save_privacy_protection(&preserved.privacy_protection)?;
        Ok(())
    }

    /// Full replacement (decision 3), returning the result **read back**: the caller always
    /// gets what is really stored in the database, not an echo of what it just sent.
    pub fn save(&self, settings: ProductSettings) -> Result<ProductSettings, AppError> {
        self.db
            .set_setting(ADVANCED_MODE_KEY, bool_value(settings.advanced_mode))
            .map_err(save_failed)?;
        self.db
            .set_setting(
                IMPORT_PROMPT_SEEN_KEY,
                bool_value(settings.import_prompt_seen),
            )
            .map_err(save_failed)?;
        self.db
            .set_setting(TOOL_SCOPE_KEY, encode_tool(settings.tool_scope))
            .map_err(save_failed)?;
        self.db
            .set_setting(EXTENSION_SCOPE_KEY, &encode_scope(settings.extension_scope))
            .map_err(save_failed)?;
        self.db
            .set_setting(EXTENSION_KIND_KEY, encode_kind(settings.extension_kind))
            .map_err(save_failed)?;
        self.db
            .set_setting(
                DOWNLOAD_STRATEGY_KEY,
                encode_download_strategy(settings.download_strategy),
            )
            .map_err(save_failed)?;
        self.db
            .set_setting(TERMINAL_APP_KEY, encode_terminal(settings.terminal_app))
            .map_err(save_failed)?;
        self.load()
    }
}

#[cfg(test)]
#[path = "settings/tests.rs"]
mod tests;
