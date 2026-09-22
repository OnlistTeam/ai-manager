pub mod adapters;
pub mod application;
pub mod domain;

mod app_config;
mod auto_launch;
mod claude_desktop_config;
mod claude_mcp;
mod claude_plugin;
mod codex_config;
mod codex_history_migration;
mod codex_state_db;
mod commands;
pub mod compat;
mod config;
mod database;
mod error;
mod gemini_config;
mod gemini_mcp;
mod grok_config;
pub mod hermes_config;
pub mod infrastructure;
mod init_status;
mod lightweight;
#[cfg(target_os = "linux")]
mod linux_fix;
mod mcp;
mod model_capabilities;
mod openclaw_config;
mod opencode_config;
mod panic_hook;
mod pi_config;
pub mod platform;
mod prompt;
mod prompt_files;
mod provider;
mod proxy;
pub mod repositories;
mod services;
mod session_manager;
mod settings;
mod store;

mod tray;
mod usage_events;
mod usage_script;

pub use app_config::{AppType, InstalledSkill, McpApps, McpServer, MultiAppConfig, SkillApps};
pub use claude_mcp::read_mcp_json as read_claude_mcp_json;
pub use codex_config::{
    extract_codex_experimental_bearer_token, get_codex_auth_path, get_codex_config_path,
    read_codex_live_settings, write_codex_live_atomic,
};
pub use config::{get_claude_mcp_path, get_claude_settings_path, read_json_file};
pub use database::{Database, Profile};
pub use error::AppError;
pub use grok_config::get_grok_config_path;
pub use mcp::{
    import_from_claude, import_from_codex, import_from_gemini, import_from_grokbuild,
    remove_server_from_claude, remove_server_from_codex, remove_server_from_gemini,
    remove_server_from_grokbuild, sync_enabled_to_claude, sync_enabled_to_codex,
    sync_enabled_to_gemini, sync_single_server_to_claude, sync_single_server_to_codex,
    sync_single_server_to_gemini, sync_single_server_to_grokbuild,
};
pub use prompt::Prompt;
pub use provider::{Provider, ProviderMeta};
pub use services::{
    profile::{ProfilePayload, ProfileScope, ProfileService},
    provider::reapply_current_codex_official_live,
    skill::{migrate_skills_to_ssot, ImportSkillSelection},
    ConfigService, EndpointLatency, EndpointTestFailure, McpService, PromptService,
    ProviderService, ProxyService, SkillService, SpeedtestService,
};
pub use settings::{update_settings, AppSettings};
pub use store::AppState;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
#[cfg(target_os = "macos")]
use tauri::image::Image;
use tauri::tray::TrayIconBuilder;
use tauri::Manager;
use tauri::RunEvent;
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

#[cfg(target_os = "windows")]
fn set_windows_app_user_model_id(app: &tauri::AppHandle) {
    let app_id = app.config().identifier.clone();
    let wide_app_id: Vec<u16> = app_id.encode_utf16().chain(std::iter::once(0)).collect();

    let result = unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(wide_app_id.as_ptr())
    };

    if result < 0 {
        log::warn!("Failed to set Windows AppUserModelID: 0x{result:08X}");
    } else {
        log::debug!("Windows AppUserModelID set to {app_id}");
    }
}

/// Minimum length for a known secret to take part in substring redaction: short
/// values such as "api" would clobber unrelated text, so only long values that are
/// very unlikely to be ordinary words get replaced.
const MIN_KNOWN_SECRET_LEN: usize = 8;

/// The only secret-redaction primitive: replace secret values we actually hold
/// with [REDACTED]. No "looks like a secret" shape guessing — hiding known values
/// only keeps this naturally convergent and never mangles normal paths.
pub(crate) fn redact_known_secrets(text: &str, known_secrets: &[String]) -> String {
    redact_known_secrets_with_min_length(text, known_secrets, MIN_KNOWN_SECRET_LEN)
}

fn redact_known_secrets_with_min_length(
    text: &str,
    known_secrets: &[String],
    minimum_chars: usize,
) -> String {
    let mut output = text.to_string();
    for secret in known_secrets {
        if secret.chars().count() >= minimum_chars {
            output = output.replace(secret.as_str(), "[REDACTED]");
        }
    }
    output
}

/// Strip userinfo from a bare authority without a scheme (e.g. `user:pass@host/path`):
/// only treated as credentials when `@` appears before the first `/`.
fn strip_bare_userinfo(input: &str) -> &str {
    let authority_end = input.find('/').unwrap_or(input.len());
    match input[..authority_end].rfind('@') {
        Some(at) => &input[at + 1..],
        None => input,
    }
}

pub(crate) fn redact_url_for_log(url_str: &str) -> String {
    redact_url_for_log_with_secrets(url_str, &[])
}

/// Redact a URL for logging: strip userinfo (user:pass@) and the whole
/// query/fragment, keeping scheme/host/port/path for diagnostics (e.g. a base_url
/// pointing at the wrong path and returning 404), then redact known secret values.
pub(crate) fn redact_url_for_log_with_secrets(url_str: &str, known_secrets: &[String]) -> String {
    let scheme_relative = url_str.starts_with("//");
    let parsed = if scheme_relative {
        url::Url::parse(&format!("https:{url_str}"))
    } else {
        url::Url::parse(url_str)
    };

    let sanitized = match parsed {
        Ok(mut url) if url.has_host() => {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            let rendered = url.as_str();
            if scheme_relative {
                rendered
                    .strip_prefix("https:")
                    .unwrap_or(rendered)
                    .to_string()
            } else {
                rendered.to_string()
            }
        }
        _ => {
            // Parse failure (relative path, illegal URL with bare userinfo, ...):
            // drop query/fragment, strip userinfo best-effort, keep the rest as is.
            let without_tail = url_str.split(['?', '#']).next().unwrap_or(url_str);
            strip_bare_userinfo(without_tail).to_string()
        }
    };

    redact_known_secrets(&sanitized, known_secrets)
}

/// Keep only `scheme://host:port`, dropping path/query/userinfo. Used when we hold
/// no known secret to redact the path with: credentials may be embedded in the
/// base_url path itself, so logging the path cannot be guaranteed leak-free and the
/// only safe fallback is the origin.
pub(crate) fn redact_url_origin_for_log(url_str: &str) -> String {
    let scheme_relative = url_str.starts_with("//");
    let parsed = if scheme_relative {
        url::Url::parse(&format!("https:{url_str}"))
    } else {
        url::Url::parse(url_str)
    };

    match parsed {
        Ok(url) if url.has_host() => {
            let authority = &url[url::Position::BeforeHost..url::Position::AfterPort];
            if scheme_relative {
                format!("//{authority}")
            } else {
                format!("{}://{authority}", url.scheme())
            }
        }
        _ => "[invalid target]".to_string(),
    }
}

fn runtime_log_level_allows(level: log::Level, max_level: log::LevelFilter) -> bool {
    max_level.to_level().is_some_and(|maximum| level <= maximum)
}

#[cfg(target_os = "macos")]
fn macos_tray_icon() -> Option<Image<'static>> {
    const ICON_BYTES: &[u8] = include_bytes!("../icons/tray/macos/statusbar_template_3x.png");

    match Image::from_bytes(ICON_BYTES) {
        Ok(icon) => Some(icon),
        Err(err) => {
            log::warn!("Failed to load macOS tray icon: {err}");
            None
        }
    }
}

