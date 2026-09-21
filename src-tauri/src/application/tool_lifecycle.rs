use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures::future::FutureExt;

use crate::adapters::{
    AdapterRegistry, AuthorizedUpdate, LifecycleContext, LifecycleNetworkPolicy, ToolAdapter,
};
use crate::application::tool_version_history::{history_write_failed, ToolVersionHistoryService};
use crate::compat::ccswitch::versioning::compare_versions;
use crate::domain::operation::phase;
use crate::domain::{
    validate_observed_tool_version, validate_tool_version, AppError, DownloadStrategy, ErrorCode,
    OperationId, OperationKind, OperationLogKind, OperationStatus, Tool, ToolId, ToolInstallSource,
    ToolStatus, ToolUpdateRecovery, UninstallOptions,
};
use crate::infrastructure::OperationManager;
use crate::platform::executor::{CommandCancellation, CommandExecutor, SystemExecutor};
use crate::platform::redact::{redact_secrets, truncate_tail};

/// Resolves ToolId -> adapter. Production uses `AdapterRegistry` and tests inject a stub, so
/// full-path use-case tests spawn no subprocess.
pub type AdapterResolver = Arc<dyn Fn(ToolId) -> Option<Box<dyn ToolAdapter>> + Send + Sync>;

enum PreparedUpdateRecovery {
    Available {
        target_version: String,
        source: ToolInstallSource,
    },
    Advisory(ToolUpdateRecovery),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleRequest {
    Install(ToolId),
    /// Carries the one authorization `authorize_update` produced for this
    /// click; `run` never re-checks the registry or the installation. Boxed:
    /// the observation and plan dwarf every other variant.
    Update(ToolId, Box<AuthorizedUpdate>),
    InstallVersion(ToolId, String),
    Repair(ToolId),
    Uninstall(ToolId, UninstallOptions),
}

impl LifecycleRequest {
    pub fn tool(&self) -> ToolId {
        match self {
            Self::Install(id)
            | Self::Update(id, _)
            | Self::InstallVersion(id, _)
            | Self::Repair(id)
            | Self::Uninstall(id, _) => *id,
        }
    }

    pub fn kind(&self) -> OperationKind {
        match self {
            Self::Install(_) => OperationKind::Install,
            Self::Update(_, _) => OperationKind::Update,
            Self::InstallVersion(_, _) => OperationKind::ChangeVersion,
            Self::Repair(_) => OperationKind::Repair,
            Self::Uninstall(_, _) => OperationKind::Uninstall,
        }
    }

    fn failure_code(&self) -> ErrorCode {
        match self {
            Self::Install(_) => ErrorCode::InstallFailed,
            Self::Update(_, _) | Self::InstallVersion(_, _) | Self::Repair(_) => {
                ErrorCode::UpdateFailed
            }
            Self::Uninstall(_, _) => ErrorCode::UninstallFailed,
        }
    }

    /// AI_RULES rule 8: branch on capability, never `if tool == ClaudeCode`.
    fn is_supported_by(&self, adapter: &dyn ToolAdapter) -> bool {
        let capabilities = adapter.capabilities();
        match self {
            Self::Install(_) => capabilities.can_install,
            Self::Update(_, _) => capabilities.can_update,
            Self::InstallVersion(_, _) => capabilities.can_manage_version,
            Self::Repair(_) => capabilities.can_repair,
            Self::Uninstall(_, _) => capabilities.can_uninstall,
        }
    }

    /// The manual fallback shown to the user when a capability is missing. Telling the user
    /// "you can install it yourself" after a failed uninstall is the copy bug recorded in the
    /// 3a final review; the `uninstallManually` key already existed, it was just never wired up.
    fn manual_remediation(&self) -> &'static str {
        match self {
            Self::Install(_)
            | Self::Update(_, _)
            | Self::InstallVersion(_, _)
            | Self::Repair(_) => "error.remediation.installManually",
            Self::Uninstall(_, _) => "error.remediation.uninstallManually",
        }
    }
}

