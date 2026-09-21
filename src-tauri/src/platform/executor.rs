use futures::future::BoxFuture;
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::domain::{AppError, ErrorCode};
use crate::platform::command::CommandSpec;
use crate::platform::redact::{redact_secrets, truncate_tail};

mod cancellation;

pub use cancellation::CommandCancellation;
use cancellation::{cancellation_failed_error, cancelled_error};

/// Polling interval for the child process exit status. Matches upstream `commands/misc.rs:3140`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Output budget for `technical_message` (spec §42: technical details may contain raw output, but bounded).
const TECHNICAL_MAX_LINES: usize = 8;
const TECHNICAL_MAX_BYTES: usize = 512;
/// Upper bound for keeping one output stream of a child process in memory. Install scripts
/// can emit hundreds of MB; line-by-line pushing already goes through the observer and
/// `technical_detail()` only looks at the tail, so only the tail is kept: on overflow whole
/// lines are dropped from the front and a marker is recorded.
const RETAINED_OUTPUT_MAX_BYTES: usize = 4 * 1024 * 1024;
const RETAINED_OUTPUT_TRUNCATION_MARKER: &[u8] = b"[earlier output dropped]\n";

/// Suppresses the console flash on Windows. Per spec §13, `cfg(target_os)` is concentrated in the platform layer.
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandChunk {
    pub stream: CommandStream,
    pub text: String,
}

pub type CommandObserver = Arc<dyn Fn(CommandChunk) + Send + Sync>;

impl CommandOutput {
    /// For the caller to put into `AppError::technical_message`: redacted first, then
    /// truncated. stderr takes precedence and stdout is only used when it is empty (the key
    /// errors of npm/pip are at the end of stderr).
    pub fn technical_detail(&self) -> String {
        let raw = if self.stderr.trim().is_empty() {
            &self.stdout
        } else {
            &self.stderr
        };
        truncate_tail(
            &redact_secrets(raw),
            TECHNICAL_MAX_LINES,
            TECHNICAL_MAX_BYTES,
        )
    }
}

/// Executor abstraction: the production implementation really spawns processes while the
/// test implementation returns scripted results, so adapter / application tests never touch
/// the OS.
pub trait CommandExecutor: Send + Sync {
    fn execute(&self, spec: CommandSpec) -> BoxFuture<'static, Result<CommandOutput, AppError>>;

    /// A test stub can implement `execute` alone and the default implementation replays
    /// both streams once it finishes; the production executor overrides it to push line by
    /// line, so the UI keeps seeing progress during a long install.
    fn execute_streaming(
        &self,
        spec: CommandSpec,
        observer: CommandObserver,
    ) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
        let output = self.execute(spec);
        Box::pin(async move {
            let output = output.await?;
            publish_complete_output(&observer, &output);
            Ok(output)
        })
    }

    /// Cancellation-aware variant used only by operations that registered an
    /// authoritative cancellation handle. Test executors may keep the default:
    /// it prevents a not-yet-started command and never pretends it interrupted
    /// an already-running future.
    fn execute_streaming_cancellable(
        &self,
        spec: CommandSpec,
        observer: CommandObserver,
        cancellation: CommandCancellation,
    ) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
        if cancellation.confirm_if_requested() {
            return Box::pin(async { Err(cancelled_error()) });
        }
        let output = self.execute_streaming(spec, observer);
        Box::pin(async move {
            let result = output.await;
            if cancellation.is_requested() {
                cancellation.fail_if_requested();
                return Err(cancellation_failed_error());
            }
            result
        })
    }
}

pub struct SystemExecutor;

impl CommandExecutor for SystemExecutor {
    fn execute(&self, spec: CommandSpec) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
        execute_system(spec, None, None)
    }

    fn execute_streaming(
        &self,
        spec: CommandSpec,
        observer: CommandObserver,
    ) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
        execute_system(spec, Some(observer), None)
    }

    fn execute_streaming_cancellable(
        &self,
        spec: CommandSpec,
        observer: CommandObserver,
        cancellation: CommandCancellation,
    ) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
        execute_system(spec, Some(observer), Some(cancellation))
    }
}

fn execute_system(
    spec: CommandSpec,
    observer: Option<CommandObserver>,
    cancellation: Option<CommandCancellation>,
) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
    Box::pin(async move {
        spec.validate()?;
        // tokio::process is not used: this crate's tokio does not enable the `process`
        // feature, and enabling it would mean editing Cargo.toml and pulling in
        // signal-hook-registry (violating the no-new-dependency rule).
        // spawn_blocking + std::process is isomorphic to the upstream lifecycle exec path.
        tokio::task::spawn_blocking(move || run_blocking(&spec, observer, cancellation))
            .await
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, "error.command.joinFailed")
                    .with_technical(e.to_string())
            })?
    })
}