/// Registers the one deep-link handler both start paths converge on
/// (ADR-0029 decision 7).
///
/// `get_current` covers the cold start, where the operating system launched the
/// process with the URL; `on_open_url` covers every later delivery, including a
/// second instance the single-instance plugin forwards here. Both go through
/// the same parser, and both are argv by definition — a link that arrives this
/// way may not carry a credential.
fn register_deep_link_handling(app: &tauri::AppHandle) {
    use crate::application::deep_link_import::DeepLinkQueue;
    use tauri_plugin_deep_link::DeepLinkExt;

    app.manage(Arc::new(DeepLinkQueue::new()));

    // Windows writes the registry entry from the installer and macOS reads the
    // bundle metadata, so only a Linux install and a Windows development build
    // need the runtime registration the official example describes. A desktop
    // environment that refuses the handler costs the user one-click import, not
    // the application, so this never blocks startup.
    #[cfg(any(target_os = "linux", all(debug_assertions, target_os = "windows")))]
    if let Err(error) = app.deep_link().register_all() {
        log::warn!("Could not register the AI Manager link handler: {error}");
    }

    if let Ok(Some(urls)) = app.deep_link().get_current() {
        accept_deep_links(app, &urls);
    }

    let handle = app.clone();
    app.deep_link()
        .on_open_url(move |event| accept_deep_links(&handle, &event.urls()));
}

