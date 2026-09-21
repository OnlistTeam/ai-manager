use std::path::PathBuf;

use crate::commands::misc::wsl_distro_for_tool;
use crate::compat::ccswitch::install_probe::{probe, LifecycleProbe};
use crate::compat::ccswitch::lifecycle_specs::tool_program;
use crate::compat::ccswitch::tools::tool_id_to_cli_name;
use crate::domain::{AppError, ErrorCode, ToolId};
use crate::platform::{CommandSpec, TerminalLaunchSpec};

/// Re-probe the install location, then build the terminal launch spec; an executable path handed back by the renderer is never accepted.
pub async fn launch_spec(
    id: ToolId,
    working_directory: PathBuf,
) -> Result<TerminalLaunchSpec, AppError> {
    let distro = wsl_distro_for_tool(tool_id_to_cli_name(id));
    if distro.is_some() {
        return launch_spec_from_probe(id, working_directory, LifecycleProbe::default(), distro);
    }
    launch_spec_from_probe(id, working_directory, probe(id).await?, None)
}

fn launch_spec_from_probe(
    id: ToolId,
    working_directory: PathBuf,
    probed: LifecycleProbe,
    wsl_distro: Option<String>,
) -> Result<TerminalLaunchSpec, AppError> {
    let program = tool_program(id);
    if let Some(distro) = wsl_distro {
        return Ok(TerminalLaunchSpec::wsl(
            working_directory,
            distro,
            CommandSpec::new(program, launch_args(id)),
        ));
    }

    let entry = probed.entry.ok_or_else(|| {
        AppError::new(ErrorCode::LaunchFailed, "error.tool.notInstalled")
            .with_remediation("error.remediation.installManually")
    })?;
    if !entry.runnable {
        return Err(
            AppError::new(ErrorCode::LaunchFailed, "error.tool.launchFailed")
                .with_technical("the detected tool entry is not runnable")
                .with_remediation("error.remediation.retryOrViewDetails"),
        );
    }

    let mut command = CommandSpec::new(program, launch_args(id)).with_program_path(entry.bin_path);
    if let Some((key, value)) = probed.path_env {
        command = command.with_env(key, value);
    }
    let spec = TerminalLaunchSpec::native(working_directory, command);
    spec.validate()?;
    Ok(spec)
}

fn launch_args(id: ToolId) -> Vec<String> {
    match id {
        // Bare `dsh` requires a profile; `web` is the documented interactive
        // entry point and the command shown by DeepSeek's official README.
        ToolId::DeepSeekDsh => vec!["web".to_string()],
        ToolId::ClaudeCode
        | ToolId::Codex
        | ToolId::OpenCode
        | ToolId::GeminiCli
        | ToolId::GrokBuild
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::launch_spec_from_probe;
    use crate::compat::ccswitch::install_probe::{InstallSource, InstalledEntry, LifecycleProbe};
    use crate::domain::{ErrorCode, ToolId};
    use crate::platform::{AllowedProgram, TerminalEnvironment};

    fn entry(name: &str, runnable: bool) -> InstalledEntry {
        let path = std::env::temp_dir().join(name);
        InstalledEntry {
            bin_path: path.clone(),
            real_path: path,
            source: InstallSource::Unmanaged,
            brew_formula: None,
            runnable,
            npm_package: None,
            hermes_owner: None,
        }
    }

    #[test]
    fn native_launches_are_anchored_to_the_probed_entry_and_keep_path_data() {
        let cwd = std::env::temp_dir().join("project");
        let spec = launch_spec_from_probe(
            ToolId::Codex,
            cwd.clone(),
            LifecycleProbe {
                entry: Some(entry("codex", true)),
                path_env: Some(("PATH".to_string(), "/custom/bin".to_string())),
            },
            None,
        )
        .expect("native launch spec");

        assert_eq!(spec.working_directory, cwd);
        assert_eq!(spec.environment, TerminalEnvironment::Native);
        assert_eq!(
            spec.command.program_path,
            Some(std::env::temp_dir().join("codex"))
        );
        assert_eq!(
            spec.command.env,
            vec![("PATH".to_string(), "/custom/bin".to_string())]
        );
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn absent_and_broken_native_entries_fail_before_terminal_handoff() {
        let missing = launch_spec_from_probe(
            ToolId::ClaudeCode,
            std::env::temp_dir(),
            LifecycleProbe::default(),
            None,
        )
        .expect_err("missing install fails");
        assert_eq!(missing.code, ErrorCode::LaunchFailed);
        assert_eq!(missing.message_key, "error.tool.notInstalled");

        let broken = launch_spec_from_probe(
            ToolId::ClaudeCode,
            std::env::temp_dir(),
            LifecycleProbe {
                entry: Some(entry("claude", false)),
                path_env: None,
            },
            None,
        )
        .expect_err("broken install fails");
        assert_eq!(broken.code, ErrorCode::LaunchFailed);
        assert_eq!(broken.message_key, "error.tool.launchFailed");
    }

    #[test]
    fn wsl_launches_use_only_the_allowlisted_tool_and_validated_distro() {
        let spec = launch_spec_from_probe(
            ToolId::OpenCode,
            std::env::temp_dir(),
            LifecycleProbe::default(),
            Some("Ubuntu-24.04".to_string()),
        )
        .expect("WSL launch spec");
        assert_eq!(spec.command.program, AllowedProgram::OpenCode);
        assert_eq!(spec.command.program_path, None);
        assert_eq!(
            spec.environment,
            TerminalEnvironment::Wsl {
                distro: "Ubuntu-24.04".to_string()
            }
        );
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn all_tool_ids_map_to_the_same_closed_terminal_program_allowlist() {
        let expected = [
            (ToolId::ClaudeCode, AllowedProgram::ClaudeCode, "claude"),
            (ToolId::Codex, AllowedProgram::Codex, "codex"),
            (ToolId::OpenCode, AllowedProgram::OpenCode, "opencode"),
            (ToolId::GeminiCli, AllowedProgram::GeminiCli, "gemini"),
            (ToolId::GrokBuild, AllowedProgram::GrokBuild, "grok"),
            (ToolId::OpenClaw, AllowedProgram::OpenClaw, "openclaw"),
            (ToolId::Hermes, AllowedProgram::Hermes, "hermes"),
            (ToolId::Pi, AllowedProgram::Pi, "pi"),
            (ToolId::KimiCode, AllowedProgram::KimiCode, "kimi"),
            (ToolId::DeepSeekDsh, AllowedProgram::DeepSeekDsh, "dsh"),
        ];
        assert_eq!(expected.len(), ToolId::ALL.len());
        for (id, program, name) in expected {
            let spec = launch_spec_from_probe(
                id,
                std::env::temp_dir(),
                LifecycleProbe {
                    entry: Some(entry(name, true)),
                    path_env: None,
                },
                None,
            )
            .expect("launch spec");
            assert_eq!(spec.command.program, program);
            assert_eq!(
                spec.command.args,
                if id == ToolId::DeepSeekDsh {
                    vec!["web"]
                } else {
                    Vec::<&str>::new()
                }
            );
        }
    }
}
