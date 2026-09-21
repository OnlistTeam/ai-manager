//! Compatibility layer for backup and restore (ADR-0003 / spec §92 / §18).
//!
//! The only place that knows the upstream `Database` backup API, the post-restore
//! write-back, and the two cross-subsystem locks. The signatures the layers above see
//! contain only `BackupList` / `RestoreOutcome` from `crate::domain`.
//!
//! The post-restore write-back **no longer** reuses the upstream `run_post_import_sync`:
//! that would project the whole restored, stale snapshot back onto the tools (a full MCP /
//! skill sync deletes live entries "the snapshot says are disabled", and a full prompt sync
//! overwrites CLAUDE.md / AGENTS.md without a backup). The product's own projection lives
//! in `post_restore.rs`: it writes only "which service is currently in use" and leaves the
//! extensions on disk untouched.

use std::sync::Arc;

use crate::compat::ccswitch::settings::SettingsStore;
use crate::database::backup::BackupEntry;
use crate::database::Database;
use crate::domain::{AppError, BackupFile, BackupList, ErrorCode, RestoreOutcome};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::services::skill::skill_state_write_guard;
use crate::services::sync_protocol::sync_mutex;
use crate::store::AppState;

mod post_restore;
mod transfer;

use post_restore::project_restored_database;

fn detail<E: std::fmt::Display>(error: E) -> String {
    truncate_tail(&redact_secrets(&error.to_string()), 20, 2000)
}

fn list_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.backup.listFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn create_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.backup.createFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn restore_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.backup.restoreFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn delete_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.backup.deleteFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn rename_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.backup.renameFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

/// Upstream expresses "there is no database file to back up" as `Ok(None)`. For the user,
/// "I pressed backup and nothing happened" has to be said out loud, so it is translated
/// into a real error here.
fn no_database() -> AppError {
    AppError::new(ErrorCode::Internal, "error.backup.noDatabase")
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn not_found(name: &str) -> AppError {
    AppError::new(ErrorCode::BackupNotFound, "error.backup.notFound")
        .with_technical(name.to_string())
}

fn to_file(entry: BackupEntry) -> BackupFile {
    BackupFile {
        name: entry.filename,
        created_at: entry.created_at,
        size_bytes: entry.size_bytes,
    }
}

/// Our own precondition: the backup being acted on must **really be in the list right now**.
///
/// This blocks two things at once: path traversal (`../x.db` never appears in the list) and
/// "the one the retention policy just deleted" — for the latter, upstream only returns a
/// bare localized `InvalidInput`, and we refuse to guess which failure it was from the
/// error string (the same approach as `find_raw` in Phase 4).
fn require_known(files: &[BackupFile], name: &str) -> Result<(), AppError> {
    if files.iter().any(|file| file.name == name) {
        Ok(())
    } else {
        Err(not_found(name))
    }
}

/// Normalize the user-facing label to the exact filename shape expected by
/// upstream `Database::rename_backup`. Keeping the validation here means the
/// product never has to branch on upstream's English `InvalidInput` strings.
fn normalize_rename_target(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(
            ErrorCode::ConfigWriteFailed,
            "error.backup.nameRequired",
        ));
    }

    let name_part = trimmed.strip_suffix(".db").unwrap_or(trimmed);
    if name_part.is_empty() {
        return Err(AppError::new(
            ErrorCode::ConfigWriteFailed,
            "error.backup.nameRequired",
        ));
    }
    if name_part.len() > 100 {
        return Err(AppError::new(
            ErrorCode::ConfigWriteFailed,
            "error.backup.nameTooLong",
        ));
    }
    if name_part.contains("..")
        || name_part.contains('/')
        || name_part.contains('\\')
        || name_part.contains('\0')
    {
        return Err(AppError::new(
            ErrorCode::ConfigWriteFailed,
            "error.backup.nameInvalid",
        ));
    }

    Ok(format!("{name_part}.db"))
}

fn require_available_target(
    files: &[BackupFile],
    source: &str,
    target: &str,
) -> Result<(), AppError> {
    if files
        .iter()
        .any(|file| file.name == target && file.name != source)
    {
        Err(AppError::new(
            ErrorCode::OperationConflict,
            "error.backup.nameExists",
        ))
    } else {
        Ok(())
    }
}

/// Backup list. Needs no database connection: the upstream directory scan is a static method.
pub fn list() -> Result<BackupList, AppError> {
    let files = Database::list_backups()
        .map_err(list_failed)?
        .into_iter()
        .map(to_file)
        .collect();
    Ok(BackupList { files })
}

fn create_with(db: &Arc<Database>) -> Result<BackupList, AppError> {
    match db.backup_database_file().map_err(create_failed)? {
        Some(_) => list(),
        None => Err(no_database()),
    }
}

