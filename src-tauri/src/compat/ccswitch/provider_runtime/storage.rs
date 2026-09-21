use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::compat::ccswitch::session::SessionStore;
use crate::domain::{ProviderRuntimeResourceKind, ProviderRuntimeStorage, ToolId};

use super::ResolvedRuntimeResource;

/// Capacity checks are informational and must never turn opening AI Services
/// into an unbounded disk crawl. The result remains useful as a lower bound.
const MAX_STORAGE_ENTRIES: usize = 50_000;
const MAX_STORAGE_DEPTH: usize = 64;
const MAX_WIRE_BYTES: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PathMeasurement {
    bytes: Option<u64>,
    limited: bool,
}

pub(super) fn measure_runtime_storage(
    tool: ToolId,
    resources: &mut [ResolvedRuntimeResource],
) -> ProviderRuntimeStorage {
    let mut remaining = MAX_STORAGE_ENTRIES;
    let mut measured_paths: HashMap<PathBuf, PathMeasurement> = HashMap::new();
    let mut total_bytes = 0_u64;
    let mut session_bytes = 0_u64;
    let mut measurement_limited = false;

    for resolved in resources {
        if let Some(measurement) = measured_paths.get(&resolved.path).copied() {
            resolved.resource.size_bytes = measurement.bytes;
            resolved.resource.measurement_limited = measurement.limited;
            continue;
        }
        let measurement = measure_path(&resolved.path, &mut remaining);
        measured_paths.insert(resolved.path.clone(), measurement);
        resolved.resource.size_bytes = measurement.bytes;
        resolved.resource.measurement_limited = measurement.limited;
        measurement_limited |= measurement.limited;
        if let Some(bytes) = measurement.bytes {
            total_bytes = capped_add(total_bytes, bytes);
            if resolved.resource.kind == ProviderRuntimeResourceKind::SessionData {
                session_bytes = capped_add(session_bytes, bytes);
            }
        }
    }

    let session_count = SessionStore::list(None, Some(tool))
        .map(|sessions| sessions.total_count)
        .unwrap_or_else(|error| {
            log::warn!(
                "Session count unavailable for tool={}: {}",
                tool.as_str(),
                error.message_key
            );
            measurement_limited = true;
            0
        });

    ProviderRuntimeStorage {
        total_bytes,
        session_bytes,
        session_count,
        measurement_limited,
    }
}

fn measure_path(path: &Path, remaining: &mut usize) -> PathMeasurement {
    if !path.exists() {
        return PathMeasurement {
            bytes: None,
            limited: false,
        };
    }

    let mut stack = vec![(path.to_path_buf(), 0_usize)];
    let mut bytes = 0_u64;
    let mut measured_any = false;
    let mut limited = false;

    while let Some((candidate, depth)) = stack.pop() {
        if *remaining == 0 {
            limited = true;
            break;
        }
        *remaining -= 1;

        let metadata = match std::fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(_) => {
                limited = true;
                continue;
            }
        };
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            limited = true;
            continue;
        }
        if file_type.is_file() {
            bytes = capped_add(bytes, metadata.len());
            measured_any = true;
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        measured_any = true;
        if depth >= MAX_STORAGE_DEPTH {
            limited = true;
            continue;
        }
        match std::fs::read_dir(&candidate) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(entry) => stack.push((entry.path(), depth + 1)),
                        Err(_) => limited = true,
                    }
                }
            }
            Err(_) => limited = true,
        }
    }

    PathMeasurement {
        bytes: measured_any.then_some(bytes),
        limited,
    }
}

fn capped_add(left: u64, right: u64) -> u64 {
    left.saturating_add(right).min(MAX_WIRE_BYTES)
}

#[cfg(test)]
mod tests {
    use super::{measure_path, MAX_STORAGE_ENTRIES};

    #[test]
    fn bounded_measurement_counts_regular_files_without_following_symlinks() {
        let temp = tempfile::tempdir().expect("temporary storage tree");
        std::fs::write(temp.path().join("one.bin"), [1_u8; 3]).expect("first file");
        std::fs::create_dir(temp.path().join("nested")).expect("nested directory");
        std::fs::write(temp.path().join("nested/two.bin"), [2_u8; 5]).expect("second file");

        let mut remaining = MAX_STORAGE_ENTRIES;
        let measured = measure_path(temp.path(), &mut remaining);
        assert_eq!(measured.bytes, Some(8));
        assert!(!measured.limited);
    }

    #[test]
    fn exhausted_budget_returns_an_explicit_lower_bound() {
        let temp = tempfile::tempdir().expect("temporary storage tree");
        std::fs::write(temp.path().join("one.bin"), [1_u8; 3]).expect("file");
        let mut remaining = 1;
        let measured = measure_path(temp.path(), &mut remaining);
        assert_eq!(measured.bytes, Some(0));
        assert!(measured.limited);
    }

    #[test]
    fn a_missing_resource_has_no_fake_zero_byte_measurement() {
        let temp = tempfile::tempdir().expect("temporary storage tree");
        let mut remaining = MAX_STORAGE_ENTRIES;
        let measured = measure_path(&temp.path().join("missing"), &mut remaining);
        assert_eq!(measured.bytes, None);
        assert!(!measured.limited);
    }
}
