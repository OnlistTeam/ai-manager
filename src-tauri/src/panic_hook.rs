//! Panic hook module.
//!
//! Captures panic information when the app crashes and logs it to `crash.log`
//! under the Tauri product's AppData directory, to help users and developers
//! diagnose crashes.

use crate::infrastructure::logging::{
    self, CRASH_ENTRY_MAX_SIZE, CRASH_LOG_ARCHIVES_TO_KEEP, CRASH_LOG_MAX_SIZE,
};
use crate::platform::redact::{redact_secrets, truncate_tail};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// App version number (read from Cargo.toml).
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

static PRODUCT_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static CRASH_LOG_LOCK: Mutex<()> = Mutex::new(());

pub fn init_product_data_dir(dir: PathBuf) {
    let _ = PRODUCT_DATA_DIR.set(dir);
}

/// Product-specific fallback for when there is no Tauri context. A normal desktop
/// startup injects `app.path().app_data_dir()` before the hook is installed; this only
/// serves tests and very early diagnostics.
fn default_product_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ai-manager")
}

/// Gets the product data directory (prefers the value injected by Tauri setup; never panics).
fn get_product_data_dir() -> PathBuf {
    PRODUCT_DATA_DIR
        .get()
        .cloned()
        .unwrap_or_else(default_product_data_dir)
}

/// Gets the crash log file path.
fn get_crash_log_path() -> PathBuf {
    logging::crash_log_path(&get_product_data_dir())
}

fn rotated_crash_log_path(path: &Path, index: usize) -> PathBuf {
    let mut rotated = path.as_os_str().to_os_string();
    rotated.push(format!(".{index}"));
    PathBuf::from(rotated)
}

fn rotate_crash_log_if_needed_with_limit(
    path: &Path,
    max_size: u64,
    archives_to_keep: usize,
    pending_size: u64,
) -> std::io::Result<()> {
    let size = match fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if size.saturating_add(pending_size) <= max_size {
        return Ok(());
    }

    if archives_to_keep == 0 {
        fs::remove_file(path)?;
        return Ok(());
    }

    for index in (1..=archives_to_keep).rev() {
        let source = if index == 1 {
            path.to_path_buf()
        } else {
            rotated_crash_log_path(path, index - 1)
        };
        if !source.exists() {
            continue;
        }

        let destination = rotated_crash_log_path(path, index);
        if destination.exists() {
            fs::remove_file(&destination)?;
        }
        fs::rename(source, destination)?;
    }

    Ok(())
}

fn rotate_crash_log_if_needed(path: &Path, pending_size: u64) -> std::io::Result<()> {
    rotate_crash_log_if_needed_with_limit(
        path,
        CRASH_LOG_MAX_SIZE,
        CRASH_LOG_ARCHIVES_TO_KEEP,
        pending_size,
    )
}

fn append_crash_entry(path: &Path, entry: &str) -> std::io::Result<()> {
    rotate_crash_log_if_needed(path, entry.len() as u64)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(entry.as_bytes())?;
    file.flush()
}

/// Gets the log directory path.
pub fn get_log_dir() -> PathBuf {
    logging::log_dir(&get_product_data_dir())
}

/// Safely gets environment information (never panics).
fn get_system_info() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let family = std::env::consts::FAMILY;

    // Safely get the current working directory.
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Safely get the current thread information.
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("unnamed");
    let thread_id = format!("{:?}", thread.id());

    format!(
        "OS: {os} ({family})\n\
         Arch: {arch}\n\
         App Version: {APP_VERSION}\n\
         Working Dir: {cwd}\n\
         Thread: {thread_name} (ID: {thread_id})"
    )
}

fn bounded_redacted_field(input: &str, max_lines: usize, max_bytes: usize) -> String {
    std::panic::catch_unwind(|| truncate_tail(&redact_secrets(input), max_lines, max_bytes))
        .unwrap_or_else(|_| "[diagnostic field unavailable]".to_string())
}

