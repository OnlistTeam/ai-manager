//! Read-only ownership proof for Python-distributed Hermes installations.
//!
//! A launcher path alone is not evidence: the official Hermes installer also
//! writes `~/.local/bin/hermes` while merely using uv to build its private venv.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;

use crate::platform::command::{AllowedProgram, CommandSpec};
use crate::platform::executor::{CommandExecutor, CommandOutput, SystemExecutor};

use super::{HermesInstallOwner, InstallSource, InstalledEntry, LifecycleProbe};

const OWNER_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_INVENTORY_BYTES: usize = 1_048_576;
const MAX_MANAGER_CANDIDATES: usize = 4;
const MAX_LAUNCHER_COPY_BYTES: u64 = 8 * 1024 * 1024;

pub(super) async fn refine(probe: LifecycleProbe) -> LifecycleProbe {
    refine_with_executor(probe, Arc::new(SystemExecutor)).await
}

async fn refine_with_executor(
    mut probe: LifecycleProbe,
    executor: Arc<dyn CommandExecutor>,
) -> LifecycleProbe {
    let Some(entry) = probe
        .entry
        .as_ref()
        // Hermes has no npm package mapping, so generic path prefixes such as
        // `.volta` or `/homebrew/` are not ownership evidence for this tool.
        // A real Cellar formula remains Brew-owned; every other path-only tag
        // may be refined by a receipt without expanding destructive access.
        .filter(|entry| {
            entry.hermes_owner.is_none()
                && !matches!(
                    entry.source,
                    InstallSource::Brew | InstallSource::BrewCask | InstallSource::Native
                )
        })
        .cloned()
    else {
        return probe;
    };

    let uv_paths = manager_candidates(&probe, "uv");
    let pipx_paths = manager_candidates(&probe, "pipx");
    let env = probe.path_env.clone().into_iter().collect::<Vec<_>>();
    let uv_entry = entry.clone();
    let uv_executor = executor.clone();
    let uv = async move {
        join_all(uv_paths.into_iter().map(|program_path| {
            uv_claim(
                program_path,
                uv_entry.clone(),
                env.clone(),
                uv_executor.clone(),
            )
        }))
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
    };
    let pipx_entry = entry.clone();
    let pipx_executor = executor.clone();
    let pipx_env = probe.path_env.clone().into_iter().collect::<Vec<_>>();
    let pipx = async move {
        join_all(pipx_paths.into_iter().map(|program_path| {
            pipx_claims(
                program_path,
                pipx_entry.clone(),
                pipx_env.clone(),
                pipx_executor.clone(),
            )
        }))
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
    };
    let (mut claims, pipx_claims) = tokio::join!(uv, pipx);
    claims.extend(pipx_claims);

    if let Some(owner) = choose_unique_claim(claims) {
        if let Some(selected) = probe.entry.as_mut() {
            selected.source = match &owner {
                HermesInstallOwner::Uv { .. } => InstallSource::UvTool,
                HermesInstallOwner::Pipx { .. } => InstallSource::Pipx,
            };
            selected.hermes_owner = Some(owner);
        }
    }
    probe
}

async fn uv_claim(
    program_path: PathBuf,
    entry: InstalledEntry,
    env: Vec<(String, String)>,
    executor: Arc<dyn CommandExecutor>,
) -> Option<HermesInstallOwner> {
    let stdout = run_inventory(
        executor,
        inventory_spec(
            AllowedProgram::Uv,
            program_path.clone(),
            vec!["tool", "list", "--show-paths"],
            env,
        ),
    )
    .await?;
    uv_inventory_owns(&stdout, &entry).then_some(HermesInstallOwner::Uv { program_path })
}

async fn pipx_claims(
    program_path: PathBuf,
    entry: InstalledEntry,
    env: Vec<(String, String)>,
    executor: Arc<dyn CommandExecutor>,
) -> Vec<HermesInstallOwner> {
    let local_list = run_inventory(
        executor.clone(),
        inventory_spec(
            AllowedProgram::Pipx,
            program_path.clone(),
            vec!["list", "--json"],
            env.clone(),
        ),
    );
    let local_bin = run_inventory(
        executor.clone(),
        inventory_spec(
            AllowedProgram::Pipx,
            program_path.clone(),
            vec!["environment", "--value", "PIPX_BIN_DIR"],
            env.clone(),
        ),
    );
    let global_list = run_inventory(
        executor.clone(),
        inventory_spec(
            AllowedProgram::Pipx,
            program_path.clone(),
            vec!["--global", "list", "--json"],
            env.clone(),
        ),
    );
    let global_bin = run_inventory(
        executor,
        inventory_spec(
            AllowedProgram::Pipx,
            program_path.clone(),
            vec!["--global", "environment", "--value", "PIPX_BIN_DIR"],
            env,
        ),
    );
    let (local_list, local_bin, global_list, global_bin) =
        tokio::join!(local_list, local_bin, global_list, global_bin);

    let mut claims = Vec::new();
    if local_list
        .zip(local_bin)
        .is_some_and(|(inventory, bin)| pipx_inventory_owns(&inventory, &bin, &entry))
    {
        claims.push(HermesInstallOwner::Pipx {
            program_path: program_path.clone(),
            global: false,
        });
    }
    if global_list
        .zip(global_bin)
        .is_some_and(|(inventory, bin)| pipx_inventory_owns(&inventory, &bin, &entry))
    {
        claims.push(HermesInstallOwner::Pipx {
            program_path,
            global: true,
        });
    }
    claims
}

