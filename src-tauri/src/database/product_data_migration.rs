//! One-time migration from the inherited Tauri product identifier.
//!
//! This migration is deliberately narrower than the explicit CC Switch import:
//! it moves only data written by earlier AI Manager development builds under
//! `com.ccswitch.desktop`. The source is never modified or removed.

use super::Database;
use crate::error::AppError;
use crate::infrastructure::paths::APP_DB_FILE_NAME;
use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags};
use std::fs;
use std::path::{Component, Path};
use tempfile::Builder;

const LEGACY_PRODUCT_IDENTIFIER: &str = "com.ccswitch.desktop";
const MIGRATED_TREES: &[&str] = &["skills", "skill-backups"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProductDataMigrationOutcome {
    NoLegacyDatabase,
    DestinationDatabaseExists,
    Migrated {
        backup_files: usize,
        tree_files: usize,
    },
}

/// Locate the inherited product directory beside the current Tauri AppData
/// directory and migrate its allow-listed contents.
pub(crate) fn migrate_legacy_product_data(
    product_data_dir: &Path,
) -> Result<ProductDataMigrationOutcome, AppError> {
    let parent = product_data_dir.parent().ok_or_else(|| {
        AppError::InvalidInput(format!(
            "Product data directory has no parent directory usable for legacy-identifier migration: {}",
            product_data_dir.display()
        ))
    })?;
    migrate_product_data_from(&parent.join(LEGACY_PRODUCT_IDENTIFIER), product_data_dir)
}

fn migrate_product_data_from(
    legacy_dir: &Path,
    product_data_dir: &Path,
) -> Result<ProductDataMigrationOutcome, AppError> {
    let destination_db = product_data_dir.join(APP_DB_FILE_NAME);
    if path_entry_exists(&destination_db)? {
        return Ok(ProductDataMigrationOutcome::DestinationDatabaseExists);
    }

    let source_db = legacy_dir.join(APP_DB_FILE_NAME);
    if !path_entry_exists(&source_db)? {
        return Ok(ProductDataMigrationOutcome::NoLegacyDatabase);
    }

    require_real_directory(legacy_dir, false)?;
    require_regular_file(&source_db)?;
    ensure_real_directory(product_data_dir)?;

    // Stage and fully validate the database before copying any auxiliary
    // allow-listed data. The database is published last, so an interrupted run
    // remains retryable and never accepts a partial image.
    let mut staged_path = Builder::new()
        .prefix(".ai-manager-product-migration-")
        .suffix(".tmp")
        .tempfile_in(product_data_dir)
        .map_err(|e| AppError::io(product_data_dir, e))?
        .into_temp_path();

    {
        let source = open_read_only_database(&source_db)?;
        Database::validate_sqlite_integrity(&source)?;
        Database::validate_imported_schema(&source)?;

        let staged_database_path: &Path = staged_path.as_ref();
        let mut staged = Connection::open(staged_database_path).map_err(|e| {
            AppError::Database(format!(
                "Failed to open the migration staging database: {e}"
            ))
        })?;
        {
            let backup = Backup::new(&source, &mut staged).map_err(|e| {
                AppError::Database(format!(
                    "Failed to create the product database migration snapshot: {e}"
                ))
            })?;
            Database::complete_backup(&backup, "create product database migration snapshot")?;
        }

        Database::validate_sqlite_integrity(&staged)?;
        Database::validate_imported_schema(&staged)?;
        Database::create_tables_on_conn(&staged)?;
        Database::apply_schema_migrations_on_conn(&staged)?;
        Database::ensure_model_pricing_seeded_on_conn(&staged)?;
        Database::validate_sqlite_integrity(&staged)?;
        staged.close().map_err(|(_, e)| {
            AppError::Database(format!(
                "Failed to close the product database migration staging file: {e}"
            ))
        })?;
        source.close().map_err(|(_, e)| {
            AppError::Database(format!(
                "Failed to close the legacy-identifier read-only database: {e}"
            ))
        })?;
    }

    let backup_files = copy_known_database_backups(
        &legacy_dir.join("backups"),
        &product_data_dir.join("backups"),
    )?;
    let mut tree_files = 0;
    for tree in MIGRATED_TREES {
        tree_files += copy_known_tree(&legacy_dir.join(tree), &product_data_dir.join(tree))?;
    }

    match staged_path.persist_noclobber(&destination_db) {
        Ok(()) => Ok(ProductDataMigrationOutcome::Migrated {
            backup_files,
            tree_files,
        }),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            staged_path = error.path;
            drop(staged_path);
            Ok(ProductDataMigrationOutcome::DestinationDatabaseExists)
        }
        Err(error) => Err(AppError::io(&destination_db, error.error)),
    }
}

