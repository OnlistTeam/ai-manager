//! Mapping from each install source to a `CommandSpec`, plus the planning logic of the
//! three lifecycle actions. `lifecycle.rs` keeps the product entry points; this file is the
//! implementation body, split out to satisfy the per-file size limit.

use std::time::Duration;

use crate::commands::misc::{
    npm_package_for, official_update_args, prefers_official_update, LifecycleCommandShell,
};
use crate::compat::ccswitch::install_probe::{InstallSource, InstalledEntry, LifecycleProbe};
use crate::compat::ccswitch::lifecycle::{
    CLAUDE_INSTALL_SCRIPT, CODEX_INSTALL_SCRIPT, GROK_INSTALL_SCRIPT, HERMES_INSTALL_SCRIPT,
    KIMI_INSTALL_SCRIPT, OPENCODE_INSTALL_SCRIPT,
};
use crate::compat::ccswitch::tool_paths::native_app_paths;
use crate::compat::ccswitch::tools::tool_id_to_cli_name;
use crate::domain::{validate_tool_version, AppError, ErrorCode, ToolId};
use crate::platform::command::{AllowedProgram, CommandSpec};
use crate::platform::plan::{LifecycleAttempt, LifecyclePlan, LifecycleStep, UninstallPlan};

pub(crate) mod hermes_python;
mod support;
pub(crate) use support::{
    anchored, path_env, path_npm_spec, tool_program, uninstall_strategy_unknown, unsupported,
};

/// Install/update downloads a whole package, and 5 minutes is not enough on a slow network; uninstall is a local operation.
pub(crate) const INSTALL_TIMEOUT: Duration = Duration::from_secs(900);
pub(crate) const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(300);
/// The verified remove/reinstall proof is POSIX-only. Windows reports involve
/// locked executables and stale staging directories, so the same plan is not
/// authoritative there (openai/codex#19824, #21872).
pub(crate) const CODEX_NPM_REPAIR_SUPPORTED: bool = cfg!(not(target_os = "windows"));
const GROK_NPM_PACKAGE: &str = "@xai-official/grok";

fn automatic_package_target(package: &str) -> String {
    // Grok's `latest` tag has previously remained on 0.1.4 while newer stable
    // 1.x releases were already published. npm's `*` range selects the highest
    // stable release, which matches the version the manager recommends.
    let channel = if package == GROK_NPM_PACKAGE {
        "*"
    } else {
        "latest"
    };
    format!("{package}@{channel}")
}

/// Install tries the official installer first and then falls back to npm, keeping the two failure modes independent.
pub(crate) fn compute_install_plan(
    id: ToolId,
    probe: &LifecycleProbe,
) -> Result<LifecyclePlan, AppError> {
    let app_type = tool_id_to_cli_name(id);
    let mut attempts = Vec::new();
    if let Some(spec) = official_installer_spec(id, probe) {
        attempts.push(LifecycleAttempt::single(spec));
    }
    if let Some(package) = npm_package_for(app_type) {
        attempts.push(LifecycleAttempt::single(bare_npm_install(package, probe)));
    }

    let plan = LifecyclePlan { attempts };
    if plan.is_empty() {
        return Err(unsupported(id, ErrorCode::InstallFailed, "install"));
    }
    Ok(plan)
}

