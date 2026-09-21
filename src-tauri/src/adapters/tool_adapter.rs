use futures::future::BoxFuture;
use std::path::PathBuf;
use std::sync::Arc;

use crate::compat::ccswitch::update_preview::CheckedUpdatePlan;
use crate::domain::{
    AppError, DownloadStrategy, OperationLogKind, TerminalAppId, Tool, ToolCapabilities, ToolId,
    ToolInstallSource, ToolUpdatePreview, ToolVersionCatalog, UninstallOptions,
};
use crate::platform::executor::{CommandCancellation, CommandExecutor};
use crate::platform::TerminalLauncher;

/// Progress callback: `(progress, message_key)`. message_key may only be a constant from
/// `domain::operation::phase`, which guarantees the frontend always receives a translatable phase name.
pub type ProgressReporter = Arc<dyn Fn(u8, &'static str) + Send + Sync>;
pub type OperationLogger =
    Arc<dyn Fn(OperationLogKind, Option<&'static str>, Option<String>) + Send + Sync>;

/// Download strategy that flows only inside the native layer. A proxy URL may carry credentials, so
/// it is not serialized, never enters the operation log, and is never given to the renderer.
#[derive(Clone)]
pub struct LifecycleNetworkPolicy {
    pub download_strategy: DownloadStrategy,
    pub(crate) proxy_url: Option<String>,
}

impl Default for LifecycleNetworkPolicy {
    fn default() -> Self {
        // Low-level callers/tests must opt into network mutation explicitly.
        // Product commands always construct this from normalized settings.
        Self::new(DownloadStrategy::OfficialOnly, None)
    }
}

impl LifecycleNetworkPolicy {
    pub fn new(download_strategy: DownloadStrategy, proxy_url: Option<String>) -> Self {
        Self {
            download_strategy,
            proxy_url,
        }
    }
}

/// Dependency-injection bundle for lifecycle actions. Both the executor and the progress sink are
/// trait objects, so adapter tests spawn no process and never touch the OperationManager.
#[derive(Clone)]
pub struct LifecycleContext {
    pub executor: Arc<dyn CommandExecutor>,
    pub cancellation: CommandCancellation,
    pub progress: ProgressReporter,
    pub logger: OperationLogger,
    pub network: LifecycleNetworkPolicy,
}

#[derive(Clone)]
pub struct LaunchContext {
    pub launcher: Arc<dyn TerminalLauncher>,
    /// The terminal the user chose. None = never chosen, so the platform layer uses the system default.
    pub terminal_app: Option<TerminalAppId>,
}

#[derive(Clone)]
pub struct VersionQueryContext {
    pub executor: Arc<dyn CommandExecutor>,
    pub network: LifecycleNetworkPolicy,
}

/// One authorized update: the observation that allowed it, the exact target
/// and the plan the adapter checked against the approved preview. The check
/// costs a registry round trip plus a full installation enumeration, so it
/// runs once per click and the result is threaded through the lifecycle
/// unchanged (`validate_update` -> `LifecycleRequest::Update` -> `update`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedUpdate {
    /// Pre-update baseline: the lifecycle verifies progress against it and
    /// records it in the version history; it is never observed again before
    /// the action runs.
    pub observed: Tool,
    pub target_version: String,
    /// Proven owner of the launcher the plan acts on.
    pub source: ToolInstallSource,
    /// The adapter's own checked plan. Adapters that run no plan (test
    /// doubles) carry `None`; the upstream adapter refuses to update without it.
    pub plan: Option<CheckedUpdatePlan>,
}

impl LifecycleContext {
    pub fn report(&self, progress: u8, message_key: &'static str) {
        (self.progress)(progress, message_key);
    }

    pub fn log_message(&self, kind: OperationLogKind, message_key: &'static str) {
        (self.logger)(kind, Some(message_key), None);
    }

    pub fn log_detail(&self, kind: OperationLogKind, detail: String) {
        (self.logger)(kind, None, Some(detail));
    }
}

/// The unified tool interface of spec §11. `async fn` is not used because async-trait is not on the
/// dependency list; returning a BoxFuture keeps the trait object safe.
pub trait ToolAdapter: Send + Sync {
    fn id(&self) -> ToolId;

    fn capabilities(&self) -> ToolCapabilities;

    fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>>;

    /// The read-only local half of detection: installed or not, which version, and whether it runs —
    /// never touching the network. The returned `Tool` therefore has no `latest_version` and can never
    /// be `UpdateAvailable`.
    ///
    /// The default is simply `detect` — for an adapter that is offline anyway that is already the whole
    /// answer; any implementation whose `detect` looks up the latest version must override this, or the
    /// first screen would again wait on the network.
    fn detect_local(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        self.detect()
    }

    /// Proven owner of the currently selected launcher. Lifecycle history uses
    /// this only after `detect + verify`; a default adapter stays conservative.
    fn install_source(&self) -> BoxFuture<'_, Result<ToolInstallSource, AppError>> {
        Box::pin(async { Ok(ToolInstallSource::Unmanaged) })
    }

    fn install(&self, ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>>;

    fn update_preview(&self) -> BoxFuture<'_, Result<ToolUpdatePreview, AppError>> {
        Box::pin(async {
            Err(AppError::new(
                crate::domain::ErrorCode::UpdateFailed,
                "error.tool.actionUnsupported",
            ))
        })
    }

    /// Re-derives the update plan from a fresh observation and inspection and
    /// proves it matches the preview the user approved. The lifecycle calls it
    /// exactly once per click; what it returns is what `update` receives.
    fn validate_update(
        &self,
        preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>>;

    fn update(
        &self,
        ctx: LifecycleContext,
        authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), AppError>>;

    fn install_version(
        &self,
        _ctx: LifecycleContext,
        _version: String,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async {
            Err(AppError::new(
                crate::domain::ErrorCode::UpdateFailed,
                "error.tool.actionUnsupported",
            ))
        })
    }

    fn version_catalog(
        &self,
        _ctx: VersionQueryContext,
    ) -> BoxFuture<'_, Result<ToolVersionCatalog, AppError>> {
        Box::pin(async {
            Err(AppError::new(
                crate::domain::ErrorCode::UpdateFailed,
                "error.tool.actionUnsupported",
            ))
        })
    }

    /// Repair is a distinct operation so a broken install never gets presented
    /// as an ordinary update. Adapters opt in only for a proven recovery plan.
    fn repair(&self, _ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async {
            Err(AppError::new(
                crate::domain::ErrorCode::UpdateFailed,
                "error.tool.actionUnsupported",
            ))
        })
    }

    fn uninstall(
        &self,
        ctx: LifecycleContext,
        options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), AppError>>;

    /// An interactive launch is not a lifecycle operation; refusing by default keeps older test adapters
    /// minimal, and a production adapter only reaches its own implementation when `can_launch` is true.
    fn launch(
        &self,
        _ctx: LaunchContext,
        _working_directory: PathBuf,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async {
            Err(AppError::new(
                crate::domain::ErrorCode::LaunchFailed,
                "error.tool.actionUnsupported",
            ))
        })
    }
}