/// Queues every acceptable URL and brings the window forward so the user can
/// see what they clicked. The URL itself is never logged: it may be a link the
/// user intended to paste, and rejection must leave no trace of it either.
fn accept_deep_links(app: &tauri::AppHandle, urls: &[url::Url]) {
    use crate::application::deep_link_import::{DeepLinkImportService, DeepLinkQueue};
    use crate::commands::DEEP_LINK_PENDING_EVENT;
    use crate::domain::LinkOrigin;
    use tauri::Emitter;

    let Some(queue) = app.try_state::<Arc<DeepLinkQueue>>() else {
        log::error!("A deep link arrived before the pending queue was ready");
        return;
    };

    let mut accepted = 0_usize;
    for url in urls {
        match DeepLinkImportService::submit(queue.inner(), url.as_str(), LinkOrigin::Argv) {
            Ok(_) => accepted += 1,
            Err(error) => log::warn!("Refused an incoming link: {}", error.message_key),
        }
    }
    if accepted == 0 {
        return;
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        #[cfg(target_os = "linux")]
        linux_fix::nudge_main_window(window.clone());
    }
    if let Err(error) = app.emit(
        DEEP_LINK_PENDING_EVENT,
        serde_json::json!({ "pending": DeepLinkImportService::list(queue.inner()).len() }),
    ) {
        log::error!("Could not announce the waiting links: {error}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        // Registered first, as the deep-link plugin requires. Its `deep-link`
        // feature hands a second instance's URL to that plugin, so this
        // callback only has to bring the window forward.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            log::info!("AI Manager is already running; focusing the main window");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
                #[cfg(target_os = "linux")]
                {
                    linux_fix::nudge_main_window(window.clone());
                }
            }
        }));
        builder = builder.plugin(tauri_plugin_deep_link::init());
    }

    #[cfg(target_os = "windows")]
    {
        let startup_page_handled = AtomicBool::new(false);
        builder = builder.on_page_load(move |webview, payload| {
            if webview.label() == "main"
                && payload.event() == tauri::webview::PageLoadEvent::Finished
                && payload.url().scheme() != "about"
                && !startup_page_handled.swap(true, Ordering::Relaxed)
                && !crate::settings::get_settings().silent_startup
            {
                let _ = webview.window().show();
                log::info!("Main page finished loading; main window shown");
            }
        });
    }

    let builder = builder
        // Intercept window close: minimize to tray depending on settings
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // In "database version too new" recovery mode there is no tray to bring
                // the app back from, so closing must quit instead of hiding in the background.
                let in_db_recovery = crate::init_status::get_init_error()
                    .map(|p| p.kind.as_deref() == Some("db_version_too_new"))
                    .unwrap_or(false);
                if in_db_recovery {
                    api.prevent_close();
                    window.app_handle().exit(0);
                    return;
                }

                let settings = crate::settings::get_settings();

                if settings.minimize_to_tray_on_close {
                    api.prevent_close();
                    let _ = window.hide();
                    #[cfg(target_os = "windows")]
                    {
                        let _ = window.set_skip_taskbar(true);
                    }
                    #[cfg(target_os = "macos")]
                    {
                        tray::apply_tray_policy(window.app_handle(), false);
                    }
                } else {
                    api.prevent_close();
                    window.app_handle().exit(0);
                }
            }
        })
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(window_state_flags())
                .build(),
        )
        .setup(|app| {
            use crate::infrastructure::{logging, paths};

            // Logging and crash reporting may only be enabled once the Tauri product
            // AppData dir is known; falling back to the CC Switch directory is forbidden.
            // The product path anchor must also be set before any file logging starts.
            let product_data_dir = app.path().app_data_dir()?;
            paths::init_product_data_dir(product_data_dir.clone());
            panic_hook::init_product_data_dir(product_data_dir);
            panic_hook::setup_panic_hook();

            let _ = rustls::crypto::ring::default_provider().install_default();

            // Initialize product logging (writes to Tauri AppData/logs/ai-manager.log)
            {
                use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

                let log_dir = panic_hook::get_log_dir();

                // Make sure the log directory exists
                if let Err(e) = std::fs::create_dir_all(&log_dir) {
                    eprintln!("Failed to create log directory: {e}");
                }

                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        // Keep Trace capability at the sink so the level can be raised
                        // dynamically after user config is loaded. Right after the plugin
                        // is registered the global level is tightened to Info, so startup
                        // does not run at full Trace.
                        .level(log::LevelFilter::Trace)
                        // plugin-log's frontend command reaches the logger directly,
                        // bypassing the `log` macro global max_level; filter again in the
                        // dispatch layer so the dynamic master switch also constrains
                        // frontend logs.
                        .filter(|metadata| {
                            runtime_log_level_allows(metadata.level(), log::max_level())
                        })
                        .targets([
                            Target::new(TargetKind::Stdout),
                            Target::new(TargetKind::Folder {
                                path: log_dir,
                                file_name: Some(logging::MAIN_LOG_FILE_STEM.into()),
                            }),
                        ])
                        // Keep 4 rotated archives, so at most 5 x 10 MiB including the
                        // current file. Rotation is size-triggered only; appending continues
                        // across restarts, so the previous run's logs are no longer lost.
                        .rotation_strategy(RotationStrategy::KeepSome(
                            logging::MAIN_LOG_ARCHIVES_TO_KEEP,
                        ))
                        .max_file_size(logging::MAIN_LOG_MAX_SIZE)
                        .timezone_strategy(TimezoneStrategy::UseLocal)
                        .build(),
                )?;

                // User config lives in the database; use a conservative Info level
                // until the database has been opened.
                log::set_max_level(log::LevelFilter::Info);
                log::info!("=== AI Manager v{} started ===", env!("CARGO_PKG_VERSION"));
            }

            #[cfg(target_os = "windows")]
            set_windows_app_user_model_id(app.handle());

            // Inject the AppHandle into usage_events so log-writing paths that hold no
            // AppHandle can still push `usage-log-recorded` to the frontend.
            // Placed after logging init so logs emitted during init are visible.
            usage_events::init(app.handle().clone());

            // Signed application updates run as a background state machine.
            // Without a trusted endpoint this manager is inert and never
            // sends an update request.
            app.manage(Arc::new(
                crate::application::app_update::AppUpdateManager::new(app.handle()),
            ));

            // Initialize the database (ADR-0002: the product uses its own app.db). On the
            // first launch under the stable product identifier, run a one-shot,
            // non-destructive, allow-listed migration from the old Tauri identifier dir.
            // A failed migration must not silently create an empty database, or the next
            // launch would mistake that empty database for "target data wins".
            let db_path = paths::app_db_path();
            loop {
                match crate::database::migrate_legacy_product_data(&paths::product_data_dir()) {
                    Ok(crate::database::ProductDataMigrationOutcome::NoLegacyDatabase) => {
                        log::debug!("No legacy-identifier product database found; nothing to migrate");
                        break;
                    }
                    Ok(
                        crate::database::ProductDataMigrationOutcome::DestinationDatabaseExists,
                    ) => {
                        log::debug!("Product database already exists; legacy-identifier migration keeps target data");
                        break;
                    }
                    Ok(crate::database::ProductDataMigrationOutcome::Migrated {
                        backup_files,
                        tree_files,
                    }) => {
                        log::info!(
                            "Legacy-identifier product data migration finished: database=1, backups={backup_files}, tree_files={tree_files}"
                        );
                        break;
                    }
                    Err(error) => {
                        log::error!("Legacy-identifier product data migration failed: {error}");
                        if !show_database_init_error_dialog(
                            app.handle(),
                            &db_path,
                            &error.to_string(),
                        ) {
                            log::info!("User chose to quit the application");
                            std::process::exit(1);
                        }
                        log::info!("User chose to retry the legacy-identifier product data migration");
                    }
                }
            }

            // Create the database now (including schema migrations)
            //
            // Note: users upgrading from v3.8.* normally hit the SQLite schema migration
            // here. If it fails (corrupted database, missing permissions, user_version too
            // new, ...) the user needs a clear message, otherwise all they see is an app
            // that will not open or crashes on launch.
            //
            // Pre-check: when the stored database version is too new we must enter the
            // recovery screen before any schema write (create_tables issues DROP/ALTER
            // DDL), so an older app never writes into a newer DB it cannot read.
            match crate::database::Database::stored_user_version_exceeds_supported(&db_path) {
                Ok(Some(version)) => {
                    log::warn!("Database version is too new (v{version}); guiding the user to upgrade the app");
                    crate::init_status::set_init_error(crate::init_status::InitErrorPayload {
                        path: db_path.display().to_string(),
                        error: format!(
                            "Database version is too new ({version}); this app only supports {}. Please upgrade the app and try again.",
                            crate::database::SCHEMA_VERSION
                        ),
                        kind: Some("db_version_too_new".to_string()),
                        db_version: Some(version),
                        supported_version: Some(crate::database::SCHEMA_VERSION),
                    });
                    // The main window defaults to visible:false; the recovery screen must force it
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    return Ok(());
                }
                Ok(None) => {}
                Err(e) => {
                    log::warn!("Failed to pre-check database version; continuing normal initialization: {e}");
                }
            }

            let db = loop {
                match crate::database::Database::init() {
                    Ok(db) => break Arc::new(db),
                    Err(e) => {
                        log::error!("Failed to init database: {e}");

                        if !show_database_init_error_dialog(app.handle(), &db_path, &e.to_string())
                        {
                            log::info!("User chose to quit the application");
                            std::process::exit(1);
                        }

                        log::info!("User chose to retry database initialization");
                    }
                }
            };

            // Apply the persisted log level as soon as the database is available so later
            // service initialization stops using the startup Info fallback. A corrupted
            // config explicitly fails closed to Info.
            match db.get_log_config() {
                Ok(log_config) => {
                    log::set_max_level(log_config.to_level_filter());
                    log::info!(
                        "Loaded log config: enabled={}, level={}",
                        log_config.enabled,
                        log_config.level
                    );
                }
                Err(e) => {
                    log::set_max_level(log::LevelFilter::Info);
                    log::warn!("Failed to read log config; fell back to info: {e}");
                }
            }

            let app_state = AppState::new(db);

            // Set the AppHandle used for UI updates on proxy failover
            app_state.proxy_service.set_app_handle(app.handle().clone());
            // The first tray build reads product settings and the ProviderDirectory; inject
            // the same Arc-backed state here and keep using local clones afterwards.
            app.manage(app_state.clone());

            // ============================================================
            // Per-table import logic (each data kind is checked independently)
            // ============================================================

            // 1. Initialize the default skills repository (built-in check: skipped when the table is non-empty)
            match app_state.db.init_default_skill_repos() {
                Ok(count) if count > 0 => {
                    log::info!("✓ Initialized {count} default skill repositories");
                }
                Ok(_) => {} // Table not empty, silently skip
                Err(e) => log::warn!("✗ Failed to initialize default skill repos: {e}"),
            }

            // 1.1. Unified skills management migration: after the database moves to the v3
            // structure, auto-import from each app directory into the SSOT. Triggered when
            // the schema migration sets settings.skills_ssot_migration_pending = true.
            match app_state.db.get_setting("skills_ssot_migration_pending") {
                Ok(Some(flag)) if flag == "true" || flag == "1" => {
                    // Safety guard: never auto-wipe and rebuild when the user already has
                    // skills data in the v3 structure.
                    let has_existing = app_state
                        .db
                        .get_all_installed_skills()
                        .map(|skills| !skills.is_empty())
                        .unwrap_or(false);

                    if has_existing {
                        log::info!(
                            "Detected skills_ssot_migration_pending but skills table not empty; skipping auto import."
                        );
                        let _ = app_state
                            .db
                            .set_setting("skills_ssot_migration_pending", "false");
                    } else {
                        match crate::services::skill::migrate_skills_to_ssot(&app_state.db) {
                            Ok(count) => {
                                log::info!("✓ Auto imported {count} skill(s) into SSOT");
                                if count > 0 {
                                    crate::init_status::set_skills_migration_result(count);
                                }
                                let _ = app_state
                                    .db
                                    .set_setting("skills_ssot_migration_pending", "false");
                            }
                            Err(e) => {
                                log::warn!("✗ Failed to auto import legacy skills to SSOT: {e}");
                                crate::init_status::set_skills_migration_error(e.to_string());
                                // Keep the pending flag so the next launch retries
                            }
                        }
                    }
                }
                Ok(_) => {} // Migration flag not set, silently skip
                Err(e) => log::warn!("✗ Failed to read skills migration flag: {e}"),
            }

            // 1.5. Auto-import live config + seed the official preset providers (Claude / Codex / Gemini)
            //
            // Importing before seeding is deliberate: first turn the user's hand-written
            // settings.json / auth.json / .env into a "default" provider marked current,
            // then append the official presets (is_current=false). That way, when the user
            // switches to an official preset, the write-back mechanism protects the
            // original live config from being lost.
            //
            // Capture the first-run snapshot, keeping the compat field for older databases.
            // On read failure default to not prompting: better to miss the notice than to
            // bother the user because of a failure.
            let first_run_already_confirmed = crate::settings::get_settings()
                .first_run_notice_confirmed
                .unwrap_or(false);
            let fresh_install_at_startup =
                app_state.db.is_providers_empty().unwrap_or(false);

            for app_type in
                crate::app_config::AppType::all().filter(|t| !t.is_additive_mode())
            {
                if !crate::services::provider::should_import_default_config_on_startup(
                    &app_state,
                    &app_type,
                )
                .unwrap_or(false)
                {
                    log::debug!(
                        "○ {} already has providers; live import skipped",
                        app_type.as_str()
                    );
                    continue;
                }

                match crate::services::provider::import_default_config(
                    &app_state,
                    app_type.clone(),
                ) {
                    Ok(true) => log::info!(
                        "✓ Imported live config for {} as default provider",
                        app_type.as_str()
                    ),
                    Ok(false) => log::debug!(
                        "○ {} already has providers; live import skipped",
                        app_type.as_str()
                    ),
                    Err(e) => log::debug!(
                        "○ No live config to import for {}: {e}",
                        app_type.as_str()
                    ),
                }
            }

            match app_state.db.init_default_official_providers() {
                Ok(count) if count > 0 => {
                    log::info!("✓ Seeded {count} official provider(s)");
                }
                Ok(_) => {}
                Err(e) => log::warn!("✗ Failed to seed official providers: {e}"),
            }

            {
                let db_for_codex_history_migration = app_state.db.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    match crate::codex_history_migration::maybe_migrate_codex_third_party_history_provider_bucket(
                        &db_for_codex_history_migration,
                    ) {
                        Ok(outcome) => {
                            if let Some(reason) = outcome.skipped_reason {
                                log::debug!("○ Codex history provider bucket migration skipped: {reason}");
                            } else {
                                log::info!(
                                    "✓ Codex history provider bucket migration completed: sources={}, jsonl_files={}, state_rows={}",
                                    outcome.source_provider_ids.len(),
                                    outcome.migrated_jsonl_files,
                                    outcome.migrated_state_rows
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!("✗ Codex history provider bucket migration failed: {e}");
                        }
                    }

                    match crate::codex_history_migration::maybe_migrate_codex_provider_template_bucket(
                        &db_for_codex_history_migration,
                    ) {
                        Ok(outcome) => {
                            if let Some(reason) = outcome.skipped_reason {
                                log::debug!("○ Codex provider template bucket migration skipped: {reason}");
                            } else if !outcome.migrated_provider_ids.is_empty() {
                                log::info!(
                                    "✓ Codex provider template bucket migration completed: providers={}",
                                    outcome.migrated_provider_ids.len()
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!("✗ Codex provider template bucket migration failed: {e}");
                        }
                    }

                });
            }

            // Existing users / already-confirmed paths are filtered out by
            // `fresh_install_at_startup` itself; nothing is written here. The field is only
            // written back by the frontend via save_settings when the user clicks
            // "Got it", so it always means "the user explicitly confirmed".
            if !first_run_already_confirmed && fresh_install_at_startup {
                log::info!("✓ First-run welcome notice pending");
            }

            // 1.6. Sync native providers of additive-mode apps into the database
            //
            // The additive-mode import functions are idempotent per id: a new id is
            // imported, an existing id has its settings and display name updated. Running
            // them on every launch is therefore safe — fresh installs see the providers
            // from live out of the box, and live files edited outside the app are synced
            // into the database after a restart (previously this required manually
            // clicking "import current config" in the frontend).
            //
            // The underlying read_*_config returns an empty default config when the file
            // is missing, so a fresh install without live files takes the Ok(0) path and
            // produces no error-log noise.
            match crate::services::provider::import_opencode_providers_from_live(&app_state) {
                Ok(count) if count > 0 => {
                    log::info!("✓ Synced {count} OpenCode provider(s) from live config");
                }
                Ok(_) => log::debug!("○ No OpenCode provider changes from live config"),
                Err(e) => log::warn!("✗ Failed to import OpenCode providers: {e}"),
            }
            match crate::services::provider::import_openclaw_providers_from_live(&app_state) {
                Ok(count) if count > 0 => {
                    log::info!("✓ Synced {count} OpenClaw provider(s) from live config");
                }
                Ok(_) => log::debug!("○ No OpenClaw provider changes from live config"),
                Err(e) => log::warn!("✗ Failed to import OpenClaw providers: {e}"),
            }
            match crate::services::provider::import_hermes_providers_from_live(&app_state) {
                Ok(count) if count > 0 => {
                    log::info!("✓ Synced {count} Hermes provider(s) from live config");
                }
                Ok(_) => log::debug!("○ No Hermes provider changes from live config"),
                Err(e) => log::warn!("✗ Failed to import Hermes providers: {e}"),
            }
            match crate::services::provider::import_pi_providers_from_live(&app_state) {
                Ok(count) if count > 0 => {
                    log::info!("✓ Synced {count} Pi provider(s) from native config");
                }
                Ok(_) => log::debug!("○ No Pi provider changes from native config"),
                Err(e) => log::warn!("✗ Failed to import Pi providers: {e}"),
            }

            // 2. OMO config import (imported from local files when the database has no OMO provider)
            {
                let has_omo = app_state
                    .db
                    .get_all_providers("opencode")
                    .map(|providers| providers.values().any(|p| p.category.as_deref() == Some("omo")))
                    .unwrap_or(false);
                if !has_omo {
                    match crate::services::OmoService::import_from_local(&app_state, &crate::services::omo::STANDARD) {
                        Ok(provider) => {
                            log::info!("✓ Imported OMO config from local as provider '{}'", provider.name);
                        }
                        Err(AppError::OmoConfigNotFound) => {
                            log::debug!("○ No OMO config to import");
                        }
                        Err(e) => {
                            log::warn!("✗ Failed to import OMO config from local: {e}");
                        }
                    }
                }
            }

            // 2.3 OMO Slim config import (when no omo-slim provider in DB, import from local)
            {
                let has_omo_slim = app_state
                    .db
                    .get_all_providers("opencode")
                    .map(|providers| {
                        providers
                            .values()
                            .any(|p| p.category.as_deref() == Some("omo-slim"))
                    })
                    .unwrap_or(false);
                if !has_omo_slim {
                    match crate::services::OmoService::import_from_local(&app_state, &crate::services::omo::SLIM) {
                        Ok(provider) => {
                            log::info!(
                                "✓ Imported OMO Slim config from local as provider '{}'",
                                provider.name
                            );
                        }
                        Err(AppError::OmoConfigNotFound) => {
                            log::debug!("○ No OMO Slim config to import");
                        }
                        Err(e) => {
                            log::warn!("✗ Failed to import OMO Slim config from local: {e}");
                        }
                    }
                }
            }

            // MCP and prompts are deliberately not silently imported on first launch: the
            // product has an explicit Detected -> user confirms adoption flow, plus the
            // prompt "import from current file" action. Writing whole live files into the
            // database at startup would bypass that confirmation and later turn into a
            // stale snapshot when a backup is restored.

            // Startup no longer saves unconditionally, to avoid overwriting user config.

            // Create the dynamic tray menu
            let menu = tray::create_tray_menu(app.handle(), &app_state)?;

            // Build the tray
            let mut tray_builder = TrayIconBuilder::with_id(tray::TRAY_ID)
                .tooltip("AI Manager")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    tray::handle_tray_menu_event(app, &event.id.0);
                })
                .show_menu_on_left_click(true);

            // Use the platform-specific tray icon (macOS uses a template icon for light/dark)
            #[cfg(target_os = "macos")]
            {
                if let Some(icon) = macos_tray_icon() {
                    tray_builder = tray_builder.icon(icon).icon_as_template(true);
                } else if let Some(icon) = app.default_window_icon() {
                    log::warn!("Falling back to default window icon for tray");
                    tray_builder = tray_builder.icon(icon.clone());
                } else {
                    log::warn!("Failed to load macOS tray icon for tray");
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                if let Some(icon) = app.default_window_icon() {
                    tray_builder = tray_builder.icon(icon.clone());
                } else {
                    log::warn!("Failed to get default window icon for tray");
                }
            }

            let tray_icon = tray_builder.build(app)?;
            if !crate::settings::get_settings().show_in_tray {
                tray_icon.set_visible(false)?;
            }
            // AI Manager long-running operation manager (spec §39/§41). Held in an Arc:
            // lifecycle commands move it into tokio::spawn background tasks, and the
            // reference borrowed from State is not 'static.
            app.manage(Arc::new(crate::infrastructure::OperationManager::new(
                Box::new(crate::infrastructure::TauriOperationEvents::new(
                    app.handle().clone(),
                )),
            )));

            // Initialize CopilotAuthManager
            {
                use crate::proxy::providers::copilot_auth::CopilotAuthManager;
                use commands::CopilotAuthState;
                use tokio::sync::RwLock;

                let product_data_dir = crate::infrastructure::paths::product_data_dir();
                let copilot_auth_manager = CopilotAuthManager::new(product_data_dir);
                app.manage(CopilotAuthState(Arc::new(RwLock::new(copilot_auth_manager))));
                log::info!("✓ CopilotAuthManager initialized");
            }

            // Initialize CodexOAuthManager (ChatGPT Plus/Pro reverse proxy)
            {
                use commands::CodexOAuthState;

                let codex_oauth_manager =
                    app.state::<AppState>().codex_oauth_manager.clone();
                app.manage(CodexOAuthState(codex_oauth_manager));
                log::info!("✓ CodexOAuthManager initialized");
            }

            // Initialize the xAI OAuthManager (Grok API reverse proxy)
            {
                use crate::proxy::providers::xai_oauth_auth::XaiOAuthManager;
                use commands::XaiOAuthState;
                use tokio::sync::RwLock;

                let product_data_dir = crate::infrastructure::paths::product_data_dir();
                let xai_oauth_manager = XaiOAuthManager::new(product_data_dir);
                app.manage(XaiOAuthState(Arc::new(RwLock::new(xai_oauth_manager))));
                log::info!("✓ XaiOAuthManager initialized");
            }

            // Initialize the global outbound-proxy HTTP client
            {
                let db = &app.state::<AppState>().db;
                let proxy_url = db.get_global_proxy_url().ok().flatten();

                if let Err(e) = crate::proxy::http_client::init(proxy_url.as_deref()) {
                    log::error!(
                        "[GlobalProxy] [GP-005] Failed to initialize with saved config: {e}"
                    );

                    // Clear the invalid proxy config
                    if proxy_url.is_some() {
                        log::warn!(
                            "[GlobalProxy] [GP-006] Clearing invalid proxy config from database"
                        );
                        if let Err(clear_err) = db.set_global_proxy_url(None) {
                            log::error!(
                                "[GlobalProxy] [GP-007] Failed to clear invalid config: {clear_err}"
                            );
                        }
                    }

                    // Re-initialize in direct-connection mode
                    if let Err(fallback_err) = crate::proxy::http_client::init(None) {
                        log::error!(
                            "[GlobalProxy] [GP-008] Failed to initialize direct connection: {fallback_err}"
                        );
                    }
                }
            }

            // Crash recovery + local maintenance. The product shell does not auto-restore
            // proxy takeovers it never exposed, but it still cleans up live backups and
            // placeholders left behind by a crash so tools stop pointing at a dead proxy.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = app_handle.state::<AppState>();

                // Check for live backups (a sign the last crash happened during a takeover)
                let has_backups = match state.db.has_any_live_backup().await {
                    Ok(v) => v,
                    Err(e) => {
                        log::error!("Failed to check live backups: {e}");
                        false
                    }
                };
                // Check whether live configs are still taken over (placeholders present)
                let live_taken_over = state.proxy_service.detect_takeover_in_live_configs();

                if has_backups || live_taken_over {
                    log::warn!("Detected an unclean shutdown (takeover leftovers); restoring live configs...");
                    if let Err(e) = state.proxy_service.recover_from_crash().await {
                        log::error!("Failed to restore live configs: {e}");
                    } else {
                        log::info!("Live configs restored");
                    }
                }

                // Must run before auto-extract: scrub credentials that historically leaked
                // into the shared Gemini snippet first, otherwise the extraction right after
                // would rewrite them from the polluted live config.
                if let Err(e) =
                    crate::services::provider::ProviderService::scrub_leaked_gemini_common_config(
                        &state,
                    )
                    .await
                {
                    log::warn!("Failed to scrub leaked credentials from the Gemini common config: {e}");
                }

                initialize_common_config_snippets(&state);

                // Periodic backup check (on startup)
                if let Err(e) = state.db.periodic_backup_if_needed() {
                    log::warn!("Periodic backup failed on startup: {e}");
                }

                // Periodic maintenance timer: run once per day while the app is running
                let db_for_timer = state.db.clone();
                tauri::async_runtime::spawn(async move {
                    const PERIODIC_MAINTENANCE_INTERVAL_SECS: u64 = 24 * 60 * 60;
                    let mut interval = tokio::time::interval(std::time::Duration::from_secs(
                        PERIODIC_MAINTENANCE_INTERVAL_SECS,
                    ));
                    interval.tick().await; // skip immediate first tick (already checked above)
                    loop {
                        interval.tick().await;
                        if let Err(e) = db_for_timer.periodic_backup_if_needed() {
                            log::warn!("Periodic maintenance timer failed: {e}");
                        }
                    }
                });
            });

            register_deep_link_handling(app.handle());

            // Linux: disable WebKitGTK hardware acceleration so a failed EGL init cannot
            // leave a white screen
            #[cfg(target_os = "linux")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.with_webview(|webview| {
                        use webkit2gtk::{WebViewExt, SettingsExt, HardwareAccelerationPolicy};
                        let wk_webview = webview.inner();
                        if let Some(settings) = WebViewExt::settings(&wk_webview) {
                            SettingsExt::set_hardware_acceleration_policy(&settings, HardwareAccelerationPolicy::Never);
                            log::info!("WebKitGTK hardware acceleration disabled");
                        }
                    });
                }
            }

            // Silent startup: show or hide the main window depending on settings
            let settings = crate::settings::get_settings();
            if let Some(window) = app.get_webview_window("main") {
                // Before the first show, so the caption is never briefly drawn
                // in the system colour. macOS gets the same result from
                // `titleBarStyle: "Overlay"` in the bundle configuration.
                crate::platform::window_chrome::apply_product_chrome(&window);
                // Sync decoration state before the window is first shown, so switching
                // after the frontend loads does not make the title bar flicker.
                // Linux only: works around unusable system window buttons under Wayland.
                #[cfg(target_os = "linux")]
                let _ = window.set_decorations(!settings.use_app_window_controls);
                if settings.silent_startup {
                    // Silent startup mode: keep the window hidden
                    let _ = window.hide();
                    #[cfg(target_os = "windows")]
                    let _ = window.set_skip_taskbar(true);
                    #[cfg(target_os = "macos")]
                    tray::apply_tray_policy(app.handle(), false);
                    log::info!("Silent startup mode: main window hidden");
                } else {
                    // Normal startup mode: show the window
                    #[cfg(not(target_os = "windows"))]
                    let _ = window.show();
                    #[cfg(target_os = "windows")]
                    log::info!("Normal startup mode: waiting for the main page to load before showing the window");
                    #[cfg(not(target_os = "windows"))]
                    log::info!("Normal startup mode: main window shown");

                    // Linux: work around an unresponsive UI on first launch (Tauri #10746
                    // + wry #637). At startup the webview never gets focus and surface size
                    // negotiation fails, so clicks do nothing. A set_focus plus a fake
                    // resize is the invisible equivalent of maximize-then-restore.
                    #[cfg(target_os = "linux")]
                    {
                        linux_fix::nudge_main_window(window.clone());
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Bootstrap, recovery, and native window integration.
            commands::get_init_error,
            commands::open_app_config_folder,
            commands::set_window_theme,
            // AI Manager product API (Phase 1 / Phase 3a).
            commands::app_tools_list,
            commands::app_tools_check_versions,
            commands::app_tools_update_preview,
            commands::app_operations_list,
            commands::app_operation_cancel,
            commands::app_tool_install,
            commands::app_tool_update,
            commands::app_tool_version_catalog,
            commands::app_tool_install_version,
            commands::app_tool_repair,
            commands::app_tool_uninstall_preview,
            commands::app_tool_uninstall,
            commands::app_tool_launch,
            commands::app_desktop_apps_list,
            commands::app_desktop_app_launch,
            commands::app_desktop_app_open_official_download,
            commands::app_desktop_app_open_uninstall,
            commands::app_providers_list,
            commands::app_provider_runtime_context,
            commands::app_provider_runtime_resource_open,
            commands::app_provider_connection_profile,
            commands::app_provider_edit_profile,
            commands::app_provider_create,
            commands::app_provider_custom_create,
            commands::app_provider_switch,
            commands::app_provider_save,
            commands::app_provider_remove,
            commands::app_provider_test,
            commands::app_provider_test_all,
            commands::app_provider_endpoints_test,
            commands::app_provider_presets_test,
            commands::app_provider_models_list,
            commands::app_provider_model_probe,
            commands::app_provider_effective_models_list,
            commands::app_provider_effective_model_probe,
            commands::app_shell_variables_locate,
            commands::app_shell_variable_write,
            commands::app_provider_activation_prepare,
            commands::app_provider_launch_prepare,
            commands::app_provider_next_healthy,
            // AI Manager product API (Phase 5).
            commands::app_extensions_local_inventory,
            commands::app_extensions_list,
            commands::app_extension_set_enabled,
            commands::app_extension_location_reveal,
            commands::app_detected_skill_resource_open,
            commands::app_detected_skill_copy,
            commands::app_extensions_adopt_detected,
            commands::app_mcp_install,
            commands::app_mcp_remove,
            commands::app_skill_catalog_list,
            commands::app_skill_install,
            commands::app_skill_zip_install,
            commands::app_skill_backups_list,
            commands::app_skill_backup_restore,
            commands::app_skill_backup_delete,
            commands::app_skill_remove,
            commands::app_skill_updates_check,
            commands::app_skill_update,
            commands::app_skill_repositories_list,
            commands::app_skill_repository_save,
            commands::app_skill_repository_remove,
            commands::app_prompt_get,
            commands::app_prompt_save,
            commands::app_prompt_remove,
            commands::app_prompt_import_current,
            // AI Manager product API (Phase 6a).
            commands::app_settings_get,
            commands::app_settings_save,
            commands::app_terminals_list,
            commands::app_desktop_preferences_get,
            commands::app_desktop_preferences_save,
            commands::app_backups_list,
            commands::app_backup_create,
            commands::app_backup_restore,
            commands::app_backup_delete,
            commands::app_backup_rename,
            commands::app_backup_export,
            commands::app_backup_import,
            commands::app_backup_schedule_get,
            commands::app_backup_schedule_save,
            // AI Manager product API (Phase 6b).
            commands::app_import_preview,
            commands::app_import_run,
            commands::app_health_snapshot,
            commands::app_reveal_path,
            commands::app_open_automation_settings,
            // AI Manager product API (Advanced Usage; ADR-0006).
            commands::app_usage_overview,
            commands::app_usage_refresh,
            // AI Manager product API (Advanced Sessions).
            commands::app_sessions_list,
            commands::app_session_thread,
            commands::app_session_resume,
            commands::app_session_reveal,
            // AI Manager product API (Advanced OpenClaw workspace; ADR-0014).
            commands::app_openclaw_workspace_overview,
            commands::app_openclaw_workspace_document,
            commands::app_openclaw_workspace_save_document,
            commands::app_openclaw_daily_memories,
            commands::app_openclaw_daily_memory,
            commands::app_openclaw_daily_memory_save,
            commands::app_openclaw_daily_memory_delete,
            commands::app_openclaw_workspace_open_directory,
            // AI Manager product API (outbound install/update proxy).
            commands::app_network_proxy_get,
            commands::app_network_proxy_save,
            commands::app_update_start,
            commands::app_update_status,
            commands::app_update_install_and_restart,
            commands::app_update_open_download_page,
            // AI Manager product API (About / Appropriate Legal Notices, AGPL-3.0 §5).
            commands::app_about_open_source_code,
            commands::app_about_open_license,
            // AI Manager product API (Advanced Routing; ADR-0007).
            commands::app_routing_overview,
            commands::app_routing_set_takeover,
            commands::app_routing_set_failover,
            commands::app_routing_queue_add,
            commands::app_routing_queue_remove,
            commands::app_routing_switch_provider,
            commands::app_routing_stop_all,
            // AI Manager product API (one-click import; ADR-0029).
            commands::app_deeplink_pending_list,
            commands::app_deeplink_preview,
            commands::app_deeplink_submit_pasted,
            commands::app_deeplink_dismiss,
            commands::app_deeplink_confirm,
        ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|app_handle, event| {
        // Handle exit requests (all platforms)
        if let RunEvent::ExitRequested { api, code, .. } = &event {
            match classify_exit_request(*code) {
                // code == None means the runtime triggered this itself (e.g. the hidden
                // window's WebView was reclaimed, leaving no live window); only prevent
                // the exit and keep running in the tray.
                ExitRequestAction::StayInTray => {
                    log::info!("Runtime-triggered exit request (no live window); preventing exit to stay in the tray");
                    api.prevent_exit();
                    return;
                }
                // code == RESTART_EXIT_CODE: a restart issued by app.restart() or by the
                // self-update relaunch. On this path prevent_exit() is ignored by Tauri,
                // the event loop is guaranteed to exit, and Tauri re-execs the new binary
                // after RunEvent::Exit (macOS resolves the executable name from the
                // updated Info.plist).
                //
                // The async cleanup task below must never be reused here: it calls
                // save_window_state on a tokio thread, holding the window-state plugin
                // lock while querying window geometry from the main thread — which is at
                // that moment leaving the event loop and waiting for the same lock inside
                // the plugin's own RunEvent::Exit hook. They deadlock and the process
                // hangs forever (update installed but the app freezes and never restarts,
                // see #3998).
                //
                // For restarts, hand control back to Tauri's default flow:
                //   - window state: saved by the plugin Exit hook on the main thread (same
                //     thread reads the geometry, no deadlock)
                //   - tray icon: cleaned up by Tauri's internal cleanup_before_exit via Drop
                //   - proxy/live config: no restore needed; the new instance takes over and
                //     restores the proxy state right after the restart
                //   - the 100ms flush wait: DB writes before a restart are command-driven
                //     and already finished, matching the default restart path of every
                //     Tauri app, so no extra wait is required
                ExitRequestAction::DeferToTauriRestart => {
                    log::info!("Restart request received (code={code:?}); deferring to Tauri's default restart re-exec");
                    return;
                }
                // Any other Some(_): the user called app.exit() explicitly (e.g. the tray
                // menu "Quit"); run the cleanup and then exit.
                ExitRequestAction::CleanupAndExit => {}
            }

            log::info!("User-initiated exit request received (code={code:?}); starting cleanup...");
            api.prevent_exit();

            let app_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                save_window_state_before_exit(&app_handle);
                cleanup_before_exit(&app_handle).await;
                // Remove the tray icon explicitly before std::process::exit. When the
                // process exits directly the Tauri runtime skips the normal Drop path and
                // never sends NIM_DELETE to the Windows shell, so the icon registered by
                // the dead process lingers in the tray (the shell only repaints and
                // notices the process is gone once the mouse hovers over it).
                remove_tray_icon_before_exit(&app_handle);
                log::info!("Cleanup finished; exiting the application");

                // Brief wait so all I/O (e.g. database writes) is flushed to disk
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                // Use std::process::exit to avoid triggering ExitRequested again
                std::process::exit(0);
            });
            return;
        }

        #[cfg(target_os = "macos")]
        {
            // macOS fires Reopen when the Dock icon is clicked and the app is reactivated;
            // restore the main window manually here
            if let RunEvent::Reopen { .. } = event {
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                    tray::apply_tray_policy(app_handle, true);
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (app_handle, event);
        }
    });
}

