//! AI Manager system-tray shell.
//!
//! Inherited usage, projects, lightweight mode, and upstream-link controls are
//! not exposed here. Advanced Mode adds a product-owned provider quick switch
//! backed by the same reviewed `ProviderDirectory` used by the main UI.

use std::collections::HashSet;

use sha2::{Digest, Sha256};
use tauri::menu::{CheckMenuItem, Menu, MenuBuilder, MenuItem, Submenu, SubmenuBuilder};
use tauri::{Emitter, Manager};

use crate::application::product_settings::ProductSettingsService;
use crate::application::provider_directory::ProviderDirectory;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{Provider, ToolId};
use crate::error::AppError;
use crate::store::AppState;

pub const TRAY_ID: &str = "ai-manager";
pub const PROVIDER_CHANGED_EVENT: &str = "provider://changed";
const PROVIDER_SWITCH_PREFIX: &str = "provider-switch:";

#[derive(Clone, Copy)]
struct TrayTexts {
    show_main: &'static str,
    quick_switch: &'static str,
    unnamed_service: &'static str,
    quit: &'static str,
}

fn map_locale_to_tray_language(locale: &str) -> &'static str {
    let locale = locale.to_lowercase();
    if locale == "zh" {
        "zh"
    } else if locale.starts_with("zh-tw")
        || locale.starts_with("zh-hk")
        || locale.starts_with("zh-mo")
        || locale.starts_with("zh-hant")
    {
        "zh-TW"
    } else if locale.starts_with("zh") {
        "zh"
    } else if locale.starts_with("ja") {
        "ja"
    } else if locale.starts_with("en") {
        "en"
    } else {
        "zh"
    }
}

fn detect_system_tray_language() -> &'static str {
    sys_locale::get_locale()
        .as_deref()
        .map(map_locale_to_tray_language)
        .unwrap_or("zh")
}

impl TrayTexts {
    fn from_language(language: &str) -> Self {
        match language {
            "en" => Self {
                show_main: "Open AI Manager",
                quick_switch: "Quick Switch",
                unnamed_service: "Unnamed service",
                quit: "Quit",
            },
            "ja" => Self {
                show_main: "AI Manager を開く",
                quick_switch: "クイック切り替え",
                unnamed_service: "名前のないサービス",
                quit: "終了",
            },
            "zh-TW" => Self {
                show_main: "開啟 AI 管家",
                quick_switch: "快速切換",
                unnamed_service: "未命名服務",
                quit: "退出",
            },
            _ => Self {
                show_main: "打开 AI 管家",
                quick_switch: "快速切换",
                unnamed_service: "未命名服务",
                quit: "退出",
            },
        }
    }
}

pub fn create_tray_menu(
    app: &tauri::AppHandle,
    _app_state: &AppState,
) -> Result<Menu<tauri::Wry>, AppError> {
    let settings = crate::settings::get_settings();
    let texts = settings
        .language
        .as_deref()
        .map(TrayTexts::from_language)
        .unwrap_or_else(|| TrayTexts::from_language(detect_system_tray_language()));

    let show_main = MenuItem::with_id(app, "show_main", texts.show_main, true, None::<&str>)
        .map_err(|error| {
            AppError::Message(format!(
                "Failed to create the open-main-window menu item: {error}"
            ))
        })?;
    let quit = MenuItem::with_id(app, "quit", texts.quit, true, None::<&str>).map_err(|error| {
        AppError::Message(format!("Failed to create the quit menu item: {error}"))
    })?;

    let mut builder = MenuBuilder::new(app).item(&show_main).separator();
    if let Some(quick_switch) = create_quick_switch_menu(app, texts) {
        builder = builder.item(&quick_switch).separator();
    }
    builder
        .item(&quit)
        .build()
        .map_err(|error| AppError::Message(format!("Failed to build the menu: {error}")))
}