pub struct ToolLifecycleService {
    operations: Arc<OperationManager>,
    executor: Arc<dyn CommandExecutor>,
    adapters: AdapterResolver,
    network: LifecycleNetworkPolicy,
    version_history: ToolVersionHistoryService,
}

impl ToolLifecycleService {
    pub fn new(
        operations: Arc<OperationManager>,
        executor: Arc<dyn CommandExecutor>,
        adapters: AdapterResolver,
        version_history: ToolVersionHistoryService,
    ) -> Self {
        Self {
            operations,
            executor,
            adapters,
            network: LifecycleNetworkPolicy::default(),
            version_history,
        }
    }

    pub fn system(
        operations: Arc<OperationManager>,
        strategy: DownloadStrategy,
        version_history: ToolVersionHistoryService,
    ) -> Self {
        let mut service = Self::new(
            operations,
            Arc::new(SystemExecutor),
            Arc::new(AdapterRegistry::get),
            version_history,
        );
        service.network = LifecycleNetworkPolicy::new(
            strategy,
            crate::compat::ccswitch::lifecycle::configured_proxy_url(),
        );
        service
    }

    /// Opens the operation **synchronously**: capability checks and the per-tool mutex both
    /// happen here, so an Ok at the command layer means the lock is already held (spec §41).
    pub fn begin(&self, request: &LifecycleRequest) -> Result<OperationId, AppError> {
        let adapter = self.resolve(request.tool())?;
        if !request.is_supported_by(adapter.as_ref()) {
            return Err(
                AppError::new(request.failure_code(), "error.tool.actionUnsupported")
                    .with_technical(format!(
                        "{:?} is not a capability of {}",
                        request.kind(),
                        request.tool().as_str()
                    ))
                    .with_remediation(request.manual_remediation()),
            );
        }
        let id = self
            .operations
            .begin_cancellable(request.kind(), request.tool())?;
        let _ = self.operations.update_cancellable_progress(
            &id,
            5,
            Some(phase::PREPARING.to_string()),
            true,
        );
        Ok(id)
    }

    /// The one update check per click. The adapter re-derives the plan from a
    /// fresh observation and inspection (a registry round trip plus a full
    /// installation enumeration) and proves it matches the approved preview;
    /// `run` then works from this authorization and never repeats the check.
    /// The command calls it before `begin`, so a stale preview or an
    /// unreachable registry is an immediate answer rather than a failed task.
    pub async fn authorize_update(
        &self,
        tool: ToolId,
        preview_fingerprint: &str,
    ) -> Result<AuthorizedUpdate, AppError> {
        let adapter = self.resolve(tool)?;
        let authorized = AssertUnwindSafe(adapter.validate_update(preview_fingerprint.to_string()))
            .catch_unwind()
            .await
            .unwrap_or_else(|payload| {
                Err(
                    AppError::new(ErrorCode::Internal, "error.tool.actionPanicked")
                        .with_technical(panic_summary(payload)),
                )
            })?;
        authorize_update_observation(&authorized.observed)?;
        Ok(authorized)
    }