fn render_crash_entry(
    timestamp: &str,
    system_info: &str,
    message: &str,
    location: &str,
    backtrace: &str,
) -> String {
    // Panic payloads can contain provider errors or command output. Every field goes through the
    // same redaction primitive used elsewhere, then receives an explicit byte budget so a single
    // panic cannot defeat crash-log rotation with an oversized record.
    let system_info = bounded_redacted_field(system_info, 32, 16 * 1024);
    let message = bounded_redacted_field(message, 128, 64 * 1024);
    let location = bounded_redacted_field(location, 16, 8 * 1024);
    let backtrace = bounded_redacted_field(backtrace, 8_192, 512 * 1024);
    let timestamp = bounded_redacted_field(timestamp, 1, 1024);
    let separator = "=".repeat(80);
    let sub_separator = "-".repeat(40);
    let entry = format!(
        r#"
{separator}
[CRASH REPORT] {timestamp}
{separator}

{sub_separator}
System Information
{sub_separator}
{system_info}

{sub_separator}
Error Details
{sub_separator}
Message: {message}

Location: {location}

{sub_separator}
Stack Trace (Backtrace)
{sub_separator}
{backtrace}

{separator}
"#
    );

    if entry.len() <= CRASH_ENTRY_MAX_SIZE {
        entry
    } else {
        // Defensive cap if the fixed formatting budget changes in the future.
        truncate_tail(&entry, usize::MAX, CRASH_ENTRY_MAX_SIZE)
    }
}

