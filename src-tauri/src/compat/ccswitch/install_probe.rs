use std::path::{Path, PathBuf};

use crate::commands::misc::{
    default_install, enumerate_tool_installations, infer_install_source, npm_package_for,
    ToolInstallation,
};
use crate::compat::ccswitch::tools::tool_id_to_cli_name;
use crate::domain::{AppError, ErrorCode, ToolId};
use crate::platform::redact::{redact_secrets, truncate_tail};

mod hermes_owner;
pub(crate) mod program_lookup;

/// Which package manager owns an installation. The decision order matches the upstream
/// `anchored_command_from_paths` (`commands/misc.rs:2864`): native installer -> Homebrew
/// formula -> path prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallSource {
    /// node from nvm / fnm / mise / Homebrew: npm sits in the same directory, so anchor to that npm.
    NodeManagerNpm,
    Volta,
    Bun,
    /// The upstream POSIX side classifies pnpm as "no sibling package manager" and returns
    /// None, while the Windows side has a dedicated branch. `pnpm add -g` /
    /// `pnpm remove -g` have no platform specifics, so this product takes the same path on
    /// both platforms — a **deliberate completion** of upstream, not a replicated omission.
    Pnpm,
    /// `uv tool list --show-paths` proves the receipt, the entrypoint and the current launcher at once.
    UvTool,
    /// The pipx local/global inventory plus the current launcher together prove which isolated environment owns it.
    Pipx,
    /// The real binary lives in `Cellar/<formula>/`: `brew upgrade` / `brew uninstall <formula>`.
    Brew,
    /// The real binary lives in `Caskroom/<token>/` (e.g. `brew install --cask claude-code`):
    /// only `brew upgrade --cask` / `brew uninstall --cask <token>` work; it is not a Node
    /// manager and cannot be upgraded as a formula.
    BrewCask,
    /// Installed by the tool's own installer (claude's version directory, opencode's `~/.opencode`).
    Native,
    /// system / unknown origin: no reliable sibling package manager, returns None just like upstream.
    Unmanaged,
}

/// Evidence of the Python tool manager for Hermes. It only flows inside the native layer;
/// absolute paths and the pipx scope must never reach the renderer or a persisted protocol.
/// `InstallSource` carries the stable category, this enum carries the anchor of this probe
/// that the follow-up actions must reuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HermesInstallOwner {
    Uv { program_path: PathBuf },
    Pipx { program_path: PathBuf, global: bool },
}

/// Structured description of "the installation the command line actually hits". The string
/// fields of the upstream `ToolInstallation` are all converted here into a shape the new
/// layer can build a CommandSpec from directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledEntry {
    /// The entry found on PATH (symlinks unresolved), which is also the executable the official self-update calls.
    pub bin_path: PathBuf,
    /// The canonicalized real binary, used to detect Cellar / native version directories.
    pub real_path: PathBuf,
    pub source: InstallSource,
    /// The Homebrew formula name (`Brew`) or cask token (`BrewCask`).
    pub brew_formula: Option<String>,
    /// Whether `--version` exits 0. false = installed but not runnable (a missing platform binary for Codex, ...).
    pub runnable: bool,
    pub npm_package: Option<&'static str>,
    pub hermes_owner: Option<HermesInstallOwner>,
}