    /// Runs and finalizes **asynchronously**. Every path calls `finish`, so no dangling
    /// Running is ever left behind — except when the pairing check fails: in that case this
    /// `id` is not this call's to finalize (it belongs either to another begin call's
    /// operation or to one that already reached a terminal state), and touching `finish` would
    /// corrupt someone else's result, so it just logs and returns.
    pub async fn run(&self, id: OperationId, request: LifecycleRequest) {
        if let Err(reason) = self.verify_pairing(&id, &request) {
            log::error!("refusing to run operation {}: {reason}", id.as_str());
            return;
        }
        let cancellation = match self.operations.cancellation(&id) {
            Ok(cancellation) => cancellation,
            Err(error) => {
                let _ = self.operations.finish(&id, Err(error));
                return;
            }
        };

        let recovery_tool = request.tool();
        // The authorization already holds the pre-update observation and the
        // proven owner, so the recovery baseline costs no further probe.
        let prepared_recovery = match &request {
            LifecycleRequest::Update(tool, authorized) => Some(
                AssertUnwindSafe(self.prepare_update_recovery(*tool, authorized))
                    .catch_unwind()
                    .await
                    .unwrap_or(PreparedUpdateRecovery::Advisory(
                        ToolUpdateRecovery::InspectionFailed,
                    )),
            ),
            _ => None,
        };

        // The execute() chain hangs third-party code such as adapter/upstream plan
        // construction off it; under `panic = "unwind"`, a panic that is not caught means
        // `finish` never runs and both the per-tool lock and the operation hang forever (only
        // a restart clears them). catch_unwind collapses a panic into an ordinary Err branch.
        let mut outcome = if cancellation.confirm_if_requested() {
            Err(cancelled_action())
        } else {
            AssertUnwindSafe(self.execute(&id, request, cancellation.clone()))
                .catch_unwind()
                .await
                .unwrap_or_else(|payload| {
                    Err(
                        AppError::new(ErrorCode::Internal, "error.tool.actionPanicked")
                            .with_technical(panic_summary(payload)),
                    )
                })
        };
        if cancellation.confirm_if_requested() {
            outcome = Err(cancelled_action());
        }

        let cancelled = cancellation.is_confirmed();
        let update_recovery = if outcome.is_err() && !cancelled {
            if let Some(prepared) = prepared_recovery {
                AssertUnwindSafe(self.failed_update_recovery(recovery_tool, prepared))
                    .catch_unwind()
                    .await
                    .unwrap_or(None)
            } else {
                None
            }
        } else {
            None
        };

        if cancelled {
            if let Err(error) = self.operations.finish_cancelled(&id) {
                log::warn!(
                    "failed to finish cancelled operation {}: {error}",
                    id.as_str()
                );
            }
            return;
        }
        if outcome.is_ok() {
            let _ = self
                .operations
                .update_progress(&id, 100, Some(phase::READY.to_string()));
        }
        if let Err(error) = self
            .operations
            .finish_tool_lifecycle(&id, outcome, update_recovery)
        {
            // A cancel request can win the small race between the last
            // checkpoint above and terminal transition. It still must be
            // explicitly confirmed before the lock is released.
            if error.message_key == "error.operation.cancelPending"
                && (cancellation.is_confirmed() || cancellation.confirm_if_requested())
            {
                if let Err(cancel_error) = self.operations.finish_cancelled(&id) {
                    log::warn!(
                        "failed to finish raced cancellation {}: {cancel_error}",
                        id.as_str()
                    );
                }
            } else {
                log::warn!("failed to finish operation {}: {error}", id.as_str());
            }
        }
    }

    async fn prepare_update_recovery(
        &self,
        tool: ToolId,
        authorized: &AuthorizedUpdate,
    ) -> PreparedUpdateRecovery {
        let Some(current_version) = authorized
            .observed
            .version
            .as_deref()
            .and_then(|version| validate_tool_version(version).ok())
        else {
            return PreparedUpdateRecovery::Advisory(ToolUpdateRecovery::HistoryUnavailable);
        };
        let Ok(history) = self.version_history.history(tool).await else {
            return PreparedUpdateRecovery::Advisory(ToolUpdateRecovery::InspectionFailed);
        };
        let Some(latest) = history
            .events
            .first()
            .filter(|event| event.to_version == current_version)
        else {
            return PreparedUpdateRecovery::Advisory(ToolUpdateRecovery::HistoryUnavailable);
        };
        let source = authorized.source;
        if source != latest.source {
            return PreparedUpdateRecovery::Advisory(ToolUpdateRecovery::OwnershipChanged);
        }
        let can_manage_version = self
            .resolve(tool)
            .map(|adapter| adapter.capabilities().can_manage_version)
            .unwrap_or(false);
        if !can_manage_version || !supports_version_restore(source) {
            return PreparedUpdateRecovery::Advisory(ToolUpdateRecovery::OwnerUnsupported);
        }
        PreparedUpdateRecovery::Available {
            target_version: current_version,
            source,
        }
    }

