//! Safe product projection over CC Switch's OpenClaw workspace support.
//!
//! The inherited implementation already knows OpenClaw's nine documented
//! workspace files and `memory/YYYY-MM-DD.md` convention. This compatibility
//! layer keeps those conventions in one native place while adding the product
//! boundary: fixed identifiers, bounded reads/search, recovery copies, atomic
//! replacement, verification, rollback, and no raw paths over IPC.

use std::fs;
use std::path::PathBuf;

use chrono::NaiveDate;

use crate::config::get_home_dir;
use crate::domain::{
    AppError, OpenClawDailyMemoryDocument, OpenClawDailyMemoryList, OpenClawDailyMemorySummary,
    OpenClawWorkspaceDirectory, OpenClawWorkspaceDocument, OpenClawWorkspaceFileId,
    OpenClawWorkspaceFileStatus, OpenClawWorkspaceFileSummary, OpenClawWorkspaceOverview,
    OpenClawWorkspaceWriteOutcome,
};
use crate::settings::get_openclaw_override_dir;

mod safe_write;

pub(crate) use safe_write::cleanup_recovery_copies;
use safe_write::{
    decode_content, ensure_safe_directory, modified_at, read_snapshot, safe_delete, safe_replace,
    workspace_error, Snapshot,
};

pub const MAX_WORKSPACE_CONTENT_BYTES: usize = 1024 * 1024;
pub const MAX_WORKSPACE_SEARCH_CHARS: usize = 200;
const MAX_ENUMERATED_MEMORY_FILES: usize = 5_000;
const MAX_RETURNED_MEMORY_FILES: usize = 200;
const MAX_SEARCHED_MEMORY_BYTES: u64 = 16 * 1024 * 1024;
const PREVIEW_CHARS: usize = 220;

#[derive(Debug, Clone)]
struct MemoryMetadata {
    date: String,
    path: PathBuf,
    size_bytes: u64,
    modified_at: Option<u64>,
}

pub struct WorkspaceStore {
    root: PathBuf,
}

impl WorkspaceStore {
    pub fn open() -> Self {
        let openclaw =
            get_openclaw_override_dir().unwrap_or_else(|| get_home_dir().join(".openclaw"));
        Self {
            root: openclaw.join("workspace"),
        }
    }

    #[cfg(test)]
    fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn overview(&self) -> Result<OpenClawWorkspaceOverview, AppError> {
        let files = OpenClawWorkspaceFileId::ALL
            .into_iter()
            .map(|id| self.file_summary(id))
            .collect::<Vec<_>>();
        let existing_files = files
            .iter()
            .filter(|file| file.status == OpenClawWorkspaceFileStatus::Ready)
            .count() as u32;
        let file_bytes = files.iter().map(|file| file.size_bytes).sum::<u64>();
        let (memories, limited) = self.memory_metadata()?;
        let daily_memory_bytes = memories.iter().map(|memory| memory.size_bytes).sum::<u64>();

        Ok(OpenClawWorkspaceOverview {
            files,
            existing_files,
            daily_memory_count: memories.len() as u32,
            daily_memory_bytes,
            total_bytes: file_bytes.saturating_add(daily_memory_bytes),
            limited,
        })
    }

    pub fn document(
        &self,
        id: OpenClawWorkspaceFileId,
    ) -> Result<OpenClawWorkspaceDocument, AppError> {
        let path = self.root.join(id.filename());
        let (snapshot, modified_at) = read_snapshot(&path)?;
        match snapshot {
            Snapshot::Missing => Ok(OpenClawWorkspaceDocument {
                id,
                filename: id.filename().to_string(),
                exists: false,
                content: String::new(),
                size_bytes: 0,
                modified_at: None,
            }),
            Snapshot::Present(bytes) => {
                let content = decode_content(bytes)?;
                Ok(OpenClawWorkspaceDocument {
                    id,
                    filename: id.filename().to_string(),
                    exists: true,
                    size_bytes: content.len() as u64,
                    content,
                    modified_at,
                })
            }
        }
    }