// ============================================================
// Application exit cleanup
// ============================================================

/// Cleanup performed before the application exits.
///
/// Checks the proxy server state before exiting and, when it is running, stops the
/// proxy and restores the live configs, so Claude Code/Codex/Gemini configs are never
/// left in a broken state. Uses stop_with_restore_keep_state to keep the proxy state
/// in the settings table, so it is restored automatically on the next launch.
pub async fn cleanup_before_exit(app_handle: &tauri::AppHandle) {
    if let Some(state) = app_handle.try_state::<store::AppState>() {
        let proxy_service = &state.proxy_service;

        // A safety net is needed on exit too: the proxy may have crashed or never run
        // while live takeover leftovers (placeholders/backups) are still around.
        let has_backups = match state.db.has_any_live_backup().await {
            Ok(v) => v,
            Err(e) => {
                log::error!("Failed to check live backups during exit: {e}");
                false
            }
        };
        let live_taken_over = proxy_service.detect_takeover_in_live_configs();
        let needs_restore = has_backups || live_taken_over;

        if needs_restore {
            log::info!(
                "Takeover leftovers detected; restoring live configs (keeping proxy state)..."
            );
            // Use the keep_state variant so the proxy state in the settings table survives
            if let Err(e) = proxy_service.stop_with_restore_keep_state().await {
                log::error!("Failed to restore live configs during exit: {e}");
            } else {
                log::info!("Live configs restored (proxy state kept; it will be restored on the next launch)");
            }
            return;
        }

        // Not in takeover mode: if the proxy is running, just stop it
        if proxy_service.is_running().await {
            log::info!("Proxy server detected as running; stopping it...");
            if let Err(e) = proxy_service.stop().await {
                log::error!("Failed to stop the proxy during exit: {e}");
            }
            log::info!("Proxy server cleanup finished");
        }
    }
}

