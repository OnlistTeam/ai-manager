#[cfg(any(target_os = "linux", all(test, unix)))]
use std::path::Path;
use std::path::PathBuf;

use crate::domain::AppError;
use crate::platform::{AllowedProgram, CommandSpec, TerminalEnvironment, TerminalLaunchSpec};

use super::invalid_spec;
#[cfg(any(target_os = "linux", all(test, unix)))]
use super::launch_failed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinuxTerminal {
    XdgTerminalExec,
    GnomeTerminal,
    Konsole,
    XTerminalEmulator,
}

impl LinuxTerminal {
    fn program(self) -> AllowedProgram {
        match self {
            Self::XdgTerminalExec => AllowedProgram::XdgTerminalExec,
            Self::GnomeTerminal => AllowedProgram::GnomeTerminal,
            Self::Konsole => AllowedProgram::Konsole,
            Self::XTerminalEmulator => AllowedProgram::XTerminalEmulator,
        }
    }

    fn prefix(self) -> &'static [&'static str] {
        match self {
            Self::XdgTerminalExec => &[],
            Self::GnomeTerminal => &["--"],
            Self::Konsole | Self::XTerminalEmulator => &["-e"],
        }
    }

    #[cfg(any(target_os = "linux", all(test, unix)))]
    fn candidates() -> [Self; 4] {
        [
            Self::XdgTerminalExec,
            Self::GnomeTerminal,
            Self::Konsole,
            Self::XTerminalEmulator,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LinuxTerminalPlan {
    pub(super) command: CommandSpec,
}

/// Linux terminals in this allowlist all accept an executable followed by
/// argv. `/usr/bin/env -C` supplies the working directory and optional PATH
/// without constructing a shell command or temporary script.
pub(super) fn terminal_plan(
    spec: &TerminalLaunchSpec,
    terminal: LinuxTerminal,
    terminal_path: PathBuf,
    env_path: PathBuf,
) -> Result<LinuxTerminalPlan, AppError> {
    spec.validate()?;
    if !matches!(spec.environment, TerminalEnvironment::Native) {
        return Err(invalid_spec("error.commandSpec.terminalTargetInvalid"));
    }

    let tool_path = spec
        .command
        .program_path
        .as_ref()
        .expect("validated native terminal target has a program path");
    let mut wrapper_args = vec![
        "-C".to_string(),
        spec.working_directory.to_string_lossy().into_owned(),
    ];
    if let Some((_, path)) = spec.command.env.iter().find(|(key, _)| key == "PATH") {
        wrapper_args.push(format!("PATH={path}"));
    }
    wrapper_args.push(tool_path.to_string_lossy().into_owned());
    wrapper_args.extend(spec.command.args.iter().cloned());

    let wrapper_sensitive = (1..wrapper_args.len()).collect();
    let wrapper = CommandSpec::new(AllowedProgram::Env, wrapper_args)
        .with_program_path(env_path)
        .with_sensitive_args(wrapper_sensitive);
    wrapper.validate()?;

    let prefix = terminal.prefix();
    let mut args = prefix
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    args.push(
        wrapper
            .program_command_name()
            .to_string_lossy()
            .into_owned(),
    );
    args.extend(wrapper.args.iter().cloned());
    let sensitive = (prefix.len()..args.len()).collect();

    let command = CommandSpec::new(terminal.program(), args)
        .with_program_path(terminal_path)
        .with_sensitive_args(sensitive);
    command.validate()?;
    Ok(LinuxTerminalPlan { command })
}

#[cfg(any(target_os = "linux", all(test, unix)))]
pub(super) async fn launch(spec: TerminalLaunchSpec) -> Result<(), AppError> {
    let env_path = known_executable(AllowedProgram::Env)
        .ok_or_else(|| launch_failed("the fixed Linux env bridge was not found"))?;

    tokio::task::spawn_blocking(move || {
        let mut found = false;
        let mut last_failure = None;
        for terminal in LinuxTerminal::candidates() {
            let Some(path) = known_executable(terminal.program()) else {
                continue;
            };
            found = true;
            let plan = terminal_plan(&spec, terminal, path, env_path.clone())?;
            match spawn_detached(plan) {
                Ok(()) => return Ok(()),
                Err(error) => last_failure = Some(error),
            }
        }

        if let Some(error) = last_failure {
            Err(error)
        } else if found {
            Err(launch_failed("every Linux terminal launch attempt failed"))
        } else {
            Err(launch_failed("no supported Linux terminal was found"))
        }
    })
    .await
    .map_err(|error| launch_failed(format!("terminal task join failed: {error}")))?
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn known_executable(program: AllowedProgram) -> Option<PathBuf> {
    ["/usr/bin", "/bin", "/usr/local/bin"]
        .into_iter()
        .map(|directory| Path::new(directory).join(program.as_str()))
        .find(|path| is_executable(path))
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn spawn_detached(plan: LinuxTerminalPlan) -> Result<(), AppError> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    plan.command.validate()?;
    let mut command = Command::new(plan.command.program_command_name());
    command
        .args(&plan.command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe and detaches the terminal process
    // from the GUI process group before exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| launch_failed(format!("terminal spawn failed: {}", error.kind())))
}