fn open_read_only_database(path: &Path) -> Result<Connection, AppError> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to open the legacy-identifier database read-only: {e}"
        ))
    })
}

fn copy_known_database_backups(
    source_dir: &Path,
    destination_dir: &Path,
) -> Result<usize, AppError> {
    if !path_entry_exists(source_dir)? {
        return Ok(0);
    }
    if !is_real_directory(source_dir)? {
        return Ok(0);
    }
    ensure_real_directory(destination_dir)?;

    let mut entries = fs::read_dir(source_dir)
        .map_err(|e| AppError::io(source_dir, e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::io(source_dir, e))?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut copied = 0;
    for entry in entries {
        let name = entry.file_name();
        let Some(name_text) = name.to_str() else {
            continue;
        };
        if !is_safe_backup_name(name_text) {
            continue;
        }

        let source = entry.path();
        let metadata = fs::symlink_metadata(&source).map_err(|e| AppError::io(&source, e))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }

        let destination = destination_dir.join(&name);
        if path_entry_exists(&destination)? {
            continue;
        }

        let temp_path = copy_regular_file_to_temp(&source, destination_dir, ".ai-manager-backup-")?;
        let validation = (|| -> Result<(), AppError> {
            let copied_db = open_read_only_database(temp_path.as_ref())?;
            Database::validate_sqlite_integrity(&copied_db)?;
            Database::validate_imported_schema(&copied_db)?;
            copied_db.close().map_err(|(_, e)| {
                AppError::Database(format!("Failed to close the migrated database backup: {e}"))
            })
        })();
        if let Err(error) = validation {
            log::warn!(
                "Skipping invalid legacy-identifier database backup {}: {error}",
                source.display()
            );
            continue;
        }

        match temp_path.persist_noclobber(&destination) {
            Ok(()) => copied += 1,
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(AppError::io(&destination, error.error)),
        }
    }
    Ok(copied)
}

fn copy_known_tree(source_dir: &Path, destination_dir: &Path) -> Result<usize, AppError> {
    if !path_entry_exists(source_dir)? || !is_real_directory(source_dir)? {
        return Ok(0);
    }
    ensure_real_directory(destination_dir)?;
    copy_tree_contents(source_dir, destination_dir)
}