/// Explicitly remove the tray icon from the system tray.
///
/// `std::process::exit` bypasses the Tauri runtime, so `TrayIcon::drop()` never runs
/// and `NIM_DELETE` is never sent to the Windows shell. The result is a cached dead
/// icon left in the tray after the process exits (the shell does not repaint on its
/// own; it only refreshes when the mouse hovers over it).
///
/// `set_visible(false)` goes through the `WM_USER_HIDE_TRAYICON` message path, which
/// triggers tray-icon's internal `remove_tray_icon` -> `Shell_NotifyIconW(NIM_DELETE)`
/// and removes the icon cleanly before the process ends. On other platforms
/// `set_visible(false)` simply hides/removes the icon, so it is a safe cross-platform
/// fallback as well.
pub(crate) fn remove_tray_icon_before_exit(app_handle: &tauri::AppHandle) {
    if let Some(tray) = app_handle.tray_by_id(tray::TRAY_ID) {
        if let Err(e) = tray.set_visible(false) {
            log::warn!("Failed to remove the tray icon during exit: {e}");
        } else {
            log::info!("Tray icon explicitly removed from the system tray");
        }
    }
}

fn initialize_common_config_snippets(state: &store::AppState) {
    // Auto-extract common config snippets from clean live files when snippet is missing.
    // Crash recovery is attempted before this call so proxy placeholders are not treated
    // as source data. Product startup does not re-enable the hidden takeover afterward.
    for app_type in crate::app_config::AppType::all() {
        if !state
            .db
            .should_auto_extract_config_snippet(app_type.as_str())
            .unwrap_or(false)
        {
            continue;
        }

        let settings = match crate::services::provider::ProviderService::read_live_settings(
            app_type.clone(),
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };

        match crate::services::provider::ProviderService::extract_common_config_snippet_from_settings(
            app_type.clone(),
            &settings,
        ) {
            Ok(snippet) if !snippet.is_empty() && snippet != "{}" => {
                match state.db.set_config_snippet(app_type.as_str(), Some(snippet)) {
                    Ok(()) => {
                        let _ = state.db.set_config_snippet_cleared(app_type.as_str(), false);
                        log::info!(
                            "✓ Auto-extracted common config snippet for {}",
                            app_type.as_str()
                        );
                    }
                    Err(e) => log::warn!(
                        "✗ Failed to save config snippet for {}: {e}",
                        app_type.as_str()
                    ),
                }
            }
            Ok(_) => log::debug!(
                "○ Live config for {} has no extractable common fields",
                app_type.as_str()
            ),
            Err(e) => log::warn!(
                "✗ Failed to extract config snippet for {}: {e}",
                app_type.as_str()
            ),
        }
    }

    let should_run_legacy_migration = state
        .db
        .is_legacy_common_config_migrated()
        .map(|done| !done)
        .unwrap_or(true);

    if should_run_legacy_migration {
        for app_type in [
            crate::app_config::AppType::Claude,
            crate::app_config::AppType::Codex,
            crate::app_config::AppType::Gemini,
        ] {
            if let Err(e) = crate::services::provider::ProviderService::migrate_legacy_common_config_usage_if_needed(
                state,
                app_type.clone(),
            ) {
                log::warn!(
                    "✗ Failed to migrate legacy common-config usage for {}: {e}",
                    app_type.as_str()
                );
            }
        }

        if let Err(e) = state.db.set_legacy_common_config_migrated(true) {
            log::warn!("✗ Failed to persist legacy common-config migration flag: {e}");
        }
    }
}

