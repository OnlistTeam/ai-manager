//! Eight-step external Prompt-file writer (spec section 18).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::config::atomic_write;
use crate::domain::{AppError, ErrorCode};

use super::{save_failed, verify_failed};

#[derive(Debug)]
pub(super) enum LiveSnapshot {
    Missing,
    Present(Vec<u8>),
}

pub(super) fn read_live(path: &Path) -> Result<LiveSnapshot, AppError> {
    match fs::read(path) {
        Ok(bytes) => Ok(LiveSnapshot::Present(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LiveSnapshot::Missing),
        Err(error) => Err(save_failed(error)),
    }
}

fn validate_live_bytes(bytes: &[u8]) -> Result<(), AppError> {
    std::str::from_utf8(bytes).map(|_| ()).map_err(|_| {
        AppError::new(ErrorCode::ConfigParseFailed, "error.prompt.liveInvalid")
            .with_technical("the current Prompt file is not UTF-8 text")
            .with_remediation("error.remediation.checkPromptSettings")
    })
}

pub(super) fn backup_live(path: &Path, bytes: &[u8]) -> Result<PathBuf, AppError> {
    validate_live_bytes(bytes)?;
    let parent = path
        .parent()
        .ok_or_else(|| save_failed("Prompt path has no parent"))?;
    let directory = parent.join(".ai-manager-backups");
    fs::create_dir_all(&directory).map_err(save_failed)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("prompt");
    let backup = directory.join(format!(
        "{}.{}.{}.bak",
        file_name,
        Utc::now().format("%Y%m%dT%H%M%S%.3fZ"),
        uuid::Uuid::new_v4().simple()
    ));
    atomic_write(&backup, bytes).map_err(save_failed)?;
    verify_live(&backup, bytes)?;
    // Every switch or edit of the active Prompt adds a copy; keep the
    // directory at the same retain count the workspace writer uses.
    crate::compat::ccswitch::workspace::cleanup_recovery_copies(&directory);
    Ok(backup)
}

pub(super) fn replace_live(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    validate_live_bytes(bytes)?;
    let parent = path
        .parent()
        .ok_or_else(|| save_failed("Prompt path has no parent"))?;
    fs::create_dir_all(parent).map_err(save_failed)?;
    let stage = parent.join(format!(
        ".ai-manager-prompt-stage-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let staged = (|| -> Result<Vec<u8>, AppError> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options.open(&stage).map_err(save_failed)?;
        file.write_all(bytes).map_err(save_failed)?;
        file.flush().map_err(save_failed)?;
        drop(file);
        let staged = fs::read(&stage).map_err(save_failed)?;
        validate_live_bytes(&staged)?;
        if staged != bytes {
            return Err(verify_failed("staged Prompt bytes changed before replace"));
        }
        Ok(staged)
    })();
    let _ = fs::remove_file(&stage);
    let staged = staged?;
    atomic_write(path, &staged).map_err(save_failed)?;
    verify_live(path, bytes)
}

pub(super) fn restore_live(
    path: &Path,
    snapshot: &LiveSnapshot,
) -> Result<(), crate::error::AppError> {
    match snapshot {
        LiveSnapshot::Missing => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(crate::error::AppError::io(path, error)),
        },
        LiveSnapshot::Present(bytes) => {
            atomic_write(path, bytes)?;
            let restored =
                fs::read(path).map_err(|error| crate::error::AppError::io(path, error))?;
            if restored == *bytes {
                Ok(())
            } else {
                Err(crate::error::AppError::Message(
                    "Prompt rollback verification failed".to_string(),
                ))
            }
        }
    }
}

pub(super) fn verify_live(path: &Path, expected: &[u8]) -> Result<(), AppError> {
    let actual = fs::read(path).map_err(save_failed)?;
    if actual == expected {
        Ok(())
    } else {
        Err(verify_failed("Prompt file differs after atomic replace"))
    }
}
