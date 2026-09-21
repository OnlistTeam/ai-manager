use std::process::{Command, Stdio};

use crate::domain::{AppError, ErrorCode};

use super::command::CommandSpec;

/// A narrow process handoff for graphical applications that remain alive after launch.
/// The request is still a validated `CommandSpec`; callers cannot provide shell text.
pub trait DetachedCommandLauncher: Send + Sync {
    fn launch(&self, spec: CommandSpec) -> Result<(), AppError>;
}

pub struct SystemDetachedCommandLauncher;

impl DetachedCommandLauncher for SystemDetachedCommandLauncher {
    fn launch(&self, spec: CommandSpec) -> Result<(), AppError> {
        spec.validate()?;
        let mut command = Command::new(spec.program_command_name());
        command
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        isolate_graphical_child(&mut command);

        let display = spec.redacted_display();
        let mut child = command.spawn().map_err(|error| {
            let (code, message_key) = match error.kind() {
                std::io::ErrorKind::NotFound => {
                    (ErrorCode::ToolNotFound, "error.command.programNotFound")
                }
                std::io::ErrorKind::PermissionDenied => (
                    ErrorCode::PermissionDenied,
                    "error.command.permissionDenied",
                ),
                _ => (ErrorCode::Internal, "error.command.spawnFailed"),
            };
            AppError::new(code, message_key).with_technical(format!("{display}: {error}"))
        })?;

        // Dropping `Child` would leave a zombie on POSIX when the app exits. A tiny
        // reaper thread owns only this child and never blocks the product runtime.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        log::debug!("detached command handed off: {display}");
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn isolate_graphical_child(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    // SAFETY: `setsid` is async-signal-safe and the freshly forked child is not
    // a process-group leader, matching the isolation used by SystemExecutor.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(target_os = "windows")]
fn isolate_graphical_child(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(test)]
mod tests {
    use super::{DetachedCommandLauncher, SystemDetachedCommandLauncher};
    use crate::platform::{AllowedProgram, CommandSpec};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn invalid_specs_are_rejected_before_spawn() {
        let error = SystemDetachedCommandLauncher
            .launch(CommandSpec::bash_script("true").with_timeout(Duration::ZERO))
            .expect_err("invalid spec");
        assert_eq!(error.message_key, "error.commandSpec.timeoutZero");
    }

    #[test]
    fn missing_anchored_programs_return_a_structured_error() {
        // The anchor must be absolute for `validate`, and absoluteness is platform specific:
        // a bare POSIX path has no drive prefix and would be rejected on Windows before the
        // spawn that this test is about ever happens.
        let missing = if cfg!(target_os = "windows") {
            PathBuf::from(r"C:\nonexistent-ai-manager-desktop\claude-desktop.exe")
        } else {
            PathBuf::from("/nonexistent-ai-manager-desktop/claude-desktop")
        };
        let error = SystemDetachedCommandLauncher
            .launch(
                CommandSpec::new(AllowedProgram::ClaudeDesktop, Vec::new())
                    .with_program_path(missing),
            )
            .expect_err("missing app");
        assert_eq!(error.message_key, "error.command.programNotFound");
    }
}