fn publish_complete_output(observer: &CommandObserver, output: &CommandOutput) {
    if !output.stdout.is_empty() {
        observer(CommandChunk {
            stream: CommandStream::Stdout,
            text: output.stdout.clone(),
        });
    }
    if !output.stderr.is_empty() {
        observer(CommandChunk {
            stream: CommandStream::Stderr,
            text: output.stderr.clone(),
        });
    }
}

fn run_blocking(
    spec: &CommandSpec,
    observer: Option<CommandObserver>,
    cancellation: Option<CommandCancellation>,
) -> Result<CommandOutput, AppError> {
    let mut command = Command::new(spec.program_command_name());
    if cancellation
        .as_ref()
        .is_some_and(CommandCancellation::confirm_if_requested)
    {
        return Err(cancelled_error());
    }
    command
        .args(&spec.args)
        // Close stdin explicitly: an inherited stdin may be a terminal or a pipe, and a read
        // inside an install script would block forever in an unattended GUI process.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    isolate_child(&mut command);

    let child = command.spawn().map_err(|e| spawn_error(spec, &e))?;
    let output =
        wait_with_timeout(child, spec.timeout, observer, cancellation).map_err(|failure| {
            match failure {
                WaitFailure::Timeout => AppError::new(ErrorCode::Internal, "error.command.timeout")
                    .with_technical(format!(
                        "{} timed out after {}s",
                        spec.redacted_display(),
                        spec.timeout.as_secs()
                    )),
                WaitFailure::Io(e) => {
                    AppError::new(ErrorCode::Internal, "error.command.spawnFailed")
                        .with_technical(format!("{}: {e}", spec.redacted_display()))
                }
                WaitFailure::Cancelled => cancelled_error(),
                WaitFailure::CancellationFailed => cancellation_failed_error(),
            }
        })?;

    let result = CommandOutput {
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    };
    // The log records only the redacted command and the exit code; child-process output is never logged (AI_RULES rule 6).
    log::debug!(
        "command finished: {} -> {:?}",
        spec.redacted_display(),
        result.exit_code
    );
    Ok(result)
}

fn spawn_error(spec: &CommandSpec, error: &std::io::Error) -> AppError {
    let (code, message_key) = match error.kind() {
        std::io::ErrorKind::NotFound => (ErrorCode::ToolNotFound, "error.command.programNotFound"),
        std::io::ErrorKind::PermissionDenied => (
            ErrorCode::PermissionDenied,
            "error.command.permissionDenied",
        ),
        _ => (ErrorCode::Internal, "error.command.spawnFailed"),
    };
    AppError::new(code, message_key).with_technical(format!("{}: {error}", spec.redacted_display()))
}

enum WaitFailure {
    Timeout,
    Io(std::io::Error),
    Cancelled,
    CancellationFailed,
}

