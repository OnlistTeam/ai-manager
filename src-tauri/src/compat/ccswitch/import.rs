//! CC Switch import compatibility facade (spec §17 / ADR-0002).
//!
//! This is the only layer that combines the product repository with the upstream database
//! snapshot/import machinery and the two cross-subsystem restore locks.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::database::{Database, SCHEMA_VERSION};
use crate::domain::{AppError, ErrorCode, ImportOutcome, ImportPreview};
use crate::infrastructure::paths::cc_switch_db_path;
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::repositories::ccswitch_import::{
    merge_import, preview_import, snapshot_source_database,
};
use crate::services::skill::skill_state_write_guard;
use crate::services::sync_protocol::sync_mutex;
use crate::store::AppState;

fn detail(error: impl std::fmt::Display) -> String {
    truncate_tail(&redact_secrets(&error.to_string()), 20, 2000)
}

fn invalid_source(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::ConfigParseFailed, "error.import.invalidSource")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn merge_failed(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.import.mergeFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn source_not_found() -> AppError {
    AppError::new(ErrorCode::BackupNotFound, "error.import.notFound")
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn prepare_source(path: &std::path::Path) -> Result<Option<Connection>, AppError> {
    let Some(source) = snapshot_source_database(path, SCHEMA_VERSION)? else {
        return Ok(None);
    };
    // Both calls mutate only the memory snapshot. The disk source has already been dropped.
    Database::create_tables_on_conn(&source).map_err(invalid_source)?;
    Database::apply_schema_migrations_on_conn(&source).map_err(invalid_source)?;
    Ok(Some(source))
}

/// Product import handle. Its fields stay private so application/commands cannot reach the
/// upstream `AppState`, database connection, or source path.
pub struct ImportStore {
    state: AppState,
    source_path: PathBuf,
}

impl ImportStore {
    pub fn open(app_handle: &tauri::AppHandle) -> Result<Self, AppError> {
        let state = tauri::Manager::try_state::<AppState>(app_handle).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.import.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        Ok(Self {
            state: state.inner().clone(),
            source_path: cc_switch_db_path(),
        })
    }

    pub fn preview(&self) -> Result<ImportPreview, AppError> {
        let Some(source) = prepare_source(&self.source_path)? else {
            return Ok(ImportPreview::default());
        };
        Ok(ImportPreview {
            available: true,
            summary: preview_import(&source)?,
        })
    }

    /// Import is serialized with database restore/sync, then moved off the async runtime.
    pub async fn run(self) -> Result<ImportOutcome, AppError> {
        let _sync_guard = sync_mutex().lock().await;
        tauri::async_runtime::spawn_blocking(move || self.run_blocking())
            .await
            .map_err(|error| {
                AppError::new(ErrorCode::Internal, "error.command.joinFailed")
                    .with_technical(detail(error))
                    .with_remediation("error.remediation.retryOrViewDetails")
            })?
    }

    fn run_blocking(&self) -> Result<ImportOutcome, AppError> {
        let source = prepare_source(&self.source_path)?.ok_or_else(source_not_found)?;

        // Exclude Skill mutations from the target snapshot through the final database swap.
        // Import intentionally changes database state only; it never writes Skill directories.
        let _skill_state_guard = skill_state_write_guard();
        let mut target = self.state.db.snapshot_to_memory().map_err(merge_failed)?;
        let imported = merge_import(&source, &mut target)?;

        let staged = Database {
            conn: Mutex::new(target),
        };
        let sql = staged.export_sql_string().map_err(merge_failed)?;
        self.state
            .db
            .import_sql_string(&sql)
            .map_err(merge_failed)?;

        Ok(ImportOutcome { imported })
    }
}

#[cfg(test)]
mod tests {
    use super::ImportStore;
    use crate::database::{Database, SCHEMA_VERSION};
    use crate::domain::ImportSummary;
    use crate::store::AppState;
    use rusqlite::Connection;
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn source_at(path: &std::path::Path) {
        let source = Connection::open(path).expect("source database");
        Database::create_tables_on_conn(&source).expect("source schema");
        Database::set_user_version(&source, SCHEMA_VERSION).expect("source version");
        source
            .execute(
                "INSERT INTO providers
                 (id,app_type,name,settings_config,meta,is_current,in_failover_queue)
                 VALUES ('source','claude','Imported service','{}','{}',1,0)",
                [],
            )
            .expect("source provider");
        source
            .execute(
                "INSERT INTO settings (key,value) VALUES ('aimgr.importPromptSeen','false')",
                [],
            )
            .expect("source product-looking setting");
    }

    fn current_target() -> Arc<Database> {
        let target = Arc::new(Database::memory().expect("target database"));
        {
            let connection = target.conn.lock().expect("target connection");
            Database::set_user_version(&connection, SCHEMA_VERSION).expect("target version");
        }
        target
            .set_setting("aimgr.importPromptSeen", "true")
            .expect("target product setting");
        target
    }

    #[test]
    fn preview_is_optional_and_never_changes_the_source() {
        let directory = tempdir().expect("temporary directory");
        let missing = ImportStore {
            state: AppState::new(current_target()),
            source_path: directory.path().join("missing.db"),
        };
        assert!(!missing.preview().expect("missing preview").available);

        let path = directory.path().join("cc-switch.db");
        source_at(&path);
        let before = fs::read(&path).expect("source before");
        let store = ImportStore {
            state: AppState::new(current_target()),
            source_path: path.clone(),
        };
        let preview = store.preview().expect("preview");
        assert!(preview.available);
        assert_eq!(preview.summary.services, 1);
        assert_eq!(fs::read(path).expect("source after"), before);
    }

    #[test]
    fn staged_import_preserves_product_settings_and_source_bytes() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("cc-switch.db");
        source_at(&path);
        let before = fs::read(&path).expect("source before");
        let target = current_target();
        let store = ImportStore {
            state: AppState::new(target.clone()),
            source_path: path.clone(),
        };

        let outcome = store.run_blocking().expect("staged import");
        assert_eq!(
            outcome.imported,
            ImportSummary {
                services: 1,
                mcp_servers: 0,
                skills: 0,
            }
        );
        assert_eq!(
            target
                .get_setting("aimgr.importPromptSeen")
                .expect("read product setting")
                .as_deref(),
            Some("true"),
            "source settings must never overwrite product preferences"
        );
        assert_eq!(
            target
                .get_all_providers("claude")
                .expect("imported providers")
                .get("source")
                .map(|provider| provider.name.as_str()),
            Some("Imported service")
        );
        assert_eq!(fs::read(path).expect("source after"), before);
    }

    #[test]
    fn run_reports_when_the_source_disappears_after_preview() {
        let directory = tempdir().expect("temporary directory");
        let store = ImportStore {
            state: AppState::new(current_target()),
            source_path: directory.path().join("missing.db"),
        };
        let error = store.run_blocking().expect_err("missing source");
        assert_eq!(error.message_key, "error.import.notFound");
    }
}
