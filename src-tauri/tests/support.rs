use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use ai_manager_lib::{update_settings, AppSettings, AppState, Database, MultiAppConfig};

/// Point tests at an isolated HOME directory so real user data is never touched.
pub fn ensure_test_home() -> &'static Path {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let base = std::env::temp_dir().join("cc-switch-test-home");
        if base.exists() {
            let _ = std::fs::remove_dir_all(&base);
        }
        std::fs::create_dir_all(&base).expect("create test home");
        // On Windows `dirs::home_dir()` ignores HOME/USERPROFILE (it uses the Known Folder
        // API), so override AI_MANAGER_TEST_HOME explicitly to keep tests off the real home.
        std::env::set_var("AI_MANAGER_TEST_HOME", &base);
        std::env::set_var("HOME", &base);
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &base);
        // On Windows the Claude Desktop config dir only reads LOCALAPPDATA (see
        // `windows_local_app_data_dir` in claude_desktop_config.rs); it honours neither
        // AI_MANAGER_TEST_HOME nor HOME. Without this override, Claude Desktop provider
        // switching tests would write into the developer's real desktop config.
        #[cfg(windows)]
        std::env::set_var("LOCALAPPDATA", base.join("AppData").join("Local"));
        base
    })
    .as_path()
}

/// Product-owned AppData fallback used by native integration tests.
#[allow(dead_code)]
pub fn product_data_dir() -> PathBuf {
    ensure_test_home().join("ai-manager")
}

/// Remove config files and caches generated inside the test home.
pub fn reset_test_fs() {
    let home = ensure_test_home();
    for sub in [
        ".claude",
        ".codex",
        ".cc-switch",
        ".gemini",
        ".grok",
        ".config",
        ".openclaw",
        "ai-manager",
        "profiles",
    ] {
        let path = home.join(sub);
        if path.exists() {
            if let Err(err) = std::fs::remove_dir_all(&path) {
                eprintln!("failed to clean {}: {}", path.display(), err);
            }
        }
    }
    let claude_json = home.join(".claude.json");
    if claude_json.exists() {
        let _ = std::fs::remove_file(&claude_json);
    }

    // Reset the in-memory settings cache so one test cannot leak into the next.
    let _ = update_settings(AppSettings::default());
}

#[allow(dead_code)]
pub fn enable_codex_official_auth_preservation() {
    update_settings(AppSettings {
        preserve_codex_official_auth_on_switch: true,
        ..Default::default()
    })
    .expect("enable Codex official auth preservation");
}

/// Global mutex preventing concurrent tests from writing the same HOME directory.
pub fn test_mutex() -> &'static Mutex<()> {
    static MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    MUTEX.get_or_init(|| Mutex::new(()))
}

/// Build an AppState for tests, backed by an empty database.
#[allow(dead_code)]
pub fn create_test_state() -> Result<AppState, Box<dyn std::error::Error>> {
    let db = Arc::new(Database::init()?);
    Ok(AppState::new(db))
}

/// Build an AppState for tests and migrate data from a MultiAppConfig.
#[allow(dead_code)]
pub fn create_test_state_with_config(
    config: &MultiAppConfig,
) -> Result<AppState, Box<dyn std::error::Error>> {
    let db = Arc::new(Database::init()?);
    db.migrate_from_json(config)?;
    Ok(AppState::new(db))
}