/// Put the child process into its own session (POSIX) / no window (Windows).
///
/// POSIX uses `setsid` rather than `process_group(0)`: a new session comes with a new
/// process group (so the `kill(-pid)` whole-group semantics are unchanged) and additionally
/// detaches the controlling terminal — otherwise, when dev mode is started from a terminal,
/// an interactive sub-shell inside an install script would be stopped by SIGTTIN/SIGTTOU
/// for being in a background process group and `wait()` would never see it exit (upstream
/// recorded this measured trap in `commands/misc.rs:3074`).
#[cfg(not(target_os = "windows"))]
fn isolate_child(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    // SAFETY: setsid is async-signal-safe; the forked child inherits the parent's process
    // group and cannot be the group leader, so the call cannot fail with EPERM.
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
fn isolate_child(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// Kill the whole child process tree. Returns whether the kill succeeded — the caller
/// should only `wait()` on success, otherwise the child is still alive (a setuid binary, or
/// every descendant returning EPERM) and `wait()` would block forever, causing exactly the
/// hang the timeout mechanism exists to prevent. Isomorphic to the upstream
/// `terminate_child_tree` (`commands/misc.rs:3053-3071`).
#[cfg(not(target_os = "windows"))]
fn kill_tree(child: &mut Child) -> bool {
    let group = -(child.id() as libc::pid_t);
    // SAFETY: `isolate_child` already put the child into its own session/process group
    // before exec, so a negative pid only hits that group and never this process.
    (unsafe { libc::kill(group, libc::SIGKILL) } == 0) || child.kill().is_ok()
}

#[cfg(target_os = "windows")]
fn kill_tree(child: &mut Child) -> bool {
    use std::os::windows::process::CommandExt;

    let status = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    matches!(status, Ok(status) if status.success()) || child.kill().is_ok()
}

/// Drain both pipes + wait for exit with a timeout.
///
/// The pipes must be read to EOF by **separate threads**: the kernel pipe buffer is only
/// 64KB and install-script output greatly exceeds it, so calling `wait()` before reading
/// deadlocks both ways once the buffer fills. On the timeout path the reader threads are
/// deliberately **not** joined — if the group kill failed, some descendant still holds the
/// write end and EOF never arrives (upstream handles it the same way).
fn wait_with_timeout(
    mut child: Child,
    timeout: Duration,
    observer: Option<CommandObserver>,
    cancellation: Option<CommandCancellation>,
) -> Result<Output, WaitFailure> {
    let stdout_observer = observer.clone();
    let mut stdout_handle = child.stdout.take().map(|pipe| {
        std::thread::spawn(move || read_pipe(pipe, CommandStream::Stdout, stdout_observer))
    });
    let mut stderr_handle = child
        .stderr
        .take()
        .map(|pipe| std::thread::spawn(move || read_pipe(pipe, CommandStream::Stderr, observer)));

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(cancellation) = cancellation
            .as_ref()
            .filter(|cancellation| cancellation.is_requested())
        {
            if kill_tree(&mut child) {
                let _ = child.wait();
                cancellation.confirm_if_requested();
                drop(stdout_handle.take());
                drop(stderr_handle.take());
                return Err(WaitFailure::Cancelled);
            }
            match child.try_wait() {
                Ok(Some(_)) => {
                    cancellation.fail_if_requested();
                    drop(stdout_handle.take());
                    drop(stderr_handle.take());
                    return Err(WaitFailure::CancellationFailed);
                }
                Ok(None) => {
                    // Do not release the Operation lock while an unconfirmed
                    // child is alive. Retrying here is intentionally not bound
                    // by the ordinary command timeout.
                    std::thread::sleep(POLL_INTERVAL);
                    continue;
                }
                Err(e) => return Err(WaitFailure::Io(e)),
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let now = Instant::now();
                if now >= deadline {
                    if kill_tree(&mut child) {
                        let _ = child.wait();
                    }
                    drop(stdout_handle.take());
                    drop(stderr_handle.take());
                    return Err(WaitFailure::Timeout);
                }
                std::thread::sleep(POLL_INTERVAL.min(deadline - now));
            }
            Err(e) => {
                if kill_tree(&mut child) {
                    let _ = child.wait();
                }
                return Err(WaitFailure::Io(e));
            }
        }
    };

    // The child has exited, but a grandchild may still hold the write end; the deadline covers that too.
    while stdout_handle
        .as_ref()
        .is_some_and(|handle| !handle.is_finished())
        || stderr_handle
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    {
        let now = Instant::now();
        if now >= deadline {
            // The child has already exited (this loop is only entered after that), so the
            // kill here only cleans up grandchildren still holding the pipe write end; there
            // is no need — and no reason — to `wait()` on the already-reaped child again.
            let _ = kill_tree(&mut child);
            drop(stdout_handle.take());
            drop(stderr_handle.take());
            return Err(WaitFailure::Timeout);
        }
        std::thread::sleep(POLL_INTERVAL.min(deadline - now));
    }

    let stdout = stdout_handle
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = stderr_handle
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_pipe<R: std::io::Read>(
    pipe: R,
    stream: CommandStream,
    observer: Option<CommandObserver>,
) -> Vec<u8> {
    use std::io::BufRead;

    let mut reader = std::io::BufReader::new(pipe);
    let mut output = Vec::new();
    let mut line = Vec::new();
    let mut truncated = false;
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {
                retain_tail(
                    &mut output,
                    &line,
                    RETAINED_OUTPUT_MAX_BYTES,
                    &mut truncated,
                );
                if let Some(observer) = &observer {
                    observer(CommandChunk {
                        stream,
                        text: String::from_utf8_lossy(&line).to_string(),
                    });
                }
            }
            // Same as the old `read_to_end` path: a read error must not mask the real child
            // exit status, and what was already read is kept for diagnostics upstream.
            Err(_) => break,
        }
    }
    if truncated {
        let mut marked = RETAINED_OUTPUT_TRUNCATION_MARKER.to_vec();
        marked.extend_from_slice(&output);
        return marked;
    }
    output
}

/// Append one line; once `limit` is exceeded, whole lines are dropped from the front (or
/// cut by bytes when a single overlong line remains), keeping only the tail. It cuts
/// straight down to `limit / 2` in one go: the frontend `drain` is a full memmove, so
/// cutting once per line would move the entire buffer for every line an install script
/// emits, which is quadratic.
fn retain_tail(output: &mut Vec<u8>, line: &[u8], limit: usize, truncated: &mut bool) {
    output.extend_from_slice(line);
    if output.len() <= limit {
        return;
    }
    let excess = output.len() - limit / 2;
    // Cut on the first line boundary that drops at least `excess` bytes.
    let start = excess - 1;
    let cut = match output[start..].iter().position(|byte| *byte == b'\n') {
        Some(offset) if start + offset + 1 < output.len() => start + offset + 1,
        _ => excess,
    };
    output.drain(..cut);
    *truncated = true;
}

// POSIX process tests live beside the executor to keep this production module bounded.
#[cfg(all(test, not(target_os = "windows")))]
#[path = "executor/tests.rs"]
mod tests;
