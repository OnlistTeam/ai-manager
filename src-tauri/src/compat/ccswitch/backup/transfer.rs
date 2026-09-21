use std::fs;
use std::path::Path;

use crate::compat::ccswitch::settings::SettingsStore;
use crate::domain::{AppError, ErrorCode, RestoreOutcome};
use crate::services::skill::skill_state_write_guard;
use crate::services::sync_protocol::sync_mutex;

use super::{detail, list, project_restored_database, BackupStore};

/// A configuration archive is text and is parsed in memory by the inherited
/// engine. Keep the allocation bounded before untrusted input reaches it.
pub(crate) const MAX_IMPORT_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;

fn export_failed(error: impl std::fmt::Display) -> AppError {
    log::warn!("[Backup transfer] export failed: {}", detail(error));
    AppError::new(ErrorCode::ConfigWriteFailed, "error.backup.exportFailed")
        .with_technical("the selected destination could not be written")
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn export_location_refused() -> AppError {
    AppError::new(
        ErrorCode::PermissionDenied,
        "error.backup.exportLocationRefused",
    )
    .with_technical("configuration archives cannot replace managed product data")
    .with_remediation("error.remediation.chooseAnotherFolder")
}

fn import_failed(error: impl std::fmt::Display) -> AppError {
    log::warn!("[Backup transfer] import failed: {}", detail(error));
    AppError::new(ErrorCode::ConfigWriteFailed, "error.backup.importFailed")
        .with_technical("the selected configuration archive was rejected")
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn validate_import_archive(path: &Path) -> Result<(), AppError> {
    let metadata = fs::metadata(path).map_err(import_failed)?;
    if !metadata.is_file() {
        return Err(import_failed("the selected archive is not a regular file"));
    }
    if metadata.len() > MAX_IMPORT_ARCHIVE_BYTES {
        return Err(
            AppError::new(ErrorCode::ConfigParseFailed, "error.backup.importTooLarge")
                .with_technical(format!(
                    "archive size {} exceeds the {} byte limit",
                    metadata.len(),
                    MAX_IMPORT_ARCHIVE_BYTES
                ))
                .with_remediation("error.remediation.retryOrViewDetails"),
        );
    }
    Ok(())
}

fn validate_export_target(target: &Path, protected_root: &Path) -> Result<(), AppError> {
    if !target
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sql"))
    {
        return Err(export_location_refused());
    }

    let parent = target.parent().ok_or_else(export_location_refused)?;
    let resolved_parent = fs::canonicalize(parent).map_err(export_failed)?;
    let resolved_protected =
        fs::canonicalize(protected_root).unwrap_or_else(|_| protected_root.to_path_buf());
    if resolved_parent.starts_with(&resolved_protected) {
        return Err(export_location_refused());
    }
    Ok(())
}

impl BackupStore {
    pub fn export_archive(&self, target: &Path) -> Result<(), AppError> {
        validate_export_target(target, &crate::infrastructure::paths::product_data_dir())?;
        let archive = self.state.db.export_sql_string().map_err(export_failed)?;
        crate::config::atomic_write_private(target, archive.as_bytes()).map_err(export_failed)
    }

    pub async fn import_archive(self, source: &Path) -> Result<RestoreOutcome, AppError> {
        validate_import_archive(source)?;
        let source = source.to_path_buf();
        let _sync_guard = sync_mutex().lock().await;
        tauri::async_runtime::spawn_blocking(move || self.import_archive_blocking(&source))
            .await
            .map_err(|_| {
                AppError::new(ErrorCode::Internal, "error.command.joinFailed")
                    .with_technical("configuration archive worker did not finish")
                    .with_remediation("error.remediation.retryOrViewDetails")
            })?
    }

    fn import_archive_blocking(&self, source: &Path) -> Result<RestoreOutcome, AppError> {
        let tools_out_of_sync =
            self.replace_database_from_archive(source, project_restored_database)?;
        Ok(RestoreOutcome {
            backups: list()?,
            tools_out_of_sync,
        })
    }

    fn replace_database_from_archive<F>(
        &self,
        source: &Path,
        post_sync: F,
    ) -> Result<bool, AppError>
    where
        F: FnOnce(&crate::store::AppState) -> Result<(), crate::error::AppError>,
    {
        // Product preferences describe this computer and person, not the
        // portable service/extension configuration. Preserve them exactly as
        // the ordinary restore path does.
        let settings = SettingsStore::with_db(self.state.db.clone());
        let preserved = settings.load()?;

        {
            let _skill_state_guard = skill_state_write_guard();
            self.state.db.import_sql(source).map_err(import_failed)?;
        }

        let tools_out_of_sync = match post_sync(&self.state) {
            Ok(()) => false,
            Err(error) => {
                log::warn!(
                    "[Backup transfer] archive imported but live projection failed: {}",
                    detail(error)
                );
                true
            }
        };

        if let Err(error) = settings.save(preserved) {
            log::warn!(
                "[Backup transfer] archive imported but product preferences could not be restored: {}",
                detail(error)
            );
        }

        Ok(tools_out_of_sync)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::tempdir;

    use crate::database::Database;
    use crate::store::AppState;

    use super::{
        export_failed, import_failed, validate_export_target, validate_import_archive, BackupStore,
        MAX_IMPORT_ARCHIVE_BYTES,
    };

    #[test]
    fn archive_validation_accepts_a_bounded_regular_file() {
        let dir = tempdir().expect("temp dir");
        let archive = dir.path().join("config.sql");
        fs::write(&archive, "-- portable config").expect("archive");
        validate_import_archive(&archive).expect("bounded file");
    }

    #[test]
    fn archive_validation_rejects_directories_and_oversized_files() {
        let dir = tempdir().expect("temp dir");
        assert_eq!(
            validate_import_archive(dir.path())
                .expect_err("directory")
                .message_key,
            "error.backup.importFailed"
        );

        let archive = dir.path().join("too-large.sql");
        let file = fs::File::create(&archive).expect("sparse archive");
        file.set_len(MAX_IMPORT_ARCHIVE_BYTES + 1)
            .expect("sparse length");
        let error = validate_import_archive(&archive).expect_err("oversized archive");
        assert_eq!(error.message_key, "error.backup.importTooLarge");
        assert!(!error
            .technical_message
            .expect("bounded detail")
            .contains(archive.to_string_lossy().as_ref()));
    }

    #[test]
    fn export_never_overwrites_managed_product_data_or_a_non_archive_file() {
        let dir = tempdir().expect("temp dir");
        let protected = dir.path().join("product-data");
        let outside = dir.path().join("exports");
        fs::create_dir_all(&protected).expect("protected root");
        fs::create_dir_all(&outside).expect("outside root");

        let protected_target = protected.join("app.sql");
        let error =
            validate_export_target(&protected_target, &protected).expect_err("managed data target");
        assert_eq!(error.message_key, "error.backup.exportLocationRefused");
        assert!(!error
            .technical_message
            .expect("safe detail")
            .contains(protected.to_string_lossy().as_ref()));

        assert_eq!(
            validate_export_target(&outside.join("config.db"), &protected)
                .expect_err("wrong extension")
                .message_key,
            "error.backup.exportLocationRefused"
        );
        validate_export_target(&outside.join("config.sql"), &protected)
            .expect("outside archive target");
    }

    #[cfg(unix)]
    #[test]
    fn exported_archives_use_private_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("temp dir");
        let archive = dir.path().join("portable.sql");
        let store = BackupStore {
            state: AppState::new(Arc::new(Database::memory().expect("database"))),
        };
        store.export_archive(&archive).expect("private export");
        let mode = fs::metadata(&archive)
            .expect("archive metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn importing_real_archive_data_preserves_this_computers_product_preferences() {
        use crate::compat::ccswitch::settings::SettingsStore;
        use crate::domain::{
            DesktopAppId, DownloadStrategy, ExtensionKind, ExtensionScope, ProductSettings,
            TerminalAppId, ToolId,
        };

        let dir = tempdir().expect("temp dir");
        let archive = dir.path().join("portable.sql");
        let source = Database::memory().expect("source database");
        source
            .set_setting("portable-marker", "from-archive")
            .expect("source marker");
        fs::write(
            &archive,
            source.export_sql_string().expect("source archive"),
        )
        .expect("write archive");

        let target = Arc::new(Database::memory().expect("target database"));
        target
            .set_setting("portable-marker", "old")
            .expect("target marker");
        let preferences = ProductSettings {
            advanced_mode: true,
            import_prompt_seen: true,
            tool_scope: Some(ToolId::OpenCode),
            extension_scope: Some(ExtensionScope::desktop_app(DesktopAppId::ClaudeDesktop)),
            extension_kind: Some(ExtensionKind::Prompt),
            download_strategy: DownloadStrategy::Automatic,
            automatic_provider_failover: true,
            terminal_app: Some(TerminalAppId::Ghostty),
        };
        SettingsStore::with_db(target.clone())
            .save(preferences)
            .expect("save local preferences");
        let store = BackupStore {
            state: AppState::new(target.clone()),
        };

        let warning = store
            .replace_database_from_archive(&archive, |_| Ok(()))
            .expect("import archive");
        assert!(!warning);
        assert_eq!(
            target
                .get_setting("portable-marker")
                .expect("imported marker")
                .as_deref(),
            Some("from-archive")
        );
        assert_eq!(
            SettingsStore::with_db(target)
                .load()
                .expect("preserved preferences"),
            preferences
        );
    }

    #[test]
    fn transfer_errors_never_disclose_selected_paths_or_secrets() {
        for error in [
            export_failed("/Users/private/export.sql token=do-not-show"),
            import_failed("/Users/private/import.sql sk-ant-do-not-show"),
        ] {
            let technical = error.technical_message.expect("safe technical detail");
            assert!(!technical.contains("/Users/private"));
            assert!(!technical.contains("do-not-show"));
        }
    }
}
