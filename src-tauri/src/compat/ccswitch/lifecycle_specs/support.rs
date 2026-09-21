use std::path::Path;

use crate::compat::ccswitch::install_probe::program_lookup::npm_on_path;
use crate::compat::ccswitch::install_probe::{sibling_program, InstalledEntry, LifecycleProbe};
use crate::domain::{AppError, ErrorCode, ToolId};
use crate::platform::command::{AllowedProgram, CommandSpec};

use super::INSTALL_TIMEOUT;

/// An npm command with no owner to anchor to: fresh installs, catalog queries
/// for a foreign owner. PATH is the only locator, which
/// is why `path_env` is injected (upstream `commands/misc.rs:2173`). Windows
/// additionally anchors the PATH `npm.cmd`, because the bare name never starts
/// there (`program_lookup::npm_on_path`); without one the spec stays bare and
/// the executor reports `programNotFound` truthfully.
pub(crate) fn path_npm_spec(args: Vec<String>, probe: &LifecycleProbe) -> CommandSpec {
    let spec = CommandSpec::new(AllowedProgram::Npm, args).with_env_pairs(path_env(probe));
    match npm_on_path(probe) {
        Some(npm) => spec.with_program_path(npm),
        None => spec,
    }
}

pub(crate) fn tool_program(id: ToolId) -> AllowedProgram {
    match id {
        ToolId::ClaudeCode => AllowedProgram::ClaudeCode,
        ToolId::Codex => AllowedProgram::Codex,
        ToolId::OpenCode => AllowedProgram::OpenCode,
        ToolId::GeminiCli => AllowedProgram::GeminiCli,
        ToolId::GrokBuild => AllowedProgram::GrokBuild,
        ToolId::OpenClaw => AllowedProgram::OpenClaw,
        ToolId::Hermes => AllowedProgram::Hermes,
        ToolId::Pi => AllowedProgram::Pi,
        ToolId::KimiCode => AllowedProgram::KimiCode,
        ToolId::DeepSeekDsh => AllowedProgram::DeepSeekDsh,
    }
}

pub(crate) fn unsupported(id: ToolId, code: ErrorCode, action: &str) -> AppError {
    AppError::new(code, "error.tool.actionUnsupported")
        .with_technical(format!("no {action} strategy for {}", id.as_str()))
        .with_remediation("error.remediation.installManually")
}

pub(crate) fn uninstall_strategy_unknown(id: ToolId, entry: &InstalledEntry) -> AppError {
    AppError::new(
        ErrorCode::UninstallFailed,
        "error.tool.uninstallStrategyUnknown",
    )
    .with_technical(format!(
        "{} at {} ({:?}) has no known owner",
        id.as_str(),
        entry.bin_path.display(),
        entry.source
    ))
    .with_remediation("error.remediation.uninstallManually")
}

/// Anchor to the package manager sitting in the same directory as the tool's entry point.
///
/// With `prefix_dir = true`, that directory is put first on PATH: the POSIX launcher of npm is a
/// `#!/usr/bin/env node` script, and calling npm by absolute path alone does not let it find the
/// node next to it (upstream `commands/misc.rs:2633-2652` says exactly this). brew / volta / bun /
/// pnpm are self-contained binaries and do not need it.
pub(crate) fn anchored(
    program: AllowedProgram,
    entry: &InstalledEntry,
    binary_name: &str,
    args: Vec<String>,
    probe: &LifecycleProbe,
    prefix_dir: bool,
) -> Option<CommandSpec> {
    let program_path = sibling_program(&entry.bin_path, binary_name)?;
    let env = if prefix_dir {
        let dir = program_path.parent()?;
        prefixed_path_env(dir, probe)
    } else {
        path_env(probe)
    };
    Some(
        CommandSpec::new(program, args)
            .with_program_path(program_path)
            .with_timeout(INSTALL_TIMEOUT)
            .with_env_pairs(env),
    )
}

pub(crate) fn path_env(probe: &LifecycleProbe) -> Vec<(String, String)> {
    probe.path_env.clone().into_iter().collect()
}

pub(crate) fn prefixed_path_env(dir: &Path, probe: &LifecycleProbe) -> Vec<(String, String)> {
    let base = probe
        .path_env
        .as_ref()
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
    let mut entries = vec![dir.to_path_buf()];
    entries.extend(std::env::split_paths(&base));
    // join_paths only fails when a segment itself contains the separator; in that case fall back to
    // the un-prefixed PATH — the command still runs by absolute path, only the interpreter
    // resolution degrades to the existing behaviour.
    match std::env::join_paths(entries) {
        Ok(joined) => vec![("PATH".to_string(), joined.to_string_lossy().into_owned())],
        Err(_) => path_env(probe),
    }
}

#[cfg(test)]
mod tests {
    use super::path_npm_spec;
    use crate::compat::ccswitch::install_probe::LifecycleProbe;
    use crate::platform::command::AllowedProgram;

    /// The bare `npm` only starts where the OS resolves `npm.cmd` itself.
    /// Windows needs the explicit anchor; POSIX must stay bare so the injected
    /// PATH keeps choosing the interpreter for the npm launcher.
    #[test]
    fn an_ownerless_npm_spec_is_anchored_exactly_where_the_bare_name_would_not_start() {
        let dir = tempfile::tempdir().expect("fake PATH dir");
        let npm = dir.path().join("npm.cmd");
        std::fs::write(&npm, b"@echo off").expect("fake npm.cmd");
        let path = dir.path().to_string_lossy().into_owned();
        let probe = LifecycleProbe {
            entry: None,
            path_env: Some(("PATH".to_string(), path.clone())),
        };

        let spec = path_npm_spec(vec!["view".to_string(), "pkg".to_string()], &probe);

        assert_eq!(spec.program, AllowedProgram::Npm);
        assert_eq!(spec.args, vec!["view", "pkg"]);
        assert_eq!(
            spec.program_path,
            cfg!(target_os = "windows").then_some(npm)
        );
        assert!(spec
            .env
            .iter()
            .any(|(key, value)| key == "PATH" && *value == path));
        spec.validate()
            .expect("an anchored or bare npm spec is valid");
    }
}
