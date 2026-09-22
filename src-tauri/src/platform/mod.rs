pub mod command;
pub mod desktop_app;
pub mod detached;
pub mod executor;
pub mod plan;
pub mod redact;
pub mod shell_environment;
pub mod terminal;

pub use command::{AllowedProgram, CommandSpec};
pub use desktop_app::{
    claude_desktop_mcp_config_path, DesktopAppPlatform, DesktopAppPlatformState,
    SystemDesktopAppPlatform,
};
pub use detached::{DetachedCommandLauncher, SystemDetachedCommandLauncher};
pub use shell_environment::{probe_shell_environment, ShellEnvironment, ShellEnvironmentSource};
pub use terminal::{
    installed_terminals, SystemTerminalLauncher, TerminalEnvironment, TerminalLaunchSpec,
    TerminalLauncher,
};

use serde::{Deserialize, Serialize};

/// Convert a canonicalized Windows path back to the form accepted by shell
/// commands. `std::fs::canonicalize` prefixes local paths with `\\?\` (and UNC
/// paths with `\\?\UNC\`), but `cmd.exe` cannot `call` a batch file through
/// those verbatim paths and reports "The system cannot find the path
/// specified." Direct Win32 executable launches accept the prefix; batch
/// scripts do not.
///
/// The conversion is pure string work, so it is compiled for tests on every
/// platform: a regression here only shows up on Windows at runtime, which is
/// the worst place to find it.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn windows_shell_compatible_path(path: &std::path::Path) -> std::path::PathBuf {
    let raw = path.to_string_lossy();
    if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
        std::path::PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(local) = raw.strip_prefix(r"\\?\") {
        std::path::PathBuf::from(local)
    } else {
        path.to_path_buf()
    }
}

/// Spec §13: `cfg(target_os)` is concentrated in the platform layer, and business code always reads `Platform::current()`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Platform {
    #[serde(rename = "macos")]
    MacOs,
    #[serde(rename = "windows")]
    Windows,
    #[serde(rename = "linux")]
    Linux,
    #[serde(rename = "unknown")]
    Unknown,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unknown
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MacOs => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Platform;

    #[test]
    fn windows_shell_compatible_path_strips_verbatim_prefixes() {
        use std::path::{Path, PathBuf};

        assert_eq!(
            super::windows_shell_compatible_path(Path::new(r"\\?\C:\tools\codex.cmd")),
            PathBuf::from(r"C:\tools\codex.cmd")
        );
        assert_eq!(
            super::windows_shell_compatible_path(Path::new(
                r"\\?\UNC\server\share\tools\codex.cmd"
            )),
            PathBuf::from(r"\\server\share\tools\codex.cmd")
        );
        assert_eq!(
            super::windows_shell_compatible_path(Path::new(r"C:\tools\codex.cmd")),
            PathBuf::from(r"C:\tools\codex.cmd")
        );
    }

    #[test]
    fn platform_strings_are_stable() {
        let pairs = [
            (Platform::MacOs, "macos"),
            (Platform::Windows, "windows"),
            (Platform::Linux, "linux"),
            (Platform::Unknown, "unknown"),
        ];
        for (platform, expected) in pairs {
            assert_eq!(platform.as_str(), expected);
            let json = serde_json::to_string(&platform).expect("serialize platform");
            assert_eq!(json, format!("\"{expected}\""));
        }
    }

    #[test]
    fn current_platform_is_one_of_the_supported_targets() {
        let current = Platform::current();
        assert!(matches!(
            current,
            Platform::MacOs | Platform::Windows | Platform::Linux
        ));
    }
}
