//! Database backup and restore
//!
//! Provides SQL export/import and binary snapshot backups.

use super::{lock_conn, Database};
use crate::error::AppError;
use chrono::{Local, Utc};
use rusqlite::backup::{Backup, StepResult};
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tempfile::{Builder, NamedTempFile};

/// Header written into SQL exports produced by this app.
const SQL_EXPORT_HEADER: &str = "-- AI Manager SQLite export";

/// Header written by CC Switch itself. Its text is Chinese, spelled out in escapes so the
/// source stays ASCII; the bytes must match the upstream marker exactly or SQL backups
/// exported from CC Switch can no longer be imported.
const CC_SWITCH_SQL_EXPORT_HEADER: &str = "-- CC Switch SQLite \u{5bfc}\u{51fa}";

/// Bound combined INSERT batches while still amortizing statement parsing.
/// A row larger than this cap is emitted alone because it cannot be split.
const INSERT_BATCH_MAX_ROWS: usize = 200;
const INSERT_BATCH_MAX_BYTES: usize = 1024 * 1024;

/// Serialize every operation that observes or mutates the database-backup
/// directory. Always acquire this guard before `Database.conn`.
static BACKUP_FILE_OPERATION_LOCK: Mutex<()> = Mutex::new(());
type BackupFileOperationGuard = MutexGuard<'static, ()>;

fn lock_backup_file_operations() -> Result<BackupFileOperationGuard, AppError> {
    BACKUP_FILE_OPERATION_LOCK
        .lock()
        .map_err(|e| AppError::Database(format!("Backup file operation lock failed: {e}")))
}

/// PRAGMAs that `dump_sql` emits. Every other PRAGMA is rejected: `temp_store_directory` can
/// redirect temp files to any directory, and `writable_schema` bypasses schema integrity checks.
const IMPORT_ALLOWED_PRAGMAS: &[&str] = &["foreign_keys", "user_version"];

/// Authorizer used while executing external SQL: deny every action that can **leave the
/// temporary database file**.
///
/// The header check (`validate_cc_switch_sql_export`) only compares a comment prefix, so anyone
/// can append further statements after a valid prefix. The side effect of
/// `ATTACH DATABASE '/path/x.db'` happens before the staging schema is validated, so the file
/// is created even when the import ultimately fails; and the `settings` table is in neither
/// `SYNC_SKIP_TABLES` nor `SYNC_PRESERVE_TABLES`, so WebDAV/S3 sync goes through the same
/// `import_sql_string_inner`. Input on this path cannot be trusted.
///
/// Why an authorizer rather than scanning for the `ATTACH` keyword: string scanning is defeated
/// by `/*x*/ATTACH`, casing and newlines, and misses `VACUUM INTO` entirely. The authorizer runs
/// at prepare time against the **parse result**, so it cannot be bypassed at the syntax level.
///
/// Why "deny escaping actions" rather than "allow only what dump_sql emits": this SQL runs on a
/// throwaway database created by `NamedTempFile`, whose entire content is defined by this SQL.
/// So `DELETE` / `DROP` / `UPDATE` give an attacker nothing new — **the only meaningful boundary
/// is that temporary file itself**. A strict allowlist modelled on dump_sql output would only
/// risk false positives (one unexpected object in a user database makes the backup
/// unrestorable) without blocking any extra attack.
///
/// The escaping actions were measured, not inferred:
/// - `ATTACH DATABASE 'x'`, `VACUUM INTO 'x'` and a bare `VACUUM` **all three** report
///   `AuthAction::Attach`, so denying `Attach` alone covers all of them
/// - file-backed virtual table modules (`csvfile`, `zipfile`, ...) can read and write arbitrary
///   paths -> deny vtable
/// - `Unknown` is rusqlite's fallback for unrecognized action codes -> deny by default, so any
///   cross-file statement SQLite adds in the future lands here without anyone updating a list
fn import_authorizer(context: rusqlite::hooks::AuthContext<'_>) -> rusqlite::hooks::Authorization {
    use rusqlite::hooks::{AuthAction, Authorization};

    let escapes_temp_db = match context.action {
        AuthAction::Attach { .. } | AuthAction::Detach { .. } => true,
        AuthAction::CreateVtable { .. } | AuthAction::DropVtable { .. } => true,
        AuthAction::Unknown { .. } => true,
        AuthAction::Pragma { pragma_name, .. } => !IMPORT_ALLOWED_PRAGMAS
            .iter()
            .any(|allowed| pragma_name.eq_ignore_ascii_case(allowed)),
        _ => false,
    };

    if escapes_temp_db {
        // SQLite only reports "not authorized"; without a log there is no way to tell which
        log::warn!(
            "SQL import rejected an escaping statement: {:?}",
            context.action
        );
        Authorization::Deny
    } else {
        Authorization::Allow
    }
}

/// Tables whose data rows are skipped when exporting for WebDAV sync.
const SYNC_SKIP_TABLES: &[&str] = &[
    "proxy_request_logs",
    "stream_check_logs",
    "provider_health",
    "proxy_live_backup",
    "usage_daily_rollups",
    "session_log_sync",
    "session_usage_dedup",
];

/// A database backup entry for the UI
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub filename: String,
    pub size_bytes: u64,
    pub created_at: String, // ISO 8601
}

impl Database {
    /// Backup directory. ADR-0002: it sits next to `app.db` in the product data directory and is
    /// **no longer** the upstream `~/.cc-switch/backups` — both products share the same backup
    /// schema, so a shared directory would let them prune or even restore each other's databases.
    pub(crate) fn backups_dir() -> PathBuf {
        crate::infrastructure::paths::product_data_dir().join("backups")
    }

    /// Export as SQLite-compatible SQL text (in-memory string, full export)
    pub fn export_sql_string(&self) -> Result<String, AppError> {
        let snapshot = self.snapshot_to_memory()?;
        Self::dump_sql(&snapshot, &[])
    }

    /// Export SQL for sync (WebDAV), skipping local-only tables' data
    pub fn export_sql_string_for_sync(&self) -> Result<String, AppError> {
        let snapshot = self.snapshot_to_memory()?;
        Self::dump_sql(&snapshot, SYNC_SKIP_TABLES)
    }

    /// Export as SQLite-compatible SQL text
    pub fn export_sql(&self, target_path: &Path) -> Result<(), AppError> {
        let dump = self.export_sql_string()?;

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }

        crate::config::atomic_write(target_path, dump.as_bytes())
    }

    /// Import from a SQL file; returns the generated backup id (empty when no backup was made)
    pub fn import_sql(&self, source_path: &Path) -> Result<String, AppError> {
        if !source_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "SQL file does not exist: {}",
                source_path.display()
            )));
        }

        let sql_raw = fs::read_to_string(source_path).map_err(|e| AppError::io(source_path, e))?;
        let sql_content = sql_raw.trim_start_matches('\u{feff}');
        self.import_sql_string(sql_content)
    }

    /// Import from a SQL string; returns the generated backup id (empty when no backup was made)
    pub fn import_sql_string(&self, sql_raw: &str) -> Result<String, AppError> {
        self.import_sql_string_inner(sql_raw, &[])
    }

    fn import_sql_string_inner(
        &self,
        sql_raw: &str,
        preserve_tables: &[&str],
    ) -> Result<String, AppError> {
        self.import_sql_string_inner_with_hook(sql_raw, preserve_tables, || Ok(()))
    }

    fn import_sql_string_inner_with_hook<F>(
        &self,
        sql_raw: &str,
        preserve_tables: &[&str],
        on_staging_ready: F,
    ) -> Result<String, AppError>
    where
        F: FnOnce() -> Result<(), AppError>,
    {
        let sql_content = sql_raw.trim_start_matches('\u{feff}');
        Self::validate_cc_switch_sql_export(sql_content)?;

        // Run the import in a temporary database so a failure cannot corrupt the main one
        let temp_file = NamedTempFile::new().map_err(|e| AppError::IoContext {
            context: "Failed to create the temporary database file".to_string(),
            source: e,
        })?;
        let temp_path = temp_file.path().to_path_buf();
        let temp_conn =
            Connection::open(&temp_path).map_err(|e| AppError::Database(e.to_string()))?;
        // SQLite Backup copies the source database header into the destination.
        // Configure the empty staging database before creating any tables so a
        // SQL import cannot downgrade the main DB from incremental vacuum to NONE.
        temp_conn
            .execute("PRAGMA auto_vacuum = INCREMENTAL;", [])
            .map_err(|e| {
                AppError::Database(format!(
                    "Failed to set auto_vacuum on the staging database: {e}"
                ))
            })?;

        // The authorizer only covers external SQL and is removed right afterwards: the
        // `create_tables_on_conn` / `apply_schema_migrations_on_conn` calls that follow are our own
        // schema maintenance statements, not untrusted input, so they need not pass the guard.
        temp_conn.authorizer(Some(import_authorizer));
        let batch_result = temp_conn.execute_batch(sql_content);
        temp_conn.authorizer(
            None::<fn(rusqlite::hooks::AuthContext<'_>) -> rusqlite::hooks::Authorization>,
        );
        batch_result
            .map_err(|e| AppError::Database(format!("Failed to execute the SQL import: {e}")))?;
        if !temp_conn.is_autocommit() {
            let _ = temp_conn.execute_batch("ROLLBACK;");
            return Err(AppError::localized(
                "backup.sql.incomplete_transaction",
                "The SQL backup transaction is incomplete; the file may be truncated.",
            ));
        }

        // Validate the schema produced by the input itself before migrations
        // can create missing tables and accidentally make a truncated file look valid.
        Self::validate_imported_schema(&temp_conn)?;

        // Add any missing tables/indexes and run the migrations
        Self::create_tables_on_conn(&temp_conn)?;
        Self::apply_schema_migrations_on_conn(&temp_conn)?;
        on_staging_ready()?;

        let backup_file_guard = lock_backup_file_operations()?;
        // Keep one main-DB guard across the safety snapshot, local-table read,
        // and final replacement so neither the rollback point nor preserved
        // device-local rows can miss writes that arrived during staging.
        let backup_path = {
            let mut main_conn = lock_conn!(self.conn);
            let backup_path =
                Self::backup_database_file_from_conn(&backup_file_guard, &main_conn, &[])?;
            if !preserve_tables.is_empty() {
                Self::restore_tables(&main_conn, &temp_conn, preserve_tables)?;
            }
            let backup = Backup::new(&temp_conn, &mut main_conn)
                .map_err(|e| AppError::Database(e.to_string()))?;
            Self::complete_backup(&backup, "replace the main database")?;
            backup_path
        };

        let backup_id = backup_path
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_default();

        Ok(backup_id)
    }

    /// Take an in-memory snapshot to avoid holding the database lock for a long time
    pub(crate) fn snapshot_to_memory(&self) -> Result<Connection, AppError> {
        let conn = lock_conn!(self.conn);
        let mut snapshot =
            Connection::open_in_memory().map_err(|e| AppError::Database(e.to_string()))?;

        {
            let backup =
                Backup::new(&conn, &mut snapshot).map_err(|e| AppError::Database(e.to_string()))?;
            Self::complete_backup(&backup, "create the in-memory database snapshot")?;
        }

        Ok(snapshot)
    }

    pub(crate) fn complete_backup(backup: &Backup<'_, '_>, context: &str) -> Result<(), AppError> {
        let result = backup
            .step(-1)
            .map_err(|e| AppError::Database(format!("Failed to {context}: {e}")))?;
        match result {
            StepResult::Done => Ok(()),
            StepResult::More | StepResult::Busy | StepResult::Locked => Err(AppError::Database(
                format!("Failed to {context}: SQLite Backup returned {result:?}"),
            )),
            _ => Err(AppError::Database(format!(
                "Failed to {context}: SQLite Backup returned an unknown status"
            ))),
        }
    }

    fn validate_cc_switch_sql_export(sql: &str) -> Result<(), AppError> {
        let trimmed = sql.trim_start();
        if trimmed.starts_with(SQL_EXPORT_HEADER)
            || trimmed.starts_with(CC_SWITCH_SQL_EXPORT_HEADER)
        {
            return Ok(());
        }

        Err(AppError::localized(
            "backup.sql.invalid_format",
            "Only SQL backups exported by AI Manager or CC Switch are supported.",
        ))
    }

    fn restore_tables(
        source_conn: &Connection,
        target_conn: &Connection,
        tables: &[&str],
    ) -> Result<(), AppError> {
        // The whole restore runs in one transaction: the old implementation issued one implicitly
        // auto-committed INSERT per row against an on-disk staging database, i.e. one fsync per
        // row — 119 seconds measured for 26k rows. A single transaction leaves just the final
        // commit; a mid-way failure rolls everything back instead of leaving half-filled tables.
        let tx = target_conn.unchecked_transaction().map_err(|e| {
            AppError::Database(format!("Failed to start the restore transaction: {e}"))
        })?;

        for table in tables {
            if !Self::table_exists(source_conn, table)? || !Self::table_exists(&tx, table)? {
                continue;
            }

            let columns = Self::get_table_columns(source_conn, table)?;
            if columns.is_empty() {
                continue;
            }

            let quoted_table = Self::quote_identifier(table);
            let quoted_columns = columns
                .iter()
                .map(|column| Self::quote_identifier(column))
                .collect::<Vec<_>>()
                .join(", ");

            tx.execute(&format!("DELETE FROM {quoted_table}"), [])
                .map_err(|e| AppError::Database(format!("Failed to clear table {table}: {e}")))?;

            let placeholders = (1..=columns.len())
                .map(|idx| format!("?{idx}"))
                .collect::<Vec<_>>()
                .join(", ");
            let insert_sql =
                format!("INSERT INTO {quoted_table} ({quoted_columns}) VALUES ({placeholders})");

            // The INSERT statement is prepared once per table instead of re-parsed for every row.
            let mut insert_stmt = tx.prepare(&insert_sql).map_err(|e| {
                AppError::Database(format!(
                    "Failed to prepare the insert statement for table {table}: {e}"
                ))
            })?;

            let mut stmt = source_conn
                .prepare(&format!("SELECT {quoted_columns} FROM {quoted_table}"))
                .map_err(|e| AppError::Database(format!("Failed to read table {table}: {e}")))?;
            let mut rows = stmt.query([]).map_err(|e| {
                AppError::Database(format!("Failed to query data from table {table}: {e}"))
            })?;

            while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
                let mut values = Vec::with_capacity(columns.len());
                for idx in 0..columns.len() {
                    values.push(
                        row.get::<_, rusqlite::types::Value>(idx)
                            .map_err(|e| AppError::Database(e.to_string()))?,
                    );
                }

                insert_stmt
                    .execute(rusqlite::params_from_iter(values.iter()))
                    .map_err(|e| {
                        AppError::Database(format!(
                            "Failed to restore data into table {table}: {e}"
                        ))
                    })?;
            }
        }

        Self::restore_sqlite_sequences(source_conn, &tx, tables)?;

        tx.commit().map_err(|e| {
            AppError::Database(format!("Failed to commit the restore transaction: {e}"))
        })?;
        Ok(())
    }

    fn restore_sqlite_sequences(
        source_conn: &Connection,
        target_conn: &Connection,
        tables: &[&str],
    ) -> Result<(), AppError> {
        if !Self::table_exists(source_conn, "sqlite_sequence")?
            || !Self::table_exists(target_conn, "sqlite_sequence")?
        {
            return Ok(());
        }

        let mut source_stmt = source_conn
            .prepare(
                "SELECT seq FROM sqlite_sequence
                 WHERE name = ?1 ORDER BY rowid DESC LIMIT 1",
            )
            .map_err(|e| {
                AppError::Database(format!("Failed to read the AUTOINCREMENT sequences: {e}"))
            })?;
        for table in tables {
            target_conn
                .execute("DELETE FROM sqlite_sequence WHERE name = ?1", [*table])
                .map_err(|e| {
                    AppError::Database(format!(
                        "Failed to clear the AUTOINCREMENT sequence of table {table}: {e}"
                    ))
                })?;

            let mut rows = source_stmt.query([*table]).map_err(|e| {
                AppError::Database(format!(
                    "Failed to query the sequence of table {table}: {e}"
                ))
            })?;
            if let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
                let sequence = row.get::<_, rusqlite::types::Value>(0).map_err(|e| {
                    AppError::Database(format!(
                        "Failed to parse the sequence of table {table}: {e}"
                    ))
                })?;
                target_conn
                    .execute(
                        "INSERT INTO sqlite_sequence (name, seq) VALUES (?1, ?2)",
                        rusqlite::params![table, sequence],
                    )
                    .map_err(|e| {
                        AppError::Database(format!(
                            "Failed to restore the AUTOINCREMENT sequence of table {table}: {e}"
                        ))
                    })?;
            }
        }
        Ok(())
    }

    /// Periodic backup: create a new backup if the latest one is older than the configured interval
    pub(crate) fn periodic_backup_if_needed(&self) -> Result<(), AppError> {
        let interval_hours = crate::settings::effective_backup_interval_hours();
        if interval_hours > 0 {
            let backup_file_guard = lock_backup_file_operations()?;
            let backup_dir = Self::backups_dir();
            if !backup_dir.exists() {
                self.backup_database_file_locked(&backup_file_guard)?;
            } else {
                let latest = fs::read_dir(&backup_dir).ok().and_then(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().map(|ext| ext == "db").unwrap_or(false))
                        .filter_map(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
                        .max()
                });

                let interval_secs = u64::from(interval_hours) * 3600;
                let needs_backup = match latest {
                    None => true,
                    Some(last_modified) => {
                        last_modified.elapsed().unwrap_or_default()
                            > std::time::Duration::from_secs(interval_secs)
                    }
                };

                if needs_backup {
                    log::info!(
                        "Periodic backup: latest backup is older than {interval_hours} hours, creating new backup"
                    );
                    self.backup_database_file_locked(&backup_file_guard)?;
                }
            }
        }

        // Periodic maintenance is always enabled, regardless of auto-backup settings.
        let mut reclaimed_rows = 0u64;
        match self.cleanup_old_stream_check_logs(7) {
            Ok(deleted) => {
                reclaimed_rows += deleted;
            }
            Err(e) => {
                log::warn!("Periodic stream_check_logs cleanup failed: {e}");
            }
        }
        match self.rollup_and_prune(30) {
            Ok(deleted) => {
                reclaimed_rows += deleted;
            }
            Err(e) => {
                log::warn!("Periodic rollup_and_prune failed: {e}");
            }
        }
        if reclaimed_rows > 0 {
            let conn = lock_conn!(self.conn);
            if let Err(e) = conn.execute_batch("PRAGMA incremental_vacuum;") {
                log::warn!("Periodic incremental vacuum failed: {e}");
            }
        }

        Ok(())
    }

    /// Produce a consistent snapshot backup; returns the backup file path (None when there is no main database)
    pub(crate) fn backup_database_file(&self) -> Result<Option<PathBuf>, AppError> {
        let backup_file_guard = lock_backup_file_operations()?;
        self.backup_database_file_locked(&backup_file_guard)
    }

    fn backup_database_file_locked(
        &self,
        backup_file_guard: &BackupFileOperationGuard,
    ) -> Result<Option<PathBuf>, AppError> {
        let conn = lock_conn!(self.conn);
        Self::backup_database_file_from_conn(backup_file_guard, &conn, &[])
    }

    /// Create a safety backup from a connection whose caller already owns both
    /// the backup-file operation guard and the appropriate database guard.
    fn backup_database_file_from_conn(
        backup_file_guard: &BackupFileOperationGuard,
        source_conn: &Connection,
        protected_paths: &[&Path],
    ) -> Result<Option<PathBuf>, AppError> {
        Self::backup_database_file_from_conn_with_hook(
            backup_file_guard,
            source_conn,
            protected_paths,
            |_, _| Ok(()),
        )
    }

    fn backup_database_file_from_conn_with_hook<F>(
        _backup_file_guard: &BackupFileOperationGuard,
        source_conn: &Connection,
        protected_paths: &[&Path],
        before_publish: F,
    ) -> Result<Option<PathBuf>, AppError>
    where
        F: FnOnce(&Path, &Path) -> Result<(), AppError>,
    {
        // The path must come from **this connection itself**: global resolution would point at a
        // file unrelated to the current connection once the user overrides the directory at
        // runtime, silently turning the backup into a no-op (Phase 1 final review TD).
        // An in-memory database returns Some(""), which likewise has no file to back up.
        let Some(path) = source_conn.path().filter(|path| !path.is_empty()) else {
            return Ok(None);
        };
        let db_path = PathBuf::from(path);
        if !db_path.exists() {
            return Ok(None);
        }

        let backup_dir = Self::backups_dir();

        fs::create_dir_all(&backup_dir).map_err(|e| AppError::io(&backup_dir, e))?;

        let base_id = format!("db_backup_{}", Local::now().format("%Y%m%d_%H%M%S"));
        let mut next_suffix = 0;
        let mut backup_path =
            Self::next_available_backup_path(&backup_dir, &base_id, &mut next_suffix);

        // Build and validate the backup under a non-.db temporary name. Backup
        // discovery and retention only see the final path after the complete
        // SQLite image has been atomically published.
        let mut temp_path = Builder::new()
            .prefix(".cc-switch-backup-")
            .suffix(".tmp")
            .tempfile_in(&backup_dir)
            .map_err(|e| AppError::io(&backup_dir, e))?
            .into_temp_path();
        let temp_db_path: &Path = temp_path.as_ref();
        let mut dest_conn =
            Connection::open(temp_db_path).map_err(|e| AppError::Database(e.to_string()))?;
        let backup = Backup::new(source_conn, &mut dest_conn)
            .map_err(|e| AppError::Database(e.to_string()))?;
        Self::complete_backup(&backup, "create the database safety backup")?;
        drop(backup);
        Self::validate_sqlite_integrity(&dest_conn)?;
        dest_conn.close().map_err(|(_, e)| {
            AppError::Database(format!("Failed to close the database safety backup: {e}"))
        })?;
        before_publish(temp_db_path, &backup_path)?;

        loop {
            match temp_path.persist_noclobber(&backup_path) {
                Ok(()) => break,
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    temp_path = error.path;
                    backup_path =
                        Self::next_available_backup_path(&backup_dir, &base_id, &mut next_suffix);
                }
                Err(error) => return Err(AppError::io(&backup_path, error.error)),
            }
        }

        // The newly created safety backup must never be the cleanup victim.
        // During restore, the selected source is protected as well. If the
        // configured retention is too small to keep both, temporarily exceed
        // it instead of deleting either side of the recovery operation.
        let mut cleanup_protected = Vec::with_capacity(protected_paths.len() + 1);
        cleanup_protected.push(backup_path.as_path());
        cleanup_protected.extend_from_slice(protected_paths);
        Self::cleanup_db_backups(&backup_dir, &cleanup_protected)?;
        Ok(Some(backup_path))
    }

    fn next_available_backup_path(
        backup_dir: &Path,
        base_id: &str,
        next_suffix: &mut usize,
    ) -> PathBuf {
        loop {
            let backup_id = if *next_suffix == 0 {
                base_id.to_string()
            } else {
                format!("{base_id}_{}", *next_suffix)
            };
            *next_suffix += 1;
            let backup_path = backup_dir.join(format!("{backup_id}.db"));
            if !backup_path.exists() {
                return backup_path;
            }
        }
    }

    fn same_existing_backup_path(left: &Path, right: &Path) -> bool {
        match (fs::canonicalize(left), fs::canonicalize(right)) {
            (Ok(left), Ok(right)) => left == right,
            _ => left == right,
        }
    }

    /// Remove old database backups, keeping the newest N
    fn cleanup_db_backups(dir: &Path, protected_paths: &[&Path]) -> Result<(), AppError> {
        let retain = crate::settings::effective_backup_retain_count();
        let entries = match fs::read_dir(dir) {
            Ok(iter) => iter
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .map(|ext| ext == "db")
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>(),
            Err(_) => return Ok(()),
        };

        if entries.len() <= retain {
            return Ok(());
        }

        let remove_count = entries.len().saturating_sub(retain);
        let mut sorted = entries;
        sorted.sort_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok());

        let mut removed = 0;
        for entry in sorted {
            if removed >= remove_count {
                break;
            }
            let path = entry.path();
            if protected_paths
                .iter()
                .any(|protected| Self::same_existing_backup_path(&path, protected))
            {
                continue;
            }

            if let Err(err) = fs::remove_file(&path) {
                log::warn!(
                    "Failed to delete old database backup {}: {}",
                    path.display(),
                    err
                );
            } else {
                removed += 1;
            }
        }
        Ok(())
    }

    pub(crate) fn validate_sqlite_integrity(conn: &Connection) -> Result<(), AppError> {
        let mut stmt = conn
            .prepare("PRAGMA quick_check;")
            .map_err(|e| AppError::Database(format!("Failed to check database integrity: {e}")))?;
        let results = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| AppError::Database(format!("Failed to check database integrity: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(format!("Failed to check database integrity: {e}")))?;

        if results.len() == 1 && results[0].eq_ignore_ascii_case("ok") {
            return Ok(());
        }

        Err(AppError::localized(
            "backup.db.integrity_failed",
            format!(
                "Database backup integrity check failed: {}",
                results.join("; ")
            ),
        ))
    }

    /// Validate that the external SQL created a recognizable CC Switch schema.
    ///
    /// These tables all existed in the oldest supported SQL-export schema
    /// (v3.8.x). Checking before migrations keeps header-only/truncated files
    /// from being completed by `create_tables_on_conn`, while allowing a valid
    /// backup whose user-owned configuration tables happen to contain no rows.
    pub(crate) fn validate_imported_schema(conn: &Connection) -> Result<(), AppError> {
        const REQUIRED_TABLES: &[&str] = &[
            "providers",
            "provider_endpoints",
            "mcp_servers",
            "prompts",
            "skills",
            "skill_repos",
            "settings",
        ];

        let mut missing = Vec::new();
        for table in REQUIRED_TABLES {
            if !Self::table_exists(conn, table)? {
                missing.push(*table);
            }
        }
        if !missing.is_empty() {
            let names = missing.join(", ");
            return Err(AppError::localized(
                "backup.sql.invalid_schema",
                format!("The imported SQL is missing required CC Switch tables: {names}"),
            ));
        }
        Ok(())
    }

    /// Dump the database as SQL text
    fn dump_sql(conn: &Connection, skip_tables: &[&str]) -> Result<String, AppError> {
        let mut output = String::new();
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_version: i64 = conn
            .query_row("PRAGMA user_version;", [], |row| row.get(0))
            .unwrap_or(0);

        output.push_str(&format!(
            "{SQL_EXPORT_HEADER}\n-- Generated at: {timestamp}\n-- user_version: {user_version}\n"
        ));
        output.push_str("PRAGMA foreign_keys=OFF;\n");
        output.push_str(&format!("PRAGMA user_version={user_version};\n"));
        output.push_str("BEGIN TRANSACTION;\n");

        // Export the schema
        let mut stmt = conn
            .prepare(
                "SELECT type, name, tbl_name, sql
                 FROM sqlite_master
                 WHERE sql NOT NULL AND type IN ('table','index','trigger','view')
                 ORDER BY type='table' DESC, name",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut tables = Vec::new();
        let mut triggers = Vec::new();
        let mut rows = stmt
            .query([])
            .map_err(|e| AppError::Database(e.to_string()))?;
        while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
            let obj_type: String = row.get(0).map_err(|e| AppError::Database(e.to_string()))?;
            let name: String = row.get(1).map_err(|e| AppError::Database(e.to_string()))?;
            let sql: String = row.get(3).map_err(|e| AppError::Database(e.to_string()))?;

            // Skip SQLite internal objects (e.g. sqlite_sequence)
            if name.starts_with("sqlite_") {
                continue;
            }

            if obj_type == "trigger" {
                triggers.push(sql);
                continue;
            }

            output.push_str(&sql);
            output.push_str(";\n");
            if obj_type == "table" {
                tables.push(name);
            }
        }

        // Export the data
        for table in tables {
            if skip_tables.iter().any(|t| *t == table) {
                continue;
            }
            let columns = Self::get_table_columns(conn, &table)?;
            if columns.is_empty() {
                continue;
            }

            // One INSERT per row is the root cause of slow imports: the restore side has to parse,
            // prepare and finalize every statement separately — 21 seconds measured for 20k rows
            // (equally slow on an in-memory database, so it is pure CPU rather than I/O). Merging
            // into multi-row VALUES brings the same data below 100ms. SQLite has supported
            // multi-row VALUES since 3.7.11 (2012), and the import side is a generic
            // execute_batch that reads both the old and the new format.
            let quoted_table = Self::quote_identifier(&table);
            let quoted_columns = columns
                .iter()
                .map(|column| Self::quote_identifier(column))
                .collect::<Vec<_>>()
                .join(", ");
            let insert_prefix = format!("INSERT INTO {quoted_table} ({quoted_columns}) VALUES ");

            let mut stmt = conn
                .prepare(&format!("SELECT {quoted_columns} FROM {quoted_table}"))
                .map_err(|e| AppError::Database(e.to_string()))?;
            let mut rows = stmt
                .query([])
                .map_err(|e| AppError::Database(e.to_string()))?;

            let mut pending_rows = 0usize;
            let mut batch = String::new();
            while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
                let mut values = Vec::with_capacity(columns.len());
                for idx in 0..columns.len() {
                    let value = row
                        .get_ref(idx)
                        .map_err(|e| AppError::Database(e.to_string()))?;
                    values.push(Self::format_sql_value(value)?);
                }

                let row_sql = format!("({})", values.join(", "));
                let separator_bytes = usize::from(pending_rows > 0);
                if pending_rows > 0
                    && batch.len() + separator_bytes + row_sql.len() + 2 > INSERT_BATCH_MAX_BYTES
                {
                    batch.push_str(";\n");
                    output.push_str(&batch);
                    pending_rows = 0;
                }

                if pending_rows == 0 {
                    batch.clear();
                    batch.push_str(&insert_prefix);
                } else {
                    batch.push(',');
                }
                batch.push_str(&row_sql);
                pending_rows += 1;

                if pending_rows >= INSERT_BATCH_MAX_ROWS {
                    batch.push_str(";\n");
                    output.push_str(&batch);
                    pending_rows = 0;
                }
            }
            if pending_rows > 0 {
                batch.push_str(";\n");
                output.push_str(&batch);
            }
        }

        Self::dump_sqlite_sequences(conn, skip_tables, &mut output)?;

        // Triggers must be created after loading table data so they cannot
        // change dump rows or abandon the remainder of a multi-row INSERT.
        for sql in triggers {
            output.push_str(&sql);
            output.push_str(";\n");
        }

        output.push_str("COMMIT;\nPRAGMA foreign_keys=ON;\n");
        Ok(output)
    }

    fn dump_sqlite_sequences(
        conn: &Connection,
        skip_tables: &[&str],
        output: &mut String,
    ) -> Result<(), AppError> {
        if !Self::table_exists(conn, "sqlite_sequence")? {
            return Ok(());
        }

        let mut stmt = conn
            .prepare("SELECT name, seq FROM sqlite_sequence ORDER BY name")
            .map_err(|e| {
                AppError::Database(format!("Failed to read the AUTOINCREMENT sequences: {e}"))
            })?;
        let mut rows = stmt.query([]).map_err(|e| {
            AppError::Database(format!("Failed to query the AUTOINCREMENT sequences: {e}"))
        })?;
        let mut values = Vec::new();
        while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
            let table: String = row.get(0).map_err(|e| {
                AppError::Database(format!("Failed to parse the AUTOINCREMENT table name: {e}"))
            })?;
            if skip_tables.iter().any(|skipped| *skipped == table) {
                continue;
            }
            let sequence = row.get_ref(1).map_err(|e| {
                AppError::Database(format!(
                    "Failed to parse the sequence of table {table}: {e}"
                ))
            })?;
            values.push(format!(
                "({}, {})",
                Self::format_sql_value(ValueRef::Text(table.as_bytes()))?,
                Self::format_sql_value(sequence)?
            ));
        }

        // Data INSERTs update sqlite_sequence to MAX(rowid), which loses a
        // deleted high-water mark. Replace those derived values with the exact
        // source metadata after all user-table rows have been loaded.
        output.push_str("DELETE FROM sqlite_sequence;\n");
        if !values.is_empty() {
            output.push_str("INSERT INTO sqlite_sequence (name, seq) VALUES ");
            output.push_str(&values.join(","));
            output.push_str(";\n");
        }
        Ok(())
    }

    fn quote_identifier(identifier: &str) -> String {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }

    /// Get the column names of a table
    fn get_table_columns(conn: &Connection, table: &str) -> Result<Vec<String>, AppError> {
        let quoted_table = Self::quote_identifier(table);
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({quoted_table})"))
            .map_err(|e| AppError::Database(e.to_string()))?;
        let iter = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut columns = Vec::new();
        for col in iter {
            columns.push(col.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(columns)
    }

    /// Format a SQL value
    fn format_sql_value(value: ValueRef<'_>) -> Result<String, AppError> {
        match value {
            ValueRef::Null => Ok("NULL".to_string()),
            ValueRef::Integer(i) => Ok(i.to_string()),
            ValueRef::Real(f) => Ok(Self::format_sql_real(f)),
            ValueRef::Text(t) => match std::str::from_utf8(t) {
                // SQLite's SQL parser treats NUL as the end of the statement.
                // Keep readable literals for normal UTF-8, and use a hex cast
                // whenever a TEXT value cannot safely appear in SQL source.
                Ok(text) if !text.contains('\0') => {
                    let escaped = text.replace('\'', "''");
                    Ok(format!("'{escaped}'"))
                }
                _ => Ok(format!("CAST({} AS TEXT)", Self::format_sql_blob(t))),
            },
            ValueRef::Blob(bytes) => Ok(Self::format_sql_blob(bytes)),
        }
    }

    fn format_sql_real(value: f64) -> String {
        if value.is_nan() {
            // SQLite normalizes bound NaN values to NULL as well.
            return "NULL".to_string();
        }
        if value.is_infinite() {
            return if value.is_sign_negative() {
                "-9.0e999".to_string()
            } else {
                "9.0e999".to_string()
            };
        }
        if value == 0.0 && value.is_sign_negative() {
            return "-0.0".to_string();
        }

        let mut literal = value.to_string();
        if !literal.contains(['.', 'e', 'E']) {
            // Without a decimal point/exponent SQLite stores integer-valued
            // REALs as INTEGER in columns without REAL affinity (e.g. STRICT ANY).
            literal.push_str(".0");
        }
        literal
    }

    fn format_sql_blob(bytes: &[u8]) -> String {
        let mut s = String::from("X'");
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(&mut s, "{b:02X}");
        }
        s.push('\'');
        s
    }

    /// List all database backup files, sorted by creation time (newest first)
    pub fn list_backups() -> Result<Vec<BackupEntry>, AppError> {
        let _backup_file_guard = lock_backup_file_operations()?;
        let backup_dir = Self::backups_dir();
        if !backup_dir.exists() {
            return Ok(vec![]);
        }

        let mut entries: Vec<BackupEntry> = fs::read_dir(&backup_dir)
            .map_err(|e| AppError::io(&backup_dir, e))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "db").unwrap_or(false))
            .filter_map(|e| {
                let metadata = e.metadata().ok()?;
                let filename = e.file_name().to_string_lossy().to_string();
                let size_bytes = metadata.len();
                let created_at = metadata
                    .modified()
                    .ok()
                    .map(|t| {
                        let dt: chrono::DateTime<Utc> = t.into();
                        dt.to_rfc3339()
                    })
                    .unwrap_or_default();
                Some(BackupEntry {
                    filename,
                    size_bytes,
                    created_at,
                })
            })
            .collect();

        // Sort by created_at descending (newest first)
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(entries)
    }

    /// Restore database from a backup file. Returns the safety backup ID.
    pub fn restore_from_backup(&self, filename: &str) -> Result<String, AppError> {
        self.restore_from_backup_with_hook(filename, |_| Ok(()))
    }

    fn restore_from_backup_with_hook<F>(
        &self,
        filename: &str,
        before_replace: F,
    ) -> Result<String, AppError>
    where
        F: FnOnce(Option<&Path>) -> Result<(), AppError>,
    {
        // Security: validate filename to prevent path traversal
        if filename.contains("..")
            || filename.contains('/')
            || filename.contains('\\')
            || !filename.ends_with(".db")
        {
            return Err(AppError::InvalidInput(
                "Invalid backup filename".to_string(),
            ));
        }

        let backup_file_guard = lock_backup_file_operations()?;
        let backup_dir = Self::backups_dir();
        let backup_path = backup_dir.join(filename);

        if !backup_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "Backup file not found: {filename}"
            )));
        }

        // Open read-only before creating the safety backup. `Connection::open`
        // would recreate a source removed by retention cleanup as an empty DB.
        let source_conn = Connection::open_with_flags(
            &backup_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Stage and fully validate the selected file before touching the live
        // connection. A corrupt/future-schema backup or failed migration must
        // leave the current database unchanged.
        let temp_file = NamedTempFile::new().map_err(|e| AppError::IoContext {
            context: "Failed to create the database restore staging file".to_string(),
            source: e,
        })?;
        let mut staging_conn =
            Connection::open(temp_file.path()).map_err(|e| AppError::Database(e.to_string()))?;
        {
            let backup = Backup::new(&source_conn, &mut staging_conn)
                .map_err(|e| AppError::Database(e.to_string()))?;
            Self::complete_backup(&backup, "read the database backup")?;
        }
        drop(source_conn);

        Self::validate_sqlite_integrity(&staging_conn)?;
        Self::validate_imported_schema(&staging_conn)?;
        Self::ensure_incremental_auto_vacuum_on_conn(&staging_conn)?;
        Self::create_tables_on_conn(&staging_conn)?;
        Self::apply_schema_migrations_on_conn(&staging_conn)?;
        Self::ensure_model_pricing_seeded_on_conn(&staging_conn)?;
        Self::validate_sqlite_integrity(&staging_conn)?;

        // Keep one main-DB guard across the safety snapshot and final apply so
        // the safety file exactly represents the state being replaced.
        let safety_backup = {
            let mut main_conn = lock_conn!(self.conn);
            let safety_backup = Self::backup_database_file_from_conn(
                &backup_file_guard,
                &main_conn,
                &[backup_path.as_path()],
            )?;
            before_replace(safety_backup.as_deref())?;
            let backup = Backup::new(&staging_conn, &mut main_conn)
                .map_err(|e| AppError::Database(e.to_string()))?;
            Self::complete_backup(&backup, "restore the main database")?;
            safety_backup
        };
        let safety_id = safety_backup
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_default();

        log::info!("Database restored from backup: {filename}, safety backup: {safety_id}");
        Ok(safety_id)
    }

    /// Rename a backup file. Returns the new filename.
    pub fn rename_backup(old_filename: &str, new_name: &str) -> Result<String, AppError> {
        // Validate old filename (path traversal + .db suffix)
        if old_filename.contains("..")
            || old_filename.contains('/')
            || old_filename.contains('\\')
            || !old_filename.ends_with(".db")
        {
            return Err(AppError::InvalidInput(
                "Invalid backup filename".to_string(),
            ));
        }

        // Clean new name
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err(AppError::InvalidInput(
                "New name cannot be empty".to_string(),
            ));
        }

        // Length limit (without .db suffix)
        let name_part = trimmed.strip_suffix(".db").unwrap_or(trimmed);
        if name_part.len() > 100 {
            return Err(AppError::InvalidInput(
                "Name too long (max 100 characters)".to_string(),
            ));
        }

        // Prevent path traversal in new name
        if name_part.contains("..")
            || name_part.contains('/')
            || name_part.contains('\\')
            || name_part.contains('\0')
        {
            return Err(AppError::InvalidInput(
                "Invalid characters in new name".to_string(),
            ));
        }

        let new_filename = format!("{name_part}.db");

        let _backup_file_guard = lock_backup_file_operations()?;
        let backup_dir = Self::backups_dir();
        let old_path = backup_dir.join(old_filename);
        let new_path = backup_dir.join(&new_filename);

        if !old_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "Backup file not found: {old_filename}"
            )));
        }

        if new_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "A backup named '{new_filename}' already exists"
            )));
        }

        fs::rename(&old_path, &new_path).map_err(|e| AppError::io(&old_path, e))?;
        log::info!("Renamed backup: {old_filename} -> {new_filename}");
        Ok(new_filename)
    }

    /// Delete a backup file permanently.
    pub fn delete_backup(filename: &str) -> Result<(), AppError> {
        // Validate filename (path traversal + .db suffix)
        if filename.contains("..")
            || filename.contains('/')
            || filename.contains('\\')
            || !filename.ends_with(".db")
        {
            return Err(AppError::InvalidInput(
                "Invalid backup filename".to_string(),
            ));
        }

        let _backup_file_guard = lock_backup_file_operations()?;
        let backup_path = Self::backups_dir().join(filename);
        if !backup_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "Backup file not found: {filename}"
            )));
        }

        fs::remove_file(&backup_path).map_err(|e| AppError::io(&backup_path, e))?;
        log::info!("Deleted backup: {filename}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{lock_backup_file_operations, Database};
    use crate::error::AppError;
    use crate::settings::{get_settings, update_settings, AppSettings};
    use rusqlite::Connection;
    use serial_test::serial;

    struct TestHomeGuard {
        previous_test_home: Option<std::ffi::OsString>,
        temp_dir: tempfile::TempDir,
    }

    impl TestHomeGuard {
        fn new() -> Self {
            let temp_dir = tempfile::tempdir().expect("create isolated test home");
            let previous_test_home = std::env::var_os("AI_MANAGER_TEST_HOME");
            std::env::set_var("AI_MANAGER_TEST_HOME", temp_dir.path());
            // Prevent the Windows legacy-HOME fallback without mutating HOME:
            // an existing default DB keeps CC Switch discovery anchored under
            // AI_MANAGER_TEST_HOME and makes import exercise its safety backup.
            let config_dir = temp_dir.path().join(".cc-switch");
            std::fs::create_dir_all(&config_dir).expect("create isolated config directory");
            std::fs::File::create(config_dir.join("cc-switch.db"))
                .expect("create isolated database sentinel");
            let guard = Self {
                previous_test_home,
                temp_dir,
            };
            let resolved = crate::config::get_default_cc_switch_config_dir();
            assert!(
                resolved.starts_with(guard.temp_dir.path()),
                "isolated test home resolved outside its temp directory: {}",
                resolved.display()
            );
            guard
        }

        fn path(&self) -> &std::path::Path {
            self.temp_dir.path()
        }
    }

    impl Drop for TestHomeGuard {
        fn drop(&mut self) {
            match self.previous_test_home.as_ref() {
                Some(previous) => std::env::set_var("AI_MANAGER_TEST_HOME", previous),
                None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
            }
        }
    }

    struct SettingsGuard {
        previous: AppSettings,
    }

    impl SettingsGuard {
        fn with_backup_retain_count(retain: u32) -> Self {
            let previous = get_settings();
            let mut next = previous.clone();
            next.backup_retain_count = Some(retain);
            update_settings(next).expect("set backup retention for test");
            Self { previous }
        }
    }

    impl Drop for SettingsGuard {
        fn drop(&mut self) {
            let _ = update_settings(self.previous.clone());
        }
    }

    #[test]
    #[serial]
    fn backups_live_next_to_the_product_database() {
        let _guard = TestHomeGuard::new();
        let backups = Database::backups_dir();
        assert_eq!(
            backups.parent(),
            Some(crate::infrastructure::paths::product_data_dir().as_path()),
            "backups must sit in the product data dir, not the upstream config dir"
        );
        assert_eq!(
            backups.file_name().and_then(|name| name.to_str()),
            Some("backups")
        );
        assert_eq!(
            backups.parent(),
            crate::infrastructure::paths::app_db_path().parent(),
            "the backup dir and app.db must never drift apart"
        );
    }

    #[test]
    #[serial]
    fn the_backup_gate_reads_the_path_of_the_connection_it_was_given() {
        let _guard = TestHomeGuard::new();
        // An in-memory database has no file path: the gate must decide "no file to back up" and
        // return None instead of resolving a global path unrelated to this connection.
        let memory = Connection::open_in_memory().expect("open in-memory database");
        let file_guard = lock_backup_file_operations().expect("backup file lock");
        let produced = Database::backup_database_file_from_conn(&file_guard, &memory, &[])
            .expect("the gate must not error on a pathless connection");
        assert_eq!(produced, None);
    }

    #[test]
    #[serial]
    fn import_rejects_cross_file_statements_and_leaves_no_file_behind() -> Result<(), AppError> {
        let test_home = TestHomeGuard::new();
        // `VACUUM INTO` is the case a keyword scan misses most easily: it contains no "ATTACH",
        // yet lands on `AuthAction::Attach` just like ATTACH (measured), so one rule blocks both.
        let cases: [(&str, &str); 2] = [
            ("attach", "ATTACH DATABASE '{path}' AS evil;"),
            ("vacuum-into", "VACUUM INTO '{path}';"),
        ];

        for (label, template) in cases {
            let target = test_home
                .path()
                .join(format!("cc-switch-authorizer-{label}.sqlite"));

            // A valid export header plus an escaping statement. The header check only compares the
            // prefix, so this input passes it; the authorizer is what must actually stop it.
            let malicious = format!(
                "{}\n{}\n",
                super::CC_SWITCH_SQL_EXPORT_HEADER,
                template.replace("{path}", &target.to_string_lossy().replace('\'', "''"))
            );

            let db = Database::memory()?;
            let result = db.import_sql_string(&malicious);

            let error = result.expect_err("escaping SQL must be rejected");
            assert!(
                error.to_string().to_ascii_lowercase().contains("authoriz"),
                "{label} must be rejected by the authorizer, actual error: {error}"
            );
            // An error alone is not enough: the file is created after prepare but before the
            // staging schema is validated, so a broken guard leaves the file on disk even when
            // the import as a whole fails.
            assert!(
                !target.exists(),
                "a rejected {label} must not leave a file on disk: {}",
                target.display()
            );
        }
        Ok(())
    }

    #[test]
    #[serial]
    fn import_still_accepts_a_genuine_export() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        // The allowlist is tight, so a regression test must prove it does not break our own export
        // format — if this test goes red, dump_sql emitted a statement the allowlist misses.
        let source = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(source.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('p1', 'claude', 'Provider One', '{}', '{}')",
                [],
            )?;
        }
        let exported = source.export_sql_string()?;

        let target = Database::memory()?;
        target.import_sql_string(&exported)?;

        let conn = crate::database::lock_conn!(target.conn);
        let name: String = conn.query_row(
            "SELECT name FROM providers WHERE id = 'p1' AND app_type = 'claude'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(name, "Provider One");
        Ok(())
    }

    #[test]
    #[serial]
    fn import_accepts_genuine_export_without_provider_or_mcp_rows() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let source = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(source.conn);
            let counts: (i64, i64) = conn.query_row(
                "SELECT
                    (SELECT COUNT(*) FROM providers),
                    (SELECT COUNT(*) FROM mcp_servers)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(counts, (0, 0));
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('empty-export-marker', 'kept')",
                [],
            )?;
        }
        let sql = source.export_sql_string()?;

        let target = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(target.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('target-sentinel', 'claude', 'Must Be Cleared', '{}', '{}')",
                [],
            )?;
        }
        target.import_sql_string(&sql)?;

        let conn = crate::database::lock_conn!(target.conn);
        let counts: (i64, i64) = conn.query_row(
            "SELECT
                (SELECT COUNT(*) FROM providers),
                (SELECT COUNT(*) FROM mcp_servers)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(counts, (0, 0));
        let marker: String = conn.query_row(
            "SELECT value FROM settings WHERE key = 'empty-export-marker'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(marker, "kept");
        Ok(())
    }

    #[test]
    fn sql_import_accepts_both_the_product_and_cc_switch_headers() {
        assert!(Database::validate_cc_switch_sql_export("-- AI Manager SQLite export\n").is_ok());
        assert!(
            Database::validate_cc_switch_sql_export("-- CC Switch SQLite \u{5bfc}\u{51fa}\n")
                .is_ok()
        );
        assert!(Database::validate_cc_switch_sql_export("-- someone else's dump\n").is_err());
    }

    #[test]
    #[serial]
    fn import_rejects_header_only_sql_and_keeps_existing_database() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let target = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(target.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('sentinel', 'claude', 'Existing Provider', '{}', '{}')",
                [],
            )?;
        }

        let header_only = format!(
            "{}\nPRAGMA foreign_keys=OFF;\nBEGIN TRANSACTION;\nCOMMIT;\n",
            super::CC_SWITCH_SQL_EXPORT_HEADER
        );
        let error = target
            .import_sql_string(&header_only)
            .expect_err("a file missing the original schema must be rejected");
        assert!(
            error.to_string().contains("required CC Switch tables"),
            "the original schema check should reject it, actual error: {error}"
        );

        let conn = crate::database::lock_conn!(target.conn);
        let provider: (i64, String) = conn.query_row(
            "SELECT COUNT(*), MIN(name) FROM providers WHERE id = 'sentinel'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(provider, (1, "Existing Provider".into()));
        Ok(())
    }

    #[test]
    #[serial]
    fn sql_import_preserves_incremental_auto_vacuum() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let source = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(source.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('vacuum-provider', 'claude', 'Vacuum Provider', '{}', '{}')",
                [],
            )?;
        }
        let sql = source.export_sql_string()?;

        let target = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(target.conn);
            assert_eq!(Database::get_auto_vacuum_mode(&conn)?, 2);
        }

        target.import_sql_string(&sql)?;

        let conn = crate::database::lock_conn!(target.conn);
        assert_eq!(
            Database::get_auto_vacuum_mode(&conn)?,
            2,
            "a SQL import must not downgrade the main database from INCREMENTAL auto_vacuum to NONE"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn sql_file_api_round_trips_existing_export_behavior() -> Result<(), AppError> {
        let test_home = TestHomeGuard::new();
        let source = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(source.conn);
            conn.execute_batch(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('file-provider', 'claude', 'File Provider', '{}', '{}');
                 INSERT INTO proxy_request_logs (
                     request_id, provider_id, app_type, model,
                     input_tokens, output_tokens, total_cost_usd,
                     latency_ms, status_code, created_at
                 ) VALUES ('file-request', 'file-provider', 'claude', 'claude-file', 5, 3, '0', 10, 200, 1);",
            )?;
        }

        let backup_path = test_home.path().join("round-trip.sql");
        source.export_sql(&backup_path)?;

        let target = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(target.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('target-sentinel', 'claude', 'Must Be Replaced', '{}', '{}')",
                [],
            )?;
        }
        target.import_sql(&backup_path)?;

        let conn = crate::database::lock_conn!(target.conn);
        let providers = conn
            .prepare("SELECT id FROM providers ORDER BY id")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(providers, vec!["file-provider"]);
        let request_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM proxy_request_logs WHERE request_id = 'file-request')",
            [],
            |row| row.get(0),
        )?;
        assert!(
            request_exists,
            "the file API must fully restore the exported data"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn failed_sql_import_keeps_the_existing_database_unchanged() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let target = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(target.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('sentinel', 'claude', 'Existing Provider', '{}', '{}')",
                [],
            )?;
        }

        let invalid_sql = format!(
            "{}\nBEGIN TRANSACTION;\nCREATE TABLE partial (id INTEGER);\nTHIS IS NOT SQL;\n",
            super::CC_SWITCH_SQL_EXPORT_HEADER
        );
        assert!(target.import_sql_string(&invalid_sql).is_err());

        let conn = crate::database::lock_conn!(target.conn);
        let provider: (i64, String, String) = conn.query_row(
            "SELECT COUNT(*), MIN(id), MIN(name) FROM providers",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(provider, (1, "sentinel".into(), "Existing Provider".into()));
        let partial_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'partial')",
            [],
            |row| row.get(0),
        )?;
        assert!(
            !partial_exists,
            "temporary objects of a failed import must not reach the main database"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn import_rejects_truncated_open_transaction_and_keeps_live_database() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let source = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(source.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('remote-provider', 'claude', 'Remote Provider', '{}', '{}')",
                [],
            )?;
        }
        let exported = source.export_sql_string()?;
        let truncated = exported
            .strip_suffix("COMMIT;\nPRAGMA foreign_keys=ON;\n")
            .expect("CC Switch export should end with a committed transaction");

        let target = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(target.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('live-provider', 'claude', 'Live Provider', '{}', '{}')",
                [],
            )?;
        }

        let error = target
            .import_sql_string(truncated)
            .expect_err("an export truncated before COMMIT must be rejected");
        assert!(
            error.to_string().contains("incomplete") || error.to_string().contains("truncated"),
            "unexpected error: {error}"
        );

        let conn = crate::database::lock_conn!(target.conn);
        let providers = conn
            .prepare("SELECT id FROM providers ORDER BY id")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(providers, vec!["live-provider"]);
        Ok(())
    }

    #[test]
    #[serial]
    fn import_still_accepts_legacy_single_row_insert_exports() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        // This schema is copied from the v3.8.3 tag. Its data statements use
        // the historical one-row-per-INSERT format and omit all newer columns.
        let legacy = format!(
            "{}\nPRAGMA foreign_keys=OFF;\nPRAGMA user_version=1;\nBEGIN TRANSACTION;\n{}
             INSERT INTO providers (
                 id, app_type, name, settings_config, meta, is_current
             ) VALUES (
                 'legacy-provider', 'claude', 'Legacy Provider',
                 '{{\"anthropicApiKey\":\"sk-old\"}}', '{{}}', 1
             );
             INSERT INTO skills (key, installed, installed_at)
             VALUES ('claude:legacy-skill', 1, 1700000000);
             COMMIT;\nPRAGMA foreign_keys=ON;\n",
            super::CC_SWITCH_SQL_EXPORT_HEADER,
            crate::database::tests::V3_8_SCHEMA_V1_SQL,
        );

        let target = Database::memory()?;
        target.import_sql_string(&legacy)?;

        let conn = crate::database::lock_conn!(target.conn);
        let user_version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        assert_eq!(user_version, crate::database::SCHEMA_VERSION);
        let provider: (String, String) = conn.query_row(
            "SELECT name, settings_config FROM providers WHERE id = 'legacy-provider'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(
            provider,
            (
                "Legacy Provider".into(),
                "{\"anthropicApiKey\":\"sk-old\"}".into()
            )
        );
        let cost_multiplier: String = conn.query_row(
            "SELECT cost_multiplier FROM providers WHERE id = 'legacy-provider'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(cost_multiplier, "1.0");
        let skill_snapshot: String = conn.query_row(
            "SELECT value FROM settings WHERE key = 'skills_ssot_migration_snapshot'",
            [],
            |row| row.get(0),
        )?;
        assert!(
            skill_snapshot.contains("legacy-skill"),
            "rebuilding the skills table must preserve the legacy data migration snapshot"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn dump_sql_batches_rows_into_multi_row_inserts() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        // One INSERT per row is the root cause of slow imports (the restore side parses them one
        // by one, 21s measured for 20k rows). This test pins the batch format: 450 rows must merge
        // into ceil(450/200) = 3 statements, and going back to per-row export turns it red.
        let db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(db.conn);
            for i in 0..450 {
                conn.execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, meta)
                     VALUES (?1, 'claude', 'p', '{}', '{}')",
                    [format!("p{i}")],
                )?;
            }
        }

        let sql = db.export_sql_string()?;
        let insert_count = sql.matches("INSERT INTO \"providers\"").count();
        assert_eq!(
            insert_count, 3,
            "450 rows should merge into 3 multi-row INSERTs (200 rows per batch), got {insert_count}"
        );

        let target = Database::memory()?;
        target.import_sql_string(&sql)?;
        let conn = crate::database::lock_conn!(target.conn);
        let row_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))?;
        assert_eq!(
            row_count, 450,
            "batch boundaries must not drop or duplicate rows"
        );
        for boundary in [0, 199, 200, 399, 400, 449] {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM providers WHERE id = ?1)",
                [format!("p{boundary}")],
                |row| row.get(0),
            )?;
            assert!(
                exists,
                "batch boundary row p{boundary} must be fully restored"
            );
        }
        Ok(())
    }

    #[test]
    fn dump_sql_splits_large_rows_by_statement_bytes() -> Result<(), AppError> {
        let source = Connection::open_in_memory()?;
        source.execute(
            "CREATE TABLE large_rows (id INTEGER PRIMARY KEY, payload TEXT NOT NULL)",
            [],
        )?;

        // Each row fits below the byte cap, while any pair exceeds it.
        let payload = "x".repeat(super::INSERT_BATCH_MAX_BYTES / 2 + 1024);
        for id in 1..=3 {
            source.execute(
                "INSERT INTO large_rows (id, payload) VALUES (?1, ?2)",
                rusqlite::params![id, payload],
            )?;
        }

        let sql = Database::dump_sql(&source, &[])?;
        let inserts = sql
            .lines()
            .filter(|line| line.starts_with("INSERT INTO \"large_rows\""))
            .collect::<Vec<_>>();
        assert_eq!(
            inserts.len(),
            3,
            "oversized fields should split the batch early by SQL byte size"
        );
        assert!(
            inserts
                .iter()
                .all(|statement| statement.len() <= super::INSERT_BATCH_MAX_BYTES),
            "every INSERT that fits on its own should stay within the byte cap"
        );

        let target = Connection::open_in_memory()?;
        target.execute_batch(&sql)?;
        let (count, min_len, max_len): (i64, i64, i64) = target.query_row(
            "SELECT COUNT(*), MIN(length(payload)), MAX(length(payload)) FROM large_rows",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(count, 3);
        assert_eq!(min_len, payload.len() as i64);
        assert_eq!(max_len, payload.len() as i64);
        Ok(())
    }

    #[test]
    fn dump_sql_round_trips_generated_columns_and_quoted_identifiers() -> Result<(), AppError> {
        let source = Connection::open_in_memory()?;
        source.execute_batch(
            r#"
            CREATE TABLE "generated""values" (
                "a" TEXT NOT NULL,
                "computed" TEXT GENERATED ALWAYS AS ("a" || '-generated') STORED,
                "b""tail" TEXT NOT NULL
            );
            INSERT INTO "generated""values" ("a", "b""tail")
            VALUES ('source', 'ordinary-tail');
            "#,
        )?;

        let sql = Database::dump_sql(&source, &[])?;
        assert!(sql.contains("INSERT INTO \"generated\"\"values\" (\"a\", \"b\"\"tail\") VALUES"));

        let target = Connection::open_in_memory()?;
        target.execute_batch(&sql)?;
        let values: (String, String, String) = target.query_row(
            "SELECT \"a\", \"computed\", \"b\"\"tail\" FROM \"generated\"\"values\"",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(
            values,
            (
                "source".to_string(),
                "source-generated".to_string(),
                "ordinary-tail".to_string()
            )
        );
        Ok(())
    }

    #[test]
    fn dump_sql_preserves_text_bytes_and_real_storage_class() -> Result<(), AppError> {
        let source = Connection::open_in_memory()?;
        source.execute_batch(
            "CREATE TABLE scalar_values (
                 label TEXT PRIMARY KEY,
                 value ANY
             ) STRICT;
             INSERT INTO scalar_values VALUES ('nul-text', CAST(X'610062' AS TEXT));
             INSERT INTO scalar_values VALUES ('invalid-text', CAST(X'80FF' AS TEXT));",
        )?;
        for (label, value) in [
            ("real-one", 1.0),
            ("negative-zero", -0.0),
            ("positive-infinity", f64::INFINITY),
            ("negative-infinity", f64::NEG_INFINITY),
        ] {
            source.execute(
                "INSERT INTO scalar_values (label, value) VALUES (?1, ?2)",
                rusqlite::params![label, value],
            )?;
        }

        let sql = Database::dump_sql(&source, &[])?;
        assert!(
            sql.contains("CAST(X'610062' AS TEXT)") && sql.contains("CAST(X'80FF' AS TEXT)"),
            "TEXT containing NUL or invalid UTF-8 must use a hexadecimal expression"
        );

        let target = Connection::open_in_memory()?;
        target.execute_batch(&sql)?;

        for (label, expected_hex) in [("nul-text", "610062"), ("invalid-text", "80FF")] {
            let (storage_class, bytes): (String, String) = target.query_row(
                "SELECT typeof(value), hex(value) FROM scalar_values WHERE label = ?1",
                [label],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(storage_class, "text");
            assert_eq!(bytes, expected_hex);
        }

        let real_value = |label: &str| -> Result<(String, f64), rusqlite::Error> {
            target.query_row(
                "SELECT typeof(value), value FROM scalar_values WHERE label = ?1",
                [label],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        };
        let (storage_class, one) = real_value("real-one")?;
        assert_eq!(storage_class, "real");
        assert_eq!(one, 1.0);

        let (storage_class, negative_zero) = real_value("negative-zero")?;
        assert_eq!(storage_class, "real");
        assert_eq!(negative_zero, 0.0);
        assert!(negative_zero.is_sign_negative());

        let (storage_class, positive_infinity) = real_value("positive-infinity")?;
        assert_eq!(storage_class, "real");
        assert_eq!(positive_infinity, f64::INFINITY);

        let (storage_class, negative_infinity) = real_value("negative-infinity")?;
        assert_eq!(storage_class, "real");
        assert_eq!(negative_infinity, f64::NEG_INFINITY);
        Ok(())
    }

    #[test]
    fn dump_sql_preserves_autoincrement_high_water_marks() -> Result<(), AppError> {
        let source = Connection::open_in_memory()?;
        source.execute_batch(
            "CREATE TABLE autoincrement_rows (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 value TEXT NOT NULL
             );
             INSERT INTO autoincrement_rows (value) VALUES ('one'), ('two'), ('deleted-high');
             DELETE FROM autoincrement_rows WHERE id = 3;",
        )?;

        let sql = Database::dump_sql(&source, &[])?;
        let target = Connection::open_in_memory()?;
        target.execute_batch(&sql)?;

        let sequence: i64 = target.query_row(
            "SELECT seq FROM sqlite_sequence WHERE name = 'autoincrement_rows'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            sequence, 3,
            "the deleted highest id must stay in the sequence high-water mark"
        );
        target.execute(
            "INSERT INTO autoincrement_rows (value) VALUES ('after-restore')",
            [],
        )?;
        assert_eq!(target.last_insert_rowid(), 4);
        Ok(())
    }

    #[test]
    fn sync_style_restore_preserves_local_autoincrement_high_water_marks() -> Result<(), AppError> {
        let source = Connection::open_in_memory()?;
        source.execute_batch(
            "CREATE TABLE autoincrement_rows (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 value TEXT NOT NULL
             );
             INSERT INTO autoincrement_rows (value) VALUES ('one'), ('two'), ('deleted-high');
             DELETE FROM autoincrement_rows WHERE id = 3;",
        )?;

        // A sync dump skips device-local rows and their sequence metadata.
        let staged_sql = Database::dump_sql(&source, &["autoincrement_rows"])?;
        let target = Connection::open_in_memory()?;
        target.execute_batch(&staged_sql)?;
        let staged_sequence_count: i64 = target.query_row(
            "SELECT COUNT(*) FROM sqlite_sequence WHERE name = 'autoincrement_rows'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(staged_sequence_count, 0);

        Database::restore_tables(&source, &target, &["autoincrement_rows"])?;
        let restored_sequence: i64 = target.query_row(
            "SELECT seq FROM sqlite_sequence WHERE name = 'autoincrement_rows'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(restored_sequence, 3);
        target.execute(
            "INSERT INTO autoincrement_rows (value) VALUES ('after-sync')",
            [],
        )?;
        assert_eq!(target.last_insert_rowid(), 4);
        Ok(())
    }

    #[test]
    fn restore_tables_reads_only_insertable_columns() -> Result<(), AppError> {
        let source = Connection::open_in_memory()?;
        let target = Connection::open_in_memory()?;
        for conn in [&source, &target] {
            conn.execute_batch(
                r#"
                CREATE TABLE generated_values (
                    a TEXT NOT NULL,
                    computed TEXT GENERATED ALWAYS AS (a || '-generated') STORED,
                    "b""tail" TEXT NOT NULL
                );
                "#,
            )?;
        }
        source.execute(
            "INSERT INTO generated_values (a, \"b\"\"tail\") VALUES ('new', 'new-tail')",
            [],
        )?;
        target.execute(
            "INSERT INTO generated_values (a, \"b\"\"tail\") VALUES ('old', 'old-tail')",
            [],
        )?;

        Database::restore_tables(&source, &target, &["generated_values"])?;

        let values: (String, String, String) = target.query_row(
            "SELECT a, computed, \"b\"\"tail\" FROM generated_values",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(
            values,
            (
                "new".to_string(),
                "new-generated".to_string(),
                "new-tail".to_string()
            )
        );
        Ok(())
    }

    #[test]
    fn restore_tables_rolls_back_all_tables_on_late_failure() -> Result<(), AppError> {
        let source = Connection::open_in_memory()?;
        source.execute_batch(
            "CREATE TABLE first_table (value TEXT NOT NULL);
             CREATE TABLE second_table (value INTEGER NOT NULL);
             INSERT INTO first_table VALUES ('replacement');
             INSERT INTO second_table VALUES (-1);",
        )?;

        let target = Connection::open_in_memory()?;
        target.execute_batch(
            "CREATE TABLE first_table (value TEXT NOT NULL);
             CREATE TABLE second_table (value INTEGER NOT NULL CHECK (value >= 0));
             INSERT INTO first_table VALUES ('sentinel-first');
             INSERT INTO second_table VALUES (7);",
        )?;

        let result = Database::restore_tables(&source, &target, &["first_table", "second_table"]);
        assert!(
            result.is_err(),
            "a constraint error on the second table must abort the restore"
        );

        let first: String =
            target.query_row("SELECT value FROM first_table", [], |row| row.get(0))?;
        let second: i64 =
            target.query_row("SELECT value FROM second_table", [], |row| row.get(0))?;
        assert_eq!(
            first, "sentinel-first",
            "the first table must roll back with the transaction"
        );
        assert_eq!(
            second, 7,
            "the DELETE on the failing table must roll back too"
        );
        Ok(())
    }

    #[test]
    fn dump_sql_loads_rows_before_creating_triggers() -> Result<(), AppError> {
        let source = Connection::open_in_memory()?;
        source.execute_batch(
            "CREATE TABLE triggered_rows (seq INTEGER PRIMARY KEY);
             INSERT INTO triggered_rows VALUES (1), (2), (3);
             CREATE TRIGGER ignore_second_row
             BEFORE INSERT ON triggered_rows
             WHEN NEW.seq = 2
             BEGIN
                 SELECT RAISE(IGNORE);
             END;",
        )?;

        let sql = Database::dump_sql(&source, &[])?;
        let data_pos = sql.find("INSERT INTO \"triggered_rows\"").unwrap();
        let trigger_pos = sql.find("CREATE TRIGGER ignore_second_row").unwrap();
        assert!(
            data_pos < trigger_pos,
            "triggers must be created after the data has been restored"
        );

        let target = Connection::open_in_memory()?;
        target.execute_batch(&sql)?;
        let rows = target
            .prepare("SELECT seq FROM triggered_rows ORDER BY seq")?
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows, vec![1, 2, 3]);
        let trigger_exists: bool = target.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'trigger' AND name = 'ignore_second_row')",
            [],
            |row| row.get(0),
        )?;
        assert!(
            trigger_exists,
            "the trigger itself must still be restored from the backup"
        );
        Ok(())
    }

    #[test]
    fn dump_sql_preserves_indexes_and_views() -> Result<(), AppError> {
        let source = Connection::open_in_memory()?;
        source.execute_batch(
            "CREATE TABLE indexed_rows (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
             CREATE UNIQUE INDEX indexed_rows_value_idx ON indexed_rows(value);
             CREATE VIEW indexed_rows_view AS
                 SELECT id, value FROM indexed_rows WHERE value LIKE 'kept%';
             CREATE TRIGGER a_insert_indexed_rows_view
             INSTEAD OF INSERT ON indexed_rows_view
             BEGIN
                 INSERT INTO indexed_rows (id, value) VALUES (NEW.id, NEW.value);
             END;
             INSERT INTO indexed_rows VALUES (1, 'kept-value'), (2, 'hidden-value');",
        )?;

        let sql = Database::dump_sql(&source, &[])?;
        let target = Connection::open_in_memory()?;
        target.execute_batch(&sql)?;

        for (object_type, object_name) in [
            ("index", "indexed_rows_value_idx"),
            ("view", "indexed_rows_view"),
            ("trigger", "a_insert_indexed_rows_view"),
        ] {
            let exists: bool = target.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2
                )",
                [object_type, object_name],
                |row| row.get(0),
            )?;
            assert!(
                exists,
                "{object_type} {object_name} must be restored from the SQL dump"
            );
        }

        target.execute(
            "INSERT INTO indexed_rows_view (id, value) VALUES (3, 'kept-via-trigger')",
            [],
        )?;
        let view_rows = target
            .prepare("SELECT id, value FROM indexed_rows_view ORDER BY id")?
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            view_rows,
            vec![
                (1, "kept-value".to_string()),
                (3, "kept-via-trigger".to_string()),
            ]
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn multi_row_dump_round_trips_special_values() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        // Multi-row VALUES has a wider escaping surface than single-row: single quotes, newlines,
        // commas (the column separator), non-ASCII text, emoji, BLOB, NULL — any one handled wrong
        // breaks the whole batch syntactically or corrupts the data.
        let source = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(source.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('special', 'claude', ?1, ?2, '{}')",
                rusqlite::params![
                    "O'Brien,\nsecond line \"quoted\" ünïcode 😀",
                    "{\"key\": \"it's, ok\"}"
                ],
            )?;
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('with-blob', 'claude', 'blob', X'00FF10', '{}')",
                [],
            )?;
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta, category)
                 VALUES ('with-null', 'claude', 'nullcat', '{}', '{}', NULL)",
                [],
            )?;
        }

        let sql = source.export_sql_string()?;
        let target = Database::memory()?;
        target.import_sql_string(&sql)?;

        let conn = crate::database::lock_conn!(target.conn);
        let name: String = conn.query_row(
            "SELECT name FROM providers WHERE id = 'special'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(name, "O'Brien,\nsecond line \"quoted\" ünïcode 😀");
        let cfg: String = conn.query_row(
            "SELECT settings_config FROM providers WHERE id = 'special'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(cfg, "{\"key\": \"it's, ok\"}");

        let blob_type: String = conn.query_row(
            "SELECT typeof(settings_config) FROM providers WHERE id = 'with-blob'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            blob_type, "blob",
            "the BLOB storage class must survive the round trip"
        );
        let blob: Vec<u8> = conn.query_row(
            "SELECT settings_config FROM providers WHERE id = 'with-blob'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(blob, vec![0x00, 0xFF, 0x10]);

        let category: Option<String> = conn.query_row(
            "SELECT category FROM providers WHERE id = 'with-null'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(category, None, "NULL must survive the round trip");
        Ok(())
    }

    #[test]
    #[serial]
    fn full_sql_backup_still_round_trips_session_cursors() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let source = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(source.conn);
            conn.execute_batch(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('cursor-provider', 'claude', 'Cursor Provider', '{}', '{}');
                 INSERT INTO session_log_sync (
                     file_path, last_modified, last_line_offset, last_synced_at
                 ) VALUES ('/local/sessions/manual-backup.jsonl', 11, 22, 33);",
            )?;
        }

        let sql = source.export_sql_string()?;
        let target = Database::memory()?;
        target.import_sql_string(&sql)?;

        let conn = crate::database::lock_conn!(target.conn);
        let cursor: (String, i64, i64, i64) = conn.query_row(
            "SELECT file_path, last_modified, last_line_offset, last_synced_at
             FROM session_log_sync",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        assert_eq!(
            cursor,
            ("/local/sessions/manual-backup.jsonl".into(), 11, 22, 33,)
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn failed_backup_publish_leaves_no_visible_or_temporary_file() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let db = Database::init()?;
        let backup_dir = Database::backups_dir();
        std::fs::create_dir_all(&backup_dir).map_err(|e| AppError::io(&backup_dir, e))?;
        let mut files_before = std::fs::read_dir(&backup_dir)
            .map_err(|e| AppError::io(&backup_dir, e))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name())
            .collect::<Vec<_>>();
        files_before.sort();
        let mut visible_before = Database::list_backups()?
            .into_iter()
            .map(|entry| entry.filename)
            .collect::<Vec<_>>();
        visible_before.sort();

        let error = {
            let backup_file_guard = lock_backup_file_operations()?;
            let conn = crate::database::lock_conn!(db.conn);
            Database::backup_database_file_from_conn_with_hook(
                &backup_file_guard,
                &conn,
                &[],
                |temp_path, target_path| {
                    assert!(
                        temp_path.exists(),
                        "completed backup should exist before publish"
                    );
                    assert_ne!(
                        temp_path.extension().and_then(|ext| ext.to_str()),
                        Some("db"),
                        "staging files must stay invisible to backup discovery"
                    );
                    assert!(!target_path.exists());
                    Err(AppError::Config("simulated publish failure".to_string()))
                },
            )
        }
        .expect_err("publish failure must be returned");
        assert!(error.to_string().contains("simulated publish failure"));
        let mut visible_after = Database::list_backups()?
            .into_iter()
            .map(|entry| entry.filename)
            .collect::<Vec<_>>();
        visible_after.sort();
        assert_eq!(visible_after, visible_before);
        let mut files_after = std::fs::read_dir(&backup_dir)
            .map_err(|e| AppError::io(&backup_dir, e))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name())
            .collect::<Vec<_>>();
        files_after.sort();
        assert_eq!(
            files_after, files_before,
            "failed publish must not leave either a visible backup or a temporary file"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn backup_publish_retries_a_noclobber_name_collision() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let _settings = SettingsGuard::with_backup_retain_count(10);
        let db = Database::init()?;
        let mut claimed_path = None;

        let published_path = {
            let backup_file_guard = lock_backup_file_operations()?;
            let conn = crate::database::lock_conn!(db.conn);
            Database::backup_database_file_from_conn_with_hook(
                &backup_file_guard,
                &conn,
                &[],
                |_, target_path| {
                    claimed_path = Some(target_path.to_path_buf());
                    std::fs::write(target_path, b"claimed by another process")
                        .map_err(|e| AppError::io(target_path, e))?;
                    Ok(())
                },
            )?
            .expect("file-backed database should create a backup")
        };

        let claimed_path = claimed_path.expect("publish hook should receive the first target");
        assert_ne!(published_path, claimed_path);
        assert_eq!(
            std::fs::read(&claimed_path).map_err(|e| AppError::io(&claimed_path, e))?,
            b"claimed by another process"
        );
        let published_conn = Connection::open_with_flags(
            &published_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        Database::validate_sqlite_integrity(&published_conn)?;

        let backup_dir = Database::backups_dir();
        let temporary_files = std::fs::read_dir(&backup_dir)
            .map_err(|e| AppError::io(&backup_dir, e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".cc-switch-backup-")
            })
            .count();
        assert_eq!(
            temporary_files, 0,
            "publish retry must consume the temp file"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn concurrent_backup_renames_never_overwrite_the_shared_target() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let _settings = SettingsGuard::with_backup_retain_count(10);
        let db = Database::init()?;
        let mut source_filenames = Vec::new();
        for provider_id in ["first-source", "second-source"] {
            {
                let conn = crate::database::lock_conn!(db.conn);
                conn.execute("DELETE FROM providers", [])?;
                conn.execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, meta)
                     VALUES (?1, 'claude', ?1, '{}', '{}')",
                    [provider_id],
                )?;
            }
            let source_path = db
                .backup_database_file()?
                .expect("file-backed database should create a backup");
            source_filenames.push(
                source_path
                    .file_name()
                    .expect("backup should have a filename")
                    .to_string_lossy()
                    .into_owned(),
            );
        }

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let handles = source_filenames
            .iter()
            .cloned()
            .map(|source_filename| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    Database::rename_backup(&source_filename, "shared-target")
                        .map_err(|e| e.to_string())
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| AppError::Config("rename thread panicked".to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);

        let backup_dir = Database::backups_dir();
        let target_path = backup_dir.join("shared-target.db");
        let remaining_source = source_filenames
            .iter()
            .map(|filename| backup_dir.join(filename))
            .find(|path| path.exists())
            .expect("the losing source must remain after the target collision");
        let mut provider_ids = [&target_path, &remaining_source]
            .into_iter()
            .map(|path| -> Result<String, AppError> {
                let conn =
                    Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
                conn.query_row("SELECT id FROM providers", [], |row| row.get(0))
                    .map_err(AppError::from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        provider_ids.sort();
        assert_eq!(provider_ids, vec!["first-source", "second-source"]);
        Ok(())
    }

    #[test]
    #[serial]
    fn restore_with_retain_one_keeps_source_and_exact_safety_snapshot() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let _settings = SettingsGuard::with_backup_retain_count(1);
        let db = Database::init()?;

        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute("DELETE FROM providers", [])?;
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('restore-source', 'claude', 'Restore Source', '{}', '{}')",
                [],
            )?;
        }
        let source_path = db
            .backup_database_file()?
            .expect("file-backed database should create a backup");
        let source_filename = source_path
            .file_name()
            .expect("backup should have a filename")
            .to_string_lossy()
            .into_owned();
        let backup_dir = Database::backups_dir();
        let stale_path = backup_dir.join("stale-unprotected.db");
        std::fs::write(&stale_path, b"stale").map_err(|e| AppError::io(&stale_path, e))?;

        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute("DELETE FROM providers", [])?;
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('live-before-restore', 'claude', 'Live Before Restore', '{}', '{}')",
                [],
            )?;
        }

        let safety_id = db.restore_from_backup(&source_filename)?;
        let safety_path = backup_dir.join(format!("{safety_id}.db"));
        assert!(
            source_path.exists(),
            "selected restore source must be retained"
        );
        assert!(
            safety_path.exists(),
            "pre-restore safety backup must be retained"
        );
        assert!(
            !stale_path.exists(),
            "retention should still remove an unprotected stale backup"
        );

        let backup_count = std::fs::read_dir(&backup_dir)
            .map_err(|e| AppError::io(&backup_dir, e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "db"))
            .count();
        assert_eq!(
            backup_count, 2,
            "retain=1 may be exceeded temporarily to protect both recovery endpoints"
        );

        let live_provider: String = {
            let conn = crate::database::lock_conn!(db.conn);
            conn.query_row("SELECT id FROM providers", [], |row| row.get(0))?
        };
        assert_eq!(live_provider, "restore-source");

        let safety_conn =
            Connection::open_with_flags(&safety_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let safety_provider: String =
            safety_conn.query_row("SELECT id FROM providers", [], |row| row.get(0))?;
        assert_eq!(
            safety_provider, "live-before-restore",
            "safety backup must exactly represent the live state being replaced"
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    #[serial]
    fn restore_protects_case_variant_source_path_from_retention() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let _settings = SettingsGuard::with_backup_retain_count(1);
        let db = Database::init()?;
        let source_path = db
            .backup_database_file()?
            .expect("file-backed database should create a backup");
        let source_filename = source_path
            .file_name()
            .expect("backup should have a filename")
            .to_string_lossy()
            .into_owned();
        let case_variant = format!(
            "{}.db",
            source_filename
                .strip_suffix(".db")
                .expect("generated backup should use a .db suffix")
                .to_ascii_uppercase()
        );

        db.restore_from_backup(&case_variant)?;
        assert!(
            source_path.exists(),
            "retention must recognize a case-variant path as the selected source"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn restore_blocks_backup_deletion_until_live_replacement_finishes() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let db = Database::init()?;
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute("DELETE FROM providers", [])?;
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('restore-source', 'claude', 'Restore Source', '{}', '{}')",
                [],
            )?;
        }
        let source_path = db
            .backup_database_file()?
            .expect("file-backed database should create a backup");
        let source_filename = source_path
            .file_name()
            .expect("backup should have a filename")
            .to_string_lossy()
            .into_owned();
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute("DELETE FROM providers", [])?;
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('live-before-restore', 'claude', 'Live Before Restore', '{}', '{}')",
                [],
            )?;
        }

        let (attempt_tx, attempt_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let mut delete_handle = None;
        let mut observed_safety_filename = None;
        let safety_id = db.restore_from_backup_with_hook(&source_filename, |safety_path| {
            let safety_path = safety_path.ok_or_else(|| {
                AppError::Config("restore should create a safety backup".to_string())
            })?;
            let safety_filename = safety_path
                .file_name()
                .ok_or_else(|| AppError::Config("safety backup has no filename".to_string()))?
                .to_string_lossy()
                .into_owned();
            observed_safety_filename = Some(safety_filename.clone());
            delete_handle = Some(std::thread::spawn(move || {
                let _ = attempt_tx.send(());
                let result = Database::delete_backup(&safety_filename).map_err(|e| e.to_string());
                let _ = result_tx.send(result);
            }));

            attempt_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .map_err(|e| AppError::Config(format!("delete thread did not start: {e}")))?;
            match result_rx.recv_timeout(std::time::Duration::from_millis(150)) {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(AppError::Config(
                    "delete thread disconnected before restore completed".to_string(),
                )),
                Ok(result) => Err(AppError::Config(format!(
                    "backup deletion completed before live replacement: {result:?}"
                ))),
            }
        })?;

        let delete_result = result_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .map_err(|e| AppError::Config(format!("delete did not resume after restore: {e}")))?;
        delete_result.map_err(AppError::Config)?;
        delete_handle
            .expect("delete thread should be created")
            .join()
            .map_err(|_| AppError::Config("delete thread panicked".to_string()))?;

        let expected_safety_filename = format!("{safety_id}.db");
        assert_eq!(
            observed_safety_filename.as_deref(),
            Some(expected_safety_filename.as_str())
        );
        let live_provider: String = {
            let conn = crate::database::lock_conn!(db.conn);
            conn.query_row("SELECT id FROM providers", [], |row| row.get(0))?
        };
        assert_eq!(live_provider, "restore-source");
        Ok(())
    }

    #[test]
    #[serial]
    fn restore_rejects_corrupt_db_before_touching_live_database() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let db = Database::init()?;
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute("DELETE FROM providers", [])?;
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('live-provider', 'claude', 'Live Provider', '{}', '{}')",
                [],
            )?;
        }

        let backup_dir = Database::backups_dir();
        std::fs::create_dir_all(&backup_dir).map_err(|e| AppError::io(&backup_dir, e))?;
        let corrupt_path = backup_dir.join("corrupt.db");
        std::fs::write(&corrupt_path, b"not a sqlite database")
            .map_err(|e| AppError::io(&corrupt_path, e))?;
        let mut backups_before = Database::list_backups()?
            .into_iter()
            .map(|entry| entry.filename)
            .collect::<Vec<_>>();
        backups_before.sort();

        db.restore_from_backup("corrupt.db")
            .expect_err("corrupt backup must be rejected");

        let live_provider: String = {
            let conn = crate::database::lock_conn!(db.conn);
            conn.query_row("SELECT id FROM providers", [], |row| row.get(0))?
        };
        assert_eq!(live_provider, "live-provider");
        let mut backups_after = Database::list_backups()?
            .into_iter()
            .map(|entry| entry.filename)
            .collect::<Vec<_>>();
        backups_after.sort();
        assert_eq!(
            backups_after, backups_before,
            "failed staging must not create a safety backup"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn restore_rejects_future_schema_before_touching_live_database() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();
        let db = Database::init()?;

        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute("DELETE FROM providers", [])?;
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('future-source', 'claude', 'Future Source', '{}', '{}')",
                [],
            )?;
        }
        let source_path = db
            .backup_database_file()?
            .expect("file-backed database should create a backup");
        let source_filename = source_path
            .file_name()
            .expect("backup should have a filename")
            .to_string_lossy()
            .into_owned();
        {
            let source_conn = Connection::open(&source_path)?;
            source_conn.execute_batch(&format!(
                "PRAGMA user_version = {};",
                crate::database::SCHEMA_VERSION + 1
            ))?;
        }

        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute("DELETE FROM providers", [])?;
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('live-provider', 'claude', 'Live Provider', '{}', '{}')",
                [],
            )?;
        }

        let backup_dir = Database::backups_dir();
        let backup_count_before = std::fs::read_dir(&backup_dir)
            .map_err(|e| AppError::io(&backup_dir, e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "db"))
            .count();

        let error = db
            .restore_from_backup(&source_filename)
            .expect_err("future-schema backup must be rejected");
        assert!(
            error.to_string().contains("newer") || error.to_string().contains("version"),
            "unexpected error: {error}"
        );

        let live_provider: String = {
            let conn = crate::database::lock_conn!(db.conn);
            conn.query_row("SELECT id FROM providers", [], |row| row.get(0))?
        };
        assert_eq!(
            live_provider, "live-provider",
            "failed staging validation must not replace the live database"
        );

        let backup_count_after = std::fs::read_dir(&backup_dir)
            .map_err(|e| AppError::io(&backup_dir, e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "db"))
            .count();
        assert_eq!(
            backup_count_after, backup_count_before,
            "staging failure should occur before creating a redundant safety backup"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn periodic_maintenance_runs_even_when_auto_backup_disabled() -> Result<(), AppError> {
        let _test_home = TestHomeGuard::new();

        let settings = AppSettings {
            backup_interval_hours: Some(0),
            ..AppSettings::default()
        };
        update_settings(settings).expect("disable auto backup");

        let db = Database::memory()?;
        let now = chrono::Utc::now().timestamp();
        let old_ts = now - 40 * 86400;
        let old_stream_ts = now - 8 * 86400;

        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES ('old-req', 'p1', 'claude', 'claude-3', 100, 50, '0.01', 100, 200, ?1)",
                [old_ts],
            )?;
            conn.execute(
                "INSERT INTO stream_check_logs (
                    provider_id, provider_name, app_type, status, success, message,
                    response_time_ms, http_status, model_used, retry_count, tested_at
                ) VALUES ('p1', 'Provider 1', 'claude', 'operational', 1, 'ok', 42, 200, 'claude-3', 0, ?1)",
                [old_stream_ts],
            )?;
        }

        db.periodic_backup_if_needed()?;

        let (remaining_request_logs, stream_logs, rollups): (i64, i64, i64) = {
            let conn = crate::database::lock_conn!(db.conn);
            let remaining_request_logs =
                conn.query_row("SELECT COUNT(*) FROM proxy_request_logs", [], |row| {
                    row.get(0)
                })?;
            let stream_logs =
                conn.query_row("SELECT COUNT(*) FROM stream_check_logs", [], |row| {
                    row.get(0)
                })?;
            let rollups =
                conn.query_row("SELECT COUNT(*) FROM usage_daily_rollups", [], |row| {
                    row.get(0)
                })?;
            (remaining_request_logs, stream_logs, rollups)
        };

        assert_eq!(
            remaining_request_logs, 0,
            "old request logs should still be pruned when auto backup is disabled"
        );
        assert_eq!(
            stream_logs, 0,
            "old stream check logs should still be pruned when auto backup is disabled"
        );
        assert_eq!(rollups, 1, "old request logs should be rolled up");

        Ok(())
    }

    /// Break down where import_sql_string spends its time, phase by phase.
    ///
    /// Run manually: `cargo test --lib perf_import_phases -- --ignored --nocapture`
    #[test]
    #[ignore = "perf diagnostic, run explicitly"]
    fn perf_import_phases() -> Result<(), AppError> {
        use rusqlite::Connection;
        use std::time::Instant;
        use tempfile::NamedTempFile;

        const LOG_ROWS: usize = 20_000;

        let source = Database::memory()?;
        {
            let mut conn = crate::database::lock_conn!(source.conn);
            let tx = conn.transaction()?;
            for i in 0..50 {
                tx.execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, meta)
                     VALUES (?1, 'claude', ?2, '{}', '{}')",
                    rusqlite::params![format!("p{i}"), format!("Provider {i}")],
                )?;
            }
            for i in 0..LOG_ROWS {
                tx.execute(
                    "INSERT INTO proxy_request_logs (
                        request_id, provider_id, app_type, model,
                        input_tokens, output_tokens, total_cost_usd,
                        latency_ms, status_code, created_at
                    ) VALUES (?1, 'p1', 'claude', 'claude-3', 100, 50, '0.01', 120, 200, 1000)",
                    [format!("req-{i}")],
                )?;
            }
            tx.commit()?;
        }
        let sql = source.export_sql_string()?;
        println!("payload: {} bytes, {LOG_ROWS} log rows", sql.len());

        let temp_file = NamedTempFile::new().expect("temp file");
        let temp_conn = Connection::open(temp_file.path()).expect("open temp conn");

        let t = Instant::now();
        temp_conn
            .execute_batch(&sql)
            .expect("execute_batch should succeed");
        println!("phase execute_batch: {:?}", t.elapsed());

        let t = Instant::now();
        Database::create_tables_on_conn(&temp_conn)?;
        Database::apply_schema_migrations_on_conn(&temp_conn)?;
        println!("phase schema+migrations: {:?}", t.elapsed());

        let t = Instant::now();
        let target = Database::memory()?;
        {
            let mut main_conn = crate::database::lock_conn!(target.conn);
            let backup =
                rusqlite::backup::Backup::new(&temp_conn, &mut main_conn).expect("backup init");
            backup.step(-1).expect("backup step");
        }
        println!("phase backup-to-main: {:?}", t.elapsed());

        // Control group: the same statements with journal / synchronous disabled on the temp db.
        let temp_file2 = NamedTempFile::new().expect("temp file 2");
        let temp_conn2 = Connection::open(temp_file2.path()).expect("open temp conn 2");
        temp_conn2
            .execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF;")
            .expect("pragmas");
        let t = Instant::now();
        temp_conn2
            .execute_batch(&sql)
            .expect("execute_batch should succeed");
        println!(
            "phase execute_batch (journal=MEMORY, sync=OFF): {:?}",
            t.elapsed()
        );

        // Control group B: the same script on an in-memory database, separating pure CPU/parsing
        // from file I/O.
        let mem_conn = Connection::open_in_memory().expect("open mem conn");
        let t = Instant::now();
        mem_conn
            .execute_batch(&sql)
            .expect("execute_batch mem should succeed");
        println!("phase execute_batch (in-memory): {:?}", t.elapsed());

        // Control group C: the same data as multi-row VALUES (one INSERT per 200 rows), to measure
        // the share of per-statement parsing overhead.
        let mut batched = String::from("PRAGMA foreign_keys=OFF;\nBEGIN TRANSACTION;\n");
        batched.push_str(
            "CREATE TABLE bench_logs (
                request_id TEXT, provider_id TEXT, app_type TEXT, model TEXT,
                input_tokens INTEGER, output_tokens INTEGER, total_cost_usd TEXT,
                latency_ms INTEGER, status_code INTEGER, created_at INTEGER
            );\n",
        );
        const BATCH: usize = 200;
        for chunk_start in (0..LOG_ROWS).step_by(BATCH) {
            batched.push_str("INSERT INTO bench_logs VALUES ");
            for i in chunk_start..(chunk_start + BATCH).min(LOG_ROWS) {
                if i > chunk_start {
                    batched.push(',');
                }
                batched.push_str(&format!(
                    "('req-{i}','p1','claude','claude-3',100,50,'0.01',120,200,1000)"
                ));
            }
            batched.push_str(";\n");
        }
        batched.push_str("COMMIT;\n");
        let mem_conn2 = Connection::open_in_memory().expect("open mem conn 2");
        let t = Instant::now();
        mem_conn2
            .execute_batch(&batched)
            .expect("batched should succeed");
        println!(
            "phase execute_batch (in-memory, multi-row VALUES x{BATCH}): {:?}",
            t.elapsed()
        );

        Ok(())
    }
}
