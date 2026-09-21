//! Pi-native instruction files and slash-command templates.

use crate::config::atomic_write;
use crate::error::AppError;
use crate::pi_config::get_pi_agent_dir;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};

const MAX_PROMPT_FILE_BYTES: u64 = 1024 * 1024;
const MISSING_REVISION: &str = "missing";
static PROMPT_FILE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PiAgentsFileSnapshot {
    pub content: Option<String>,
    pub revision: String,
}

/// Coordinates every CC Switch read-modify-write operation on Pi's AGENTS.md.
///
/// Keeping the guard alive across the database update lets callers compare the
/// file revision immediately before an atomic replacement and roll back their
/// database write if Pi or another editor changed the file in the meantime.
pub(crate) struct PiAgentsFileGuard {
    _guard: MutexGuard<'static, ()>,
    path: PathBuf,
}

impl PiAgentsFileGuard {
    pub(crate) fn acquire() -> Result<Self, AppError> {
        Ok(Self {
            _guard: lock_prompt_files()?,
            path: get_pi_agent_dir()?.join("AGENTS.md"),
        })
    }

    pub(crate) fn read(&self) -> Result<PiAgentsFileSnapshot, AppError> {
        let (content, file_revision) = match fs::File::open(&self.path) {
            Ok(file) => {
                let bytes = read_open_file_limited(file, &self.path, "Pi AGENTS.md")?;
                let file_revision = revision(&bytes);
                let content = String::from_utf8(bytes).map_err(|error| {
                    AppError::InvalidInput(format!(
                        "Pi AGENTS.md must be UTF-8 ({}): {error}",
                        self.path.display()
                    ))
                })?;
                (Some(content), file_revision)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (None, MISSING_REVISION.to_string())
            }
            Err(error) => return Err(AppError::io(&self.path, error)),
        };
        Ok(PiAgentsFileSnapshot {
            content,
            revision: file_revision,
        })
    }

    pub(crate) fn replace(&self, expected_revision: &str, content: &str) -> Result<(), AppError> {
        validate_content_size(content, "Pi AGENTS.md")?;
        ensure_revision(&self.path, expected_revision, "Pi AGENTS.md")?;
        atomic_write(&self.path, content.as_bytes())
    }

    pub(crate) fn delete(&self, expected_revision: &str) -> Result<(), AppError> {
        ensure_revision(&self.path, expected_revision, "Pi AGENTS.md")?;
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(AppError::io(&self.path, error)),
        }
    }
}

fn lock_prompt_files() -> Result<MutexGuard<'static, ()>, AppError> {
    PROMPT_FILE_LOCK
        .lock()
        .map_err(|error| AppError::Config(format!("Pi prompt file lock is poisoned: {error}")))
}

fn ensure_revision(path: &Path, expected: &str, label: &str) -> Result<(), AppError> {
    let actual = match fs::File::open(path) {
        Ok(file) => revision(&read_open_file_limited(file, path, label)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => MISSING_REVISION.to_string(),
        Err(error) => return Err(AppError::io(path, error)),
    };
    if actual == expected {
        Ok(())
    } else {
        Err(AppError::Conflict(format!(
            "{label} changed outside CC Switch: {}",
            path.display()
        )))
    }
}

fn read_open_file_limited(file: fs::File, path: &Path, label: &str) -> Result<Vec<u8>, AppError> {
    let metadata = file.metadata().map_err(|error| AppError::io(path, error))?;
    if metadata.len() > MAX_PROMPT_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "{label} exceeds the 1 MiB limit: {}",
            path.display()
        )));
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_PROMPT_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::io(path, error))?;
    if bytes.len() as u64 > MAX_PROMPT_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "{label} exceeds the 1 MiB limit: {}",
            path.display()
        )));
    }
    Ok(bytes)
}

fn validate_content_size(content: &str, label: &str) -> Result<(), AppError> {
    if content.len() as u64 > MAX_PROMPT_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "{label} exceeds the 1 MiB limit"
        )));
    }
    Ok(())
}

fn revision(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pi_config::test_support::TestAgentDir;
    use serial_test::serial;

    #[test]
    #[serial]
    fn agents_file_revision_rejects_an_external_edit() {
        let _agent = TestAgentDir::new();
        let guard = PiAgentsFileGuard::acquire().expect("lock AGENTS.md");
        let snapshot = guard.read().expect("read missing AGENTS.md");
        assert!(snapshot.content.is_none());

        let path = get_pi_agent_dir()
            .expect("agent directory")
            .join("AGENTS.md");
        fs::create_dir_all(path.parent().expect("agent directory")).expect("create agent dir");
        fs::write(&path, "external edit").expect("write AGENTS.md externally");

        let error = guard
            .replace(&snapshot.revision, "managed content")
            .expect_err("stale revision must not overwrite the external edit");
        assert!(matches!(error, AppError::Conflict(_)));
        assert_eq!(
            fs::read_to_string(path).expect("read external edit"),
            "external edit"
        );
    }

    #[test]
    #[serial]
    fn oversized_agents_file_is_rejected_before_it_is_loaded() {
        let _agent = TestAgentDir::new();
        let path = get_pi_agent_dir()
            .expect("agent directory")
            .join("AGENTS.md");
        fs::create_dir_all(path.parent().expect("agent directory")).expect("create agent dir");
        let file = fs::File::create(&path).expect("create AGENTS.md");
        file.set_len(MAX_PROMPT_FILE_BYTES + 1)
            .expect("make sparse oversized AGENTS.md");

        let guard = PiAgentsFileGuard::acquire().expect("lock AGENTS.md");
        let error = guard
            .read()
            .expect_err("oversized AGENTS.md must be rejected");
        assert!(error.to_string().contains("1 MiB limit"));
    }
}
