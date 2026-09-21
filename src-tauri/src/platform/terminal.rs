use futures::future::BoxFuture;
#[cfg(any(target_os = "windows", test))]
use std::path::Path;
use std::path::PathBuf;
#[cfg(any(target_os = "macos", target_os = "windows", test))]
use std::time::Duration;

use crate::domain::{AppError, ErrorCode, TerminalAppId};
#[cfg(any(target_os = "macos", target_os = "windows", test))]
use crate::platform::command::AllowedProgram;
use crate::platform::command::CommandSpec;
#[cfg(any(target_os = "macos", target_os = "windows", test))]
use crate::platform::executor::{CommandExecutor, SystemExecutor};
use crate::platform::Platform;

#[cfg(any(target_os = "linux", test))]
#[path = "terminal/linux.rs"]
mod linux;

#[cfg(any(target_os = "macos", test))]
#[path = "terminal/macos.rs"]
mod macos;

/// Only argv carries paths; the script body is a compile-time constant and never interpolates the directory the user chose.
#[cfg(any(target_os = "macos", test))]
const MACOS_TERMINAL_SCRIPT: &str = r#"on run argv
    set project_path to item 1 of argv
    set tool_path to item 2 of argv
    set path_value to item 3 of argv
    set launch_command to "cd " & quoted form of project_path
    if path_value is not "" then
        set launch_command to launch_command & " && export PATH=" & quoted form of path_value
    end if
    set launch_command to launch_command & " && exec " & quoted form of tool_path
    if (count of argv) > 3 then
        repeat with argument_index from 4 to count of argv
            set launch_command to launch_command & " " & quoted form of (item argument_index of argv)
        end repeat
    end if
    set was_running to application "Terminal" is running
    tell application "Terminal"
        if was_running then
            activate
            do script launch_command
        else
            launch
            do script launch_command
            activate
        end if
    end tell
end run"#;

/// The tool name is injected through a separate argv of the WSL `env`, and the inner script only reads allowlisted values.
#[cfg(any(target_os = "windows", test))]
const WSL_TERMINAL_SCRIPT: &str =
    r#"exec "${SHELL:-sh}" -lic 'exec "$AI_MANAGER_TOOL" "$@"' ai-manager-session "$@""#;
#[cfg(any(target_os = "macos", target_os = "windows", test))]
const TERMINAL_BRIDGE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TERMINAL_ARGS: usize = 16;
const MAX_TERMINAL_ARG_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEnvironment {
    Native,
    Wsl { distro: String },
}

/// The complete launch request the application/adapter hands to the platform layer. `command` is
/// the product CLI to run in an interactive terminal, not an arbitrary shell string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLaunchSpec {
    pub command: CommandSpec,
    pub working_directory: PathBuf,
    pub environment: TerminalEnvironment,
    /// Which terminal the user chose to take over. None = never chosen, so the platform default is
    /// used; if the chosen terminal has been uninstalled, the platform layer falls back to the
    /// system terminal instead of failing the launch.
    pub terminal_app: Option<TerminalAppId>,
}

impl TerminalLaunchSpec {
    pub fn native(working_directory: PathBuf, command: CommandSpec) -> Self {
        Self {
            command,
            working_directory,
            environment: TerminalEnvironment::Native,
            terminal_app: None,
        }
    }

    pub fn with_terminal_app(mut self, terminal_app: Option<TerminalAppId>) -> Self {
        self.terminal_app = terminal_app;
        self
    }

    pub fn wsl(
        working_directory: PathBuf,
        distro: impl Into<String>,
        command: CommandSpec,
    ) -> Self {
        Self {
            command,
            working_directory,
            environment: TerminalEnvironment::Wsl {
                distro: distro.into(),
            },
            terminal_app: None,
        }
    }

    pub fn validate(&self) -> Result<(), AppError> {
        self.command.validate()?;
        if !self.working_directory.is_absolute() {
            return Err(invalid_spec(
                "error.commandSpec.terminalWorkingDirectoryNotAbsolute",
            ));
        }
        if !self.command.program.is_tool()
            || self.command.args.len() > MAX_TERMINAL_ARGS
            || self
                .command
                .args
                .iter()
                .any(|argument| argument.len() > MAX_TERMINAL_ARG_BYTES)
        {
            return Err(invalid_spec("error.commandSpec.terminalTargetInvalid"));
        }
        if self
            .command
            .env
            .iter()
            .any(|(key, _)| key.as_str() != "PATH")
        {
            return Err(invalid_spec("error.commandSpec.terminalTargetInvalid"));
        }

        match &self.environment {
            TerminalEnvironment::Native if self.command.program_path.is_none() => {
                Err(invalid_spec("error.commandSpec.terminalTargetNotAnchored"))
            }
            TerminalEnvironment::Native => Ok(()),
            TerminalEnvironment::Wsl { distro }
                if self.command.program_path.is_some() || !valid_wsl_distro(distro) =>
            {
                Err(invalid_spec("error.commandSpec.wslDistroInvalid"))
            }
            TerminalEnvironment::Wsl { .. } => Ok(()),
        }
    }
}