/// Handle to the upstream backup capability. The fields are private, so the layers above can never reach `AppState`.
pub struct BackupStore {
    state: AppState,
}

impl BackupStore {
    pub fn open(app_handle: &tauri::AppHandle) -> Result<Self, AppError> {
        let state = tauri::Manager::try_state::<AppState>(app_handle).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.backup.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        Ok(Self {
            state: state.inner().clone(),
        })
    }

    pub fn create(&self) -> Result<BackupList, AppError> {
        create_with(&self.state.db)
    }

    pub fn delete(&self, name: &str) -> Result<BackupList, AppError> {
        require_known(&list()?.files, name)?;
        Database::delete_backup(name).map_err(delete_failed)?;
        list()
    }

    pub fn rename(&self, source: &str, name: &str) -> Result<BackupList, AppError> {
        let current = list()?;
        require_known(&current.files, source)?;
        let target = normalize_rename_target(name)?;
        if target == source {
            return Ok(current);
        }
        require_available_target(&current.files, source, &target)?;
        Database::rename_backup(source, &target).map_err(rename_failed)?;
        list()
    }

    /// Restore. `async` is not about concurrency: a tokio async lock must be taken first
    /// (mutual exclusion with the upstream WebDAV / S3 sync) before the whole blocking
    /// section is handed to the blocking thread pool — the same shape as the upstream
    /// `restore_db_backup` (`commands/import_export.rs:169`).
    pub async fn restore(self, name: String) -> Result<RestoreOutcome, AppError> {
        let _sync_guard = sync_mutex().lock().await;
        tauri::async_runtime::spawn_blocking(move || self.restore_blocking(&name))
            .await
            .map_err(|error| {
                AppError::new(ErrorCode::Internal, "error.command.joinFailed")
                    .with_technical(error.to_string())
                    .with_remediation("error.remediation.retryOrViewDetails")
            })?
    }