// ============================================================
// Dialog helpers
// ============================================================

/// Show the "database initialization / schema migration failed" dialog.
/// Returns true when the user chose to retry, false when they chose to quit.
fn show_database_init_error_dialog(
    app: &tauri::AppHandle,
    db_path: &std::path::Path,
    error: &str,
) -> bool {
    let title = "Database Initialization Failed";

    let message = format!(
        "An error occurred while initializing or migrating the database:\n\n{error}\n\n\
        Database file path:\n{db}\n\n\
        Your data is NOT lost - the app will not delete the database automatically.\n\
        Common causes include: newer database version, corrupted file, permission issues, or low disk space.\n\n\
        Suggestions:\n\
        1) Back up the entire data directory (including app.db)\n\
        2) If you see \"database version is newer\", please upgrade AI Manager\n\
        3) If this happened right after upgrading, consider rolling back to export/backup then upgrade again\n\n\
        Click 'Retry' to attempt initialization again\n\
        Click 'Exit' to close the program",
        db = db_path.display()
    );

    let retry_text = "Retry";
    let exit_text = "Exit";

    app.dialog()
        .message(&message)
        .title(title)
        .kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::OkCancelCustom(
            retry_text.to_string(),
            exit_text.to_string(),
        ))
        .blocking_show()
}

