use std::path::{Path, PathBuf};

pub const MAIN_LOG_FILE_STEM: &str = "ai-manager";
pub const MAIN_LOG_MAX_SIZE: u128 = 10 * 1024 * 1024;
pub const MAIN_LOG_ARCHIVES_TO_KEEP: usize = 4;
pub const CRASH_LOG_MAX_SIZE: u64 = 5 * 1024 * 1024;
pub const CRASH_LOG_ARCHIVES_TO_KEEP: usize = 2;
pub const CRASH_ENTRY_MAX_SIZE: usize = 1024 * 1024;

pub fn log_dir(product_data_dir: &Path) -> PathBuf {
    product_data_dir.join("logs")
}

pub fn crash_log_path(product_data_dir: &Path) -> PathBuf {
    product_data_dir.join("crash.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_logs_are_bounded_to_five_files() {
        assert_eq!(MAIN_LOG_FILE_STEM, "ai-manager");
        assert_eq!(format!("{MAIN_LOG_FILE_STEM}.log"), "ai-manager.log");
        assert_eq!(MAIN_LOG_MAX_SIZE, 10 * 1024 * 1024);
        assert_eq!(MAIN_LOG_ARCHIVES_TO_KEEP, 4);
        assert_eq!(MAIN_LOG_ARCHIVES_TO_KEEP + 1, 5);
    }

    #[test]
    fn product_logs_stay_below_the_product_data_dir() {
        let product = Path::new("product-app-data");

        assert_eq!(log_dir(product), product.join("logs"));
        assert_eq!(crash_log_path(product), product.join("crash.log"));
    }

    #[test]
    fn crash_rotation_is_bounded() {
        assert_eq!(CRASH_LOG_MAX_SIZE, 5 * 1024 * 1024);
        assert_eq!(CRASH_LOG_ARCHIVES_TO_KEEP, 2);
        assert_eq!(CRASH_ENTRY_MAX_SIZE, 1024 * 1024);
        assert!(CRASH_ENTRY_MAX_SIZE < CRASH_LOG_MAX_SIZE as usize);
    }
}