/// Implementation body of `lifecycle::update_plan`. The decision order matches the upstream
/// `anchored_command_from_paths`: no default installation -> static fallback; a corrupted
/// codex platform package (`the_codex_repair_gate_agrees_with_upstream` compares against
/// upstream) -> self-repair; native installer -> official self-update; brew -> upgrade the
/// formula; otherwise follow the preference, official first and package manager second.
/// Note: for an Unmanaged source (e.g. a binary dropped into /usr/local/bin by hand),
/// upstream falls back to static npm, but this layer deliberately refuses to return empty
/// attempts, so npm cannot install a second copy shadowed by PATH (an illusion of success
/// while the binary the user runs was never updated).
/// This is symmetric with the uninstall side's refusal for Unmanaged sources and is safer
/// for non-technical users.
pub(crate) fn compute_update_plan(
    id: ToolId,
    probe: &LifecycleProbe,
    target_version: &str,
) -> Result<LifecyclePlan, AppError> {
    let target = validate_tool_version(target_version)?;
    let app_type = tool_id_to_cli_name(id);
    let Some(entry) = probe.entry.as_ref() else {
        return static_update_plan(id, probe, &target);
    };

    if id == ToolId::Codex && !entry.runnable && entry.source == InstallSource::NodeManagerNpm {
        return codex_repair_plan(entry, probe)
            .ok_or_else(|| unsupported(id, ErrorCode::UpdateFailed, "update"));
    }

    // A proven Python tool owner must remain the update channel. Official
    // checkout/venv launchers stay Unmanaged and keep Hermes' own updater.
    if id == ToolId::Hermes {
        if matches!(entry.source, InstallSource::UvTool | InstallSource::Pipx) {
            return hermes_python::update(entry, probe)
                .map(LifecyclePlan::single)
                .ok_or_else(|| unsupported(id, ErrorCode::UpdateFailed, "update"));
        }
        let spec = official_update_spec(id, app_type, entry, probe)
            .ok_or_else(|| unsupported(id, ErrorCode::UpdateFailed, "update"))?;
        return Ok(LifecyclePlan::single(spec));
    }

    if entry.source == InstallSource::Native {
        let spec = official_update_spec(id, app_type, entry, probe)
            .ok_or_else(|| unsupported(id, ErrorCode::UpdateFailed, "update"))?;
        let mut attempts = vec![LifecycleAttempt::single(spec)];
        // Re-running the same official installer is the owner-preserving repair
        // documented by native distributions. Never cross over to npm here:
        // that would leave a second, PATH-dependent installation behind.
        // Where the registry can supply the official layout (ADR-0033) the
        // installer retry only reaches the same blocked host again, so the
        // adapter supplies the layout after this single attempt instead.
        if !crate::compat::ccswitch::native_supply::supplies(id) {
            if let Some(installer) = official_installer_spec(id, probe) {
                attempts.push(LifecycleAttempt::single(installer));
            }
        }
        return Ok(LifecyclePlan { attempts });
    }

    let package_attempt = package_manager_update(entry, probe, &target);
    if matches!(entry.source, InstallSource::Brew | InstallSource::BrewCask) {
        return package_attempt
            .map(LifecyclePlan::single)
            .ok_or_else(|| unsupported(id, ErrorCode::UpdateFailed, "update"));
    }

    let mut attempts = Vec::new();
    if prefers_official(app_type) {
        if let Some(spec) = official_update_spec(id, app_type, entry, probe) {
            attempts.push(LifecycleAttempt::single(spec));
        }
    }
    if let Some(spec) = package_attempt {
        attempts.push(LifecycleAttempt::single(spec));
    }

    let plan = LifecyclePlan { attempts };
    if plan.is_empty() {
        return Err(unsupported(id, ErrorCode::UpdateFailed, "update"));
    }
    Ok(plan)
}

/// Implementation body of `lifecycle::uninstall_plan`. Two mutually exclusive paths: what a
/// package manager installed goes through its command, and what an official installer
/// installed can only be deleted file by file (upstream has no uninstall; this knowledge is
/// new in this product).
pub(crate) fn compute_uninstall_plan(
    id: ToolId,
    probe: &LifecycleProbe,
) -> Result<UninstallPlan, AppError> {
    let Some(entry) = probe.entry.as_ref() else {
        return Err(
            AppError::new(ErrorCode::ToolNotFound, "error.tool.notInstalled")
                .with_technical(format!("no default install for {}", id.as_str())),
        );
    };

    if entry.source == InstallSource::Native {
        let paths = native_app_paths(id, entry);
        if paths.is_empty() {
            return Err(uninstall_strategy_unknown(id, entry));
        }
        return Ok(UninstallPlan::RemovePaths(paths));
    }

    let spec = package_manager_uninstall(entry, probe)
        .ok_or_else(|| uninstall_strategy_unknown(id, entry))?;
    Ok(UninstallPlan::Command(LifecyclePlan::single(spec)))
}

