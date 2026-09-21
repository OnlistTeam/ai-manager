use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use chrono::Utc;

use crate::config::atomic_write_private;
use crate::domain::{AppError, ErrorCode, OpenClawWorkspaceWriteOutcome};
use crate::settings::effective_backup_retain_count;

use super::MAX_WORKSPACE_CONTENT_BYTES;

fn write_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

#[derive(Debug)]
pub(super) enum Snapshot {
    Missing,
    Present(Vec<u8>),
}

pub(super) fn read_snapshot(path: &Path) -> Result<(Snapshot, Option<u64>), AppError> {
    let metadata = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Snapshot::Missing, None));
        }
        Ok(metadata) if metadata.file_type().is_file() => metadata,
        Ok(_) => {
            return Err(workspace_error(
                "error.openclawWorkspace.unsafeEntry",
                "workspace entry is not a regular file",
            ));
        }
        Err(error) => {
            return Err(workspace_error("error.openclawWorkspace.readFailed", error));
        }
    };
    if metadata.len() > MAX_WORKSPACE_CONTENT_BYTES as u64 {
        return Err(workspace_error(
            "error.openclawWorkspace.contentTooLarge",
            "workspace file exceeded the one MiB read limit",
        ));
    }
    let bytes = fs::read(path)
        .map_err(|error| workspace_error("error.openclawWorkspace.readFailed", error))?;
    if bytes.len() > MAX_WORKSPACE_CONTENT_BYTES {
        return Err(workspace_error(
            "error.openclawWorkspace.contentTooLarge",
            "workspace file grew beyond the one MiB read limit",
        ));
    }
    Ok((Snapshot::Present(bytes), modified_at(&metadata)))
}

pub(super) fn decode_content(bytes: Vec<u8>) -> Result<String, AppError> {
    String::from_utf8(bytes).map_err(|_| {
        workspace_error(
            "error.openclawWorkspace.contentInvalid",
            "workspace file is not UTF-8 text",
        )
    })
}

pub(super) fn safe_replace(
    path: &Path,
    content: &str,
) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
    validate_content(content)?;
    let _guard = write_guard()
        .lock()
        .map_err(|error| workspace_error("error.openclawWorkspace.saveFailed", error))?;
    let parent = path.parent().ok_or_else(|| {
        workspace_error(
            "error.openclawWorkspace.unsafeEntry",
            "workspace file has no parent directory",
        )
    })?;
    ensure_safe_directory(parent)?;
    let (snapshot, _) = read_snapshot(path)?;
    if matches!(&snapshot, Snapshot::Present(bytes) if bytes == content.as_bytes()) {
        return Ok(OpenClawWorkspaceWriteOutcome {
            backup_created: false,
        });
    }
    let backup_created = match &snapshot {
        Snapshot::Missing => false,
        Snapshot::Present(bytes) => create_recovery_copy(path, bytes)?,
    };

    let write_result = atomic_write_private(path, content.as_bytes())
        .map_err(|error| workspace_error("error.openclawWorkspace.saveFailed", error))
        .and_then(|()| verify_bytes(path, content.as_bytes()));
    if let Err(error) = write_result {
        restore_snapshot(path, &snapshot).map_err(|restore| {
            workspace_error(
                "error.openclawWorkspace.restoreFailed",
                format!("save failed and rollback failed: {restore}"),
            )
        })?;
        return Err(error);
    }

    Ok(OpenClawWorkspaceWriteOutcome { backup_created })
}

pub(super) fn safe_delete(path: &Path) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
    let _guard = write_guard()
        .lock()
        .map_err(|error| workspace_error("error.openclawWorkspace.deleteFailed", error))?;
    let (snapshot, _) = read_snapshot(path)?;
    let Snapshot::Present(bytes) = snapshot else {
        return Ok(OpenClawWorkspaceWriteOutcome {
            backup_created: false,
        });
    };
    let backup_created = create_recovery_copy(path, &bytes)?;
    let removed = fs::remove_file(path)
        .map_err(|error| workspace_error("error.openclawWorkspace.deleteFailed", error))
        .and_then(|()| match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(workspace_error(
                "error.openclawWorkspace.verifyFailed",
                "daily memory still exists after deletion",
            )),
            Err(error) => Err(workspace_error(
                "error.openclawWorkspace.verifyFailed",
                error,
            )),
        });
    if let Err(error) = removed {
        restore_snapshot(path, &Snapshot::Present(bytes)).map_err(|restore| {
            workspace_error(
                "error.openclawWorkspace.restoreFailed",
                format!("delete failed and rollback failed: {restore}"),
            )
        })?;
        return Err(error);
    }
    Ok(OpenClawWorkspaceWriteOutcome { backup_created })
}

