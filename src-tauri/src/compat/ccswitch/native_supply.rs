//! Pure decisions for supplying the native layout (ADR-0033): platform package names,
//! official layout paths, registry version document parsing and URL construction. No I/O and
//! no subprocesses; the runtime lives in `adapters::native_supply`.
//!
//! Only official layouts this product has actually measured are listed here: Claude Code's
//! `~/.local/share/claude/versions/<ver>` plus the `~/.local/bin/claude` symlink. The
//! renderer only ever submits a stable ToolId and a version that passed
//! `validate_tool_version`; package names and paths are all decided here.

use std::path::{Component, Path, PathBuf};

use base64::Engine;

use crate::compat::ccswitch::install_probe::{InstallSource, InstalledEntry};
use crate::domain::{validate_tool_version, AppError, ErrorCode, ToolId};

const SHA512_PREFIX: &str = "sha512-";
const SHA512_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSupplyTarget {
    /// The platform package in the registry, e.g. `@anthropic-ai/claude-code-darwin-arm64`.
    pub package: &'static str,
    /// The target version, already validated by `validate_tool_version`.
    pub version: String,
    /// The `package/<binary_name>` entry name inside the tgz, which is also the name of the launch symlink in the official layout.
    pub binary_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLayout {
    /// The directory where the official installer keeps the single-file executable of each version.
    pub versions_dir: PathBuf,
    /// The launch entry found on PATH (a symlink), atomically repointed at the new version file once the supply finishes.
    pub launcher: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDist {
    /// Only `https://` is accepted.
    pub tarball: String,
    /// Only `sha512-<base64>` is accepted, and it must decode to exactly 64 bytes.
    pub integrity: String,
}

/// Whether this platform can supply the official layout for the tool. Windows and Linux
/// musl are unverified and return `false`, so the caller keeps the original official-channel
/// error (fail closed).
pub fn supplies(id: ToolId) -> bool {
    platform_package(id).is_some()
}

pub fn target_for(id: ToolId, version: &str) -> Option<NativeSupplyTarget> {
    let package = platform_package(id)?;
    let version = validate_tool_version(version).ok()?;
    Some(NativeSupplyTarget {
        package,
        version,
        binary_name: binary_name(id)?,
    })
}

/// The native packages Anthropic publishes per platform. The `cfg!` branches let every
/// target compile; combinations that are not listed (Windows, musl, other architectures)
/// have no measured official layout and always yield `None`.
fn platform_package(id: ToolId) -> Option<&'static str> {
    if id != ToolId::ClaudeCode || cfg!(target_env = "musl") {
        return None;
    }
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("@anthropic-ai/claude-code-darwin-arm64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("@anthropic-ai/claude-code-darwin-x64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("@anthropic-ai/claude-code-linux-arm64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("@anthropic-ai/claude-code-linux-x64")
    } else {
        None
    }
}

fn binary_name(id: ToolId) -> Option<&'static str> {
    match id {
        ToolId::ClaudeCode => Some("claude"),
        ToolId::Codex
        | ToolId::OpenCode
        | ToolId::GeminiCli
        | ToolId::GrokBuild
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => None,
    }
}

/// Only `InstallSource::Native` counts, and both the launcher and the version directory must lie strictly inside home.
pub fn layout_for(id: ToolId, entry: &InstalledEntry) -> Option<NativeLayout> {
    layout_for_home(id, entry, &crate::config::get_home_dir())
}

/// The version directory is taken from the parent of the detected real binary
/// (`…/claude/versions/<ver>`) rather than guessed from a default location: if the real
/// binary is not under `versions/`, this is not the official layout this product verified.
pub(crate) fn layout_for_home(
    id: ToolId,
    entry: &InstalledEntry,
    home: &Path,
) -> Option<NativeLayout> {
    if entry.source != InstallSource::Native || binary_name(id).is_none() {
        return None;
    }
    let versions_dir = entry.real_path.parent()?.to_path_buf();
    if versions_dir.file_name()? != "versions" {
        return None;
    }
    let launcher = entry.bin_path.clone();
    if !strictly_inside_home(&versions_dir, home) || !strictly_inside_home(&launcher, home) {
        return None;
    }
    Some(NativeLayout {
        versions_dir,
        launcher,
    })
}

/// The same lexical rule as `tool_paths::is_removable`: under home, non-empty, and made up
/// entirely of `Normal` components. `..` is not resolved, so any path containing a
/// `ParentDir` is rejected outright.
fn strictly_inside_home(path: &Path, home: &Path) -> bool {
    if home.as_os_str().is_empty() {
        return false;
    }
    match path.strip_prefix(home) {
        Ok(relative) => {
            let mut components = relative.components().peekable();
            components.peek().is_some()
                && components.all(|component| matches!(component, Component::Normal(_)))
        }
        Err(_) => false,
    }
}

/// The registry's version document (`GET /<package>/<version>`). Only `dist.tarball` and
/// `dist.integrity` are read and every other field is ignored; the version must match the
/// request.
pub fn parse_packument_version(json: &str, version: &str) -> Result<PackageDist, AppError> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| unavailable(format!("registry response is not JSON: {error}")))?;
    let published = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| unavailable("registry response has no version"))?;
    if published != version {
        return Err(unavailable(format!(
            "registry returned {published} instead of {version}"
        )));
    }
    let dist = value
        .get("dist")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| unavailable("registry response has no dist"))?;
    let tarball = dist
        .get("tarball")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| unavailable("registry response has no tarball"))?;
    let parsed = url::Url::parse(tarball)
        .map_err(|error| unavailable(format!("tarball URL is invalid: {error}")))?;
    if !trusted_tarball_scheme(&parsed) {
        return Err(unavailable("tarball URL is not https"));
    }
    let integrity = dist
        .get("integrity")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| unavailable("registry response has no integrity"))?;
    if decode_sha512_integrity(integrity).is_none() {
        return Err(unavailable("registry integrity is not a sha512 digest"));
    }
    Ok(PackageDist {
        tarball: tarball.to_string(),
        integrity: integrity.to_string(),
    })
}

