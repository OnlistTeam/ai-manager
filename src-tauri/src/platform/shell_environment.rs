//! A snapshot of the real login-shell environment.
//!
//! The environment of a GUI process is given by launchd / SCM and contains none of the
//! variables the user `export`s in `.zshrc`, `.zprofile` or anything they `source` from
//! there; a CLI started in a terminal inherits exactly those. Grepping rc files line by line
//! (upstream `services/env_checker.rs`) cannot follow `source` chains and conditional
//! branches, so the user's own login shell is started once and asked to compute it itself.
//! The trampoline is `bash` from the allowlist (spec §14), isomorphic to the
//! `exec "${SHELL:-sh}" -lic …` in `platform/terminal.rs`; `/usr/bin/env` is written as an
//! absolute path to bypass aliases and functions.

use std::collections::BTreeMap;
#[cfg_attr(target_os = "windows", allow(unused_imports))]
use std::sync::{Mutex, OnceLock, PoisonError};
#[cfg_attr(target_os = "windows", allow(unused_imports))]
use std::time::{Duration, Instant};

#[cfg_attr(target_os = "windows", allow(unused_imports))]
use super::executor::{CommandExecutor, CommandOutput, SystemExecutor};
#[cfg_attr(target_os = "windows", allow(unused_imports))]
use super::CommandSpec;
#[cfg_attr(target_os = "windows", allow(unused_imports))]
use crate::domain::AppError;

/// Constant script: zero interpolation, executable only once `bash_script` has sealed it.
/// fish does not accept the `-i` combination, sh/dash have no rc files, and unknown shells
/// fall back to `-lc` (login, non-interactive, the safest choice).
#[cfg_attr(target_os = "windows", allow(dead_code))]
const LOGIN_SHELL_ENV_SCRIPT: &str = r#"case "${SHELL##*/}" in
  zsh|bash) exec "$SHELL" -lic /usr/bin/env ;;
  fish) exec "$SHELL" -lc /usr/bin/env ;;
  "") exec /bin/sh -c /usr/bin/env ;;
  *) exec "$SHELL" -lc /usr/bin/env ;;
esac"#;
#[cfg_attr(target_os = "windows", allow(dead_code))]
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(8);

/// How long one probe result serves all concurrent callers. A refresh of the Endpoints page
/// often fires several runtime-context requests for the same tool within a few hundred
/// milliseconds (invalidation after save/switch plus a manual refresh); 5 seconds covers that
/// burst without leaving an rc file the user just edited behind a stale result for too long.
#[cfg_attr(target_os = "windows", allow(dead_code))]
const PROBE_REUSE_WINDOW: Duration = Duration::from_secs(5);

#[cfg_attr(target_os = "windows", allow(dead_code))]
static PROBE_CACHE: OnceLock<Mutex<Option<(Instant, ShellEnvironment)>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellEnvironmentSource {
    /// The full environment after the user's login shell evaluated it, equal to what a CLI started in a terminal sees.
    LoginShell,
    /// Windows: user / system environment comes from the registry, and a GUI process inherits exactly what a terminal would see.
    Registry,
    /// The login shell failed to start or timed out, so the GUI process's own environment is used; the UI must say so honestly.
    ProcessFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellEnvironment {
    variables: BTreeMap<String, String>,
    source: ShellEnvironmentSource,
}

impl ShellEnvironment {
    pub fn new(variables: BTreeMap<String, String>, source: ShellEnvironmentSource) -> Self {
        Self { variables, source }
    }

    fn from_process(source: ShellEnvironmentSource) -> Self {
        Self::new(std::env::vars().collect(), source)
    }

    /// A blank value counts as unset: `export FOO=` means "not set" to every tool.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.variables
            .get(name)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    }

    pub fn source(&self) -> ShellEnvironmentSource {
        self.source
    }

    pub fn inspected_login_shell(&self) -> bool {
        !matches!(self.source, ShellEnvironmentSource::ProcessFallback)
    }
}

pub fn probe_shell_environment() -> ShellEnvironment {
    #[cfg(target_os = "windows")]
    {
        ShellEnvironment::from_process(ShellEnvironmentSource::Registry)
    }
    #[cfg(not(target_os = "windows"))]
    {
        // The caller runs on a spawn_blocking thread (the blocking() of a Tauri command), so
        // a tokio runtime context is available here; futures::executor::block_on is used
        // deliberately instead of tauri::async_runtime::block_on — the latter panics outright
        // when called from inside the runtime. Calling this from a thread with no runtime
        // (e.g. a plain #[test]) panics inside the executor's own spawn_blocking rather than
        // degrading gracefully.
        cached_probe(|| {
            probe_with(|spec| futures::executor::block_on(SystemExecutor.execute(spec)))
        })
    }
}

/// Concurrent callers share a single probe instead of each spawning their own login-shell
/// subprocess. The lock is held across the probe call itself: later callers queue on the
/// lock and get either a fresh cache entry or the result the previous probe just wrote —
/// there is never a second `exec`.
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn cached_probe(probe: impl FnOnce() -> ShellEnvironment) -> ShellEnvironment {
    let cache = PROBE_CACHE.get_or_init(|| Mutex::new(None));
    let mut slot = cache.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((measured_at, environment)) = slot.as_ref() {
        if measured_at.elapsed() < PROBE_REUSE_WINDOW {
            return environment.clone();
        }
    }
    let environment = probe();
    *slot = Some((Instant::now(), environment.clone()));
    environment
}