    fn restore_blocking(&self, name: &str) -> Result<RestoreOutcome, AppError> {
        require_known(&list()?.files, name)?;

        // Decision 8(c): the product preferences are "this person's habits on this
        // machine", not the business data being backed up. Without saving them first and
        // restoring them afterwards, restoring an older backup would also reset markers such
        // as "the startup import question was already answered".
        let settings = SettingsStore::with_db(self.state.db.clone());
        let preserved = settings.load()?;

        {
            // The upstream restore command takes this lock too: while restoring, the state
            // of the skills SSOT directory must not be changed by another path
            // (`commands/import_export.rs:178`).
            let _skill_state_guard = skill_state_write_guard();
            self.state
                .db
                .restore_from_backup(name)
                .map_err(restore_failed)?;
        }

        // At this point the database **has** been swapped (upstream always produces a
        // safety backup of the current state before replacing it). Failing to write the tool
        // configs back does not change that, so it is a boolean rather than an Err:
        // reporting a failure would tempt the user into restoring a second time
        // (decision 8(b)).
        //
        // This Err concatenates the raw upstream errors of each step and may contain file
        // paths. Its **only** outlet is the log line below: redacted and truncated into
        // `log::warn!`, never turned into a product AppError, never put into
        // technical_message, never into a toast. All the UI receives is `tools_out_of_sync`.
        let tools_out_of_sync = match project_restored_database(&self.state) {
            Ok(()) => false,
            Err(error) => {
                log::warn!(
                    "[Restore] the database was restored but projecting it to the tools failed: {}",
                    detail(error)
                );
                true
            }
        };

        // Writing the preferences back is the same: the database has already been swapped,
        // so a failure here must not report the whole restore as failed. The realistic case
        // is a full disk — which this very flow provoked (a complete temporary database and
        // a complete safety backup were just written). Reporting a failure would make the
        // user think nothing happened and press restore again, while the database has in
        // fact already been replaced.
        //
        // The cost is recorded honestly in the technical debt section of ARCHITECTURE: when
        // this step fails, the confirmation dialog's promise that "your preferences will be
        // kept" does not hold and the preferences become whatever the restored backup held.
        if let Err(error) = settings.save(preserved) {
            log::warn!(
                "[Restore] the database was restored but the product preferences could not be written back: {}",
                detail(error)
            );
        }
        Ok(RestoreOutcome {
            backups: list()?,
            tools_out_of_sync,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        create_with, normalize_rename_target, require_available_target, require_known,
        restore_failed, to_file,
    };
    use crate::database::backup::BackupEntry;
    use crate::database::Database;
    use crate::domain::{BackupFile, ErrorCode};
    use std::sync::Arc;

    struct TestHome(Option<std::ffi::OsString>);

    impl TestHome {
        fn set(path: &std::path::Path) -> Self {
            let previous = std::env::var_os("AI_MANAGER_TEST_HOME");
            std::env::set_var("AI_MANAGER_TEST_HOME", path);
            Self(previous)
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
                None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
            }
        }
    }

    fn entry() -> BackupEntry {
        BackupEntry {
            filename: "db_backup_20260819_101500.db".to_string(),
            size_bytes: 2_097_152,
            created_at: "2026-08-19T10:15:00+08:00".to_string(),
        }
    }

    #[test]
    fn the_projection_keeps_only_what_the_screen_needs() {
        let file = to_file(entry());
        assert_eq!(file.name, "db_backup_20260819_101500.db");
        assert_eq!(file.created_at, "2026-08-19T10:15:00+08:00");
        assert_eq!(file.size_bytes, 2_097_152);
    }

    #[test]
    #[serial_test::serial]
    fn the_backup_list_never_carries_the_folder_path_across_ipc() {
        // Paths do not cross IPC: the backup folder is an absolute path containing the user
        // name, and the UI only needs to know "which backups exist", not where they sit on
        // disk.
        let temp = tempfile::tempdir().expect("temp home");
        let _home = TestHome::set(temp.path());
        let list = super::list().expect("list backups");
        let json = serde_json::to_string(&list).expect("serialize list");
        assert!(
            !json.contains("directory"),
            "the wire format leaked the backup folder: {json}"
        );
        assert!(!json.contains(temp.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn only_a_file_that_is_really_in_the_list_may_be_touched() {
        // This is our own precondition, not upstream's. It blocks two things at once: path
        // traversal (`../cc-switch.db` never appears in the list) and "the one the retention
        // policy just deleted" — for the latter, upstream only returns a bare localized
        // InvalidInput.
        let files = vec![to_file(entry())];
        assert!(require_known(&files, "db_backup_20260819_101500.db").is_ok());
        for bad in [
            "../cc-switch.db",
            "db_backup_19700101_000000.db",
            "",
            "app.db",
        ] {
            let error = require_known(&files, bad).expect_err("unknown backup");
            assert_eq!(error.code, ErrorCode::BackupNotFound);
            assert_eq!(error.message_key, "error.backup.notFound");
        }
    }

    #[test]
    fn a_database_with_no_file_behind_it_cannot_be_backed_up() {
        // `Connection::path()` of an in-memory database is an empty string, which is why
        // upstream returns `Ok(None)`. The fact that the user pressed "back up now" and
        // nothing happened must be said out loud; it must not silently succeed.
        let db = Arc::new(Database::memory().expect("in-memory database"));
        let error = create_with(&db).expect_err("nothing to back up");
        assert_eq!(error.message_key, "error.backup.noDatabase");
    }

    #[test]
    fn rename_validation_matches_the_upstream_filename_contract() {
        assert_eq!(
            normalize_rename_target(" before-upgrade ").expect("valid label"),
            "before-upgrade.db"
        );
        assert_eq!(
            normalize_rename_target("before-upgrade.db").expect("valid filename"),
            "before-upgrade.db"
        );
        for (name, key) in [
            ("", "error.backup.nameRequired"),
            ("../outside", "error.backup.nameInvalid"),
            ("folder/name", "error.backup.nameInvalid"),
            ("folder\\name", "error.backup.nameInvalid"),
        ] {
            let error = normalize_rename_target(name).expect_err("invalid label");
            assert_eq!(error.message_key, key);
        }
        let too_long = "x".repeat(101);
        assert_eq!(
            normalize_rename_target(&too_long)
                .expect_err("long label")
                .message_key,
            "error.backup.nameTooLong"
        );
    }

    #[test]
    fn rename_rejects_an_existing_target_but_allows_a_no_op() {
        let source = to_file(entry());
        let other = BackupFile {
            name: "before-upgrade.db".to_string(),
            ..source.clone()
        };
        let files = vec![source.clone(), other];
        let error = require_available_target(&files, &source.name, "before-upgrade.db")
            .expect_err("duplicate target");
        assert_eq!(error.code, ErrorCode::OperationConflict);
        assert_eq!(error.message_key, "error.backup.nameExists");
        assert!(require_available_target(&files, &source.name, &source.name).is_ok());
    }

    #[test]
    fn upstream_wording_and_credentials_never_leave_the_technical_field() {
        // Upstream errors are bare localized strings that may also carry paths and
        // credentials; they may only go into technical_message (§42) and must be redacted
        // first (§19).
        let error = restore_failed("restore failed /Users/a/x sk-ant-123456789012345678");
        assert_eq!(error.message_key, "error.backup.restoreFailed");
        assert_eq!(error.code, ErrorCode::ConfigWriteFailed);
        let technical = error.technical_message.expect("technical detail");
        assert!(!technical.contains("sk-ant-123456789012345678"));
        assert!(technical.contains("***"));
    }
}