/// POSIX official installers; Windows Grok/Hermes specs live in `lifecycle_windows`.
pub(crate) fn official_install_script(id: ToolId) -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        return None;
    }
    match id {
        ToolId::ClaudeCode => Some(CLAUDE_INSTALL_SCRIPT),
        ToolId::Codex => Some(CODEX_INSTALL_SCRIPT),
        ToolId::OpenCode => Some(OPENCODE_INSTALL_SCRIPT),
        ToolId::GrokBuild => Some(GROK_INSTALL_SCRIPT),
        ToolId::Hermes => Some(HERMES_INSTALL_SCRIPT),
        ToolId::KimiCode => Some(KIMI_INSTALL_SCRIPT),
        ToolId::GeminiCli | ToolId::OpenClaw | ToolId::Pi | ToolId::DeepSeekDsh => None,
    }
}

/// The fixed environment the official self-update needs. Codex's `update` waits for
/// confirmation outside an interactive terminal, and `CODEX_NON_INTERACTIVE=1` is the silent
/// switch its installer documentation gives.
fn official_update_env(id: ToolId) -> Vec<(String, String)> {
    match id {
        ToolId::Codex => vec![("CODEX_NON_INTERACTIVE".to_string(), "1".to_string())],
        ToolId::ClaudeCode
        | ToolId::OpenCode
        | ToolId::GeminiCli
        | ToolId::GrokBuild
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => Vec::new(),
    }
}

fn official_installer_spec(id: ToolId, probe: &LifecycleProbe) -> Option<CommandSpec> {
    official_install_script(id)
        .map(|script| {
            CommandSpec::bash_script(script)
                .with_timeout(INSTALL_TIMEOUT)
                .with_env_pairs(path_env(probe))
        })
        .or_else(|| {
            crate::compat::ccswitch::lifecycle_windows::official_installer_spec(
                id,
                path_env(probe),
                INSTALL_TIMEOUT,
            )
        })
}

/// Fallback when no default installation can be detected (not installed, or several with no
/// determinable default), equivalent to the upstream `static_fallback_command`:
/// `<tool> update || npm i -g <pkg>@<target>`.
/// Upstream installs `@latest`; this layer installs the verified recommended version, matching the target of `verify`.
pub(crate) fn static_update_plan(
    id: ToolId,
    probe: &LifecycleProbe,
    target: &str,
) -> Result<LifecyclePlan, AppError> {
    let app_type = tool_id_to_cli_name(id);
    let mut attempts = Vec::new();
    if id == ToolId::Hermes || prefers_official(app_type) {
        if let Some(args) = official_update_args(app_type) {
            attempts.push(LifecycleAttempt::single(
                CommandSpec::new(tool_program(id), split_args(args))
                    .with_timeout(INSTALL_TIMEOUT)
                    .with_env_pairs(path_env(probe)),
            ));
        }
    }
    if let Some(package) = npm_package_for(app_type) {
        attempts.push(LifecycleAttempt::single(
            path_npm_spec(
                vec![
                    "install".to_string(),
                    "-g".to_string(),
                    format!("{package}@{target}"),
                ],
                probe,
            )
            .with_timeout(INSTALL_TIMEOUT),
        ));
    }

    let plan = LifecyclePlan { attempts };
    if plan.is_empty() {
        return Err(unsupported(id, ErrorCode::UpdateFailed, "update"));
    }
    Ok(plan)
}

pub(crate) fn official_update_spec(
    id: ToolId,
    app_type: &str,
    entry: &InstalledEntry,
    probe: &LifecycleProbe,
) -> Option<CommandSpec> {
    let args = official_update_args(app_type)?;
    Some(
        CommandSpec::new(tool_program(id), split_args(args))
            .with_program_path(entry.bin_path.clone())
            .with_timeout(INSTALL_TIMEOUT)
            // The official self-update needs no PATH prefix (the executable is a
            // self-contained binary), but it still needs the GUI PATH fix — upstream recorded
            // cases where `grok update` spawns npm internally.
            .with_env_pairs(path_env(probe))
            .with_env_pairs(official_update_env(id)),
    )
}

