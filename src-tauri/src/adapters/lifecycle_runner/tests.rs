use super::{prepare_download_spec, remove_paths_blocking, run_plan};
use crate::adapters::tool_adapter::{LifecycleContext, LifecycleNetworkPolicy};
use crate::compat::ccswitch::install_probe::{InstallSource, InstalledEntry, LifecycleProbe};
use crate::compat::ccswitch::lifecycle::{install_plan, update_plan};
use crate::domain::operation::phase;
use crate::domain::{DownloadStrategy, ErrorCode, OperationLogKind, ToolId};
use crate::platform::command::{AllowedProgram, CommandSpec};
use crate::platform::executor::{CommandExecutor, CommandOutput};
use crate::platform::plan::{LifecycleAttempt, LifecyclePlan, LifecycleStep};
use futures::future::BoxFuture;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Returns a scripted result per "nth call" and records the argv received each time.
struct ScriptedExecutor {
    outcomes: Mutex<Vec<Result<CommandOutput, crate::domain::AppError>>>,
    seen: Mutex<Vec<CommandSpec>>,
}

impl ScriptedExecutor {
    fn new(outcomes: Vec<Result<CommandOutput, crate::domain::AppError>>) -> Arc<Self> {
        Arc::new(Self {
            outcomes: Mutex::new(outcomes),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.seen
            .lock()
            .expect("seen lock")
            .iter()
            .map(|spec| spec.args.clone())
            .collect()
    }

    fn specs(&self) -> Vec<CommandSpec> {
        self.seen.lock().expect("seen lock").clone()
    }
}

impl CommandExecutor for Arc<ScriptedExecutor> {
    fn execute(
        &self,
        spec: CommandSpec,
    ) -> BoxFuture<'static, Result<CommandOutput, crate::domain::AppError>> {
        self.seen.lock().expect("seen lock").push(spec);
        let mut outcomes = self.outcomes.lock().expect("outcomes lock");
        let next = if outcomes.is_empty() {
            Ok(ok_output())
        } else {
            outcomes.remove(0)
        };
        Box::pin(async move { next })
    }
}

fn ok_output() -> CommandOutput {
    CommandOutput {
        success: true,
        exit_code: Some(0),
        stdout: String::new(),
        stderr: String::new(),
    }
}

fn failed_output(stderr: &str) -> CommandOutput {
    CommandOutput {
        success: false,
        exit_code: Some(1),
        stdout: String::new(),
        stderr: stderr.to_string(),
    }
}

type ProgressLog = Arc<Mutex<Vec<(u8, String)>>>;
type OperationLog = Arc<Mutex<Vec<(OperationLogKind, Option<String>, Option<String>)>>>;

fn context(executor: Arc<ScriptedExecutor>) -> (LifecycleContext, ProgressLog, OperationLog) {
    context_with_network(executor, LifecycleNetworkPolicy::default())
}

fn context_with_network(
    executor: Arc<ScriptedExecutor>,
    network: LifecycleNetworkPolicy,
) -> (LifecycleContext, ProgressLog, OperationLog) {
    let reported: ProgressLog = Arc::new(Mutex::new(Vec::new()));
    let sink = reported.clone();
    let logged: OperationLog = Arc::new(Mutex::new(Vec::new()));
    let log_sink = logged.clone();
    let ctx = LifecycleContext {
        executor: Arc::new(executor),
        cancellation: crate::platform::executor::CommandCancellation::default(),
        progress: Arc::new(move |value, key: &'static str| {
            sink.lock()
                .expect("progress lock")
                .push((value, key.to_string()));
        }),
        logger: Arc::new(move |kind, message_key, detail| {
            log_sink.lock().expect("operation log lock").push((
                kind,
                message_key.map(str::to_string),
                detail,
            ));
        }),
        network,
    };
    (ctx, reported, logged)
}

fn spec(arg: &str) -> CommandSpec {
    CommandSpec::new(AllowedProgram::Npm, vec![arg.to_string()])
}