fn create_quick_switch_menu(
    app: &tauri::AppHandle,
    texts: TrayTexts,
) -> Option<Submenu<tauri::Wry>> {
    let advanced = match ProductSettingsService::load(app) {
        Ok(settings) => settings.advanced_mode,
        Err(error) => {
            log::warn!("Could not read product settings for tray: {error}");
            false
        }
    };
    if !advanced {
        return None;
    }

    let mut root = SubmenuBuilder::with_id(app, "quick-switch", texts.quick_switch);
    let mut tool_count = 0_usize;
    for tool in ToolId::ALL {
        if !capabilities_for(tool).can_manage_provider {
            continue;
        }
        let mut providers = match ProviderDirectory::list(app, tool) {
            Ok(providers) => providers,
            Err(_) => continue,
        };
        if providers.is_empty() || !providers.iter().any(|provider| provider.active) {
            continue;
        }
        providers.sort_by(|left, right| {
            right
                .active
                .cmp(&left.active)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.id.cmp(&right.id))
        });
        let fingerprints = providers
            .iter()
            .map(|provider| provider_fingerprint(&provider.id))
            .collect::<Vec<_>>();
        if fingerprints.iter().collect::<HashSet<_>>().len() != fingerprints.len() {
            log::error!("Refusing ambiguous tray provider ids for {}", tool.as_str());
            continue;
        }

        let mut submenu = SubmenuBuilder::with_id(
            app,
            format!("quick-switch-tool:{}", tool.as_str()),
            tool_name(tool),
        );
        let mut valid = true;
        for (provider, fingerprint) in providers.iter().zip(fingerprints) {
            let text = provider_menu_text(&provider.name, texts.unnamed_service);
            match CheckMenuItem::with_id(
                app,
                provider_menu_id(tool, &fingerprint),
                text,
                !provider.active,
                provider.active,
                None::<&str>,
            ) {
                Ok(item) => submenu = submenu.item(&item),
                Err(error) => {
                    valid = false;
                    log::error!("Could not build {} tray item: {error}", tool.as_str());
                    break;
                }
            }
        }
        if !valid {
            continue;
        }
        match submenu.build() {
            Ok(submenu) => {
                root = root.item(&submenu);
                tool_count += 1;
            }
            Err(error) => log::error!("Could not build {} tray submenu: {error}", tool.as_str()),
        }
    }

    if tool_count == 0 {
        None
    } else {
        root.build()
            .map_err(|error| log::error!("Could not build quick-switch tray menu: {error}"))
            .ok()
    }
}