/// The recommended update version has already been validated by the checked preview, so a
/// package-manager-owned installation installs that exact version rather than `@latest` —
/// when the local version is ahead of latest, upstream recommends a prerelease tag (such as
/// Claude's `next`), and installing `@latest` would downgrade the user and never pass the
/// version re-check.
pub(crate) fn package_manager_update(
    entry: &InstalledEntry,
    probe: &LifecycleProbe,
    target: &str,
) -> Option<CommandSpec> {
    match entry.source {
        InstallSource::Brew => anchored(
            AllowedProgram::Brew,
            entry,
            "brew",
            vec!["upgrade".to_string(), entry.brew_formula.clone()?],
            probe,
            false,
        ),
        InstallSource::BrewCask => anchored(
            AllowedProgram::Brew,
            entry,
            "brew",
            vec![
                "upgrade".to_string(),
                "--cask".to_string(),
                entry.brew_formula.clone()?,
            ],
            probe,
            false,
        ),
        InstallSource::NodeManagerNpm
        | InstallSource::Pnpm
        | InstallSource::Bun
        | InstallSource::Volta => package_manager_install_version(entry, probe, target),
        InstallSource::UvTool | InstallSource::Pipx => hermes_python::update(entry, probe),
        InstallSource::Native | InstallSource::Unmanaged => None,
    }
}

/// `<manager> install <pkg>@<version>` anchored to the current owner; update (target from the
/// checked preview) and changeVersion (target from the version directory) share the same
/// table. `version` must already have passed `validate_tool_version`.
pub(crate) fn package_manager_install_version(
    entry: &InstalledEntry,
    probe: &LifecycleProbe,
    version: &str,
) -> Option<CommandSpec> {
    let package_version = format!("{}@{version}", entry.npm_package?);
    match entry.source {
        InstallSource::NodeManagerNpm => anchored(
            AllowedProgram::Npm,
            entry,
            "npm",
            vec!["install".to_string(), "-g".to_string(), package_version],
            probe,
            true,
        ),
        InstallSource::Pnpm => anchored(
            AllowedProgram::Pnpm,
            entry,
            "pnpm",
            vec!["add".to_string(), "-g".to_string(), package_version],
            probe,
            false,
        ),
        InstallSource::Bun => anchored(
            AllowedProgram::Bun,
            entry,
            "bun",
            vec!["add".to_string(), "-g".to_string(), package_version],
            probe,
            false,
        ),
        InstallSource::Volta => anchored(
            AllowedProgram::Volta,
            entry,
            "volta",
            vec!["install".to_string(), package_version],
            probe,
            false,
        ),
        InstallSource::Brew
        | InstallSource::BrewCask
        | InstallSource::UvTool
        | InstallSource::Pipx
        | InstallSource::Native
        | InstallSource::Unmanaged => None,
    }
}