    async fn failed_update_recovery(
        &self,
        tool: ToolId,
        prepared: PreparedUpdateRecovery,
    ) -> Option<ToolUpdateRecovery> {
        let adapter = self.resolve(tool).ok()?;
        let observed = adapter.detect().await.ok()?;
        if observed.status != ToolStatus::Broken {
            return None;
        }
        match prepared {
            PreparedUpdateRecovery::Advisory(recovery) => Some(recovery),
            PreparedUpdateRecovery::Available {
                target_version,
                source,
            } => match adapter.install_source().await {
                Ok(current_source) if current_source == source => {
                    ToolUpdateRecovery::available(target_version).ok()
                }
                Ok(_) => Some(ToolUpdateRecovery::OwnershipChanged),
                Err(_) => Some(ToolUpdateRecovery::InspectionFailed),
            },
        }
    }

    /// Verified before running the adapter action: this `OperationId` really exists, is still
    /// Running, and its kind/tool match the `request` the caller passed in (spec §41). If the
    /// check fails, nothing runs — a mismatched id would let another tool's real
    /// uninstall/install action run without the protection of its per-tool lock.
    fn verify_pairing(&self, id: &OperationId, request: &LifecycleRequest) -> Result<(), String> {
        let operation = self
            .operations
            .get(id)
            .ok_or_else(|| "no such operation".to_string())?;
        if operation.status != OperationStatus::Running {
            return Err(format!("operation is {:?}, not Running", operation.status));
        }
        if operation.kind != request.kind() {
            return Err(format!(
                "kind mismatch: operation is {:?}, request is {:?}",
                operation.kind,
                request.kind()
            ));
        }
        if operation.tool != Some(request.tool()) {
            return Err(format!(
                "tool mismatch: operation is {:?}, request is {:?}",
                operation.tool,
                request.tool()
            ));
        }
        Ok(())
    }

    /// Returns the post-action detection that `verify` accepted. It is the only
    /// tool state this run proved, so it is what the finished operation
    /// publishes to the renderer.
    async fn execute(
        &self,
        id: &OperationId,
        request: LifecycleRequest,
        cancellation: CommandCancellation,
    ) -> Result<Tool, AppError> {
        let adapter = self.resolve(request.tool())?;
        let ctx = self.context(id, cancellation);
        let checked_request = request.clone();
        let (from_version, expected_update_version) = match &request {
            // The authorization is the pre-update observation: no second probe.
            LifecycleRequest::Update(_, authorized) => (
                Some(
                    authorized
                        .observed
                        .version
                        .as_deref()
                        .map(validate_observed_tool_version)
                        .transpose()?,
                ),
                Some(authorized.target_version.clone()),
            ),
            LifecycleRequest::Install(_) | LifecycleRequest::InstallVersion(_, _) => {
                let observed = adapter
                    .detect()
                    .await
                    .map_err(|error| relabel(&checked_request, error))?;
                (
                    Some(
                        observed
                            .version
                            .as_deref()
                            .map(validate_observed_tool_version)
                            .transpose()?,
                    ),
                    None,
                )
            }
            LifecycleRequest::Repair(_) | LifecycleRequest::Uninstall(_, _) => (None, None),
        };

        let action = match request {
            LifecycleRequest::Install(_) => adapter.install(ctx),
            LifecycleRequest::Update(_, authorized) => adapter.update(ctx, *authorized),
            LifecycleRequest::InstallVersion(_, version) => adapter.install_version(ctx, version),
            LifecycleRequest::Repair(_) => adapter.repair(ctx),
            LifecycleRequest::Uninstall(_, options) => adapter.uninstall(ctx, options),
        };
        action
            .await
            .map_err(|error| relabel(&checked_request, error))?;

        let _ = self
            .operations
            .update_progress(id, 90, Some(phase::CHECKING.to_string()));
        let tool = adapter
            .detect()
            .await
            .map_err(|error| relabel(&checked_request, error))?;
        verify(
            &checked_request,
            &tool,
            expected_update_version.as_deref(),
            from_version.as_ref().and_then(|version| version.as_deref()),
        )?;

        if let Some(from_version) = from_version {
            let to_version = tool.version.clone().ok_or_else(|| {
                history_write_failed("verified installed tool did not expose a version")
            })?;
            let source = match adapter
                .install_source()
                .await
                .map_err(|error| relabel(&checked_request, error))?
            {
                // The post-action Tool was verified as installed. An owner probe
                // that cannot find the launcher is therefore unknown, not a
                // truthful `notInstalled` history value.
                ToolInstallSource::NotInstalled => ToolInstallSource::Unmanaged,
                source => source,
            };
            self.version_history
                .record_verified(
                    checked_request.tool(),
                    from_version,
                    to_version,
                    source,
                    checked_request.kind(),
                )
                .await?;
        }
        Ok(tool)
    }