fn invalid_spec(message_key: &'static str) -> AppError {
    AppError::new(ErrorCode::Internal, message_key)
}

fn launch_failed(technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::LaunchFailed, "error.tool.launchFailed")
        .with_technical(technical)
        .with_remediation("error.remediation.openToolManually")
}

/// Which terminals installed on this machine can take over. Only macOS lets the user choose; other
/// platforms probe in a fixed order, and an empty result means the UI should not show a picker.
pub fn installed_terminals() -> Vec<TerminalAppId> {
    #[cfg(target_os = "macos")]
    {
        macos::installed()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

pub trait TerminalLauncher: Send + Sync {
    fn launch(&self, spec: TerminalLaunchSpec) -> BoxFuture<'static, Result<(), AppError>>;
}

pub struct SystemTerminalLauncher;

impl TerminalLauncher for SystemTerminalLauncher {
    fn launch(&self, spec: TerminalLaunchSpec) -> BoxFuture<'static, Result<(), AppError>> {
        Box::pin(async move {
            spec.validate()?;
            match Platform::current() {
                Platform::MacOs => launch_macos(spec).await,
                Platform::Windows => launch_windows(spec).await,
                Platform::Linux => launch_linux(spec).await,
                Platform::Unknown => Err(AppError::new(
                    ErrorCode::LaunchFailed,
                    "error.tool.actionUnsupported",
                )),
            }
        })
    }
}

#[cfg(any(target_os = "linux", all(test, unix)))]
async fn launch_linux(spec: TerminalLaunchSpec) -> Result<(), AppError> {
    linux::launch(spec).await
}

#[cfg(not(any(target_os = "linux", all(test, unix))))]
async fn launch_linux(_spec: TerminalLaunchSpec) -> Result<(), AppError> {
    Err(launch_failed("Linux terminal bridge is unavailable"))
}

/// The AppleScript and every dynamic value are separate argv entries. The script, paths, PATH and
/// session-resume parameters are all hidden in the execution log; the AppleScript builds the
/// terminal command entry by entry with `quoted form`.
/// If the chosen terminal has been uninstalled, fall back to the system terminal — a resume should
/// not fail just because the user switched terminals.
#[cfg(any(target_os = "macos", test))]
fn macos_launch_plan(spec: &TerminalLaunchSpec) -> Result<CommandSpec, AppError> {
    spec.validate()?;
    match spec
        .terminal_app
        .filter(|id| macos::app_path(*id).is_some())
    {
        None | Some(TerminalAppId::System) => macos_bridge_spec(spec),
        Some(TerminalAppId::ITerm2) => macos_script_spec(spec, macos::ITERM_TERMINAL_SCRIPT),
        Some(other) => {
            let app = macos::app_path(other)
                .ok_or_else(|| invalid_spec("error.commandSpec.terminalTargetInvalid"))?;
            if !matches!(spec.environment, TerminalEnvironment::Native) {
                return Err(invalid_spec("error.commandSpec.terminalTargetInvalid"));
            }
            macos::open_bridge_spec(spec, other, &app)
        }
    }
}

#[cfg(any(target_os = "macos", test))]
fn macos_bridge_spec(spec: &TerminalLaunchSpec) -> Result<CommandSpec, AppError> {
    macos_script_spec(spec, MACOS_TERMINAL_SCRIPT)
}

#[cfg(any(target_os = "macos", test))]
fn macos_script_spec(
    spec: &TerminalLaunchSpec,
    script: &'static str,
) -> Result<CommandSpec, AppError> {
    spec.validate()?;
    if !matches!(spec.environment, TerminalEnvironment::Native) {
        return Err(invalid_spec("error.commandSpec.terminalTargetInvalid"));
    }
    let tool_path = spec
        .command
        .program_path
        .as_ref()
        .expect("validated native terminal target has a program path");
    let path_value = spec
        .command
        .env
        .iter()
        .find(|(key, _)| key == "PATH")
        .map(|(_, value)| value.as_str())
        .unwrap_or("");

    let mut args = vec![
        "-e".to_string(),
        script.to_string(),
        spec.working_directory.to_string_lossy().into_owned(),
        tool_path.to_string_lossy().into_owned(),
        path_value.to_string(),
    ];
    args.extend(spec.command.args.iter().cloned());
    let sensitive = (1..args.len()).collect();

    Ok(CommandSpec::new(AllowedProgram::Osascript, args)
        .with_program_path(PathBuf::from("/usr/bin/osascript"))
        .with_sensitive_args(sensitive)
        .with_timeout(TERMINAL_BRIDGE_TIMEOUT))
}

#[cfg(target_os = "macos")]
async fn launch_macos(spec: TerminalLaunchSpec) -> Result<(), AppError> {
    let output = SystemExecutor
        .execute(macos_launch_plan(&spec)?)
        .await
        .map_err(|error| launch_failed(error.to_string()))?;
    if output.success {
        Ok(())
    } else {
        Err(launch_failed(format!(
            "Terminal bridge exited with code {:?}",
            output.exit_code
        )))
    }
}

#[cfg(not(target_os = "macos"))]
async fn launch_macos(_spec: TerminalLaunchSpec) -> Result<(), AppError> {
    Err(launch_failed("macOS terminal bridge is unavailable"))
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct DetachedCommand {
    command: CommandSpec,
    working_directory: Option<PathBuf>,
}

/// The source of `cmd` is entirely fixed; the install path only goes into an environment variable
/// and the project path only into current_dir.
/// `call` is not used, so the install path cannot be parsed a second time after variable expansion.
#[cfg(any(target_os = "windows", test))]
fn windows_native_plan(spec: &TerminalLaunchSpec) -> Result<DetachedCommand, AppError> {
    spec.validate()?;
    if !matches!(spec.environment, TerminalEnvironment::Native) {
        return Err(invalid_spec("error.commandSpec.terminalTargetInvalid"));
    }
    let tool_path = spec
        .command
        .program_path
        .as_ref()
        .expect("validated native terminal target has a program path");
    if !spec.command.args.is_empty() {
        return Ok(DetachedCommand {
            command: spec.command.clone(),
            working_directory: Some(spec.working_directory.clone()),
        });
    }
    Ok(DetachedCommand {
        // Two layers of quotes on purpose. `/S` strips one outer pair before
        // anything else happens, so a single pair would leave the expansion of
        // a path containing spaces bare and `cmd` would try to run its first
        // word. The inner pair is what survives to protect the path.
        command: CommandSpec::cmd_script(r#"""%AI_MANAGER_TOOL%"""#).with_env(
            "AI_MANAGER_TOOL",
            crate::platform::windows_shell_compatible_path(tool_path).to_string_lossy(),
        ),
        working_directory: Some(spec.working_directory.clone()),
    })
}

fn valid_wsl_distro(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
}

#[cfg(any(target_os = "windows", test))]
enum WslDirectory {
    Resolved(String),
    NeedsWslPath,
    DifferentDistro,
}

/// Parsing strings is deliberate: it lets the same set of Windows UNC safety cases run on macOS CI,
/// independent of the host's `Path::components` platform semantics.
#[cfg(any(target_os = "windows", test))]
fn classify_wsl_directory(path: &Path, expected_distro: &str) -> WslDirectory {
    let mut normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.to_ascii_lowercase().starts_with("//?/unc/") {
        normalized = format!("//{}", &normalized[8..]);
    }
    let lower = normalized.to_ascii_lowercase();
    let prefix_len = if lower.starts_with("//wsl$/") {
        "//wsl$/".len()
    } else if lower.starts_with("//wsl.localhost/") {
        "//wsl.localhost/".len()
    } else {
        return WslDirectory::NeedsWslPath;
    };

    let remainder = &normalized[prefix_len..];
    let mut parts = remainder.split('/');
    let distro = parts.next().unwrap_or_default();
    if !distro.eq_ignore_ascii_case(expected_distro) {
        return WslDirectory::DifferentDistro;
    }
    let components: Vec<&str> = parts.filter(|part| !part.is_empty()).collect();
    if components
        .iter()
        .any(|component| *component == "." || *component == "..")
    {
        return WslDirectory::DifferentDistro;
    }
    let linux = if components.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", components.join("/"))
    };
    WslDirectory::Resolved(linux)
}

#[cfg(any(target_os = "windows", test))]
fn wsl_path_probe_spec(distro: &str, path: &Path) -> Result<CommandSpec, AppError> {
    if !valid_wsl_distro(distro) {
        return Err(invalid_spec("error.commandSpec.wslDistroInvalid"));
    }
    Ok(CommandSpec::new(
        AllowedProgram::Wsl,
        vec![
            "-d".to_string(),
            distro.to_string(),
            "--".to_string(),
            "wslpath".to_string(),
            "-a".to_string(),
            "-u".to_string(),
            windows_display_path(path),
        ],
    )
    .with_sensitive_args(vec![6])
    .with_timeout(TERMINAL_BRIDGE_TIMEOUT))
}

#[cfg(any(target_os = "windows", test))]
fn windows_display_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        raw.into_owned()
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_wsl_plan(
    spec: &TerminalLaunchSpec,
    linux_directory: String,
) -> Result<DetachedCommand, AppError> {
    spec.validate()?;
    let TerminalEnvironment::Wsl { distro } = &spec.environment else {
        return Err(invalid_spec("error.commandSpec.terminalTargetInvalid"));
    };
    if !linux_directory.starts_with('/') || linux_directory.contains('\0') {
        return Err(invalid_spec("error.commandSpec.terminalTargetInvalid"));
    }
    let tool = spec.command.program.as_str();
    let mut args = vec![
        "-d".to_string(),
        distro.clone(),
        "--cd".to_string(),
        linux_directory,
        "--".to_string(),
        "env".to_string(),
        format!("AI_MANAGER_TOOL={tool}"),
        "sh".to_string(),
        "-c".to_string(),
        WSL_TERMINAL_SCRIPT.to_string(),
        "ai-manager-session".to_string(),
    ];
    args.extend(spec.command.args.iter().cloned());
    let mut sensitive = vec![3];
    sensitive.extend(11..args.len());
    Ok(DetachedCommand {
        command: CommandSpec::new(AllowedProgram::Wsl, args).with_sensitive_args(sensitive),
        working_directory: None,
    })
}

#[cfg(target_os = "windows")]
async fn launch_windows(spec: TerminalLaunchSpec) -> Result<(), AppError> {
    let plan = match &spec.environment {
        TerminalEnvironment::Native => windows_native_plan(&spec)?,
        TerminalEnvironment::Wsl { distro } => {
            let linux_directory = match classify_wsl_directory(&spec.working_directory, distro) {
                WslDirectory::Resolved(path) => path,
                WslDirectory::DifferentDistro => {
                    return Err(AppError::new(
                        ErrorCode::LaunchFailed,
                        "error.tool.projectFolderUnavailable",
                    )
                    .with_remediation("error.remediation.chooseAnotherFolder"));
                }
                WslDirectory::NeedsWslPath => {
                    let output = SystemExecutor
                        .execute(wsl_path_probe_spec(distro, &spec.working_directory)?)
                        .await
                        .map_err(|error| launch_failed(error.to_string()))?;
                    let path = output.stdout.trim();
                    if !output.success || !path.starts_with('/') {
                        return Err(launch_failed(format!(
                            "wslpath exited with code {:?}",
                            output.exit_code
                        )));
                    }
                    path.to_string()
                }
            };
            windows_wsl_plan(&spec, linux_directory)?
        }
    };

    tokio::task::spawn_blocking(move || spawn_windows_detached(plan))
        .await
        .map_err(|error| launch_failed(format!("terminal task join failed: {error}")))??;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
async fn launch_windows(_spec: TerminalLaunchSpec) -> Result<(), AppError> {
    Err(launch_failed("Windows terminal bridge is unavailable"))
}

#[cfg(target_os = "windows")]
fn spawn_windows_detached(plan: DetachedCommand) -> Result<(), AppError> {
    use crate::platform::command::AllowedProgram;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

    plan.command.validate()?;
    let mut command = Command::new(plan.command.program_command_name());
    if plan.command.program == AllowedProgram::Cmd {
        // `Command::args` applies MSVCRT quoting to every argument, which
        // escapes the quotes inside the script as `\"`. `cmd.exe` has no such
        // escape: it would take `\` as the first character of a program name
        // and open a console that ran nothing. The switches are ordinary
        // arguments; only the script has to reach the command line verbatim.
        let (switches, script) = plan
            .command
            .args
            .split_at(plan.command.args.len().saturating_sub(1));
        command.args(switches);
        if let Some(script) = script.first() {
            command.raw_arg(script);
        }
    } else {
        command.args(&plan.command.args);
    }
    for (key, value) in &plan.command.env {
        command.env(key, value);
    }
    if let Some(directory) = &plan.working_directory {
        command.current_dir(directory);
    }
    command.creation_flags(CREATE_NEW_CONSOLE);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| launch_failed(format!("console spawn failed: {}", error.kind())))
}

#[cfg(test)]
#[path = "terminal/tests.rs"]
mod tests;
