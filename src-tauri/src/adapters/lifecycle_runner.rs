use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::tool_adapter::LifecycleContext;
use crate::compat::ccswitch::tool_paths::is_removable;
use crate::domain::operation::log_message;
use crate::domain::{AppError, DownloadStrategy, ErrorCode, OperationLogKind};
use crate::platform::command::CommandSpec;
use crate::platform::executor::{CommandChunk, CommandObserver, CommandOutput, CommandStream};
use crate::platform::plan::{LifecycleAttempt, LifecyclePlan, LifecycleStep};
use crate::platform::redact::{redact_secrets, truncate_tail};

/// The progress range the plan execution occupies. The ends are left to the use case's Preparing / Checking / Ready.
const PLAN_PROGRESS_START: u32 = 30;
const PLAN_PROGRESS_END: u32 = 80;
const LOG_OUTPUT_MAX_LINES: usize = 12;
const LOG_OUTPUT_MAX_BYTES: usize = 4_096;
use crate::compat::ccswitch::lifecycle::{COMMUNITY_NPM_REGISTRY, OFFICIAL_NPM_REGISTRY};

struct PreparedAttempt {
    attempt: LifecycleAttempt,
    community_mirror: bool,
}

/// Try each attempt in order and return as soon as one fully succeeds; if all fail, report the
/// last error. This is the structured equivalent of the upstream `a || b` short-circuit chain.
///
/// `step_phase` is supplied by the caller: the same execution logic is "installing" during an
/// install/update and "removing" during an uninstall, so the phase belongs to the calling context
/// rather than to the executor.
pub async fn run_plan(
    plan: &LifecyclePlan,
    ctx: &LifecycleContext,
    failure_code: ErrorCode,
    failure_message_key: &'static str,
    step_phase: &'static str,
) -> Result<(), AppError> {
    if plan.is_empty() {
        return Err(AppError::new(failure_code, failure_message_key)
            .with_technical("no executable step was produced for this tool"));
    }

    let download_action = matches!(
        failure_code,
        ErrorCode::InstallFailed | ErrorCode::UpdateFailed
    );
    let attempts = prepare_attempts(plan, ctx, download_action);
    if download_action && ctx.network.proxy_url.is_some() {
        ctx.log_message(
            OperationLogKind::System,
            log_message::USING_CONFIGURED_PROXY,
        );
    }
    let total_steps: usize = attempts
        .iter()
        .map(|prepared| prepared.attempt.steps.len())
        .sum::<usize>()
        .max(1);
    let mut completed = 0usize;
    let mut last_error: Option<AppError> = None;

    for (attempt_index, prepared) in attempts.iter().enumerate() {
        if prepared.community_mirror && !last_error.as_ref().is_some_and(community_mirror_can_help)
        {
            continue;
        }
        if prepared.community_mirror {
            ctx.log_message(
                OperationLogKind::System,
                log_message::USING_COMMUNITY_MIRROR,
            );
        }
        match run_attempt(
            &prepared.attempt,
            ctx,
            failure_code,
            failure_message_key,
            step_phase,
            total_steps,
            &mut completed,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) => {
                if ctx.cancellation.is_confirmed() || ctx.cancellation.is_failed() {
                    return Err(error);
                }
                let has_next = attempts[attempt_index + 1..].iter().any(|candidate| {
                    !candidate.community_mirror || community_mirror_can_help(&error)
                });
                last_error = Some(error);
                if has_next {
                    ctx.log_message(OperationLogKind::System, log_message::TRYING_FALLBACK);
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| AppError::new(failure_code, failure_message_key)))
}

pub(crate) fn community_mirror_can_help(error: &AppError) -> bool {
    (error.code == ErrorCode::NetworkError
        && matches!(
            error.message_key.as_str(),
            "error.tool.downloadNetworkFailed" | "error.tool.downloadRegistryUnavailable"
        ))
        || (error.code == ErrorCode::Internal && error.message_key == "error.command.timeout")
}

fn prepare_attempts(
    plan: &LifecyclePlan,
    ctx: &LifecycleContext,
    download_action: bool,
) -> Vec<PreparedAttempt> {
    let resilient = download_action && ctx.network.download_strategy == DownloadStrategy::Automatic;
    let mut prepared = Vec::new();
    for attempt in &plan.attempts {
        prepared.push(PreparedAttempt {
            attempt: prepare_attempt(attempt, ctx, false),
            community_mirror: false,
        });
        if resilient
            && attempt
                .steps
                .iter()
                .any(|step| step.spec.program.uses_npm_registry())
        {
            prepared.push(PreparedAttempt {
                attempt: prepare_attempt(attempt, ctx, true),
                community_mirror: true,
            });
        }
    }
    prepared
}

fn prepare_attempt(
    attempt: &LifecycleAttempt,
    ctx: &LifecycleContext,
    community_mirror: bool,
) -> LifecycleAttempt {
    LifecycleAttempt {
        steps: attempt
            .steps
            .iter()
            .map(|step| LifecycleStep {
                spec: prepare_download_spec(step.spec.clone(), &ctx.network, community_mirror),
                ignore_failure: step.ignore_failure,
            })
            .collect(),
    }
}

pub(crate) fn prepare_download_spec(
    mut spec: CommandSpec,
    network: &crate::adapters::LifecycleNetworkPolicy,
    community_mirror: bool,
) -> CommandSpec {
    if let Some(proxy_url) = &network.proxy_url {
        spec = spec.with_env_pairs(
            [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
            ]
            .into_iter()
            .map(|key| (key.to_string(), proxy_url.clone()))
            .collect(),
        );
    }
    if spec.program.uses_npm_registry() {
        // Product "official first" must not silently inherit a stale global
        // npm mirror. Both choices are process-scoped, so user npm settings
        // remain untouched and the catalog/install/update paths agree on the
        // meaning of the registry's `latest` tag.
        let registry = if community_mirror {
            COMMUNITY_NPM_REGISTRY
        } else {
            OFFICIAL_NPM_REGISTRY
        };
        // pnpm-launched development builds inherit lowercase `npm_config_*`
        // variables. npm normalizes both spellings, so leaving the lowercase
        // registry untouched can override this operation-scoped choice and
        // silently reinstall a stale mirror version. Pin both spellings.
        spec = spec.with_env_pairs(vec![
            ("NPM_CONFIG_REGISTRY".to_string(), registry.to_string()),
            ("npm_config_registry".to_string(), registry.to_string()),
        ]);
    }
    spec
}

async fn run_attempt(
    attempt: &LifecycleAttempt,
    ctx: &LifecycleContext,
    failure_code: ErrorCode,
    failure_message_key: &'static str,
    step_phase: &'static str,
    total_steps: usize,
    completed: &mut usize,
) -> Result<(), AppError> {
    for step in &attempt.steps {
        *completed += 1;
        ctx.report(step_progress(*completed, total_steps), step_phase);
        let command = truncate_tail(
            &redact_secrets(&step.spec.redacted_display()),
            LOG_OUTPUT_MAX_LINES,
            LOG_OUTPUT_MAX_BYTES,
        );
        ctx.log_detail(OperationLogKind::Command, command);
        let output_context = ctx.clone();
        let observer: CommandObserver = Arc::new(move |chunk: CommandChunk| {
            let detail = truncate_tail(
                &redact_secrets(&chunk.text),
                LOG_OUTPUT_MAX_LINES,
                LOG_OUTPUT_MAX_BYTES,
            );
            if detail.is_empty() {
                return;
            }
            let kind = match chunk.stream {
                CommandStream::Stdout => OperationLogKind::Stdout,
                CommandStream::Stderr => OperationLogKind::Stderr,
            };
            output_context.log_detail(kind, detail);
        });
        // Failed to start / timed out: this attempt failed, hand over to the next one; the error is
        // kept verbatim so actionable codes such as PERMISSION_DENIED are not masked by a generic
        // code higher up.
        let output = match ctx
            .executor
            .execute_streaming_cancellable(step.spec.clone(), observer, ctx.cancellation.clone())
            .await
        {
            Ok(output) => output,
            Err(error) => {
                ctx.log_message(OperationLogKind::System, log_message::COMMAND_FAILED);
                return Err(error);
            }
        };
        if !output.success && !step.ignore_failure {
            ctx.log_message(OperationLogKind::System, log_message::COMMAND_FAILED);
            return Err(download_command_failure(
                &step.spec,
                &output,
                failure_code,
                failure_message_key,
            ));
        }
        if output.success {
            ctx.log_message(OperationLogKind::System, log_message::COMMAND_SUCCEEDED);
        } else {
            ctx.log_message(
                OperationLogKind::System,
                log_message::COMMAND_FAILURE_IGNORED,
            );
        }
    }
    Ok(())
}

fn step_progress(completed: usize, total: usize) -> u8 {
    let span = PLAN_PROGRESS_END - PLAN_PROGRESS_START;
    let ratio = (completed.min(total) as u32) * span / (total as u32);
    u8::try_from(PLAN_PROGRESS_START + ratio)
        .unwrap_or(u8::MAX)
        .min(100)
}

/// Spec §42: the user-visible message_key contains no subprocess output; the exit code and the
/// redacted, truncated output only go into technical_message for "View Details".
pub(crate) fn download_command_failure(
    spec: &CommandSpec,
    output: &CommandOutput,
    failure_code: ErrorCode,
    failure_message_key: &'static str,
) -> AppError {
    let detail = output.technical_detail();
    let technical = if detail.is_empty() {
        format!(
            "{} exited with {:?}",
            spec.redacted_display(),
            output.exit_code
        )
    } else {
        format!(
            "{} exited with {:?}\n{detail}",
            spec.redacted_display(),
            output.exit_code
        )
    };
    let classification = if matches!(
        failure_code,
        ErrorCode::InstallFailed | ErrorCode::UpdateFailed
    ) {
        classify_download_failure(spec, output)
    } else {
        DownloadFailure::Other
    };
    let (code, message_key, remediation) = match classification {
        DownloadFailure::RegionUnavailable => (
            ErrorCode::NetworkError,
            "error.tool.serviceRegionUnavailable",
            "error.remediation.checkServiceRegion",
        ),
        DownloadFailure::AccessDenied => (
            ErrorCode::NetworkError,
            "error.tool.downloadAccessDenied",
            "error.remediation.configureInstallNetwork",
        ),
        DownloadFailure::RegistryUnavailable => (
            ErrorCode::NetworkError,
            "error.tool.downloadRegistryUnavailable",
            "error.remediation.configureInstallNetwork",
        ),
        DownloadFailure::Network => (
            ErrorCode::NetworkError,
            "error.tool.downloadNetworkFailed",
            "error.remediation.configureInstallNetwork",
        ),
        DownloadFailure::Other => (
            failure_code,
            failure_message_key,
            "error.remediation.retryOrViewDetails",
        ),
    };
    AppError::new(code, message_key)
        .with_technical(technical)
        .with_remediation(remediation)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DownloadFailure {
    RegionUnavailable,
    AccessDenied,
    RegistryUnavailable,
    Network,
    Other,
}

fn classify_download_failure(spec: &CommandSpec, output: &CommandOutput) -> DownloadFailure {
    let combined = format!("{}\n{}", output.stderr, output.stdout).to_ascii_lowercase();
    if [
        "not available in your region",
        "app unavailable in your region",
        "unavailable in your region",
        "unsupported country",
        "unsupported region",
        "country is not supported",
    ]
    .iter()
    .any(|needle| combined.contains(needle))
    {
        return if spec.program.uses_npm_registry() {
            DownloadFailure::RegistryUnavailable
        } else {
            DownloadFailure::RegionUnavailable
        };
    }
    if [
        "403 forbidden",
        "code e403",
        "status code 403",
        "http 403",
        "response 403",
    ]
    .iter()
    .any(|needle| combined.contains(needle))
    {
        return if spec.program.uses_npm_registry() {
            DownloadFailure::RegistryUnavailable
        } else {
            DownloadFailure::AccessDenied
        };
    }
    if [
        "401 unauthorized",
        "407 proxy authentication required",
        "code e401",
        "code e407",
        "proxy authentication required",
        "407 proxy authentication",
    ]
    .iter()
    .any(|needle| combined.contains(needle))
    {
        return DownloadFailure::AccessDenied;
    }
    if [
        "could not resolve",
        "couldn't resolve",
        "enotfound",
        "eai_again",
        "network is unreachable",
        "etimedout",
        "econnreset",
        "econnrefused",
        "connection reset",
        "connection refused",
        "failed to connect",
        "connection timed out",
        "ssl certificate",
        "certificate verify failed",
        "unable to get local issuer certificate",
        "curl: (6)",
        "curl: (7)",
        "curl: (28)",
    ]
    .iter()
    .any(|needle| combined.contains(needle))
    {
        return DownloadFailure::Network;
    }
    DownloadFailure::Other
}

/// Batch pre-check: the whole set of paths goes through `is_removable` first, and a single
/// rejected entry fails the batch **before anything is deleted**. This avoids the
/// "delete-and-check one by one" case where earlier paths are already really gone when a later one
/// turns out to be out of bounds — a retry could then never succeed while legitimate paths are
/// already lost.
///
/// Used inside `remove_paths_blocking`, and also by the registry's uninstall to run the same
/// pre-check on the cache/settings batch before touching the app (one gate, reused in two places).
pub fn precheck_removable(paths: &[PathBuf], failure_code: ErrorCode) -> Result<(), AppError> {
    match paths.iter().find(|path| !is_removable(path)) {
        Some(refused) => Err(AppError::new(failure_code, "error.tool.removePathRefused")
            .with_technical(format!(
                "{} is outside the automatic-removal safety boundary",
                refused.display()
            ))),
        None => Ok(()),
    }
}

/// Delete by path (a file or a whole directory). An already-missing target counts as done; anything
/// outside the safety boundary is rejected. A directory may hold tens of thousands of session files,
/// so it goes to the blocking thread pool.
pub async fn remove_paths(
    paths: Vec<PathBuf>,
    failure_code: ErrorCode,
    failure_message_key: &'static str,
) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        remove_paths_blocking(&paths, failure_code, failure_message_key)
    })
    .await
    .map_err(|e| {
        AppError::new(ErrorCode::Internal, "error.command.joinFailed").with_technical(e.to_string())
    })?
}

fn remove_paths_blocking(
    paths: &[PathBuf],
    failure_code: ErrorCode,
    failure_message_key: &'static str,
) -> Result<(), AppError> {
    // Two phases: pre-check the whole batch first, and only enter the real deletion loop below once everything passes.
    precheck_removable(paths, failure_code)?;

    for path in paths {
        // Do not use `Path::is_dir`: it follows a final symlink. Explicitly
        // unlink the symlink itself so an external target is never traversed.
        let outcome = match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => std::fs::remove_file(path),
            Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
            Ok(_) => std::fs::remove_file(path),
            Err(error) => Err(error),
        };
        match outcome {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return Err(AppError::new(
                    ErrorCode::PermissionDenied,
                    "error.tool.removePathDenied",
                )
                .with_technical(format!("{}: {e}", path.display()))
                .with_remediation("error.remediation.checkPermissions"))
            }
            Err(e) => {
                return Err(AppError::new(failure_code, failure_message_key)
                    .with_technical(format!("{}: {e}", path.display())))
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "lifecycle_runner/tests.rs"]
mod tests;