/// Only `https://` is accepted on the public internet; loopback addresses (a local registry
/// mirror, a test stub) may use plaintext, matching how `proxy::http_client::get_for_url`
/// connects directly to loopback targets.
fn trusted_tarball_scheme(url: &url::Url) -> bool {
    match url.host() {
        None => false,
        Some(url::Host::Domain(host)) => {
            url.scheme() == "https"
                || (url.scheme() == "http" && host.eq_ignore_ascii_case("localhost"))
        }
        Some(url::Host::Ipv4(address)) => url.scheme() == "https" || address.is_loopback(),
        Some(url::Host::Ipv6(address)) => url.scheme() == "https" || address.is_loopback(),
    }
}

/// The address of the version list the official self-update must fetch before it does
/// anything, so probing it is equivalent to probing the whole official channel:
/// `claude update` downloads nothing without this document (its measured error is exactly
/// `Failed to fetch version from https://downloads.claude.ai/claude-code-releases/latest`).
///
/// Only tools whose official channel shape this product has measured are listed. The rest
/// return `None`, so the caller never decides on their behalf that "the official channel is
/// unreachable" — guessing wrong skips the tool's own update command, which costs more than
/// one extra wait.
pub fn official_update_probe_url(id: ToolId) -> Option<&'static str> {
    match id {
        ToolId::ClaudeCode => Some("https://downloads.claude.ai/claude-code-releases/latest"),
        ToolId::Codex
        | ToolId::OpenCode
        | ToolId::GeminiCli
        | ToolId::GrokBuild
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => None,
    }
}

/// The Apple Team ID the vendor signs native programs with. On macOS, newly supplied files must be signed by it.
pub fn code_signing_team(id: ToolId) -> Option<&'static str> {
    match id {
        ToolId::ClaudeCode => Some("Q6L2SF6YDW"),
        ToolId::Codex
        | ToolId::OpenCode
        | ToolId::GeminiCli
        | ToolId::GrokBuild
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => None,
    }
}

/// `sha512-<base64>` -> a 64-byte digest. Any other algorithm or length yields `None`: a weak hash cannot serve as a trust anchor.
pub fn decode_sha512_integrity(integrity: &str) -> Option<[u8; 64]> {
    let encoded = integrity.strip_prefix(SHA512_PREFIX)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    if bytes.len() != SHA512_BYTES {
        return None;
    }
    let mut digest = [0_u8; SHA512_BYTES];
    digest.copy_from_slice(&bytes);
    Some(digest)
}

/// `<registry>/<package>/<version>`; the `/` in a scoped package name is encoded as `%2f` per npm client convention.
pub fn registry_url(registry: &str, package: &str, version: &str) -> String {
    format!(
        "{}/{}/{version}",
        registry.trim_end_matches('/'),
        package.replacen('/', "%2f", 1)
    )
}

fn unavailable(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::UpdateFailed,
        "error.tool.nativeSupplyUnavailable",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
#[path = "native_supply/tests.rs"]
mod tests;