/// All blocking probe results a single lifecycle action needs. Both spawn child processes
/// (the enumeration runs `--version`, the PATH fix runs a login shell), so they are merged
/// into one `spawn_blocking` to avoid blocking the async runtime.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LifecycleProbe {
    /// None when not installed, or installed in several places with no determinable default (upstream `default_install` semantics).
    pub entry: Option<InstalledEntry>,
    /// Result of the GUI process PATH fix (`("PATH", merged)`). Upstream
    /// `commands/misc.rs:2162-2202` explains why it is required: the probe runs through a
    /// login shell while execution runs in a non-login process, their PATHs are asymmetric,
    /// and a bare npm fallback is guaranteed to exit 127 under the narrow PATH.
    pub path_env: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectedInstallation {
    pub entry: InstalledEntry,
    pub version: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateInspection {
    pub lifecycle: LifecycleProbe,
    pub installations: Vec<InspectedInstallation>,
}

pub async fn inspect_update(id: ToolId) -> Result<UpdateInspection, AppError> {
    let app_type = tool_id_to_cli_name(id);
    let joined = tokio::task::spawn_blocking(move || {
        inspection_from_installations(id, enumerate_tool_installations(app_type), gui_path_env())
    })
    .await;
    let mut inspection = inspection_from_join(joined)?;
    if id == ToolId::Hermes {
        inspection.lifecycle = hermes_owner::refine(inspection.lifecycle).await;
        if let (Some(selected), Some(default)) = (
            inspection.lifecycle.entry.clone(),
            inspection
                .installations
                .iter_mut()
                .find(|installation| installation.is_default),
        ) {
            default.entry = selected;
        }
    }
    Ok(inspection)
}

/// When the probe thread did not finish (an upstream enumeration panic, or a cancelled
/// task), report "could not determine" and never "not installed": an empty inspection is
/// read by every caller as not installed, which leads to a bare `npm i -g` producing a
/// second copy, or to capabilities being loosened/tightened as if nothing were installed.
/// The semantics match Unknown in `tools::unwrap_local_probe`. A panic payload may embed
/// subprocess output, so it is redacted and truncated all the same.
fn inspection_from_join(
    joined: Result<UpdateInspection, tokio::task::JoinError>,
) -> Result<UpdateInspection, AppError> {
    joined.map_err(|error| {
        AppError::new(ErrorCode::Internal, "error.command.joinFailed")
            .with_technical(format!(
                "installation inspection did not complete: {}",
                truncate_tail(&redact_secrets(&error.to_string()), 8, 512)
            ))
            .with_remediation("error.remediation.retryOrViewDetails")
    })
}

pub async fn probe(id: ToolId) -> Result<LifecycleProbe, AppError> {
    Ok(inspect_update(id).await?.lifecycle)
}

fn inspection_from_installations(
    id: ToolId,
    installations: Vec<ToolInstallation>,
    path_env: Option<(String, String)>,
) -> UpdateInspection {
    let default_index = default_install(&installations).and_then(|selected| {
        installations
            .iter()
            .position(|candidate| std::ptr::eq(candidate, selected))
    });
    let installations: Vec<_> = installations
        .iter()
        .enumerate()
        .map(|(index, installation)| InspectedInstallation {
            entry: to_entry(id, installation),
            version: installation.version.clone(),
            is_default: default_index == Some(index),
        })
        .collect();
    let entry = default_index.and_then(|index| {
        installations
            .get(index)
            .map(|installation| installation.entry.clone())
    });
    UpdateInspection {
        lifecycle: LifecycleProbe { entry, path_env },
        installations,
    }
}

fn to_entry(id: ToolId, install: &ToolInstallation) -> InstalledEntry {
    let bin_path = PathBuf::from(&install.path);
    let (source, brew_formula) = classify_source(id, &bin_path, &install.real);
    InstalledEntry {
        bin_path,
        real_path: install.real.clone(),
        source,
        brew_formula,
        runnable: install.runnable,
        npm_package: npm_package_for(tool_id_to_cli_name(id)),
        hermes_owner: None,
    }
}

pub fn classify_source(
    id: ToolId,
    bin_path: &Path,
    real_path: &Path,
) -> (InstallSource, Option<String>) {
    if is_native_layout(id, bin_path, real_path) {
        return (InstallSource::Native, None);
    }
    if let Some(formula) = brew_formula(real_path) {
        return (InstallSource::Brew, Some(formula));
    }
    if let Some(token) = brew_cask(real_path) {
        return (InstallSource::BrewCask, Some(token));
    }
    (package_manager_source(bin_path), None)
}

fn package_manager_source(bin_path: &Path) -> InstallSource {
    match infer_install_source(bin_path) {
        "volta" => InstallSource::Volta,
        "bun" => InstallSource::Bun,
        "pnpm" => InstallSource::Pnpm,
        tag => node_manager_source(tag, bin_path),
    }
}

/// POSIX Node managers are recognised by their directory layout. Windows has
/// no such convention: the Node installer, nvm-windows, Scoop, winget and the
/// Store build all place global CLIs beside `npm.cmd` (upstream
/// `commands/misc.rs:2970`), so that sibling is the ownership proof there.
fn node_manager_source(tag: &str, bin_path: &Path) -> InstallSource {
    if cfg!(target_os = "windows") {
        return windows_node_manager_source(bin_path);
    }
    match tag {
        "nvm" | "fnm" | "mise" | "homebrew" => InstallSource::NodeManagerNpm,
        _ => InstallSource::Unmanaged,
    }
}

fn windows_node_manager_source(bin_path: &Path) -> InstallSource {
    let sibling_npm = bin_path.parent().and_then(|directory| {
        program_lookup::program_in_directory(directory, "npm", program_lookup::NPM_EXTENSIONS)
    });
    match sibling_npm {
        Some(_) => InstallSource::NodeManagerNpm,
        None => InstallSource::Unmanaged,
    }
}

/// Where official installers put things. The two claude criteria correspond verbatim to
/// upstream `commands/misc.rs:2870-2875`; the opencode one comes from the upstream
/// `opencode_extra_search_paths` (`commands/misc.rs:1496`) — upstream has no uninstall, so
/// it only treats `~/.opencode/bin` as a search path with no "ownership decision", which is
/// added here. The standalone installer layout for codex (measured in ADR-0033): the real
/// binary is at `~/.codex/packages/standalone/releases/<ver>/codex` and
/// `~/.local/bin/codex` is a symlink.
fn is_native_layout(id: ToolId, bin_path: &Path, real_path: &Path) -> bool {
    let bin = normalize(bin_path);
    let real = normalize(real_path);
    match id {
        ToolId::ClaudeCode => {
            real.contains("/.local/share/claude/") || real.contains("/claude/versions/")
        }
        ToolId::Codex => real.contains("/.codex/packages/standalone/"),
        ToolId::OpenCode => bin.contains("/.opencode/bin/") || real.contains("/.opencode/bin/"),
        ToolId::GrokBuild => bin.contains("/.grok/bin/") || real.contains("/.grok/downloads/grok-"),
        // The official installer writes `$KIMI_CODE_HOME/bin/kimi`; search paths
        // and the settings/cache layout already follow that variable, so the
        // ownership rule must read the same configured home.
        ToolId::KimiCode => {
            let configured_bin = format!(
                "{}/",
                normalize(&crate::compat::ccswitch::tool_paths::kimi_code_home().join("bin"))
            );
            bin.contains("/.kimi-code/bin/kimi")
                || real.contains("/.kimi-code/bin/kimi")
                || bin.starts_with(&configured_bin)
                || real.starts_with(&configured_bin)
        }
        ToolId::GeminiCli
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::DeepSeekDsh => false,
    }
}

fn normalize(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

#[cfg(not(target_os = "windows"))]
fn brew_formula(real_path: &Path) -> Option<String> {
    crate::commands::misc::brew_formula_from_path(&real_path.to_string_lossy())
}

/// Windows has no Homebrew — the Windows version of the upstream
/// `anchored_command_from_paths` (`commands/misc.rs:2953`) has no brew branch either.
#[cfg(target_os = "windows")]
fn brew_formula(_real_path: &Path) -> Option<String> {
    None
}

/// `/opt/homebrew/Caskroom/claude-code/2.1.236/claude` -> `Some("claude-code")`.
/// Upstream only recognizes Cellar and misreads a cask's `/homebrew/` prefix as a Node
/// manager; this layer adds the Caskroom case, parsed by path segment just like
/// `brew_formula_from_path`.
#[cfg(not(target_os = "windows"))]
fn brew_cask(real_path: &Path) -> Option<String> {
    let real = real_path.to_string_lossy();
    let mut segments = real.split('/');
    segments
        .by_ref()
        .find(|segment| segment.eq_ignore_ascii_case("Caskroom"))?;
    segments
        .next()
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

#[cfg(target_os = "windows")]
fn brew_cask(_real_path: &Path) -> Option<String> {
    None
}

/// `<same directory as bin_path>/<name>`. Package managers put their own CLI next to the
/// commands they install in the same bin directory, so "derive from the sibling directory"
/// is a reliable source of absolute paths (upstream `commands/misc.rs:2612`).
/// No parent directory -> None, and the caller degrades to "not anchored"; it never builds
/// a bare command that depends on PATH.
#[cfg(not(target_os = "windows"))]
pub fn sibling_program(bin_path: &Path, name: &str) -> Option<PathBuf> {
    let parent = bin_path.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    Some(parent.join(name))
}

/// On Windows the extension must be disambiguated by reading the filesystem: the Node
/// installer installs `npm.cmd` while Volta installs `volta.exe`, which pure string
/// concatenation cannot tell apart (the same trade-off as upstream `commands/misc.rs:2593`).
#[cfg(target_os = "windows")]
pub fn sibling_program(bin_path: &Path, name: &str) -> Option<PathBuf> {
    program_lookup::program_in_directory(
        bin_path.parent()?,
        name,
        program_lookup::WINDOWS_PROGRAM_EXTENSIONS,
    )
}

#[cfg(not(target_os = "windows"))]
fn gui_path_env() -> Option<(String, String)> {
    let login = crate::commands::misc::login_shell_path()?;
    let inherited = std::env::var("PATH").unwrap_or_default();
    Some((
        "PATH".to_string(),
        crate::commands::misc::merge_path_segments(&login, &inherited),
    ))
}

/// The Windows PATH fix is upstream's registry merge in `effective_path_os`, which applies
/// to probing rather than execution; the anchored commands on the execution side all use
/// absolute paths and need no extra injection.
#[cfg(target_os = "windows")]
fn gui_path_env() -> Option<(String, String)> {
    None
}

// Every case feeds POSIX absolute paths, the Windows version of `sibling_program` reads the
// filesystem and the Windows version of `brew_formula` is always None, so this whole module
// is compiled on non-Windows platforms only. Path classification on Windows is covered by
// the CI Windows job through a real-machine `probe()` smoke test.
#[cfg(all(test, not(target_os = "windows")))]
#[path = "install_probe/tests.rs"]
mod tests;