/// The probe body with an injectable executor: tests override the fake output to cover the success / no-output / failure paths.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn probe_with(
    run: impl FnOnce(CommandSpec) -> Result<CommandOutput, AppError>,
) -> ShellEnvironment {
    let spec = CommandSpec::bash_script(LOGIN_SHELL_ENV_SCRIPT).with_timeout(LOGIN_SHELL_TIMEOUT);
    match run(spec) {
        Ok(output) if output.success => {
            let variables = parse_env_output(&output.stdout);
            // No PATH means stdout is not the output of env (an rc file exec'd something else).
            if variables.contains_key("PATH") {
                return ShellEnvironment::new(variables, ShellEnvironmentSource::LoginShell);
            }
            log::debug!("login shell probe printed no environment; using the process environment");
        }
        Ok(output) => log::debug!(
            "login shell probe exited with {:?}; using the process environment",
            output.exit_code
        ),
        Err(error) => log::debug!(
            "login shell probe failed ({}); using the process environment",
            error.message_key
        ),
    }
    ShellEnvironment::from_process(ShellEnvironmentSource::ProcessFallback)
}

/// `env` prints `KEY=value` line by line; an interactive rc may print a greeting first, so lines that are not assignments are skipped.
pub fn parse_env_output(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            is_env_name(key).then(|| (key.to_string(), value.to_string()))
        })
        .collect()
}

fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::Duration;

    use super::{
        cached_probe, parse_env_output, probe_with, ShellEnvironment, ShellEnvironmentSource,
    };
    use crate::domain::{AppError, ErrorCode};
    use crate::platform::executor::CommandOutput;

    fn ok(stdout: &str) -> CommandOutput {
        CommandOutput {
            success: true,
            exit_code: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    /// Test only: ensures `AI_MANAGER_PROBE_TEST_MARKER` is cleaned up whether or not an assertion panics.
    struct ProbeTestMarker(Option<std::ffi::OsString>);

    impl ProbeTestMarker {
        fn set() -> Self {
            let previous = std::env::var_os("AI_MANAGER_PROBE_TEST_MARKER");
            std::env::set_var("AI_MANAGER_PROBE_TEST_MARKER", "present");
            Self(previous)
        }
    }

    impl Drop for ProbeTestMarker {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var("AI_MANAGER_PROBE_TEST_MARKER", value),
                None => std::env::remove_var("AI_MANAGER_PROBE_TEST_MARKER"),
            }
        }
    }

    #[test]
    fn parses_key_value_lines_and_skips_rc_banners() {
        let parsed = parse_env_output(
            "🚀 Welcome back\nPATH=/usr/bin:/bin\nANTHROPIC_BASE_URL=https://relay.example.test\nnot an assignment\n1BAD=x\n",
        );
        assert_eq!(
            parsed.get("PATH").map(String::as_str),
            Some("/usr/bin:/bin")
        );
        assert_eq!(
            parsed.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://relay.example.test")
        );
        assert_eq!(parsed.len(), 2, "{parsed:?}");
    }

    #[test]
    fn a_successful_probe_reports_the_login_shell() {
        let environment = probe_with(|spec| {
            assert!(spec.validate().is_ok(), "sealed bash script must validate");
            assert_eq!(spec.timeout, Duration::from_secs(8));
            assert!(
                spec.redacted_display().contains("-lic"),
                "{}",
                spec.redacted_display()
            );
            Ok(ok("PATH=/opt/homebrew/bin\nCODEX_API_KEY=sk-test-000000\n"))
        });
        assert_eq!(environment.source(), ShellEnvironmentSource::LoginShell);
        assert!(environment.inspected_login_shell());
        assert_eq!(environment.get("CODEX_API_KEY"), Some("sk-test-000000"));
        assert_eq!(environment.get("MISSING"), None);
    }

    #[test]
    fn empty_values_read_as_unset() {
        let environment = probe_with(|_| Ok(ok("PATH=/bin\nANTHROPIC_API_KEY=   \n")));
        assert_eq!(environment.get("ANTHROPIC_API_KEY"), None);
    }

    #[test]
    #[serial_test::serial]
    fn output_without_path_or_a_failed_probe_falls_back_to_the_process_environment() {
        let _marker = ProbeTestMarker::set();
        let no_path = probe_with(|_| Ok(ok("hello from an rc file\n")));
        assert_eq!(no_path.source(), ShellEnvironmentSource::ProcessFallback);
        assert!(!no_path.inspected_login_shell());
        assert_eq!(no_path.get("AI_MANAGER_PROBE_TEST_MARKER"), Some("present"));

        let failed =
            probe_with(|_| Err(AppError::new(ErrorCode::Internal, "error.command.timeout")));
        assert_eq!(failed.source(), ShellEnvironmentSource::ProcessFallback);

        let nonzero = probe_with(|_| {
            Ok(CommandOutput {
                success: false,
                exit_code: Some(1),
                stdout: "PATH=/bin".to_string(),
                stderr: String::new(),
            })
        });
        assert_eq!(nonzero.source(), ShellEnvironmentSource::ProcessFallback);
    }

    /// perf(platform): concurrent callers share one probe instead of each spawning their own
    /// login-shell subprocess. Four threads squeeze through `cached_probe` at once — no
    /// matter who takes the lock first, only one of them actually runs the `probe` closure
    /// and the rest reuse the result it just wrote back into the cache.
    #[test]
    fn concurrent_callers_share_a_single_probe() {
        let calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(4));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let calls = Arc::clone(&calls);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    cached_probe(|| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(20));
                        ShellEnvironment::new(BTreeMap::new(), ShellEnvironmentSource::LoginShell)
                    })
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("probing thread should not panic");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