pub(crate) fn package_manager_uninstall(
    entry: &InstalledEntry,
    probe: &LifecycleProbe,
) -> Option<CommandSpec> {
    let spec = match entry.source {
        InstallSource::Brew => anchored(
            AllowedProgram::Brew,
            entry,
            "brew",
            vec!["uninstall".to_string(), entry.brew_formula.clone()?],
            probe,
            false,
        ),
        InstallSource::BrewCask => anchored(
            AllowedProgram::Brew,
            entry,
            "brew",
            vec![
                "uninstall".to_string(),
                "--cask".to_string(),
                entry.brew_formula.clone()?,
            ],
            probe,
            false,
        ),
        InstallSource::Volta => anchored(
            AllowedProgram::Volta,
            entry,
            "volta",
            vec!["uninstall".to_string(), entry.npm_package?.to_string()],
            probe,
            false,
        ),
        InstallSource::Bun => anchored(
            AllowedProgram::Bun,
            entry,
            "bun",
            vec![
                "remove".to_string(),
                "-g".to_string(),
                entry.npm_package?.to_string(),
            ],
            probe,
            false,
        ),
        InstallSource::Pnpm => anchored(
            AllowedProgram::Pnpm,
            entry,
            "pnpm",
            vec![
                "remove".to_string(),
                "-g".to_string(),
                entry.npm_package?.to_string(),
            ],
            probe,
            false,
        ),
        InstallSource::NodeManagerNpm => anchored(
            AllowedProgram::Npm,
            entry,
            "npm",
            vec![
                "uninstall".to_string(),
                "-g".to_string(),
                entry.npm_package?.to_string(),
            ],
            probe,
            true,
        ),
        InstallSource::UvTool | InstallSource::Pipx => hermes_python::uninstall(entry, probe),
        InstallSource::Native | InstallSource::Unmanaged => None,
    }?;
    Some(spec.with_timeout(UNINSTALL_TIMEOUT))
}

/// Self-repair for a corrupted codex platform distribution package (the full rationale is in
/// upstream `commands/misc.rs:2750`): with the platform binary missing, a plain
/// `npm i -g @latest` is a no-op and only uninstall+install fixes it. Limited to node manager
/// sources that anchor to a sibling npm, gated before the call by
/// `lifecycle.rs::update_plan` using the `entry.runnable`/`entry.source` criteria (compared
/// case by case against upstream in `the_codex_repair_gate_agrees_with_upstream`).
/// `ignore_failure: true` corresponds to the upstream `uninstall … || true`: a failed
/// uninstall (e.g. silently returning non-zero for a half-corrupted package) must not abort
/// the install that follows.
pub(crate) fn codex_repair_plan(
    entry: &InstalledEntry,
    probe: &LifecycleProbe,
) -> Option<LifecyclePlan> {
    if !CODEX_NPM_REPAIR_SUPPORTED {
        return None;
    }
    let package = entry.npm_package?;
    let remove = anchored(
        AllowedProgram::Npm,
        entry,
        "npm",
        vec![
            "uninstall".to_string(),
            "-g".to_string(),
            package.to_string(),
        ],
        probe,
        true,
    )?;
    let install = anchored(
        AllowedProgram::Npm,
        entry,
        "npm",
        vec![
            "install".to_string(),
            "-g".to_string(),
            automatic_package_target(package),
        ],
        probe,
        true,
    )?;
    Some(LifecyclePlan {
        attempts: vec![LifecycleAttempt {
            steps: vec![
                LifecycleStep {
                    spec: remove,
                    ignore_failure: true,
                },
                LifecycleStep {
                    spec: install,
                    ignore_failure: false,
                },
            ],
        }],
    })
}

/// The install side has no anchor at all (nothing exists yet), so npm can only be found through PATH; see `path_npm_spec`.
pub(crate) fn bare_npm_install(package: &str, probe: &LifecycleProbe) -> CommandSpec {
    path_npm_spec(
        vec![
            "install".to_string(),
            "-g".to_string(),
            automatic_package_target(package),
        ],
        probe,
    )
    .with_timeout(INSTALL_TIMEOUT)
}

/// The official self-update needs no PATH prefix (the executable is a self-contained
/// binary), but it still needs the GUI PATH fix — `grok update`, for instance, spawns npm
/// internally.
pub(crate) fn prefers_official(app_type: &str) -> bool {
    #[cfg(target_os = "windows")]
    let shell = LifecycleCommandShell::WindowsBatch;
    #[cfg(not(target_os = "windows"))]
    let shell = LifecycleCommandShell::Posix;
    prefers_official_update(app_type, shell)
}

/// Split the fixed upstream `update` / `upgrade` subcommand into argv, with no interpolation.
fn split_args(args: &str) -> Vec<String> {
    args.split_whitespace().map(str::to_string).collect()
}