fn validate_content(content: &str) -> Result<(), AppError> {
    if content.len() > MAX_WORKSPACE_CONTENT_BYTES {
        return Err(workspace_error(
            "error.openclawWorkspace.contentTooLarge",
            "workspace content exceeded the one MiB limit",
        ));
    }
    if content.contains('\0') {
        return Err(workspace_error(
            "error.openclawWorkspace.contentInvalid",
            "workspace content contained a NUL byte",
        ));
    }
    Ok(())
}

fn create_recovery_copy(path: &Path, bytes: &[u8]) -> Result<bool, AppError> {
    let parent = path.parent().ok_or_else(|| {
        workspace_error(
            "error.openclawWorkspace.saveFailed",
            "workspace recovery copy has no parent directory",
        )
    })?;
    let directory = parent.join(".ai-manager-backups");
    ensure_safe_directory(&directory)?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace.md");
    let backup = directory.join(format!(
        "{}.{}.{}.bak",
        filename,
        Utc::now().format("%Y%m%dT%H%M%S%.3fZ"),
        uuid::Uuid::new_v4().simple()
    ));
    atomic_write_private(&backup, bytes)
        .map_err(|error| workspace_error("error.openclawWorkspace.saveFailed", error))?;
    verify_bytes(&backup, bytes)?;
    cleanup_recovery_copies(&directory);
    Ok(true)
}

/// Keeps only the newest `effective_backup_retain_count()` recovery copies in
/// a `.ai-manager-backups` directory. The Prompt live-file writer shares this
/// rule so `~/.claude/.ai-manager-backups/` cannot grow without bound either.
pub(crate) fn cleanup_recovery_copies(directory: &Path) {
    let retain = effective_backup_retain_count();
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata
                .is_file()
                .then_some((entry.path(), metadata.modified().ok()))
        })
        .collect::<Vec<_>>();
    if entries.len() <= retain {
        return;
    }
    entries.sort_by_key(|(_, modified)| *modified);
    let remove = entries.len().saturating_sub(retain);
    for (path, _) in entries.into_iter().take(remove) {
        if let Err(error) = fs::remove_file(&path) {
            log::warn!("Failed to prune a recovery copy: {error}");
        }
    }
}

fn restore_snapshot(path: &Path, snapshot: &Snapshot) -> Result<(), AppError> {
    match snapshot {
        Snapshot::Missing => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(workspace_error(
                "error.openclawWorkspace.restoreFailed",
                error,
            )),
        },
        Snapshot::Present(bytes) => {
            atomic_write_private(path, bytes)
                .map_err(|error| workspace_error("error.openclawWorkspace.restoreFailed", error))?;
            verify_bytes(path, bytes)
        }
    }
}

fn verify_bytes(path: &Path, expected: &[u8]) -> Result<(), AppError> {
    let actual = fs::read(path)
        .map_err(|error| workspace_error("error.openclawWorkspace.verifyFailed", error))?;
    if actual == expected {
        Ok(())
    } else {
        Err(workspace_error(
            "error.openclawWorkspace.verifyFailed",
            "workspace bytes differed after atomic replacement",
        ))
    }
}

pub(super) fn ensure_safe_directory(path: &Path) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(workspace_error(
            "error.openclawWorkspace.unsafeEntry",
            "workspace directory is a symbolic link or non-directory entry",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path)
                .map_err(|error| workspace_error("error.openclawWorkspace.saveFailed", error))?;
            match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
                Ok(_) => Err(workspace_error(
                    "error.openclawWorkspace.unsafeEntry",
                    "workspace directory changed while it was being created",
                )),
                Err(error) => Err(workspace_error("error.openclawWorkspace.saveFailed", error)),
            }
        }
        Err(error) => Err(workspace_error("error.openclawWorkspace.saveFailed", error)),
    }
}

pub(super) fn modified_at(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
}

pub(super) fn workspace_error(
    message_key: &'static str,
    technical: impl std::fmt::Display,
) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, message_key)
        .with_technical(technical.to_string())
        .with_remediation("error.remediation.retryOrViewDetails")
}
