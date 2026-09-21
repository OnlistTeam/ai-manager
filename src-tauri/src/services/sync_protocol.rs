//! Shared lock for whole-database snapshot operations.
//!
//! Backup restore and configuration import both replace the database and the
//! Skills SSOT wholesale, so they must never run concurrently.

use std::sync::OnceLock;

/// Serialize every snapshot restore/import across the application.
pub(crate) fn sync_mutex() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}