fn provider_fingerprint(provider_id: &str) -> String {
    Sha256::digest(provider_id.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn provider_menu_id(tool: ToolId, fingerprint: &str) -> String {
    format!("{PROVIDER_SWITCH_PREFIX}{}:{fingerprint}", tool.as_str())
}

fn parse_provider_menu_id(event_id: &str) -> Option<(ToolId, &str)> {
    let (tool, fingerprint) = event_id
        .strip_prefix(PROVIDER_SWITCH_PREFIX)?
        .split_once(':')?;
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    Some((ToolId::from_str_id(tool)?, fingerprint))
}

fn resolve_provider_id(providers: &[Provider], fingerprint: &str) -> Option<String> {
    let mut matches = providers
        .iter()
        .filter(|provider| provider_fingerprint(&provider.id) == fingerprint);
    let id = matches.next()?.id.clone();
    matches.next().is_none().then_some(id)
}

fn provider_menu_text(name: &str, fallback: &str) -> String {
    let clean = name
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let clean = if clean.is_empty() { fallback } else { &clean };
    clean
        .chars()
        .take(80)
        .collect::<String>()
        .replace('&', "&&")
}

fn tool_name(tool: ToolId) -> &'static str {
    match tool {
        ToolId::ClaudeCode => "Claude Code",
        ToolId::Codex => "Codex",
        ToolId::OpenCode => "OpenCode",
        ToolId::GeminiCli => "Gemini CLI",
        ToolId::GrokBuild => "Grok Build",
        ToolId::OpenClaw => "OpenClaw",
        ToolId::Hermes => "Hermes",
        ToolId::Pi => "Pi",
        ToolId::KimiCode => "Kimi Code",
        ToolId::DeepSeekDsh => "DeepSeek DSH",
    }
}

pub fn refresh_tray_menu(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok(menu) = create_tray_menu(app, state.inner()) else {
        return;
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Err(error) = tray.set_menu(Some(menu)) {
            log::error!("Failed to refresh the tray menu: {error}");
        }
    }
}

pub fn schedule_tray_refresh(app: &tauri::AppHandle) {
    refresh_tray_menu(app);
}

pub fn provider_changed(app: &tauri::AppHandle, tool: ToolId) {
    schedule_tray_refresh(app);
    if let Err(error) = app.emit(PROVIDER_CHANGED_EVENT, serde_json::json!({ "tool": tool })) {
        log::error!(
            "Could not emit provider change for {}: {error}",
            tool.as_str()
        );
    }
}

fn switch_provider_from_tray(app: &tauri::AppHandle, event_id: &str) -> Result<(), String> {
    let (tool, fingerprint) =
        parse_provider_menu_id(event_id).ok_or_else(|| "invalid provider menu id".to_string())?;
    let settings = ProductSettingsService::load(app).map_err(|error| error.to_string())?;
    if !settings.advanced_mode {
        return Err("advanced mode is disabled".to_string());
    }
    let providers = ProviderDirectory::list(app, tool).map_err(|error| error.to_string())?;
    let id = resolve_provider_id(&providers, fingerprint)
        .ok_or_else(|| "provider menu target is stale or ambiguous".to_string())?;
    if providers
        .iter()
        .any(|provider| provider.id == id && provider.active)
    {
        schedule_tray_refresh(app);
        return Ok(());
    }
    ProviderDirectory::switch(app, tool, &id).map_err(|error| error.to_string())?;
    provider_changed(app, tool);
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn apply_tray_policy(app: &tauri::AppHandle, dock_visible: bool) {
    use tauri::ActivationPolicy;

    let desired_policy = if dock_visible {
        ActivationPolicy::Regular
    } else {
        ActivationPolicy::Accessory
    };

    if let Err(error) = app.set_dock_visibility(dock_visible) {
        log::warn!("Failed to set the Dock visibility: {error}");
    }
    if let Err(error) = app.set_activation_policy(desired_policy) {
        log::warn!("Failed to set the activation policy: {error}");
    }
}

pub fn handle_tray_menu_event(app: &tauri::AppHandle, event_id: &str) {
    if event_id.starts_with(PROVIDER_SWITCH_PREFIX) {
        let app = app.clone();
        let event_id = event_id.to_string();
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = switch_provider_from_tray(&app, &event_id) {
                log::error!("Tray quick switch did not finish: {error}");
            }
        });
        return;
    }
    match event_id {
        "show_main" => {
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "windows")]
                {
                    let _ = window.set_skip_taskbar(false);
                }
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
                #[cfg(target_os = "linux")]
                {
                    crate::linux_fix::nudge_main_window(window.clone());
                }
                #[cfg(target_os = "macos")]
                {
                    apply_tray_policy(app, true);
                }
            } else {
                log::warn!(
                    "The AI Manager main window does not exist; cannot open it from the tray"
                );
            }
        }
        "quit" => app.exit(0),
        _ => log::warn!("Unhandled tray menu event: {event_id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        map_locale_to_tray_language, parse_provider_menu_id, provider_fingerprint,
        provider_menu_id, provider_menu_text, resolve_provider_id, TrayTexts, TRAY_ID,
    };
    use crate::domain::{Provider, ProviderKind, ToolId};

    fn provider(id: &str) -> Provider {
        Provider {
            additive: false,
            id: id.to_string(),
            tool: ToolId::Codex,
            name: "Service".to_string(),
            kind: ProviderKind::Custom,
            active: false,
            testable: true,
            base_url: None,
            api_key: None,
            website_url: None,
            can_remove: true,
        }
    }

    #[test]
    fn tray_identity_is_product_owned() {
        assert_eq!(TRAY_ID, "ai-manager");
        assert_eq!(TrayTexts::from_language("en").show_main, "Open AI Manager");
    }

    #[test]
    fn tray_language_mapping_preserves_the_four_shipped_locales() {
        assert_eq!(map_locale_to_tray_language("en-SG"), "en");
        assert_eq!(map_locale_to_tray_language("ja-JP"), "ja");
        assert_eq!(map_locale_to_tray_language("zh-CN"), "zh");
        assert_eq!(map_locale_to_tray_language("zh-Hant-HK"), "zh-TW");
    }

    #[test]
    fn provider_menu_ids_are_opaque_and_strictly_parsed() {
        let fingerprint = provider_fingerprint("private-provider-id");
        let id = provider_menu_id(ToolId::Codex, &fingerprint);
        assert!(!id.contains("private-provider-id"));
        assert_eq!(
            parse_provider_menu_id(&id),
            Some((ToolId::Codex, fingerprint.as_str()))
        );
        for malformed in [
            "provider-switch:codex:abc",
            "provider-switch:unknown:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "provider-switch:codex:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "show_main",
        ] {
            assert!(parse_provider_menu_id(malformed).is_none(), "{malformed}");
        }
    }

    #[test]
    fn stale_or_ambiguous_fingerprints_fail_closed() {
        let fingerprint = provider_fingerprint("one");
        assert_eq!(
            resolve_provider_id(&[provider("one")], &fingerprint).as_deref(),
            Some("one")
        );
        assert!(resolve_provider_id(&[provider("one"), provider("one")], &fingerprint).is_none());
        assert!(resolve_provider_id(&[provider("two")], &fingerprint).is_none());
    }

    #[test]
    fn provider_menu_copy_is_bounded_and_mnemonic_safe() {
        assert_eq!(
            provider_menu_text("  A&B\nService ", "Unnamed"),
            "A&&B Service"
        );
        assert_eq!(provider_menu_text("\0\n", "Unnamed"), "Unnamed");
        assert_eq!(
            provider_menu_text(&"x".repeat(100), "Unnamed")
                .chars()
                .count(),
            80
        );
    }
}