fn copy_tree_contents(source_dir: &Path, destination_dir: &Path) -> Result<usize, AppError> {
    let mut entries = fs::read_dir(source_dir)
        .map_err(|e| AppError::io(source_dir, e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::io(source_dir, e))?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut copied = 0;
    for entry in entries {
        let name = entry.file_name();
        if !is_single_normal_component(Path::new(&name)) {
            continue;
        }

        let source = entry.path();
        let destination = destination_dir.join(&name);
        let metadata = fs::symlink_metadata(&source).map_err(|e| AppError::io(&source, e))?;
        if metadata.file_type().is_symlink() {
            continue;
        }

        if metadata.is_dir() {
            if path_entry_exists(&destination)? {
                if !is_real_directory(&destination)? {
                    continue;
                }
            } else {
                ensure_real_directory(&destination)?;
            }
            copied += copy_tree_contents(&source, &destination)?;
        } else if metadata.is_file() {
            if path_entry_exists(&destination)? {
                continue;
            }
            let temp_path =
                copy_regular_file_to_temp(&source, destination_dir, ".ai-manager-tree-file-")?;
            match temp_path.persist_noclobber(&destination) {
                Ok(()) => copied += 1,
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(AppError::io(&destination, error.error)),
            }
        }
    }
    Ok(copied)
}

fn copy_regular_file_to_temp(
    source: &Path,
    destination_dir: &Path,
    prefix: &str,
) -> Result<tempfile::TempPath, AppError> {
    let temp_path = Builder::new()
        .prefix(prefix)
        .suffix(".tmp")
        .tempfile_in(destination_dir)
        .map_err(|e| AppError::io(destination_dir, e))?
        .into_temp_path();
    let temporary_file: &Path = temp_path.as_ref();
    fs::copy(source, temporary_file).map_err(|e| AppError::io(source, e))?;
    Ok(temp_path)
}

fn is_safe_backup_name(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix("db_backup_")
        .and_then(|value| value.strip_suffix(".db"))
    else {
        return false;
    };
    !stem.is_empty()
        && stem
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'_')
}

fn is_single_normal_component(path: &Path) -> bool {
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn path_entry_exists(path: &Path) -> Result<bool, AppError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::io(path, error)),
    }
}

