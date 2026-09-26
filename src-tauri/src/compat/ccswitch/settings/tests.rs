use super::{
    decode_kind, decode_scope, decode_tool, encode_kind, encode_scope, encode_tool, SettingsStore,
    ADVANCED_MODE_KEY, DOWNLOAD_STRATEGY_KEY, PRIVACY_MASK_PERSONAL_KEY, PRIVACY_MASK_SECRETS_KEY,
    PRIVACY_WORDS_KEY, PRODUCT_SETTING_KEYS,
};
use crate::database::Database;
use crate::domain::{
    DesktopAppId, DownloadStrategy, ExtensionKind, ExtensionScope, PrivacyProtection,
    ProductSettings, TerminalAppId, ToolId,
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
    assert_eq!(PRODUCT_SETTING_KEYS.len(), 10);
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

fn chosen_privacy() -> PrivacyProtection {
    PrivacyProtection {
        mask_secrets: false,
        mask_personal: true,
        words: vec!["Project Kite".to_owned(), "张三".to_owned()],
    }
}

#[test]
fn privacy_protection_starts_with_keys_hidden_and_nothing_else() {
    assert_eq!(
        store().load_privacy_protection().expect("defaults"),
        PrivacyProtection::default()
    );
}

#[test]
fn privacy_protection_round_trips_through_three_keys() {
    let store = store();
    let wanted = chosen_privacy();
    assert_eq!(
        store.save_privacy_protection(&wanted).expect("save"),
        wanted
    );
    let raw = |key| store.db.get_setting(key).expect("stored");
    assert_eq!(raw(PRIVACY_MASK_SECRETS_KEY).as_deref(), Some("false"));
    assert_eq!(raw(PRIVACY_MASK_PERSONAL_KEY).as_deref(), Some("true"));
    assert_eq!(
        raw(PRIVACY_WORDS_KEY).as_deref(),
        Some(r#"["Project Kite","张三"]"#)
    );
    assert_eq!(store.load_privacy_protection().expect("load"), wanted);
}

#[test]
fn unreadable_privacy_values_fall_back_to_the_defaults() {
    let store = store();
    for (key, value) in [
        (PRIVACY_MASK_SECRETS_KEY, ""),
        (PRIVACY_MASK_PERSONAL_KEY, "yes"),
        (PRIVACY_WORDS_KEY, "acme, kite"),
    ] {
        store.db.set_setting(key, value).expect("seed");
    }
    assert_eq!(
        store.load_privacy_protection().expect("load"),
        PrivacyProtection::default()
    );
}

#[test]
fn preserved_preferences_include_privacy_protection() {
    let store = store();
    store
        .save_privacy_protection(&chosen_privacy())
        .expect("choose");
    let preserved = store.preserve().expect("preserve");
    store
        .save_privacy_protection(&PrivacyProtection::default())
        .expect("simulate a restore");
    store.reinstate(preserved).expect("reinstate");
    assert_eq!(
        store.load_privacy_protection().expect("load"),
        chosen_privacy()
    );
}
