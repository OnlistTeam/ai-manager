//! Locating a package manager's executable beside a launcher or on a search
//! PATH.
//!
//! Windows must read the file system to know the extension: the Node installer
//! ships `npm.cmd` while Volta ships `volta.exe` (upstream `commands/misc.rs:2593`).
//! The same lookup anchors npm commands that have no owner to anchor to. On
//! Windows `std::process::Command::new("npm")` only appends `.exe` and never
//! resolves `npm.cmd` (rust-lang/rust#37380), so an unanchored spec can never
//! start there even though the upstream `cmd /C` string did.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::LifecycleProbe;

/// Launcher extensions in upstream's preference order (`sibling_bin_with_ext`).
#[cfg(target_os = "windows")]
pub(crate) const WINDOWS_PROGRAM_EXTENSIONS: &[&str] = &["cmd", "exe", "bat"];
/// Node and every Node manager ship `npm.cmd`; `npm.exe` covers bundled builds.
pub(crate) const NPM_EXTENSIONS: &[&str] = &["cmd", "exe"];

/// `<directory>/<name>.<extension>` for the first extension naming an existing
/// file. An empty directory (a bare launcher name) never resolves.
pub(crate) fn program_in_directory(
    directory: &Path,
    name: &str,
    extensions: &[&str],
) -> Option<PathBuf> {
    if directory.as_os_str().is_empty() {
        return None;
    }
    extensions
        .iter()
        .map(|extension| directory.join(format!("{name}.{extension}")))
        .find(|candidate| candidate.is_file())
}

/// First hit across the absolute directories of `search_path`, in PATH order.
pub(crate) fn program_on_search_path(
    search_path: &OsStr,
    name: &str,
    extensions: &[&str],
) -> Option<PathBuf> {
    std::env::split_paths(search_path)
        .filter(|directory| directory.is_absolute())
        .find_map(|directory| program_in_directory(&directory, name, extensions))
}

pub(crate) fn npm_on_search_path(search_path: &OsStr) -> Option<PathBuf> {
    program_on_search_path(search_path, "npm", NPM_EXTENSIONS)
}

/// The npm an ownerless command (fresh install, foreign-owner catalog query,
/// migration cleanup) may use. Windows anchors the first `npm.cmd`/`npm.exe`
/// on the effective PATH; when none exists the bare name stays and the
/// executor reports `programNotFound` truthfully. Other platforms return
/// `None`: the injected `path_env` lets the child resolve the bare name.
pub(crate) fn npm_on_path(probe: &LifecycleProbe) -> Option<PathBuf> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    npm_on_search_path(&effective_search_path(probe)?)
}

/// `gui_path_env` is `None` on Windows; the registry merge upstream computes
/// for detection is the effective PATH there.
fn effective_search_path(probe: &LifecycleProbe) -> Option<OsString> {
    probe
        .path_env
        .as_ref()
        .map(|(_, value)| OsString::from(value))
        .or_else(crate::commands::misc::effective_path_for_tool_detection)
}

#[cfg(test)]
mod tests {
    use super::{npm_on_path, npm_on_search_path, program_in_directory, program_on_search_path};
    use crate::compat::ccswitch::install_probe::LifecycleProbe;
    use std::path::Path;

    fn touch(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"@echo off").expect("create fake launcher");
        path
    }

    #[test]
    fn a_directory_lookup_follows_the_extension_preference_and_needs_a_file() {
        let dir = tempfile::tempdir().expect("fake bin dir");
        assert_eq!(
            program_in_directory(dir.path(), "npm", &["cmd", "exe"]),
            None
        );
        let exe = touch(dir.path(), "npm.exe");
        assert_eq!(
            program_in_directory(dir.path(), "npm", &["cmd", "exe"]),
            Some(exe)
        );
        let cmd = touch(dir.path(), "npm.cmd");
        assert_eq!(
            program_in_directory(dir.path(), "npm", &["cmd", "exe"]),
            Some(cmd)
        );
        std::fs::create_dir(dir.path().join("volta.exe")).expect("a directory is not a program");
        assert_eq!(
            program_in_directory(dir.path(), "volta", &["exe", "cmd"]),
            None
        );
        assert_eq!(program_in_directory(Path::new(""), "npm", &["cmd"]), None);
    }

    #[test]
    fn a_search_path_lookup_takes_the_first_absolute_hit_in_path_order() {
        let first = tempfile::tempdir().expect("first dir");
        let second = tempfile::tempdir().expect("second dir");
        let expected = touch(second.path(), "npm.cmd");
        touch(second.path(), "npm.exe");
        let search_path = std::env::join_paths([
            Path::new("relative/bin"),
            first.path(),
            second.path(),
            first.path(),
        ])
        .expect("joinable PATH");
        assert_eq!(
            program_on_search_path(&search_path, "npm", &["cmd", "exe"]),
            Some(expected.clone())
        );
        assert_eq!(npm_on_search_path(&search_path), Some(expected));
        assert_eq!(
            program_on_search_path(&search_path, "pnpm", &["cmd", "exe"]),
            None
        );
    }

    /// POSIX children resolve the bare name through the injected PATH; only
    /// Windows needs the explicit anchor. The probe's own PATH is the source.
    #[test]
    fn an_ownerless_npm_is_anchored_only_where_the_bare_name_cannot_start() {
        let dir = tempfile::tempdir().expect("fake bin dir");
        let npm = touch(dir.path(), "npm.cmd");
        let probe = LifecycleProbe {
            entry: None,
            path_env: Some((
                "PATH".to_string(),
                dir.path().to_string_lossy().into_owned(),
            )),
        };
        let expected = cfg!(target_os = "windows").then_some(npm);
        assert_eq!(npm_on_path(&probe), expected);
    }
}