fn require_real_directory(path: &Path, create: bool) -> Result<(), AppError> {
    if create && !path_entry_exists(path)? {
        fs::create_dir_all(path).map_err(|e| AppError::io(path, e))?;
    }
    let metadata = fs::symlink_metadata(path).map_err(|e| AppError::io(path, e))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::InvalidInput(format!(
            "Product data migration rejected a non-real directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn ensure_real_directory(path: &Path) -> Result<(), AppError> {
    require_real_directory(path, true)
}

fn is_real_directory(path: &Path) -> Result<bool, AppError> {
    let metadata = fs::symlink_metadata(path).map_err(|e| AppError::io(path, e))?;
    Ok(!metadata.file_type().is_symlink() && metadata.is_dir())
}

fn require_regular_file(path: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path).map_err(|e| AppError::io(path, e))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::InvalidInput(format!(
            "Product data migration rejected a non-regular database file: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::tempdir;

    fn create_legacy_database(path: &Path, marker: &str) {
        fs::create_dir_all(path.parent().expect("database parent")).expect("create database dir");
        let conn = Connection::open(path).expect("open fixture database");
        Database::create_tables_on_conn(&conn).expect("create fixture schema");
        Database::apply_schema_migrations_on_conn(&conn).expect("migrate fixture schema");
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params!["product_migration_marker", marker],
        )
        .expect("insert fixture marker");
        conn.close().expect("close fixture database");
    }

    fn marker(path: &Path) -> String {
        let conn = Connection::open(path).expect("open migrated database");
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'product_migration_marker'",
            [],
            |row| row.get(0),
        )
        .expect("read marker")
    }

    #[test]
    fn migrates_database_when_destination_is_absent_and_preserves_source() {
        let root = tempdir().expect("temp root");
        let old = root.path().join(LEGACY_PRODUCT_IDENTIFIER);
        let new = root.path().join("tools.aimanager.desktop");
        let source = old.join(APP_DB_FILE_NAME);
        create_legacy_database(&source, "legacy");
        let original = fs::read(&source).expect("read source bytes");

        assert_eq!(
            migrate_product_data_from(&old, &new).expect("migrate product data"),
            ProductDataMigrationOutcome::Migrated {
                backup_files: 0,
                tree_files: 0,
            }
        );
        assert_eq!(marker(&new.join(APP_DB_FILE_NAME)), "legacy");
        assert_eq!(fs::read(&source).expect("reread source"), original);
    }

    #[test]
    fn existing_destination_wins_and_repeated_migration_is_a_no_op() {
        let root = tempdir().expect("temp root");
        let old = root.path().join(LEGACY_PRODUCT_IDENTIFIER);
        let new = root.path().join("tools.aimanager.desktop");
        create_legacy_database(&old.join(APP_DB_FILE_NAME), "legacy");
        create_legacy_database(&new.join(APP_DB_FILE_NAME), "product");

        assert_eq!(
            migrate_product_data_from(&old, &new).expect("skip existing destination"),
            ProductDataMigrationOutcome::DestinationDatabaseExists
        );
        assert_eq!(marker(&new.join(APP_DB_FILE_NAME)), "product");

        fs::remove_file(new.join(APP_DB_FILE_NAME)).expect("remove test destination");
        migrate_product_data_from(&old, &new).expect("first migration");
        assert_eq!(
            migrate_product_data_from(&old, &new).expect("repeat migration"),
            ProductDataMigrationOutcome::DestinationDatabaseExists
        );
    }

    #[test]
    fn corrupt_source_never_replaces_the_destination() {
        let root = tempdir().expect("temp root");
        let old = root.path().join(LEGACY_PRODUCT_IDENTIFIER);
        let new = root.path().join("tools.aimanager.desktop");
        fs::create_dir_all(&old).expect("create old dir");
        fs::write(old.join(APP_DB_FILE_NAME), b"not sqlite").expect("write corrupt database");

        assert!(migrate_product_data_from(&old, &new).is_err());
        assert!(!new.join(APP_DB_FILE_NAME).exists());
    }

    #[test]
    fn stale_partial_file_is_ignored() {
        let root = tempdir().expect("temp root");
        let old = root.path().join(LEGACY_PRODUCT_IDENTIFIER);
        let new = root.path().join("tools.aimanager.desktop");
        create_legacy_database(&old.join(APP_DB_FILE_NAME), "complete");
        fs::create_dir_all(&new).expect("create destination");
        let stale = new.join(".ai-manager-product-migration-stale.tmp");
        fs::write(&stale, b"partial").expect("write stale temporary file");

        migrate_product_data_from(&old, &new).expect("migrate around stale file");
        assert_eq!(marker(&new.join(APP_DB_FILE_NAME)), "complete");
        assert_eq!(fs::read(stale).expect("read stale file"), b"partial");
    }

    #[test]
    fn copies_only_safe_valid_database_backups() {
        let root = tempdir().expect("temp root");
        let old = root.path().join(LEGACY_PRODUCT_IDENTIFIER);
        let new = root.path().join("tools.aimanager.desktop");
        let source = old.join(APP_DB_FILE_NAME);
        create_legacy_database(&source, "legacy");
        let backups = old.join("backups");
        fs::create_dir_all(&backups).expect("create backups");
        fs::copy(&source, backups.join("db_backup_20260820_021953.db")).expect("copy valid backup");
        fs::write(backups.join("db_backup_20260820_999999.db"), b"not sqlite")
            .expect("write corrupt safe-name backup");
        fs::write(backups.join("db_backup_corrupt.db"), b"not sqlite")
            .expect("write invalid-name backup");
        fs::write(backups.join("notes.db"), b"arbitrary").expect("write arbitrary file");

        let outcome = migrate_product_data_from(&old, &new).expect("migrate backups");
        assert_eq!(
            outcome,
            ProductDataMigrationOutcome::Migrated {
                backup_files: 1,
                tree_files: 0,
            }
        );
        assert!(new.join("backups/db_backup_20260820_021953.db").exists());
        assert!(!new.join("backups/db_backup_20260820_999999.db").exists());
        assert!(!new.join("backups/db_backup_corrupt.db").exists());
        assert!(!new.join("backups/notes.db").exists());
    }

    #[test]
    fn excludes_configuration_logs_oauth_and_arbitrary_top_level_files() {
        let root = tempdir().expect("temp root");
        let old = root.path().join(LEGACY_PRODUCT_IDENTIFIER);
        let new = root.path().join("tools.aimanager.desktop");
        create_legacy_database(&old.join(APP_DB_FILE_NAME), "legacy");
        for name in [
            "app_paths.json",
            ".window-state.json",
            "settings.json",
            "config.json",
            "skills.json",
            "codex_oauth_auth.json",
            "model_pricing.json",
            "crash.log",
            "arbitrary.txt",
        ] {
            fs::write(old.join(name), b"sensitive or regenerated").expect("write excluded file");
        }
        fs::create_dir_all(old.join("logs")).expect("create excluded logs");
        fs::write(old.join("logs/ai-manager.log"), b"secret").expect("write excluded log");

        migrate_product_data_from(&old, &new).expect("migrate allow list");
        for name in [
            "app_paths.json",
            ".window-state.json",
            "settings.json",
            "config.json",
            "skills.json",
            "codex_oauth_auth.json",
            "model_pricing.json",
            "crash.log",
            "arbitrary.txt",
            "logs",
        ] {
            assert!(!new.join(name).exists(), "excluded path copied: {name}");
        }
    }

    #[test]
    fn copies_skills_and_skill_backups_without_overwriting_product_files() {
        let root = tempdir().expect("temp root");
        let old = root.path().join(LEGACY_PRODUCT_IDENTIFIER);
        let new = root.path().join("tools.aimanager.desktop");
        create_legacy_database(&old.join(APP_DB_FILE_NAME), "legacy");
        fs::create_dir_all(old.join("skills/example/nested")).expect("create legacy skill");
        fs::write(old.join("skills/example/SKILL.md"), b"legacy skill")
            .expect("write legacy skill");
        fs::write(old.join("skills/example/nested/data.json"), b"{}")
            .expect("write nested skill file");
        fs::create_dir_all(old.join("skill-backups/example")).expect("create skill backup");
        fs::write(old.join("skill-backups/example/SKILL.md"), b"backup")
            .expect("write skill backup");
        fs::create_dir_all(new.join("skills/example")).expect("create product skill");
        fs::write(new.join("skills/example/SKILL.md"), b"product wins")
            .expect("write existing product file");

        let outcome = migrate_product_data_from(&old, &new).expect("migrate skill trees");
        assert_eq!(
            outcome,
            ProductDataMigrationOutcome::Migrated {
                backup_files: 0,
                tree_files: 2,
            }
        );
        assert_eq!(
            fs::read(new.join("skills/example/SKILL.md")).expect("read product skill"),
            b"product wins"
        );
        assert!(new.join("skills/example/nested/data.json").exists());
        assert!(new.join("skill-backups/example/SKILL.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_followed() {
        use std::os::unix::fs::symlink;

        let root = tempdir().expect("temp root");
        let old = root.path().join(LEGACY_PRODUCT_IDENTIFIER);
        let new = root.path().join("tools.aimanager.desktop");
        create_legacy_database(&old.join(APP_DB_FILE_NAME), "legacy");
        fs::create_dir_all(old.join("skills/example")).expect("create legacy skill");
        let outside = root.path().join("outside-secret");
        fs::write(&outside, b"secret").expect("write external target");
        symlink(&outside, old.join("skills/example/linked-secret")).expect("create skill symlink");
        fs::create_dir_all(old.join("backups")).expect("create backups");
        symlink(
            old.join(APP_DB_FILE_NAME),
            old.join("backups/db_backup_20260820_030000.db"),
        )
        .expect("create backup symlink");

        migrate_product_data_from(&old, &new).expect("migrate without symlinks");
        assert!(!new.join("skills/example/linked-secret").exists());
        assert!(!new.join("backups/db_backup_20260820_030000.db").exists());
    }
}
