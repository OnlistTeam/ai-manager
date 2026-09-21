//! Executes an uninstall: the app removal plan, then the optional cache and
//! settings batches the user ticked.
//!
//! Command plans go through the executor, which confirms a pending cancel
//! request before it spawns anything. Path removals have no child process, so
//! nothing stands in front of `remove_paths`; every batch therefore runs the
//! shared cancellation checkpoint itself. Without it a request that arrives
//! during the probe window would still be reported as "cancelled" by the use
//! case after the files were already gone.

use std::path::PathBuf;

use crate::adapters::lifecycle_runner::{remove_paths, run_plan};
use crate::adapters::tool_adapter::LifecycleContext;
use crate::domain::operation::phase;
use crate::domain::{AppError, ErrorCode};
use crate::platform::plan::UninstallPlan;

pub(crate) async fn run_uninstall(
    plan: UninstallPlan,
    removals: Vec<(u8, Vec<PathBuf>)>,
    ctx: &LifecycleContext,
) -> Result<(), AppError> {
    ctx.report(20, phase::REMOVING);
    match plan {
        UninstallPlan::Command(commands) => {
            run_plan(
                &commands,
                ctx,
                ErrorCode::UninstallFailed,
                "error.tool.uninstallFailed",
                phase::REMOVING,
            )
            .await?
        }
        UninstallPlan::RemovePaths(paths) => {
            ctx.cancellation.checkpoint()?;
            remove_paths(
                paths,
                ErrorCode::UninstallFailed,
                "error.tool.uninstallFailed",
            )
            .await?
        }
    }

    // Spec §32: the two flags are independent and neither deletes by default. Cache first, then
    // settings — the caches of claude / codex live inside the config directory, so the reverse
    // order would make the cache deletion a no-op.
    //
    // A deletion failure at runtime (permissions/in use) is still a whole-batch Err: the pre-check
    // can only block "out-of-bounds paths" and not residual risks such as concurrent use or
    // permission problems; no new "partial success" semantics are introduced for it (that is a
    // product decision for the application layer in T8+).
    for (progress, paths) in removals {
        ctx.cancellation.checkpoint()?;
        ctx.report(progress, phase::REMOVING);
        remove_paths(
            paths,
            ErrorCode::UninstallFailed,
            "error.tool.uninstallFailed",
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run_uninstall;
    use crate::adapters::tool_adapter::{LifecycleContext, LifecycleNetworkPolicy};
    use crate::domain::{AppError, ErrorCode};
    use crate::platform::command::{AllowedProgram, CommandSpec};
    use crate::platform::executor::{
        CommandCancellation, CommandExecutor, CommandObserver, CommandOutput,
    };
    use crate::platform::plan::{LifecyclePlan, UninstallPlan};
    use futures::future::BoxFuture;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// Succeeds like a real child that already exited, then requests cancellation
    /// so the request lands exactly between the app command and the optional
    /// removal batches.
    struct CancelAfterRunExecutor(CommandCancellation);

    impl CommandExecutor for CancelAfterRunExecutor {
        fn execute(
            &self,
            _spec: CommandSpec,
        ) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
            Box::pin(async {
                Ok(CommandOutput {
                    success: true,
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                })
            })
        }

        fn execute_streaming_cancellable(
            &self,
            spec: CommandSpec,
            _observer: CommandObserver,
            _cancellation: CommandCancellation,
        ) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
            let output = self.execute(spec);
            let cancellation = self.0.clone();
            Box::pin(async move {
                let output = output.await;
                cancellation.request();
                output
            })
        }
    }

    struct TestHomeGuard {
        previous: Option<std::ffi::OsString>,
        temp: tempfile::TempDir,
    }

    impl TestHomeGuard {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("create isolated home");
            let previous = std::env::var_os("AI_MANAGER_TEST_HOME");
            std::env::set_var("AI_MANAGER_TEST_HOME", temp.path());
            Self { previous, temp }
        }

        fn file(&self, name: &str) -> PathBuf {
            let path = self.temp.path().join(".aim-uninstall-probe").join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
            std::fs::write(&path, b"x").expect("create probe file");
            path
        }
    }

    impl Drop for TestHomeGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
                None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
            }
        }
    }

    fn context(
        executor: Arc<dyn CommandExecutor>,
        cancellation: CommandCancellation,
    ) -> LifecycleContext {
        LifecycleContext {
            executor,
            cancellation,
            progress: Arc::new(|_, _| {}),
            logger: Arc::new(|_, _, _| {}),
            network: LifecycleNetworkPolicy::default(),
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn a_cancel_requested_during_the_probe_window_never_removes_the_native_app() {
        let home = TestHomeGuard::new();
        let launcher = home.file("launcher");
        let cancellation = CommandCancellation::default();
        let ctx = context(
            Arc::new(CancelAfterRunExecutor(cancellation.clone())),
            cancellation.clone(),
        );
        assert!(
            cancellation.request(),
            "the user cancelled while probe() was running"
        );

        let error = run_uninstall(
            UninstallPlan::RemovePaths(vec![launcher.clone()]),
            Vec::new(),
            &ctx,
        )
        .await
        .expect_err("a confirmed cancellation must stop before the first deletion");

        assert_eq!(error.message_key, "error.operation.cancelled");
        assert!(cancellation.is_confirmed());
        assert!(launcher.exists(), "the app must still be on disk");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn a_cancel_that_lands_after_the_app_command_skips_the_optional_batches() {
        let home = TestHomeGuard::new();
        let cache = home.file("cache");
        let settings = home.file("settings");
        let cancellation = CommandCancellation::default();
        let ctx = context(
            Arc::new(CancelAfterRunExecutor(cancellation.clone())),
            cancellation.clone(),
        );
        let plan = UninstallPlan::Command(LifecyclePlan::single(CommandSpec::new(
            AllowedProgram::Npm,
            vec!["uninstall".to_string(), "-g".to_string(), "pkg".to_string()],
        )));

        let error = run_uninstall(
            plan,
            vec![(85, vec![cache.clone()]), (90, vec![settings.clone()])],
            &ctx,
        )
        .await
        .expect_err("the request must be honoured before user data is touched");

        assert_eq!(error.message_key, "error.operation.cancelled");
        assert_eq!(error.code, ErrorCode::Internal);
        assert!(cancellation.is_confirmed());
        assert!(cache.exists() && settings.exists());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn without_a_request_every_batch_is_removed_in_order() {
        let home = TestHomeGuard::new();
        let launcher = home.file("launcher");
        let cache = home.file("cache");
        let cancellation = CommandCancellation::default();
        let ctx = context(
            Arc::new(CancelAfterRunExecutor(CommandCancellation::default())),
            cancellation.clone(),
        );

        run_uninstall(
            UninstallPlan::RemovePaths(vec![launcher.clone()]),
            vec![(85, vec![cache.clone()])],
            &ctx,
        )
        .await
        .expect("an uncancelled uninstall completes");

        assert!(!launcher.exists() && !cache.exists());
        assert!(!cancellation.is_confirmed());
    }
}