fn inventory_spec(
    program: AllowedProgram,
    program_path: PathBuf,
    args: Vec<&str>,
    env: Vec<(String, String)>,
) -> CommandSpec {
    CommandSpec::new(program, args.into_iter().map(str::to_string).collect())
        .with_program_path(program_path)
        .with_timeout(OWNER_PROBE_TIMEOUT)
        .with_env_pairs(env)
        .with_env("NO_COLOR", "1")
}

async fn run_inventory(executor: Arc<dyn CommandExecutor>, spec: CommandSpec) -> Option<String> {
    let CommandOutput {
        success: true,
        stdout,
        ..
    } = executor.execute(spec).await.ok()?
    else {
        return None;
    };
    (stdout.len() <= MAX_INVENTORY_BYTES).then_some(stdout)
}

fn manager_candidates(probe: &LifecycleProbe, name: &str) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(parent) = probe
        .entry
        .as_ref()
        .and_then(|entry| entry.bin_path.parent())
    {
        directories.push(parent.to_path_buf());
    }
    let search_path = probe
        .path_env
        .as_ref()
        .map(|(_, value)| std::ffi::OsString::from(value))
        .or_else(crate::commands::misc::effective_path_for_tool_detection);
    if let Some(search_path) = search_path {
        directories.extend(std::env::split_paths(&search_path));
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    for directory in directories.into_iter().filter(|path| path.is_absolute()) {
        for candidate in executable_names(&directory, name) {
            if candidate.is_file() && !candidates.iter().any(|seen| paths_match(seen, &candidate)) {
                candidates.push(candidate);
                if candidates.len() == MAX_MANAGER_CANDIDATES {
                    return candidates;
                }
            }
        }
    }
    candidates
}

#[cfg(not(target_os = "windows"))]
fn executable_names(directory: &Path, name: &str) -> Vec<PathBuf> {
    vec![directory.join(name)]
}

#[cfg(target_os = "windows")]
fn executable_names(directory: &Path, name: &str) -> Vec<PathBuf> {
    ["exe", "cmd", "bat"]
        .into_iter()
        .map(|extension| directory.join(format!("{name}.{extension}")))
        .collect()
}

fn choose_unique_claim(claims: Vec<HermesInstallOwner>) -> Option<HermesInstallOwner> {
    let mut selected: Option<HermesInstallOwner> = None;
    for claim in claims {
        match &selected {
            None => selected = Some(claim),
            Some(current) if current == &claim => {}
            Some(_) => return None,
        }
    }
    selected
}

fn uv_inventory_owns(stdout: &str, entry: &InstalledEntry) -> bool {
    if stdout.len() > MAX_INVENTORY_BYTES {
        return false;
    }
    let mut hermes_tool_dir: Option<PathBuf> = None;
    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(raw_entrypoint) = line.strip_prefix("- ") {
            let Some(tool_dir) = hermes_tool_dir.as_ref() else {
                continue;
            };
            let Some((name, exposed)) = trailing_path(raw_entrypoint) else {
                continue;
            };
            if name == "hermes"
                && paths_match(&exposed, &entry.bin_path)
                && uv_environment_matches(tool_dir, entry)
            {
                return true;
            }
            continue;
        }

        hermes_tool_dir = trailing_path(line).and_then(|(header, tool_dir)| {
            let mut fields = header.split_whitespace();
            let package = fields.next()?;
            let version = fields.next()?;
            (python_name_eq(package, "hermes-agent")
                && version.starts_with('v')
                && tool_dir.is_absolute())
            .then_some(tool_dir)
        });
    }
    false
}

fn trailing_path(raw: &str) -> Option<(&str, PathBuf)> {
    let (label, path) = raw.rsplit_once(" (")?;
    let path = path.strip_suffix(')')?;
    let path = PathBuf::from(path);
    (!label.is_empty() && path.is_absolute()).then_some((label, path))
}