/// Sets up the panic hook, capturing crash information and writing it to the log file.
///
/// Call this function on app startup to ensure every panic gets logged.
/// The log format includes:
/// - Timestamp
/// - App version and system information
/// - Panic message
/// - Location (file:line)
/// - Backtrace (full call stack)
pub fn setup_panic_hook() {
    // Enable backtrace (ensures it is captured in release mode too).
    if std::env::var("RUST_BACKTRACE").is_err() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    // Replace (rather than chain to) the default hook. The default hook prints the original panic
    // payload to stderr and would therefore bypass the redaction performed below.
    let _ = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        let log_path = get_crash_log_path();

        // Ensure the directory exists.
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Build crash information (wrap time formatting in catch_unwind to avoid a nested panic).
        let timestamp = std::panic::catch_unwind(|| {
            chrono::Local::now()
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string()
        })
        .unwrap_or_else(|_| {
            // Fall back to a unix timestamp if chrono panics.
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| format!("unix:{}.{:03}", d.as_secs(), d.subsec_millis()))
                .unwrap_or_else(|_| "unknown".to_string())
        });

        // Get system information.
        let system_info = std::panic::catch_unwind(get_system_info)
            .unwrap_or_else(|_| "Failed to get system info".to_string());

        // Get the panic message (try multiple extraction methods).
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            // Fall back to the Display trait.
            format!("{panic_info}")
        };

        // Get location information.
        let location = if let Some(loc) = panic_info.location() {
            format!(
                "File: {}\n         Line: {}\n         Column: {}",
                loc.file(),
                loc.line(),
                loc.column()
            )
        } else {
            "Unknown location".to_string()
        };

        // Capture the backtrace (full call stack).
        let backtrace = std::backtrace::Backtrace::force_capture();
        let backtrace_str = format!("{backtrace}");

        let crash_entry = render_crash_entry(
            &timestamp,
            &system_info,
            &message,
            &location,
            &backtrace_str,
        );

        // Combine the size check, rotation, and append into a single critical section to
        // avoid two hooks racing on rename (and losing an archive) when multiple threads
        // panic simultaneously.
        let crash_log_guard = CRASH_LOG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let saved = append_crash_entry(&log_path, &crash_entry).is_ok();
        drop(crash_log_guard);

        if saved {
            eprintln!("\n[AI Manager] Crash log saved to: {}", log_path.display());
        }

        // Also print to stderr (useful for development debugging).
        eprintln!("{crash_entry}");
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRASH_HOOK_CHILD_ENV: &str = "AI_MANAGER_CRASH_HOOK_CHILD";
    const CRASH_HOOK_DATA_DIR_ENV: &str = "AI_MANAGER_CRASH_HOOK_DATA_DIR";
    const SECRET_FIXTURE: &str = "sk-ant-controlled-crash-fixture-123456789";

    #[test]
    fn test_crash_log_path() {
        let path = get_crash_log_path();
        assert!(path.ends_with("crash.log"));
        assert!(!path.to_string_lossy().contains(".cc-switch"));
        assert_eq!(path.parent(), Some(get_product_data_dir().as_path()));
    }

    #[test]
    fn test_system_info() {
        let info = get_system_info();
        assert!(info.contains("OS:"));
        assert!(info.contains("Arch:"));
        assert!(info.contains("App Version:"));
    }

    #[test]
    fn crash_log_rotation_keeps_bounded_archives() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.log");

        fs::write(&path, b"first").unwrap();
        rotate_crash_log_if_needed_with_limit(&path, 4, 2, 0).unwrap();
        assert!(!path.exists());
        assert_eq!(
            fs::read(rotated_crash_log_path(&path, 1)).unwrap(),
            b"first"
        );

        fs::write(&path, b"second").unwrap();
        rotate_crash_log_if_needed_with_limit(&path, 4, 2, 0).unwrap();
        assert_eq!(
            fs::read(rotated_crash_log_path(&path, 1)).unwrap(),
            b"second"
        );
        assert_eq!(
            fs::read(rotated_crash_log_path(&path, 2)).unwrap(),
            b"first"
        );

        fs::write(&path, b"third").unwrap();
        rotate_crash_log_if_needed_with_limit(&path, 4, 2, 0).unwrap();
        assert_eq!(
            fs::read(rotated_crash_log_path(&path, 1)).unwrap(),
            b"third"
        );
        assert_eq!(
            fs::read(rotated_crash_log_path(&path, 2)).unwrap(),
            b"second"
        );
        assert!(!rotated_crash_log_path(&path, 3).exists());
    }

    #[test]
    fn pending_record_rotates_before_the_current_file_exceeds_its_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.log");

        fs::write(&path, b"123").unwrap();
        rotate_crash_log_if_needed_with_limit(&path, 4, 1, 2).unwrap();

        assert!(!path.exists());
        assert_eq!(fs::read(rotated_crash_log_path(&path, 1)).unwrap(), b"123");
    }

    #[test]
    fn crash_hook_child_process() {
        if std::env::var(CRASH_HOOK_CHILD_ENV).as_deref() != Ok("1") {
            return;
        }

        let product_data_dir = PathBuf::from(
            std::env::var(CRASH_HOOK_DATA_DIR_ENV).expect("child data directory is required"),
        );
        init_product_data_dir(product_data_dir);
        setup_panic_hook();

        let result = std::panic::catch_unwind(|| {
            panic!("Authorization: Bearer {SECRET_FIXTURE}");
        });
        assert!(result.is_err());
    }

    #[test]
    fn controlled_panic_writes_a_bounded_redacted_product_record() {
        let dir = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "panic_hook::tests::crash_hook_child_process",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CRASH_HOOK_CHILD_ENV, "1")
            .env(CRASH_HOOK_DATA_DIR_ENV, dir.path())
            .output()
            .unwrap();

        assert!(output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains(SECRET_FIXTURE));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(SECRET_FIXTURE));

        let path = dir.path().join("crash.log");
        let record = fs::read_to_string(&path).unwrap();
        assert!(record.contains("[CRASH REPORT]"));
        assert!(record.contains("Authorization: Bearer ***"));
        assert!(!record.contains(SECRET_FIXTURE));
        assert!(fs::metadata(path).unwrap().len() <= CRASH_LOG_MAX_SIZE);
    }
}