#[test]
fn npm_registry_pin_overrides_both_environment_key_spellings() {
    let prepared =
        prepare_download_spec(spec("install"), &LifecycleNetworkPolicy::default(), false);

    for key in ["NPM_CONFIG_REGISTRY", "npm_config_registry"] {
        assert!(prepared.env.iter().any(|(candidate, value)| {
            candidate == key && value == "https://registry.npmjs.org"
        }));
    }
}

#[tokio::test]
async fn the_first_successful_attempt_wins_and_later_ones_never_run() {
    let executor = ScriptedExecutor::new(vec![Ok(ok_output())]);
    let (ctx, progress, logs) = context(executor.clone());
    let plan = LifecyclePlan {
        attempts: vec![
            LifecycleAttempt::single(spec("primary")),
            LifecycleAttempt::single(spec("fallback")),
        ],
    };
    run_plan(
        &plan,
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect("the primary attempt succeeds");
    assert_eq!(executor.calls(), vec![vec!["primary".to_string()]]);
    assert!(!progress.lock().expect("progress lock").is_empty());
    let logs = logs.lock().expect("operation log lock");
    assert!(logs.iter().any(|(kind, _, detail)| {
        *kind == OperationLogKind::Command
            && detail
                .as_deref()
                .is_some_and(|detail| detail.contains("primary"))
    }));
    assert!(logs.iter().any(|(_, key, _)| {
        key.as_deref() == Some(crate::domain::operation::log_message::COMMAND_SUCCEEDED)
    }));
}

#[tokio::test]
async fn a_failed_attempt_falls_through_to_the_next_one() {
    let executor = ScriptedExecutor::new(vec![Ok(failed_output("boom")), Ok(ok_output())]);
    let (ctx, _progress, logs) = context(executor.clone());
    let plan = LifecyclePlan {
        attempts: vec![
            LifecycleAttempt::single(spec("primary")),
            LifecycleAttempt::single(spec("fallback")),
        ],
    };
    run_plan(
        &plan,
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect("the fallback attempt succeeds");
    assert_eq!(
        executor.calls(),
        vec![vec!["primary".to_string()], vec!["fallback".to_string()]]
    );
    assert!(logs
        .lock()
        .expect("operation log lock")
        .iter()
        .any(|(_, key, _)| key.as_deref()
            == Some(crate::domain::operation::log_message::TRYING_FALLBACK)));
}

#[tokio::test]
async fn every_attempt_failing_reports_the_kind_specific_code_without_leaking_stderr() {
    let executor = ScriptedExecutor::new(vec![
        Ok(failed_output("npm ERR! token sk-ant-0123456789 rejected")),
        Ok(failed_output("still broken")),
    ]);
    let (ctx, _progress, logs) = context(executor);
    let plan = LifecyclePlan {
        attempts: vec![
            LifecycleAttempt::single(spec("primary")),
            LifecycleAttempt::single(spec("fallback")),
        ],
    };
    let error = run_plan(
        &plan,
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect_err("all attempts failed");
    assert_eq!(error.code, ErrorCode::InstallFailed);
    assert_eq!(error.message_key, "error.tool.installFailed");
    let technical = error.technical_message.unwrap_or_default();
    assert!(technical.contains("still broken"));
    assert!(!technical.contains("sk-ant-0123456789"));
    let logs = logs.lock().expect("operation log lock");
    assert!(logs
        .iter()
        .filter_map(|(_, _, detail)| detail.as_deref())
        .all(|detail| !detail.contains("sk-ant-0123456789")));
    assert!(logs
        .iter()
        .filter_map(|(_, _, detail)| detail.as_deref())
        .any(|detail| detail.contains("***")));
}

#[tokio::test]
async fn a_tolerated_step_failure_does_not_abort_its_attempt() {
    let executor = ScriptedExecutor::new(vec![
        Ok(failed_output("nothing to remove")),
        Ok(ok_output()),
    ]);
    let (ctx, _progress, logs) = context(executor.clone());
    let plan = LifecyclePlan {
        attempts: vec![LifecycleAttempt {
            steps: vec![
                LifecycleStep {
                    spec: spec("uninstall"),
                    ignore_failure: true,
                },
                LifecycleStep {
                    spec: spec("install"),
                    ignore_failure: false,
                },
            ],
        }],
    };
    run_plan(
        &plan,
        &ctx,
        ErrorCode::UpdateFailed,
        "error.tool.updateFailed",
        phase::INSTALLING,
    )
    .await
    .expect("the tolerated failure is ignored");
    assert_eq!(executor.calls().len(), 2);
    assert!(logs
        .lock()
        .expect("operation log lock")
        .iter()
        .any(|(_, key, _)| key.as_deref()
            == Some(crate::domain::operation::log_message::COMMAND_FAILURE_IGNORED)));
}

#[tokio::test]
async fn an_executor_error_is_preserved_when_it_is_the_last_word() {
    let denied = crate::domain::AppError::new(
        ErrorCode::PermissionDenied,
        "error.command.permissionDenied",
    );
    let executor = ScriptedExecutor::new(vec![Err(denied)]);
    let (ctx, _progress, _logs) = context(executor);
    let error = run_plan(
        &LifecyclePlan::single(spec("primary")),
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect_err("a spawn failure fails the plan");
    assert_eq!(error.code, ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn automatic_mode_retries_an_npm_download_once_with_a_scoped_mirror() {
    let executor = ScriptedExecutor::new(vec![
        Ok(failed_output(
            "npm ERR! request ENOTFOUND registry.npmjs.org",
        )),
        Ok(ok_output()),
    ]);
    let network = LifecycleNetworkPolicy::new(DownloadStrategy::Automatic, None);
    let (ctx, _progress, logs) = context_with_network(executor.clone(), network);

    run_plan(
        &LifecyclePlan::single(spec("install")),
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect("the bounded mirror retry succeeds");

    let specs = executor.specs();
    assert_eq!(specs.len(), 2, "one official try plus one mirror try");
    assert!(specs[0].env.iter().any(|(key, value)| {
        key == "NPM_CONFIG_REGISTRY" && value == "https://registry.npmjs.org"
    }));
    assert!(specs[1].env.iter().any(|(key, value)| {
        key == "NPM_CONFIG_REGISTRY" && value == "https://registry.npmmirror.com"
    }));
    assert!(logs
        .lock()
        .expect("operation log lock")
        .iter()
        .any(|(_, key, _)| key.as_deref()
            == Some(crate::domain::operation::log_message::USING_COMMUNITY_MIRROR)));
}

#[tokio::test]
async fn automatic_mode_retries_a_public_npm_registry_403_with_a_scoped_mirror() {
    let executor = ScriptedExecutor::new(vec![
        Ok(failed_output("npm ERR! code E403\n403 Forbidden")),
        Ok(ok_output()),
    ]);
    let network = LifecycleNetworkPolicy::new(DownloadStrategy::Automatic, None);
    let (ctx, _progress, logs) = context_with_network(executor.clone(), network);

    run_plan(
        &LifecyclePlan::single(spec("install")),
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect("a public npm registry restriction gets one bounded mirror retry");

    let specs = executor.specs();
    assert_eq!(
        specs.len(),
        2,
        "one default registry try plus one mirror try"
    );
    assert!(specs[0].env.iter().any(|(key, value)| {
        key == "NPM_CONFIG_REGISTRY" && value == "https://registry.npmjs.org"
    }));
    assert!(specs[1].env.iter().any(|(key, value)| {
        key == "NPM_CONFIG_REGISTRY" && value == "https://registry.npmmirror.com"
    }));
    assert!(logs
        .lock()
        .expect("operation log lock")
        .iter()
        .any(|(_, key, _)| key.as_deref()
            == Some(crate::domain::operation::log_message::USING_COMMUNITY_MIRROR)));
}

/// The official shell installer is a POSIX-only channel: `official_install_script` returns
/// `None` on Windows and `lifecycle_windows` registers no Claude Code entry, so the ladder
/// there starts at npm and only the mirror fallback remains to prove.
#[tokio::test]
async fn claude_install_falls_from_official_to_npm_then_to_the_scoped_mirror() {
    let has_official_attempt = !cfg!(target_os = "windows");
    let mut responses = Vec::new();
    if has_official_attempt {
        responses.push(Ok(failed_output(
            "curl: (6) Could not resolve host: claude.ai",
        )));
    }
    responses.push(Ok(failed_output(
        "npm ERR! request ENOTFOUND registry.npmjs.org",
    )));
    responses.push(Ok(ok_output()));
    let executor = ScriptedExecutor::new(responses);
    let network = LifecycleNetworkPolicy::new(DownloadStrategy::Automatic, None);
    let (ctx, _progress, logs) = context_with_network(executor.clone(), network);
    let plan = install_plan(
        ToolId::ClaudeCode,
        &LifecycleProbe {
            entry: None,
            path_env: Some(("PATH".to_string(), "/usr/bin:/bin".to_string())),
        },
    )
    .expect("Claude Code has an install plan");

    run_plan(
        &plan,
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect("the npm mirror fallback installs Claude Code");

    let mut expected = Vec::new();
    if has_official_attempt {
        expected.push(AllowedProgram::Bash);
    }
    expected.extend([AllowedProgram::Npm, AllowedProgram::Npm]);
    let specs = executor.specs();
    assert_eq!(
        specs.iter().map(|spec| spec.program).collect::<Vec<_>>(),
        expected
    );
    let first_npm = usize::from(has_official_attempt);
    assert!(specs[first_npm].env.iter().any(|(key, value)| {
        key == "NPM_CONFIG_REGISTRY" && value == "https://registry.npmjs.org"
    }));
    assert!(specs[first_npm + 1].env.iter().any(|(key, value)| {
        key == "NPM_CONFIG_REGISTRY" && value == "https://registry.npmmirror.com"
    }));
    assert!(logs
        .lock()
        .expect("operation log lock")
        .iter()
        .any(|(_, key, _)| key.as_deref()
            == Some(crate::domain::operation::log_message::USING_COMMUNITY_MIRROR)));
}

#[tokio::test]
async fn native_claude_update_runs_the_official_updater_through_the_local_proxy() {
    let proxy = "http://127.0.0.1:7890";
    let executor = ScriptedExecutor::new(vec![Ok(ok_output())]);
    let network = LifecycleNetworkPolicy::new(DownloadStrategy::Automatic, Some(proxy.to_string()));
    let (ctx, _progress, logs) = context_with_network(executor.clone(), network);
    let plan = update_plan(
        ToolId::ClaudeCode,
        &LifecycleProbe {
            entry: Some(InstalledEntry {
                bin_path: PathBuf::from("/Users/a/.local/bin/claude"),
                real_path: PathBuf::from("/Users/a/.local/bin/claude"),
                source: InstallSource::Native,
                brew_formula: None,
                runnable: true,
                npm_package: Some("@anthropic-ai/claude-code"),
                hermes_owner: None,
            }),
            path_env: Some(("PATH".to_string(), "/usr/bin:/bin".to_string())),
        },
        "2.0.0",
    )
    .expect("native Claude Code has an owner-preserving update plan");

    run_plan(
        &plan,
        &ctx,
        ErrorCode::UpdateFailed,
        "error.tool.updateFailed",
        phase::INSTALLING,
    )
    .await
    .expect("the official updater succeeds through the local proxy");

    let specs = executor.specs();
    let mut expected = vec![AllowedProgram::ClaudeCode];
    if !crate::compat::ccswitch::native_supply::supplies(ToolId::ClaudeCode) {
        expected.push(AllowedProgram::Bash);
    }
    assert_eq!(
        specs.iter().map(|spec| spec.program).collect::<Vec<_>>(),
        expected[..specs.len()].to_vec()
    );
    assert_eq!(specs.len(), 1, "the first attempt succeeded");
    for spec in &specs {
        for key in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
            assert!(spec
                .env
                .iter()
                .any(|(candidate, value)| candidate == key && value == proxy));
        }
        assert!(!spec.env.iter().any(|(key, _)| key == "NPM_CONFIG_REGISTRY"));
    }
    assert!(logs
        .lock()
        .expect("operation log lock")
        .iter()
        .any(|(_, key, _)| key.as_deref()
            == Some(crate::domain::operation::log_message::USING_CONFIGURED_PROXY)));
}

#[tokio::test]
async fn automatic_mirror_does_not_retry_auth_permission_or_metadata_failures() {
    let cases = [
        (
            "npm ERR! code EACCES\nnpm ERR! permission denied",
            ErrorCode::InstallFailed,
            "error.tool.installFailed",
        ),
        (
            "npm ERR! code E401\n401 Unauthorized",
            ErrorCode::NetworkError,
            "error.tool.downloadAccessDenied",
        ),
        (
            "npm ERR! code E407\n407 Proxy Authentication Required",
            ErrorCode::NetworkError,
            "error.tool.downloadAccessDenied",
        ),
        (
            "npm ERR! code E404\n404 Not Found",
            ErrorCode::InstallFailed,
            "error.tool.installFailed",
        ),
        (
            "npm ERR! code ETARGET\nnpm ERR! notarget No matching version found",
            ErrorCode::InstallFailed,
            "error.tool.installFailed",
        ),
    ];

    for (stderr, expected_code, expected_key) in cases {
        let executor = ScriptedExecutor::new(vec![Ok(failed_output(stderr))]);
        let network = LifecycleNetworkPolicy::new(DownloadStrategy::Automatic, None);
        let (ctx, _progress, logs) = context_with_network(executor.clone(), network);

        let error = run_plan(
            &LifecyclePlan::single(spec("install")),
            &ctx,
            ErrorCode::InstallFailed,
            "error.tool.installFailed",
            phase::INSTALLING,
        )
        .await
        .expect_err("a mirror cannot repair auth, permission, or package metadata failures");

        assert_eq!(error.code, expected_code, "{stderr}");
        assert_eq!(error.message_key, expected_key, "{stderr}");
        assert_eq!(executor.specs().len(), 1, "{stderr}");
        assert!(!logs
            .lock()
            .expect("operation log lock")
            .iter()
            .any(|(_, key, _)| key.as_deref()
                == Some(crate::domain::operation::log_message::USING_COMMUNITY_MIRROR)));
    }
}

#[tokio::test]
async fn a_vendor_403_is_not_misrepresented_as_an_npm_registry_restriction() {
    let executor = ScriptedExecutor::new(vec![Ok(failed_output("403 Forbidden"))]);
    let network = LifecycleNetworkPolicy::new(DownloadStrategy::Automatic, None);
    let (ctx, _progress, logs) = context_with_network(executor.clone(), network);
    let plan = LifecyclePlan::single(CommandSpec::new(
        AllowedProgram::ClaudeCode,
        vec!["update".to_string()],
    ));

    let error = run_plan(
        &plan,
        &ctx,
        ErrorCode::UpdateFailed,
        "error.tool.updateFailed",
        phase::INSTALLING,
    )
    .await
    .expect_err("a vendor channel must not be replaced by an npm mirror");

    assert_eq!(error.code, ErrorCode::NetworkError);
    assert_eq!(error.message_key, "error.tool.downloadAccessDenied");
    assert_eq!(executor.specs().len(), 1);
    assert!(!logs
        .lock()
        .expect("operation log lock")
        .iter()
        .any(|(_, key, _)| key.as_deref()
            == Some(crate::domain::operation::log_message::USING_COMMUNITY_MIRROR)));
}

#[tokio::test]
async fn automatic_mode_retries_an_npm_command_timeout_once() {
    let executor = ScriptedExecutor::new(vec![
        Err(crate::domain::AppError::new(
            ErrorCode::Internal,
            "error.command.timeout",
        )),
        Ok(ok_output()),
    ]);
    let network = LifecycleNetworkPolicy::new(DownloadStrategy::Automatic, None);
    let (ctx, _progress, _logs) = context_with_network(executor.clone(), network);

    run_plan(
        &LifecyclePlan::single(spec("install")),
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect("an npm timeout gets one bounded mirror retry");

    let specs = executor.specs();
    assert_eq!(specs.len(), 2);
    assert!(specs[1].env.iter().any(|(key, value)| {
        key == "NPM_CONFIG_REGISTRY" && value == "https://registry.npmmirror.com"
    }));
}

#[tokio::test]
async fn automatic_mode_never_changes_a_native_tool_into_an_npm_install() {
    let executor = ScriptedExecutor::new(vec![Ok(failed_output(
        "curl: (6) Could not resolve host: claude.ai",
    ))]);
    let network = LifecycleNetworkPolicy::new(DownloadStrategy::Automatic, None);
    let (ctx, _progress, logs) = context_with_network(executor.clone(), network);
    let plan = LifecyclePlan::single(CommandSpec::new(
        AllowedProgram::ClaudeCode,
        vec!["update".to_string()],
    ));

    run_plan(
        &plan,
        &ctx,
        ErrorCode::UpdateFailed,
        "error.tool.updateFailed",
        phase::INSTALLING,
    )
    .await
    .expect_err("a native failure remains an official-channel failure");

    let specs = executor.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].program, AllowedProgram::ClaudeCode);
    assert!(!specs[0]
        .env
        .iter()
        .any(|(key, _)| key == "NPM_CONFIG_REGISTRY"));
    assert!(!logs
        .lock()
        .expect("operation log lock")
        .iter()
        .any(|(_, key, _)| key.as_deref()
            == Some(crate::domain::operation::log_message::USING_COMMUNITY_MIRROR)));
}

#[tokio::test]
async fn official_only_mode_classifies_network_failure_without_using_a_mirror() {
    let executor = ScriptedExecutor::new(vec![Ok(failed_output(
        "curl: (6) Could not resolve host: downloads.claude.ai",
    ))]);
    let (ctx, _progress, _logs) = context(executor.clone());
    let error = run_plan(
        &LifecyclePlan::single(spec("install")),
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect_err("official-only network failure stays failed");

    assert_eq!(executor.specs().len(), 1);
    assert_eq!(error.code, ErrorCode::NetworkError);
    assert_eq!(error.message_key, "error.tool.downloadNetworkFailed");
    assert_eq!(
        error.remediation.as_deref(),
        Some("error.remediation.configureInstallNetwork")
    );
}

#[tokio::test]
async fn a_configured_proxy_is_injected_but_never_written_to_the_operation_log() {
    let proxy = "http://alice:do-not-show@127.0.0.1:7890";
    let executor = ScriptedExecutor::new(vec![Ok(CommandOutput {
        success: true,
        exit_code: Some(0),
        stdout: format!("installer used proxy {proxy}"),
        stderr: String::new(),
    })]);
    let network =
        LifecycleNetworkPolicy::new(DownloadStrategy::OfficialOnly, Some(proxy.to_string()));
    let (ctx, _progress, logs) = context_with_network(executor.clone(), network);
    run_plan(
        &LifecyclePlan::single(spec("install")),
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect("proxied command succeeds");

    let specs = executor.specs();
    for key in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
        assert!(specs[0]
            .env
            .iter()
            .any(|(candidate, value)| candidate == key && value == proxy));
    }
    let rendered = logs
        .lock()
        .expect("operation log lock")
        .iter()
        .flat_map(|(_, key, detail)| [key.as_deref(), detail.as_deref()])
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("operation.log.usingConfiguredProxy"));
    assert!(rendered.contains("http://***@127.0.0.1:7890"));
    assert!(!rendered.contains("alice"));
    assert!(!rendered.contains("do-not-show"));
}

#[tokio::test]
async fn an_explicit_vendor_region_error_is_not_presented_as_a_mirror_problem() {
    let executor = ScriptedExecutor::new(vec![Ok(failed_output("App unavailable in your region"))]);
    let (ctx, _progress, _logs) = context(executor);
    let error = run_plan(
        &LifecyclePlan::single(CommandSpec::new(
            AllowedProgram::ClaudeCode,
            vec!["update".to_string()],
        )),
        &ctx,
        ErrorCode::UpdateFailed,
        "error.tool.updateFailed",
        phase::INSTALLING,
    )
    .await
    .expect_err("vendor region restriction remains a failure");

    assert_eq!(error.code, ErrorCode::NetworkError);
    assert_eq!(error.message_key, "error.tool.serviceRegionUnavailable");
    assert_eq!(
        error.remediation.as_deref(),
        Some("error.remediation.checkServiceRegion")
    );
}

#[tokio::test]
async fn an_empty_plan_is_an_error_not_a_silent_success() {
    let executor = ScriptedExecutor::new(Vec::new());
    let (ctx, _progress, _logs) = context(executor.clone());
    let error = run_plan(
        &LifecyclePlan {
            attempts: Vec::new(),
        },
        &ctx,
        ErrorCode::InstallFailed,
        "error.tool.installFailed",
        phase::INSTALLING,
    )
    .await
    .expect_err("an empty plan must fail loudly");
    assert_eq!(error.code, ErrorCode::InstallFailed);
    assert!(executor.calls().is_empty());
}

/// Isolated home: both `is_removable` and `tool_paths` read `get_home_dir()`, which prefers
/// `AI_MANAGER_TEST_HOME` (the test hook of upstream `config.rs:23`).
/// Deletion cases must never touch the real user directory.
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

    fn path(&self) -> &std::path::Path {
        self.temp.path()
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

#[test]
#[serial_test::serial]
fn removing_paths_is_idempotent_and_refuses_targets_outside_home() {
    let home = TestHomeGuard::new();
    let root = home.path().join(".aim-remove-probe").join("data");
    std::fs::create_dir_all(root.join("versions").join("1.0.0")).expect("create probe tree");
    let file = home.path().join(".aim-remove-probe").join("launcher");
    std::fs::write(&file, b"x").expect("create probe file");

    remove_paths_blocking(
        &[file.clone(), root.clone()],
        ErrorCode::UninstallFailed,
        "error.tool.uninstallFailed",
    )
    .expect("removal succeeds");
    assert!(!root.exists());
    assert!(!file.exists());

    // Delete again: already gone = goal achieved, not an error
    remove_paths_blocking(
        std::slice::from_ref(&root),
        ErrorCode::UninstallFailed,
        "error.tool.uninstallFailed",
    )
    .expect("removing a missing path is a no-op");

    for outside in [
        std::path::PathBuf::from("/"),
        std::path::PathBuf::from("/usr/local/bin/definitely-not-ours"),
        home.path().to_path_buf(),
    ] {
        let error = remove_paths_blocking(
            std::slice::from_ref(&outside),
            ErrorCode::UninstallFailed,
            "error.tool.uninstallFailed",
        )
        .expect_err("a path outside the home directory must be refused");
        assert_eq!(error.code, ErrorCode::UninstallFailed);
        assert_eq!(error.message_key, "error.tool.removePathRefused");
    }
    assert!(home.path().exists(), "the guard must not have been deleted");
}

/// The whole batch must pass validation before deletion starts.
#[test]
#[serial_test::serial]
fn a_refused_path_blocks_the_whole_batch_before_anything_is_deleted() {
    let home = TestHomeGuard::new();
    let legit = home.path().join(".aim-remove-probe").join("keep-me");
    std::fs::create_dir_all(legit.parent().expect("test path has a parent"))
        .expect("create safe parent");
    std::fs::write(&legit, b"x").expect("create legit probe file");
    let outside = std::path::PathBuf::from("/usr/local/bin/definitely-not-ours");

    let error = remove_paths_blocking(
        &[legit.clone(), outside],
        ErrorCode::UninstallFailed,
        "error.tool.uninstallFailed",
    )
    .expect_err("a refused path must block the entire batch");
    assert_eq!(error.code, ErrorCode::UninstallFailed);
    assert_eq!(error.message_key, "error.tool.removePathRefused");
    assert!(
        legit.exists(),
        "paths preceding the refused one must survive the precheck"
    );
}

#[cfg(unix)]
#[test]
#[serial_test::serial]
fn removing_a_final_symlink_never_follows_its_external_target() {
    use std::os::unix::fs::symlink;

    let home = TestHomeGuard::new();
    let outside = tempfile::tempdir().expect("outside");
    let survivor = outside.path().join("must-survive");
    std::fs::write(&survivor, b"important").expect("outside file");
    let link = home.path().join(".aim-remove-probe").join("tool-data");
    std::fs::create_dir_all(link.parent().expect("test path has a parent"))
        .expect("create safe parent");
    symlink(outside.path(), &link).expect("target symlink");

    remove_paths_blocking(
        std::slice::from_ref(&link),
        ErrorCode::UninstallFailed,
        "error.tool.uninstallFailed",
    )
    .expect("remove the link only");

    assert!(!link.exists());
    assert!(survivor.exists(), "external target content must survive");
}

#[tokio::test]
async fn every_step_reports_the_phase_the_caller_asked_for() {
    let executor = ScriptedExecutor::new(vec![Ok(ok_output()), Ok(ok_output())]);
    let (ctx, progress, _logs) = context(executor.clone());
    let plan = LifecyclePlan {
        attempts: vec![LifecycleAttempt {
            steps: vec![
                LifecycleStep {
                    spec: spec("a"),
                    ignore_failure: false,
                },
                LifecycleStep {
                    spec: spec("b"),
                    ignore_failure: false,
                },
            ],
        }],
    };

    run_plan(
        &plan,
        &ctx,
        ErrorCode::UninstallFailed,
        "error.tool.uninstallFailed",
        phase::REMOVING,
    )
    .await
    .expect("plan succeeds");

    let reported = progress.lock().expect("progress lock").clone();
    assert!(!reported.is_empty());
    assert!(
        reported.iter().all(|(_, key)| *key == phase::REMOVING),
        "uninstall steps must not report the install phase: {reported:?}"
    );
}

#[tokio::test]
async fn a_confirmed_cancellation_never_falls_through_to_another_attempt() {
    let executor = ScriptedExecutor::new(vec![Ok(ok_output()), Ok(ok_output())]);
    let (ctx, _progress, _logs) = context(executor.clone());
    let plan = LifecyclePlan {
        attempts: vec![
            LifecycleAttempt::single(spec("primary")),
            LifecycleAttempt::single(spec("fallback")),
        ],
    };
    assert!(ctx.cancellation.request());

    let error = run_plan(
        &plan,
        &ctx,
        ErrorCode::UpdateFailed,
        "error.tool.updateFailed",
        phase::DOWNLOADING,
    )
    .await
    .expect_err("a requested plan must stop");

    assert_eq!(error.message_key, "error.operation.cancelled");
    assert!(ctx.cancellation.is_confirmed());
    assert!(
        executor.calls().is_empty(),
        "no attempt may start after cancel"
    );
}