// ============================================================
// Exit request classification
// ============================================================

/// The three sources of `RunEvent::ExitRequested`, which must be handled differently.
///
/// Key constraint: for restart requests (`code == RESTART_EXIT_CODE`), `prevent_exit()`
/// is silently ignored by Tauri (see the `ExitRequestApi::prevent_exit` docs); the event
/// loop always continues to exit and fires every plugin's `RunEvent::Exit` hook. Any
/// custom cleanup task running concurrently may contend with those hooks over the same
/// state and deadlock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitRequestAction {
    /// `code` is `None`: triggered by the runtime itself (e.g. the hidden window's
    /// WebView was reclaimed, leaving no live window); prevent the exit and keep
    /// running in the tray.
    StayInTray,
    /// `code` is `RESTART_EXIT_CODE`: a restart from `app.restart()` or the self-update
    /// relaunch; do not intercept, do no custom cleanup, defer to Tauri's default re-exec.
    DeferToTauriRestart,
    /// Any other `Some(_)`: the user quit explicitly (tray "Quit" and friends); run the
    /// full async cleanup and then end the process.
    CleanupAndExit,
}

fn classify_exit_request(code: Option<i32>) -> ExitRequestAction {
    match code {
        None => ExitRequestAction::StayInTray,
        Some(tauri::RESTART_EXIT_CODE) => ExitRequestAction::DeferToTauriRestart,
        Some(_) => ExitRequestAction::CleanupAndExit,
    }
}

