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
    AppError, DesktopAppId, DownloadStrategy, ErrorCode, ExtensionKind, ExtensionScope,
    ProductSettings, TerminalAppId, ToolId,
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
const AUTOMATIC_PROVIDER_FAILOVER_KEY: &str = "aimgr.automaticProviderFailover";
const TERMINAL_APP_KEY: &str = "aimgr.terminalApp";

/// All product keys. For the guard test and manual inspection; the production path never iterates it.
pub const PRODUCT_SETTING_KEYS: [&str; 8] = [
    ADVANCED_MODE_KEY,
    IMPORT_PROMPT_SEEN_KEY,
    TOOL_SCOPE_KEY,
    EXTENSION_SCOPE_KEY,
    EXTENSION_KIND_KEY,
    DOWNLOAD_STRATEGY_KEY,
    AUTOMATIC_PROVIDER_FAILOVER_KEY,
    TERMINAL_APP_KEY,
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
            automatic_provider_failover: self
                .db
                .get_bool_flag(AUTOMATIC_PROVIDER_FAILOVER_KEY)
                .map_err(load_failed)?,
            terminal_app: decode_terminal(
                self.db.get_setting(TERMINAL_APP_KEY).map_err(load_failed)?,
            ),
        })
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
            .set_setting(
                AUTOMATIC_PROVIDER_FAILOVER_KEY,
                bool_value(settings.automatic_provider_failover),
            )
            .map_err(save_failed)?;
        self.db
            .set_setting(TERMINAL_APP_KEY, encode_terminal(settings.terminal_app))
            .map_err(save_failed)?;
        self.load()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_kind, decode_scope, decode_tool, encode_kind, encode_scope, encode_tool,
        SettingsStore, ADVANCED_MODE_KEY, DOWNLOAD_STRATEGY_KEY, PRODUCT_SETTING_KEYS,
    };
    use crate::database::Database;
    use crate::domain::{
        DesktopAppId, DownloadStrategy, ExtensionKind, ExtensionScope, ProductSettings,
        TerminalAppId, ToolId,
    };
    use std::sync::Arc;

    fn store() -> SettingsStore {
        SettingsStore::with_db(Arc::new(Database::memory().expect("in-memory database")))
    }

    #[test]
    fn every_product_key_carries_the_product_prefix() {
        // Decision 1: all 16 keys upstream writes into this table are snake_case with not a
        // single dot, so a dotted prefix cannot structurally collide. This test guards the
        // prefix itself — so nobody adding a key later casually writes a bare name.
        for key in PRODUCT_SETTING_KEYS {
            assert!(
                key.starts_with("aimgr."),
                "{key} must live under the product prefix"
            );
        }
        assert_eq!(PRODUCT_SETTING_KEYS.len(), 8);
    }

    #[test]
    fn a_database_without_any_product_key_reads_as_defaults() {
        assert_eq!(store().load().expect("load"), ProductSettings::default());
    }

    #[test]
    fn legacy_preferences_migrate_to_automatic_official_first() {
        for legacy in ["officialOnly", "chinaResilient"] {
            let store = store();
            store
                .db
                .set_setting(DOWNLOAD_STRATEGY_KEY, legacy)
                .expect("seed legacy preference");
            assert_eq!(
                store.load().expect("load").download_strategy,
                DownloadStrategy::Automatic
            );
        }
    }

    #[test]
    fn saving_round_trips_through_the_real_table() {
        let store = store();
        let wanted = ProductSettings {
            advanced_mode: true,
            import_prompt_seen: true,
            tool_scope: Some(ToolId::OpenCode),
            extension_scope: Some(ExtensionScope::desktop_app(DesktopAppId::ClaudeDesktop)),
            extension_kind: Some(ExtensionKind::Prompt),
            download_strategy: DownloadStrategy::Automatic,
            automatic_provider_failover: true,
            terminal_app: Some(TerminalAppId::Ghostty),
        };
        assert_eq!(store.save(wanted).expect("save"), wanted);
        assert_eq!(store.load().expect("load"), wanted);
        assert_eq!(
            store
                .db
                .get_setting(DOWNLOAD_STRATEGY_KEY)
                .expect("read persisted strategy")
                .as_deref(),
            Some("automatic")
        );
    }

    #[test]
    fn clearing_a_scope_reads_back_as_never_chosen() {
        let store = store();
        store
            .save(ProductSettings {
                tool_scope: Some(ToolId::Codex),
                ..ProductSettings::default()
            })
            .expect("save a scope");
        let cleared = store.save(ProductSettings::default()).expect("clear it");
        assert_eq!(cleared.tool_scope, None);
        assert_eq!(store.load().expect("load").tool_scope, None);
    }

    #[test]
    fn a_value_this_build_does_not_understand_degrades_to_never_chosen() {
        // Restoring another machine's database, or removing a ToolId in the future, both end
        // up here. One forgotten tab is not worth making the whole settings read fail.
        assert_eq!(decode_tool(Some("claude".to_string())), None);
        assert_eq!(decode_tool(Some(String::new())), None);
        assert_eq!(decode_tool(None), None);
        assert_eq!(decode_kind(Some("mcpServer".to_string())), None);
        assert_eq!(decode_kind(Some(String::new())), None);
        assert_eq!(decode_scope(Some("tool:claude".to_string())), None);
        assert_eq!(decode_scope(Some("desktop-app:future".to_string())), None);
    }

    #[test]
    fn the_encoding_is_the_domain_string_and_nothing_else() {
        for id in ToolId::ALL {
            assert_eq!(encode_tool(Some(id)), id.as_str());
            assert_eq!(
                decode_tool(Some(encode_tool(Some(id)).to_string())),
                Some(id)
            );
        }
        for kind in ExtensionKind::ALL {
            assert_eq!(encode_kind(Some(kind)), kind.as_str());
            assert_eq!(
                decode_kind(Some(encode_kind(Some(kind)).to_string())),
                Some(kind)
            );
        }
        for scope in [
            ExtensionScope::tool(ToolId::Codex),
            ExtensionScope::desktop_app(DesktopAppId::ClaudeDesktop),
        ] {
            assert_eq!(decode_scope(Some(encode_scope(Some(scope)))), Some(scope));
        }
        assert_eq!(encode_tool(None), "");
        assert_eq!(encode_kind(None), "");
        assert_eq!(encode_scope(None), "");
    }

    #[test]
    fn what_we_write_is_what_the_upstream_bool_reader_understands() {
        // `get_bool_flag` only accepts "true" and "1". This pins our writing down to match it —
        // writing "yes" would make advanced mode impossible to turn on, without any error.
        let store = store();
        store
            .save(ProductSettings {
                advanced_mode: true,
                ..ProductSettings::default()
            })
            .expect("save");
        assert!(store.db.get_bool_flag(ADVANCED_MODE_KEY).expect("flag"));
        assert!(store.load().expect("load").advanced_mode);
    }
}
