use std::path::PathBuf;
use std::sync::OnceLock;

/// ADR-0002: the database file name of this product. The upstream `cc-switch.db` must no longer be used.
pub const APP_DB_FILE_NAME: &str = "app.db";
/// ADR-0002: the upstream database file name discovered during a read-only import.
pub const CC_SWITCH_DB_FILE_NAME: &str = "cc-switch.db";

static PRODUCT_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

fn default_product_data_dir() -> PathBuf {
    // Existing native tests isolate filesystem access through this explicit
    // test/debug home. Keep the fallback product-owned instead of reusing the
    // `.cc-switch` compatibility directory.
    if let Ok(test_home) = std::env::var("AI_MANAGER_TEST_HOME") {
        let trimmed = test_home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("ai-manager");
        }
    }

    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ai-manager")
}

/// Injected once during the Tauri setup phase via `app.path().app_data_dir()`.
/// Repeated calls have no effect (the first write wins), so the path cannot drift within a process.
pub fn init_product_data_dir(dir: PathBuf) {
    let _ = PRODUCT_DATA_DIR.set(dir);
}

/// Resolve the product data directory. A desktop run only accepts the Tauri AppData; a
/// compatibility directory override must not change where the product database, backups or internal
/// config are written.
pub fn product_data_dir() -> PathBuf {
    PRODUCT_DATA_DIR
        .get()
        .cloned()
        .unwrap_or_else(default_product_data_dir)
}

pub fn app_db_path() -> PathBuf {
    product_data_dir().join(APP_DB_FILE_NAME)
}

/// CC Switch keeps its own directory semantics; this path may only be handed to the read-only import repository.
pub fn cc_switch_db_path() -> PathBuf {
    crate::config::get_default_cc_switch_config_dir().join(CC_SWITCH_DB_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::{
        app_db_path, cc_switch_db_path, product_data_dir, APP_DB_FILE_NAME, CC_SWITCH_DB_FILE_NAME,
    };

    #[test]
    fn product_database_file_is_app_db() {
        assert_eq!(APP_DB_FILE_NAME, "app.db");
    }

    #[test]
    fn app_db_path_sits_directly_in_the_product_data_dir() {
        let path = app_db_path();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("app.db")
        );
        assert_eq!(path.parent(), Some(product_data_dir().as_path()));
    }

    #[test]
    fn product_data_dir_never_points_at_the_upstream_database_file() {
        let path = app_db_path();
        assert!(
            !path.ends_with("cc-switch.db"),
            "the product must never read or write the CC Switch database"
        );
        assert!(
            path.parent().and_then(|parent| parent.file_name()) != Some(".cc-switch".as_ref()),
            "the product fallback must not use the CC Switch compatibility directory"
        );
    }

    #[test]
    fn cc_switch_discovery_never_targets_the_product_database() {
        let path = cc_switch_db_path();
        assert_eq!(CC_SWITCH_DB_FILE_NAME, "cc-switch.db");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("cc-switch.db")
        );
        assert_ne!(path, app_db_path());
    }
}
