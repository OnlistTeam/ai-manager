//! Use cases for backup and restore (spec §9 / §92 / §18).
//!
//! As thin as `product_settings`, for the same reason. The **destructive semantics** of a restore
//! (safety backup, live config write-back, preserving product preferences) all live in the
//! compatibility layer, because those semantics can only be written correctly where the upstream
//! API is known (decision 8).

use std::path::PathBuf;

use crate::compat::ccswitch::backup::{self, BackupStore};
use crate::domain::{AppError, BackupList, RestoreOutcome};

pub struct BackupDirectory;

impl BackupDirectory {
    /// Listing needs no open store: scanning the backup directory is independent of the database connection.
    pub fn list() -> Result<BackupList, AppError> {
        backup::list()
    }

    pub fn create(app_handle: &tauri::AppHandle) -> Result<BackupList, AppError> {
        BackupStore::open(app_handle)?.create()
    }

    pub fn delete(app_handle: &tauri::AppHandle, name: &str) -> Result<BackupList, AppError> {
        BackupStore::open(app_handle)?.delete(name)
    }

    pub fn rename(
        app_handle: &tauri::AppHandle,
        source: &str,
        name: &str,
    ) -> Result<BackupList, AppError> {
        BackupStore::open(app_handle)?.rename(source, name)
    }

    /// The only `async` use case in the whole phase: a restore first takes a cross-subsystem async
    /// lock and then hands the whole blocking section to the blocking thread pool (see the
    /// compatibility layer for the shape).
    pub async fn restore(
        app_handle: &tauri::AppHandle,
        name: String,
    ) -> Result<RestoreOutcome, AppError> {
        BackupStore::open(app_handle)?.restore(name).await
    }

    pub fn export_archive(app_handle: &tauri::AppHandle, target: PathBuf) -> Result<(), AppError> {
        BackupStore::open(app_handle)?.export_archive(&target)
    }

    pub async fn import_archive(
        app_handle: &tauri::AppHandle,
        source: PathBuf,
    ) -> Result<RestoreOutcome, AppError> {
        BackupStore::open(app_handle)?.import_archive(&source).await
    }
}