// ============================================================
// Explicitly persist window state before the app exits on its own
// ============================================================

fn window_state_flags() -> StateFlags {
    StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED
}

/// The app's exit path intercepts `ExitRequested` and ultimately calls
/// `std::process::exit(0)` directly, so the state must be flushed manually before the
/// process ends; otherwise the window-state plugin's default exit hook is bypassed.
pub fn save_window_state_before_exit(app_handle: &tauri::AppHandle) {
    if let Err(err) = app_handle.save_window_state(window_state_flags()) {
        log::error!("Failed to save window state before exit: {err}");
    } else {
        log::info!("Window state saved before exit");
    }
}

/// Explicitly release the single-instance lock.
///
/// On macOS single-instance uses `/tmp/{identifier}.sock`. Several of our paths call
/// `std::process::exit(0)` directly and never fire the plugin's `RunEvent::Exit` cleanup
/// hook. Destroying the lock before a restart keeps the new process from connecting to
/// the stale listener and quitting itself.
pub fn destroy_single_instance_lock(app_handle: &tauri::AppHandle) {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    tauri_plugin_single_instance::destroy(app_handle);
}

/// Clean up the tray icon, release the single-instance lock, then restart the app.
///
/// This goes straight through `tauri::process::restart` (spawn a new process +
/// `exit(0)`) without leaving the event loop, so neither Tauri's internal
/// `cleanup_before_exit` nor the plugins' `RunEvent::Exit` hooks run. The required
/// cleanup is compensated explicitly by the caller and by this function: window state
/// and proxy/live restore (caller); tray icon and single-instance lock (here).
///
/// `AppHandle::cleanup_before_exit()` is deliberately not called: it drops the tray icon
/// on the calling thread, while macOS NSStatusItem operations must happen on the main
/// thread. `set_visible(false)` goes through the `run_item_main_thread` proxy and is
/// therefore safe across threads (see `remove_tray_icon_before_exit`).
pub fn restart_process(app_handle: &tauri::AppHandle) -> ! {
    remove_tray_icon_before_exit(app_handle);
    destroy_single_instance_lock(app_handle);
    tauri::process::restart(&app_handle.env());
}

#[cfg(test)]
mod tests {
    use super::{
        classify_exit_request, redact_url_for_log, redact_url_for_log_with_secrets,
        redact_url_origin_for_log, runtime_log_level_allows, ExitRequestAction,
    };

    #[test]
    fn log_url_redaction_strips_credentials_and_query_keeps_path() {
        // userinfo and the whole query are stripped; the path is kept to diagnose a
        // misconfigured base_url.
        assert_eq!(
            redact_url_for_log(
                "https://user:secret@example.com:8443/v1/models?key=top-secret&alt=sse"
            ),
            "https://example.com:8443/v1/models"
        );
        // scheme-relative URLs keep their shape; userinfo is removed.
        assert_eq!(
            redact_url_for_log("//user:sk-secret@gw.example.com/v1"),
            "//gw.example.com/v1"
        );
        // Bare userinfo without a scheme.
        assert_eq!(
            redact_url_for_log("user:sk-secret@gw.example.com/v1"),
            "gw.example.com/v1"
        );
        // When it cannot be parsed as an absolute URL: drop the query, keep the rest.
        assert_eq!(redact_url_for_log("not-a-url?token=secret"), "not-a-url");
        // No "looks like a secret" shape guessing on path segments anymore; normal
        // paths are kept intact.
        assert_eq!(
            redact_url_for_log("https://host.example/v1/models/gemini-2.5-pro"),
            "https://host.example/v1/models/gemini-2.5-pro"
        );
    }

    #[test]
    fn log_url_redaction_replaces_known_secret_values() {
        // Exact match on known secret values: wiped whether they appear in the path or
        // anywhere else.
        let secrets = vec!["k-9f3a7c2b1e".to_string()];
        assert_eq!(
            redact_url_for_log_with_secrets("https://gw.example.com/k-9f3a7c2b1e/v1", &secrets),
            "https://gw.example.com/[REDACTED]/v1"
        );
        // Known values shorter than 8 chars stay out of substring redaction, so normal
        // paths such as /v1/ are not clobbered.
        let short_secrets = vec!["api".to_string()];
        assert_eq!(
            redact_url_for_log_with_secrets("https://api.example.com/v1", &short_secrets),
            "https://api.example.com/v1"
        );
    }

    #[test]
    fn log_url_origin_drops_path_for_credential_in_path() {
        // With no known secret to redact, credentials may be embedded in the path, so
        // only the origin is logged.
        assert_eq!(
            redact_url_origin_for_log("https://gw.example.com/k-9f3a7c2b1e/v1"),
            "https://gw.example.com"
        );
        assert_eq!(
            redact_url_origin_for_log("https://user:pass@gw.example.com:8443/secret/v1"),
            "https://gw.example.com:8443"
        );
        assert_eq!(
            redact_url_origin_for_log("//gw.example.com/secret/v1"),
            "//gw.example.com"
        );
    }

    #[test]
    fn runtime_log_filter_honors_dynamic_max_level() {
        assert!(!runtime_log_level_allows(
            log::Level::Error,
            log::LevelFilter::Off
        ));
        assert!(runtime_log_level_allows(
            log::Level::Error,
            log::LevelFilter::Info
        ));
        assert!(runtime_log_level_allows(
            log::Level::Info,
            log::LevelFilter::Info
        ));
        assert!(!runtime_log_level_allows(
            log::Level::Debug,
            log::LevelFilter::Info
        ));
    }

    #[test]
    fn no_code_keeps_app_alive_in_tray() {
        assert_eq!(classify_exit_request(None), ExitRequestAction::StayInTray);
    }

    #[test]
    fn restart_exit_code_defers_to_tauri_default_restart() {
        assert_eq!(
            classify_exit_request(Some(tauri::RESTART_EXIT_CODE)),
            ExitRequestAction::DeferToTauriRestart
        );
    }

    #[test]
    fn user_exit_codes_run_cleanup_then_exit() {
        assert_eq!(
            classify_exit_request(Some(0)),
            ExitRequestAction::CleanupAndExit
        );
        assert_eq!(
            classify_exit_request(Some(1)),
            ExitRequestAction::CleanupAndExit
        );
    }
}