    fn context(&self, id: &OperationId, cancellation: CommandCancellation) -> LifecycleContext {
        let progress_operations = self.operations.clone();
        let progress_id = id.clone();
        let log_operations = self.operations.clone();
        let log_id = id.clone();
        LifecycleContext {
            executor: self.executor.clone(),
            cancellation,
            // A failed progress push (the operation already reached a terminal state, or was trimmed) does not affect the action itself.
            progress: Arc::new(move |value, message_key: &'static str| {
                let can_cancel = matches!(message_key, phase::PREPARING | phase::DOWNLOADING);
                let _ = progress_operations.update_cancellable_progress(
                    &progress_id,
                    value,
                    Some(message_key.to_string()),
                    can_cancel,
                );
            }),
            // Same as logging: late reader callbacks are silently dropped once the operation is in a terminal state.
            logger: Arc::new(
                move |kind: OperationLogKind,
                      message_key: Option<&'static str>,
                      detail: Option<String>| {
                    if let Some(message_key) = message_key {
                        let _ = log_operations.append_log_message(&log_id, kind, message_key);
                    } else if let Some(detail) = detail {
                        let _ = log_operations.append_log_detail(&log_id, kind, detail);
                    }
                },
            ),
            network: self.network.clone(),
        }
    }

    fn resolve(&self, id: ToolId) -> Result<Box<dyn ToolAdapter>, AppError> {
        (self.adapters)(id).ok_or_else(|| {
            AppError::new(ErrorCode::ToolNotFound, "error.tool.notFound")
                .with_technical(id.as_str().to_string())
        })
    }
}

/// The observation behind an authorization must still say an update is due:
/// status `UpdateAvailable`, the capability on and an exact, valid target.
/// Anything else means the preview the user approved no longer describes the
/// tool, whichever adapter produced the authorization.
fn authorize_update_observation(tool: &Tool) -> Result<(), AppError> {
    let target = tool
        .latest_version
        .as_deref()
        .map(validate_tool_version)
        .transpose()?;
    if tool.status == ToolStatus::UpdateAvailable
        && tool.capabilities.can_update
        && target.is_some()
    {
        return Ok(());
    }
    Err(AppError::new(
        ErrorCode::UpdatePreviewStale,
        "error.tool.updatePreviewStale",
    )
    .with_technical("tool observation no longer authorizes an exact update target")
    .with_remediation("error.remediation.recheckUpdate"))
}

fn supports_version_restore(source: ToolInstallSource) -> bool {
    matches!(
        source,
        ToolInstallSource::Npm
            | ToolInstallSource::Pnpm
            | ToolInstallSource::Bun
            | ToolInstallSource::Volta
            | ToolInstallSource::Uv
            | ToolInstallSource::Pipx
    )
}