    pub fn save_document(
        &self,
        id: OpenClawWorkspaceFileId,
        content: &str,
    ) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
        safe_replace(&self.root.join(id.filename()), content)
    }

    pub fn memories(&self, query: Option<&str>) -> Result<OpenClawDailyMemoryList, AppError> {
        let query = validate_query(query)?;
        let (metadata, metadata_limited) = self.memory_metadata()?;
        let total_count = metadata.len() as u32;
        let total_bytes = metadata.iter().map(|memory| memory.size_bytes).sum::<u64>();
        let mut items = Vec::new();
        let mut scanned_bytes = 0_u64;
        let mut limited = metadata_limited;

        for memory in &metadata {
            if items.len() >= MAX_RETURNED_MEMORY_FILES {
                limited = true;
                break;
            }
            if scanned_bytes.saturating_add(memory.size_bytes) > MAX_SEARCHED_MEMORY_BYTES {
                limited = true;
                break;
            }
            scanned_bytes = scanned_bytes.saturating_add(memory.size_bytes);

            let content = match read_snapshot(&memory.path) {
                Ok((Snapshot::Present(bytes), _)) => match decode_content(bytes) {
                    Ok(content) => content,
                    Err(_) => {
                        limited = true;
                        continue;
                    }
                },
                Ok((Snapshot::Missing, _)) => {
                    limited = true;
                    continue;
                }
                Err(_) => {
                    limited = true;
                    continue;
                }
            };

            let (matches, preview) = match query.as_deref() {
                Some(query) => {
                    let lowered = content.to_lowercase();
                    let date_match = memory.date.to_lowercase().contains(query);
                    let content_matches = lowered.matches(query).count();
                    if content_matches == 0 && !date_match {
                        continue;
                    }
                    let count = content_matches.saturating_add(usize::from(date_match));
                    (count as u32, matching_preview(&content, query))
                }
                None => (0, preview(&content)),
            };

            items.push(OpenClawDailyMemorySummary {
                date: memory.date.clone(),
                size_bytes: memory.size_bytes,
                modified_at: memory.modified_at,
                preview,
                match_count: matches,
            });
        }

        Ok(OpenClawDailyMemoryList {
            items,
            total_count,
            total_bytes,
            limited,
        })
    }

    pub fn memory_document(&self, date: &str) -> Result<OpenClawDailyMemoryDocument, AppError> {
        let date = validate_date(date)?;
        let path = self.memory_directory().join(format!("{date}.md"));
        let (snapshot, modified_at) = read_snapshot(&path)?;
        match snapshot {
            Snapshot::Missing => Ok(OpenClawDailyMemoryDocument {
                date,
                exists: false,
                content: String::new(),
                size_bytes: 0,
                modified_at: None,
            }),
            Snapshot::Present(bytes) => {
                let content = decode_content(bytes)?;
                Ok(OpenClawDailyMemoryDocument {
                    date,
                    exists: true,
                    size_bytes: content.len() as u64,
                    content,
                    modified_at,
                })
            }
        }
    }

    pub fn save_memory(
        &self,
        date: &str,
        content: &str,
    ) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
        let date = validate_date(date)?;
        safe_replace(&self.memory_directory().join(format!("{date}.md")), content)
    }

    pub fn delete_memory(&self, date: &str) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
        let date = validate_date(date)?;
        safe_delete(&self.memory_directory().join(format!("{date}.md")))
    }

    pub fn directory(&self, directory: OpenClawWorkspaceDirectory) -> Result<PathBuf, AppError> {
        let path = match directory {
            OpenClawWorkspaceDirectory::Workspace => self.root.clone(),
            OpenClawWorkspaceDirectory::DailyMemory => self.memory_directory(),
        };
        ensure_safe_directory(&path)?;
        Ok(path)
    }

    fn memory_directory(&self) -> PathBuf {
        self.root.join("memory")
    }

    fn file_summary(&self, id: OpenClawWorkspaceFileId) -> OpenClawWorkspaceFileSummary {
        let path = self.root.join(id.filename());
        let (status, size_bytes, modified_at) = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (OpenClawWorkspaceFileStatus::Missing, 0, None)
            }
            Ok(metadata) if metadata.file_type().is_file() => (
                OpenClawWorkspaceFileStatus::Ready,
                metadata.len(),
                modified_at(&metadata),
            ),
            Ok(_) | Err(_) => (OpenClawWorkspaceFileStatus::Unavailable, 0, None),
        };
        OpenClawWorkspaceFileSummary {
            id,
            filename: id.filename().to_string(),
            status,
            size_bytes,
            modified_at,
        }
    }

    fn memory_metadata(&self) -> Result<(Vec<MemoryMetadata>, bool), AppError> {
        let directory = self.memory_directory();
        let metadata = match fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), false));
            }
            Ok(metadata) if metadata.file_type().is_dir() => metadata,
            Ok(_) => {
                return Err(workspace_error(
                    "error.openclawWorkspace.unsafeEntry",
                    "daily memory location is not a regular directory",
                ));
            }
            Err(error) => {
                return Err(workspace_error("error.openclawWorkspace.listFailed", error));
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(workspace_error(
                "error.openclawWorkspace.unsafeEntry",
                "daily memory location is a symbolic link",
            ));
        }

        let entries = fs::read_dir(&directory)
            .map_err(|error| workspace_error("error.openclawWorkspace.listFailed", error))?;
        let mut memories = Vec::new();
        let mut limited = false;

        for entry in entries {
            if memories.len() >= MAX_ENUMERATED_MEMORY_FILES {
                limited = true;
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    limited = true;
                    continue;
                }
            };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let Some(date) = name.strip_suffix(".md") else {
                continue;
            };
            if validate_date(date).is_err() {
                continue;
            }
            let metadata = match fs::symlink_metadata(entry.path()) {
                Ok(metadata) if metadata.file_type().is_file() => metadata,
                _ => continue,
            };
            memories.push(MemoryMetadata {
                date: date.to_string(),
                path: entry.path(),
                size_bytes: metadata.len(),
                modified_at: modified_at(&metadata),
            });
        }
        memories.sort_by(|left, right| right.date.cmp(&left.date));
        Ok((memories, limited))
    }
}

fn validate_query(query: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return Ok(None);
    };
    if query.chars().count() > MAX_WORKSPACE_SEARCH_CHARS || query.contains('\0') {
        return Err(workspace_error(
            "error.openclawWorkspace.queryInvalid",
            "workspace search query exceeded its accepted shape",
        ));
    }
    Ok(Some(query.to_lowercase()))
}

fn validate_date(date: &str) -> Result<String, AppError> {
    let date = date.trim();
    if date.len() != 10 || NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
        return Err(workspace_error(
            "error.openclawWorkspace.dateInvalid",
            "daily memory date must be a real YYYY-MM-DD date",
        ));
    }
    Ok(date.to_string())
}

fn preview(content: &str) -> String {
    clean_preview(content.chars().take(PREVIEW_CHARS).collect())
}

fn matching_preview(content: &str, query: &str) -> String {
    let selected = content
        .lines()
        .find(|line| line.to_lowercase().contains(query))
        .unwrap_or(content);
    clean_preview(selected.chars().take(PREVIEW_CHARS).collect())
}

fn clean_preview(value: String) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests;