fn uv_environment_matches(tool_dir: &Path, entry: &InstalledEntry) -> bool {
    [
        tool_dir.join("bin/hermes"),
        tool_dir.join("Scripts/hermes.exe"),
        tool_dir.join("Scripts/hermes.cmd"),
        tool_dir.join("Scripts/hermes.bat"),
    ]
    .into_iter()
    .any(|internal| launcher_matches(&internal, entry))
}

fn pipx_inventory_owns(stdout: &str, raw_bin_dir: &str, entry: &InstalledEntry) -> bool {
    if stdout.len() > MAX_INVENTORY_BYTES || raw_bin_dir.lines().count() != 1 {
        return false;
    }
    let bin_dir = PathBuf::from(raw_bin_dir.trim());
    if !bin_dir.is_absolute()
        || !entry
            .bin_path
            .parent()
            .is_some_and(|parent| paths_match(parent, &bin_dir))
        || !launcher_name_is(&entry.bin_path, "hermes")
    {
        return false;
    }
    let Ok(root) = serde_json::from_str::<serde_json::Value>(stdout) else {
        return false;
    };
    if !root
        .get("pipx_spec_version")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|version| !version.is_empty() && version.len() <= 16)
    {
        return false;
    }
    let Some(venvs) = root.get("venvs").and_then(serde_json::Value::as_object) else {
        return false;
    };
    venvs.iter().any(|(environment, record)| {
        if !python_name_eq(environment, "hermes-agent") {
            return false;
        }
        let Some(metadata) = record.get("metadata") else {
            return false;
        };
        if metadata
            .get("exposure_enabled")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            return false;
        }
        let Some(package) = metadata.get("main_package") else {
            return false;
        };
        if package
            .get("package")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|name| !python_name_eq(name, "hermes-agent"))
            || package
                .get("include_apps")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
            || package.get("suffix").and_then(serde_json::Value::as_str) != Some("")
        {
            return false;
        }
        let Some(apps) = package.get("apps").and_then(serde_json::Value::as_array) else {
            return false;
        };
        let Some(paths) = package
            .get("app_paths")
            .and_then(serde_json::Value::as_array)
        else {
            return false;
        };
        apps.iter().zip(paths).any(|(app, path)| {
            app.as_str() == Some("hermes")
                && json_path(path).is_some_and(|internal| launcher_matches(&internal, entry))
        })
    })
}

fn json_path(value: &serde_json::Value) -> Option<PathBuf> {
    let raw = value.as_str().or_else(|| {
        let object = value.as_object()?;
        if object.get("__type__")?.as_str()? != "Path" {
            return None;
        }
        object.get("__Path__")?.as_str()
    })?;
    let path = PathBuf::from(raw);
    path.is_absolute().then_some(path)
}

fn launcher_matches(internal: &Path, entry: &InstalledEntry) -> bool {
    internal.is_file()
        && (same_file_or_bounded_copy(internal, &entry.real_path)
            || same_file_or_bounded_copy(internal, &entry.bin_path))
}

fn same_file_or_bounded_copy(left: &Path, right: &Path) -> bool {
    if std::fs::canonicalize(left)
        .ok()
        .zip(std::fs::canonicalize(right).ok())
        .is_some_and(|(left, right)| left == right)
    {
        return true;
    }
    let Ok(left_meta) = std::fs::metadata(left) else {
        return false;
    };
    let Ok(right_meta) = std::fs::metadata(right) else {
        return false;
    };
    let size = left_meta.len();
    size > 0
        && size == right_meta.len()
        && size <= MAX_LAUNCHER_COPY_BYTES
        && std::fs::read(left)
            .ok()
            .zip(std::fs::read(right).ok())
            .is_some_and(|(left, right)| left == right)
}

fn paths_match(left: &Path, right: &Path) -> bool {
    std::fs::canonicalize(left)
        .ok()
        .zip(std::fs::canonicalize(right).ok())
        .is_some_and(|(left, right)| left == right)
        || normalized_path(left) == normalized_path(right)
}

fn normalized_path(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if cfg!(target_os = "windows") {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

fn launcher_name_is(path: &Path, expected: &str) -> bool {
    path.file_stem()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(expected))
}

fn python_name_eq(left: &str, right: &str) -> bool {
    python_name(left) == python_name(right)
}

fn python_name(raw: &str) -> String {
    let mut normalized = String::with_capacity(raw.len());
    let mut separator = false;
    for character in raw.chars() {
        if matches!(character, '-' | '_' | '.') {
            if !separator {
                normalized.push('-');
            }
            separator = true;
        } else {
            normalized.push(character.to_ascii_lowercase());
            separator = false;
        }
    }
    normalized
}

#[cfg(test)]
mod tests;