/// Re-detect afterwards: a command returning 0 does not mean the job got done (upstream
/// recorded `codex update` exiting 0 even with the platform binary missing, a false success).
/// The probe result is authoritative.
fn verify(
    request: &LifecycleRequest,
    tool: &crate::domain::Tool,
    expected_update_version: Option<&str>,
    from_version: Option<&str>,
) -> Result<(), AppError> {
    let status = tool.status;
    match request {
        LifecycleRequest::Install(_) | LifecycleRequest::Repair(_) => {
            if matches!(status, ToolStatus::Installed | ToolStatus::UpdateAvailable) {
                Ok(())
            } else {
                Err(
                    AppError::new(request.failure_code(), "error.tool.verifyFailed")
                        .with_technical(format!("post-action status is {status:?}"))
                        .with_remediation("error.remediation.retryOrViewDetails"),
                )
            }
        }
        LifecycleRequest::Update(_, _) => {
            if matches!(status, ToolStatus::Installed | ToolStatus::UpdateAvailable)
                && update_made_progress(
                    tool.version.as_deref(),
                    expected_update_version,
                    from_version,
                )
            {
                Ok(())
            } else {
                Err(
                    AppError::new(request.failure_code(), "error.tool.verifyFailed")
                        .with_technical(format!(
                            "post-action version is {:?} with status {status:?}, expected {:?} (was {:?})",
                            tool.version, expected_update_version, from_version
                        ))
                        .with_remediation("error.remediation.retryOrViewDetails"),
                )
            }
        }
        LifecycleRequest::InstallVersion(_, expected) => {
            if matches!(status, ToolStatus::Installed | ToolStatus::UpdateAvailable)
                && tool.version.as_deref() == Some(expected.as_str())
            {
                Ok(())
            } else {
                Err(
                    AppError::new(request.failure_code(), "error.tool.verifyFailed")
                        .with_technical(format!(
                            "post-action version is {:?}, expected {expected}",
                            tool.version
                        ))
                        .with_remediation("error.remediation.retryOrViewDetails"),
                )
            }
        }
        LifecycleRequest::Uninstall(_, _) => {
            if status == ToolStatus::NotInstalled {
                Ok(())
            } else {
                Err(
                    AppError::new(ErrorCode::UninstallFailed, "error.tool.uninstallIncomplete")
                        .with_technical(format!("post-action status is {status:?}"))
                        .with_remediation("error.remediation.uninstallManually"),
                )
            }
        }
    }
}

/// Package managers install the authorised target exactly; self-updaters
/// (`claude update`, `hermes update`) and Homebrew decide the version
/// themselves. Reaching or passing the target, or a genuine advance from the
/// pre-update baseline, proves the update ran. An unchanged or regressed
/// version (Codex's exit-0 no-op, an update applied to a shadowed copy) stays
/// a failure.
fn update_made_progress(
    observed: Option<&str>,
    target: Option<&str>,
    baseline: Option<&str>,
) -> bool {
    let Some(observed) = observed else {
        return false;
    };
    let reached = target.is_some_and(|target| {
        observed == target
            || matches!(
                compare_versions(observed, target),
                Some(std::cmp::Ordering::Greater)
            )
    });
    let advanced = baseline.is_some_and(|baseline| {
        matches!(
            compare_versions(observed, baseline),
            Some(std::cmp::Ordering::Greater)
        )
    });
    reached || advanced
}

/// `INTERNAL` has no branching value for the user (the frontend can only show a generic
/// failure), so it is swapped for an action-specific code; `message_key` stays as is, so the
/// concrete reason (timeout / failed to start) remains translatable.
/// The other codes (PERMISSION_DENIED, NETWORK_ERROR, TOOL_NOT_FOUND, ...) are actionable and
/// are kept unchanged.
fn relabel(request: &LifecycleRequest, error: AppError) -> AppError {
    if error.code != ErrorCode::Internal {
        return error;
    }
    AppError {
        code: request.failure_code(),
        ..error
    }
}

fn cancelled_action() -> AppError {
    AppError::new(ErrorCode::Internal, "error.operation.cancelled")
}

/// Display summary of a panic payload. `&str` (a literal panic) and `String` (`format!` or a
/// parameterized `panic!`) cover the vast majority of cases; when neither matches, a fallback
/// message containing no caller data is used — `technical_message` must never accidentally
/// carry sensitive content such as subprocess output. The panic message itself may be
/// upstream code splicing subprocess output into `panic!()`, so it still passes through both
/// the redact and truncate gates (the same standard as `technical_message` on the adapter
/// failure path).
fn panic_summary(payload: Box<dyn Any + Send>) -> String {
    let raw = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "tool lifecycle action panicked with a non-string payload".to_string()
    };
    truncate_tail(&redact_secrets(&raw), 8, 512)
}

// Split into a subfile because this file plus its full test suite would
// exceed the project's 500-line-per-file limit; see AI_RULES.
#[cfg(test)]
#[path = "tool_lifecycle/tests.rs"]
mod tests;
