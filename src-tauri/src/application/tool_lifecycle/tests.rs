use super::{
    authorize_update_observation, AdapterResolver, LifecycleRequest, ToolLifecycleService,
};
use crate::adapters::{AuthorizedUpdate, LifecycleContext, ToolAdapter};
use crate::application::tool_version_history::ToolVersionHistoryService;
use crate::domain::operation::phase;
use crate::domain::{
    AppError, ErrorCode, Operation, OperationOutput, OperationStatus, Tool, ToolCapabilities,
    ToolId, ToolInstallSource, ToolStatus, UninstallOptions,
};
use crate::infrastructure::{OperationEvents, OperationManager};
use crate::platform::command::CommandSpec;
use crate::platform::executor::{CommandExecutor, CommandOutput};
use futures::future::BoxFuture;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingEvents {
    seen: Mutex<Vec<Operation>>,
}

impl OperationEvents for Arc<RecordingEvents> {
    fn emit(&self, operation: &Operation) {
        self.seen
            .lock()
            .expect("events lock")
            .push(operation.clone());
    }
}

impl RecordingEvents {
    fn message_keys(&self) -> Vec<String> {
        self.seen
            .lock()
            .expect("events lock")
            .iter()
            .filter_map(|operation| operation.message_key.clone())
            .collect()
    }

    fn last(&self) -> Operation {
        self.seen
            .lock()
            .expect("events lock")
            .last()
            .cloned()
            .expect("at least one event")
    }
}

struct NeverExecutor;

impl CommandExecutor for NeverExecutor {
    fn execute(&self, _spec: CommandSpec) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
        Box::pin(async { panic!("the use case must never reach the executor in these tests") })
    }
}

/// The observation and target a stub hands back from `validate_update`: the
/// same shape the upstream adapter produces, minus the plan it would run.
fn authorization_for(id: ToolId, target: &str) -> AuthorizedUpdate {
    AuthorizedUpdate {
        observed: Tool {
            id,
            name: "Stub".to_string(),
            description_key: "tool.stub.description".to_string(),
            discovery: crate::domain::ToolDiscovery::for_tool(id),
            status: ToolStatus::UpdateAvailable,
            version: Some("1.0.0".to_string()),
            latest_version: Some(target.to_string()),
            capabilities: ToolCapabilities {
                can_update: true,
                ..ToolCapabilities::default()
            },
            sessions_inside_settings: false,
            environment: Some("macos".to_string()),
            configuration_shared_with: None,
        },
        target_version: target.to_string(),
        source: ToolInstallSource::Unmanaged,
        plan: None,
    }
}

struct StubAdapter {
    id: ToolId,
    capabilities: ToolCapabilities,
    action: Result<(), AppError>,
    detected: ToolStatus,
    update_target: Option<String>,
    post_update_version: Option<String>,
    /// Every registry-backed observation the lifecycle asked for.
    detect_count: AtomicUsize,
    /// Every owner probe the lifecycle asked for.
    install_source_count: AtomicUsize,
    calls: Arc<Mutex<Vec<String>>>,
}

impl StubAdapter {
    fn new(id: ToolId, detected: ToolStatus) -> Self {
        Self {
            id,
            capabilities: ToolCapabilities {
                can_install: true,
                can_update: true,
                can_uninstall: true,
                can_repair: true,
                can_manage_version: true,
                ..ToolCapabilities::default()
            },
            action: Ok(()),
            detected,
            update_target: None,
            post_update_version: None,
            detect_count: AtomicUsize::new(0),
            install_source_count: AtomicUsize::new(0),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn with_update_target(mut self, target: &str) -> Self {
        self.update_target = Some(target.to_string());
        self
    }

    /// The version the tool reports after the update ran (defaults to the target).
    fn with_post_update_version(mut self, version: &str) -> Self {
        self.post_update_version = Some(version.to_string());
        self
    }

    fn record(&self, what: &str) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(what.to_string());
    }
}

impl ToolAdapter for StubAdapter {
    fn id(&self) -> ToolId {
        self.id
    }

    fn capabilities(&self) -> ToolCapabilities {
        self.capabilities
    }

    fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        self.detect_count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let status = self.detected;
            let version = match status {
                ToolStatus::NotInstalled => None,
                ToolStatus::Installed if self.update_target.is_some() => self
                    .post_update_version
                    .clone()
                    .or_else(|| self.update_target.clone()),
                _ => Some("1.0.0".to_string()),
            };
            Ok(Tool {
                id: self.id,
                name: "Stub".to_string(),
                description_key: "tool.stub.description".to_string(),
                discovery: crate::domain::ToolDiscovery::for_tool(self.id),
                status,
                version,
                latest_version: None,
                capabilities: self.capabilities,
                sessions_inside_settings: false,
                environment: Some("macos".to_string()),
                configuration_shared_with: None,
            })
        })
    }

    fn install_source(&self) -> BoxFuture<'_, Result<ToolInstallSource, AppError>> {
        self.install_source_count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(ToolInstallSource::Unmanaged) })
    }

    fn install(&self, ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        self.record("install");
        ctx.report(50, phase::INSTALLING);
        Box::pin(async move { self.action.clone() })
    }

    fn validate_update(
        &self,
        preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
        self.record(&format!("validate-update {preview_fingerprint}"));
        let target = self
            .update_target
            .clone()
            .unwrap_or_else(|| "2.0.0".to_string());
        let authorized = authorization_for(self.id, &target);
        Box::pin(async move { Ok(authorized) })
    }

    fn update(
        &self,
        ctx: LifecycleContext,
        authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        self.record(&format!("update {}", authorized.target_version));
        ctx.report(50, phase::INSTALLING);
        Box::pin(async move { self.action.clone() })
    }

    fn install_version(
        &self,
        ctx: LifecycleContext,
        version: String,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        self.record(&format!("install-version {version}"));
        ctx.report(50, phase::INSTALLING);
        Box::pin(async move { self.action.clone() })
    }

    fn repair(&self, ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        self.record("repair");
        ctx.report(50, phase::INSTALLING);
        Box::pin(async move { self.action.clone() })
    }

    fn uninstall(
        &self,
        ctx: LifecycleContext,
        options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        self.record(&format!(
            "uninstall settings={} cache={}",
            options.remove_settings, options.remove_cache
        ));
        ctx.report(50, phase::INSTALLING);
        Box::pin(async move { self.action.clone() })
    }
}

fn service_with(
    adapter: StubAdapter,
) -> (
    ToolLifecycleService,
    Arc<RecordingEvents>,
    Arc<Mutex<Vec<String>>>,
) {
    let (service, events, shared) = service_with_shared(adapter);
    let calls = shared.calls.clone();
    (service, events, calls)
}

/// Mirrors the command layer: one `authorize_update` per click, after which
/// the authorization travels inside the request and `run` trusts it.
async fn authorized_update(service: &ToolLifecycleService, fingerprint: &str) -> LifecycleRequest {
    LifecycleRequest::Update(
        ToolId::Codex,
        Box::new(
            service
                .authorize_update(ToolId::Codex, fingerprint)
                .await
                .expect("update authorized"),
        ),
    )
}

fn service_with_shared(
    adapter: StubAdapter,
) -> (ToolLifecycleService, Arc<RecordingEvents>, Arc<StubAdapter>) {
    let events = Arc::new(RecordingEvents::default());
    let operations = Arc::new(OperationManager::new(Box::new(events.clone())));
    let shared = Arc::new(adapter);
    let resolver = {
        let shared = shared.clone();
        Arc::new(move |id: ToolId| {
            if id == shared.id {
                Some(Box::new(SharedAdapter(shared.clone())) as Box<dyn ToolAdapter>)
            } else {
                None
            }
        })
    };
    (
        ToolLifecycleService::new(
            operations,
            Arc::new(NeverExecutor),
            resolver,
            ToolVersionHistoryService::discarding(),
        ),
        events,
        shared,
    )
}

/// `AdapterResolver` requires a `Box<dyn ToolAdapter>` while the tests want to share one piece of
/// state, so a thin forwarding wrapper is enough.
struct SharedAdapter(Arc<StubAdapter>);

impl ToolAdapter for SharedAdapter {
    fn id(&self) -> ToolId {
        self.0.id()
    }
    fn capabilities(&self) -> ToolCapabilities {
        self.0.capabilities()
    }
    fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        self.0.detect()
    }
    fn install_source(&self) -> BoxFuture<'_, Result<ToolInstallSource, AppError>> {
        self.0.install_source()
    }
    fn install(&self, ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        self.0.install(ctx)
    }
    fn validate_update(
        &self,
        preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
        self.0.validate_update(preview_fingerprint)
    }
    fn update(
        &self,
        ctx: LifecycleContext,
        authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        self.0.update(ctx, authorized)
    }
    fn install_version(
        &self,
        ctx: LifecycleContext,
        version: String,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        self.0.install_version(ctx, version)
    }
    fn repair(&self, ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        self.0.repair(ctx)
    }
    fn uninstall(
        &self,
        ctx: LifecycleContext,
        options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        self.0.uninstall(ctx, options)
    }
}

#[tokio::test]
async fn a_successful_install_walks_the_whole_section_30_narrative() {
    let (service, events, calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::Installed));
    let request = LifecycleRequest::Install(ToolId::Codex);
    let id = service.begin(&request).expect("install begins");
    service.run(id.clone(), request.clone()).await;

    assert_eq!(
        *calls.lock().expect("calls lock"),
        vec!["install".to_string()]
    );
    let keys = events.message_keys();
    assert_eq!(keys.first().map(String::as_str), Some(phase::PREPARING));
    assert!(keys.iter().any(|key| key == phase::INSTALLING));
    assert!(keys.iter().any(|key| key == phase::CHECKING));
    assert_eq!(keys.last().map(String::as_str), Some(phase::READY));

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Success);
    assert_eq!(final_operation.progress, 100);
    assert_eq!(final_operation.error, None);
}

#[tokio::test]
async fn a_repair_is_a_distinct_operation_and_rechecks_the_tool() {
    let (service, events, calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::Installed));
    let request = LifecycleRequest::Repair(ToolId::Codex);
    let id = service.begin(&request).expect("repair begins");
    service.run(id, request).await;

    assert_eq!(*calls.lock().expect("calls lock"), vec!["repair"]);
    let final_operation = events.last();
    assert_eq!(final_operation.kind, crate::domain::OperationKind::Repair);
    assert_eq!(final_operation.status, OperationStatus::Success);
    assert!(events
        .message_keys()
        .iter()
        .any(|key| key == phase::CHECKING));
}

#[tokio::test]
async fn a_version_change_preserves_the_exact_target_and_verifies_it() {
    let (service, events, calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::Installed));
    let request = LifecycleRequest::InstallVersion(ToolId::Codex, "1.0.0".to_string());
    let id = service.begin(&request).expect("version change begins");
    service.run(id, request).await;

    assert_eq!(
        *calls.lock().expect("calls lock"),
        vec!["install-version 1.0.0"]
    );
    let final_operation = events.last();
    assert_eq!(
        final_operation.kind,
        crate::domain::OperationKind::ChangeVersion
    );
    assert_eq!(final_operation.status, OperationStatus::Success);
}

#[tokio::test]
async fn a_version_change_that_does_not_reach_the_target_fails_verification() {
    let (service, events, _calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::Installed));
    let request = LifecycleRequest::InstallVersion(ToolId::Codex, "2.0.0".to_string());
    let id = service.begin(&request).expect("version change begins");
    service.run(id, request).await;

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Failed);
    assert_eq!(
        final_operation
            .error
            .expect("verification error")
            .message_key,
        "error.tool.verifyFailed"
    );
}

#[tokio::test]
async fn a_second_mutation_on_the_same_tool_is_refused_before_any_work_starts() {
    let (service, _events, calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::Installed));
    let first = LifecycleRequest::Install(ToolId::Codex);
    let _id = service.begin(&first).expect("first install begins");
    let error = service
        .begin(&LifecycleRequest::Update(
            ToolId::Codex,
            Box::new(authorization_for(ToolId::Codex, "2.0.0")),
        ))
        .expect_err("the tool is busy");
    assert_eq!(error.code, ErrorCode::OperationConflict);
    assert!(calls.lock().expect("calls lock").is_empty());
}

#[tokio::test]
async fn an_adapter_failure_becomes_the_operation_error_without_leaking_details() {
    let mut adapter = StubAdapter::new(ToolId::Codex, ToolStatus::NotInstalled);
    adapter.action = Err(
        AppError::new(ErrorCode::InstallFailed, "error.tool.installFailed")
            .with_technical("npm ERR! code E404"),
    );
    let (service, events, _calls) = service_with(adapter);
    let request = LifecycleRequest::Install(ToolId::Codex);
    let id = service.begin(&request).expect("install begins");
    service.run(id, request).await;

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Failed);
    let error = final_operation.error.expect("a failure carries an error");
    assert_eq!(error.code, ErrorCode::InstallFailed);
    assert_eq!(error.message_key, "error.tool.installFailed");
    assert_eq!(
        error.technical_message.as_deref(),
        Some("npm ERR! code E404")
    );
}

#[tokio::test]
async fn an_install_that_does_not_show_up_in_detect_is_reported_as_failed() {
    let (service, events, _calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::NotInstalled));
    let request = LifecycleRequest::Install(ToolId::Codex);
    let id = service.begin(&request).expect("install begins");
    service.run(id, request).await;

    let error = events.last().error.expect("verification failed");
    assert_eq!(error.code, ErrorCode::InstallFailed);
    assert_eq!(error.message_key, "error.tool.verifyFailed");
}

/// An adapter whose fresh observation says the update is no longer due.
struct LostUpdateAuthorityAdapter;

impl LostUpdateAuthorityAdapter {
    fn observed() -> Tool {
        Tool {
            id: ToolId::Codex,
            name: "Codex".to_string(),
            description_key: "tool.codex.description".to_string(),
            discovery: crate::domain::ToolDiscovery::for_tool(ToolId::Codex),
            status: ToolStatus::Installed,
            version: Some("1.0.0".to_string()),
            latest_version: None,
            capabilities: ToolCapabilities {
                can_update: true,
                ..ToolCapabilities::default()
            },
            sessions_inside_settings: false,
            environment: Some("test".to_string()),
            configuration_shared_with: None,
        }
    }
}

impl ToolAdapter for LostUpdateAuthorityAdapter {
    fn id(&self) -> ToolId {
        ToolId::Codex
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            can_update: true,
            ..ToolCapabilities::default()
        }
    }

    fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        Box::pin(async { Ok(Self::observed()) })
    }

    fn install(&self, _ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { panic!("install is not part of this test") })
    }

    fn validate_update(
        &self,
        _preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
        Box::pin(async {
            Ok(AuthorizedUpdate {
                observed: Self::observed(),
                target_version: "2.0.0".to_string(),
                source: ToolInstallSource::Unmanaged,
                plan: None,
            })
        })
    }

    fn update(
        &self,
        _ctx: LifecycleContext,
        _authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { panic!("update must not run without an authorized target") })
    }

    fn uninstall(
        &self,
        _ctx: LifecycleContext,
        _options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { panic!("uninstall is not part of this test") })
    }
}

/// The service, not the adapter, owns the rule that an authorization must
/// rest on an observation that still says `UpdateAvailable`.
#[tokio::test]
async fn an_observation_that_no_longer_allows_an_update_is_refused_before_the_lock() {
    let events = Arc::new(RecordingEvents::default());
    let operations = Arc::new(OperationManager::new(Box::new(events.clone())));
    let service = ToolLifecycleService::new(
        operations,
        Arc::new(NeverExecutor),
        Arc::new(|id| {
            (id == ToolId::Codex)
                .then(|| Box::new(LostUpdateAuthorityAdapter) as Box<dyn ToolAdapter>)
        }),
        ToolVersionHistoryService::discarding(),
    );
    let error = service
        .authorize_update(ToolId::Codex, &"a".repeat(64))
        .await
        .expect_err("authority lost");

    assert_eq!(error.code, ErrorCode::UpdatePreviewStale);
    assert!(
        events.seen.lock().expect("events lock").is_empty(),
        "a refused authorization never begins an operation"
    );
}

#[test]
fn an_update_authorization_requires_a_fresh_update_available_observation() {
    let mut current = authorization_for(ToolId::Codex, "2.0.0").observed;
    assert!(authorize_update_observation(&current).is_ok());

    current.status = ToolStatus::Installed;
    let current_error = authorize_update_observation(&current).unwrap_err();
    assert_eq!(current_error.code, ErrorCode::UpdatePreviewStale);

    current.status = ToolStatus::UpdateAvailable;
    current.capabilities.can_update = false;
    let capability_error = authorize_update_observation(&current).unwrap_err();
    assert_eq!(capability_error.code, ErrorCode::UpdatePreviewStale);

    current.capabilities.can_update = true;
    current.latest_version = None;
    let target_error = authorize_update_observation(&current).unwrap_err();
    assert_eq!(target_error.code, ErrorCode::UpdatePreviewStale);
}

struct StaleBeforeDetectionAdapter;

impl ToolAdapter for StaleBeforeDetectionAdapter {
    fn id(&self) -> ToolId {
        ToolId::Codex
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            can_update: true,
            ..ToolCapabilities::default()
        }
    }

    fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        Box::pin(async { panic!("detect must not run before update authorization") })
    }

    fn install(&self, _ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { panic!("install is not part of this test") })
    }

    fn validate_update(
        &self,
        _preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
        Box::pin(async {
            Err(AppError::new(
                ErrorCode::UpdatePreviewStale,
                "error.tool.updatePreviewStale",
            ))
        })
    }

    fn update(
        &self,
        _ctx: LifecycleContext,
        _authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { panic!("update must not run after stale authorization") })
    }

    fn uninstall(
        &self,
        _ctx: LifecycleContext,
        _options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { panic!("uninstall is not part of this test") })
    }
}

#[tokio::test]
async fn a_stale_preview_is_refused_before_any_detection() {
    let events = Arc::new(RecordingEvents::default());
    let operations = Arc::new(OperationManager::new(Box::new(events.clone())));
    let service = ToolLifecycleService::new(
        operations,
        Arc::new(NeverExecutor),
        Arc::new(|id| {
            (id == ToolId::Codex)
                .then(|| Box::new(StaleBeforeDetectionAdapter) as Box<dyn ToolAdapter>)
        }),
        ToolVersionHistoryService::discarding(),
    );
    let error = service
        .authorize_update(ToolId::Codex, &"a".repeat(64))
        .await
        .expect_err("stale preview");

    assert_eq!(error.code, ErrorCode::UpdatePreviewStale);
    assert!(events.seen.lock().expect("events lock").is_empty());
}

#[tokio::test]
async fn update_passes_the_authorized_target_to_the_locked_adapter() {
    let (service, events, calls) = service_with(
        StubAdapter::new(ToolId::Codex, ToolStatus::Installed).with_update_target("2.0.0"),
    );
    let fingerprint = "a".repeat(64);
    let request = authorized_update(&service, &fingerprint).await;
    let id = service.begin(&request).expect("update begins");
    service.run(id, request).await;

    assert_eq!(
        *calls.lock().expect("calls lock"),
        vec![
            format!("validate-update {fingerprint}"),
            "update 2.0.0".to_string(),
        ]
    );
    assert_eq!(events.last().status, OperationStatus::Success);
}

/// P1 regression guard: one click costs one registry-backed check. The
/// authorization is computed once (`validate_update`), the recovery baseline
/// and the history baseline reuse it, and the only further observation is
/// the post-action verification (plus one owner probe for the history row).
#[tokio::test]
async fn an_update_click_runs_the_registry_and_owner_checks_exactly_once() {
    let (service, events, adapter) = service_with_shared(
        StubAdapter::new(ToolId::Codex, ToolStatus::Installed).with_update_target("2.0.0"),
    );
    let fingerprint = "a".repeat(64);
    let request = authorized_update(&service, &fingerprint).await;
    assert_eq!(adapter.detect_count.load(Ordering::SeqCst), 0);
    assert_eq!(adapter.install_source_count.load(Ordering::SeqCst), 0);

    let id = service.begin(&request).expect("update begins");
    service.run(id, request).await;

    assert_eq!(events.last().status, OperationStatus::Success);
    assert_eq!(
        *adapter.calls.lock().expect("calls lock"),
        vec![
            format!("validate-update {fingerprint}"),
            "update 2.0.0".to_string(),
        ],
        "the plan is checked once and never re-derived by run()"
    );
    assert_eq!(
        adapter.detect_count.load(Ordering::SeqCst),
        1,
        "only the post-action verification observes the tool again"
    );
    assert_eq!(
        adapter.install_source_count.load(Ordering::SeqCst),
        1,
        "only the history row asks for the owner again"
    );
}

#[tokio::test]
async fn a_successful_action_carries_its_verified_detection_to_the_renderer() {
    let (service, events, _calls) = service_with(
        StubAdapter::new(ToolId::Codex, ToolStatus::Installed)
            .with_update_target("2.0.0")
            .with_post_update_version("2.0.0"),
    );
    let request = authorized_update(&service, &"a".repeat(64)).await;
    let id = service.begin(&request).expect("update begins");
    service.run(id, request).await;

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Success);
    let Some(OperationOutput::ToolInventory { tool }) = final_operation.output else {
        panic!("a successful lifecycle action must publish what it verified");
    };
    assert_eq!(tool.id, ToolId::Codex);
    assert_eq!(tool.version.as_deref(), Some("2.0.0"));
    assert_eq!(tool.status, ToolStatus::Installed);
}

#[tokio::test]
async fn a_failed_action_publishes_no_detection_for_the_renderer_to_trust() {
    let mut adapter = StubAdapter::new(ToolId::Codex, ToolStatus::Installed);
    adapter.action = Err(AppError::new(
        ErrorCode::UpdateFailed,
        "error.tool.updateFailed",
    ));
    let (service, events, _calls) = service_with(adapter);
    let request = LifecycleRequest::Repair(ToolId::Codex);
    let id = service.begin(&request).expect("repair begins");
    service.run(id, request).await;

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Failed);
    assert_eq!(final_operation.output, None);
}

#[tokio::test]
async fn an_uninstall_publishes_the_absence_it_verified() {
    let (service, events, _calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::NotInstalled));
    let request = LifecycleRequest::Uninstall(ToolId::Codex, UninstallOptions::default());
    let id = service.begin(&request).expect("uninstall begins");
    service.run(id, request).await;

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Success);
    let Some(OperationOutput::ToolInventory { tool }) = final_operation.output else {
        panic!("a verified removal is a detection the card can trust");
    };
    assert_eq!(tool.status, ToolStatus::NotInstalled);
    assert_eq!(tool.version, None);
}

#[tokio::test]
async fn stale_update_preview_keeps_its_actionable_error_code() {
    let mut adapter =
        StubAdapter::new(ToolId::Codex, ToolStatus::Installed).with_update_target("2.0.0");
    adapter.action = Err(AppError::new(
        ErrorCode::UpdatePreviewStale,
        "error.tool.updatePreviewStale",
    )
    .with_remediation("error.remediation.recheckUpdate"));
    let (service, events, calls) = service_with(adapter);
    let request = authorized_update(&service, &"a".repeat(64)).await;
    let id = service.begin(&request).expect("update begins");
    service.run(id, request).await;

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Failed);
    let error = final_operation.error.expect("stale error");
    assert_eq!(error.code, ErrorCode::UpdatePreviewStale);
    assert_eq!(error.message_key, "error.tool.updatePreviewStale");
    assert_eq!(
        calls.lock().expect("calls lock").as_slice(),
        [
            format!("validate-update {}", "a".repeat(64)),
            "update 2.0.0".to_string(),
        ]
    );
}

/// Self-updaters (`claude update`, `brew upgrade`, `hermes update`) decide the
/// version themselves. A result at or beyond the authorised target, or any real
/// advance from the recorded baseline, proves the update happened; only an
/// unchanged or regressed version is a failure.
#[tokio::test]
async fn an_update_whose_result_differs_from_the_target_verifies_by_progress() {
    for (post, expected) in [
        ("2.1.0", OperationStatus::Success),
        ("1.5.0", OperationStatus::Success),
        ("0.9.0", OperationStatus::Failed),
    ] {
        let (service, events, _calls) = service_with(
            StubAdapter::new(ToolId::Codex, ToolStatus::Installed)
                .with_update_target("2.0.0")
                .with_post_update_version(post),
        );
        let request = authorized_update(&service, &"a".repeat(64)).await;
        let id = service.begin(&request).expect("update begins");
        service.run(id, request).await;

        let final_operation = events.last();
        assert_eq!(final_operation.status, expected, "post-update {post}");
        if expected == OperationStatus::Failed {
            assert_eq!(
                final_operation.error.expect("regression").message_key,
                "error.tool.verifyFailed"
            );
        }
    }
}

#[tokio::test]
async fn an_update_that_is_still_available_is_not_reported_as_ready() {
    let (service, events, _calls) = service_with(
        StubAdapter::new(ToolId::Codex, ToolStatus::UpdateAvailable).with_update_target("2.0.0"),
    );
    let request = authorized_update(&service, &"a".repeat(64)).await;
    let id = service.begin(&request).expect("update begins");
    service.run(id, request).await;

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Failed);
    assert_eq!(
        final_operation
            .error
            .expect("verification failed")
            .message_key,
        "error.tool.verifyFailed"
    );
    assert_ne!(
        final_operation.message_key.as_deref(),
        Some(phase::READY),
        "an unchanged stale version must never reach the ready phase"
    );
}

#[tokio::test]
async fn an_uninstall_that_leaves_the_tool_behind_is_reported_as_incomplete() {
    let (service, events, calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::Installed));
    let request = LifecycleRequest::Uninstall(ToolId::Codex, UninstallOptions::default());
    let id = service.begin(&request).expect("uninstall begins");
    service.run(id, request).await;

    assert_eq!(
        *calls.lock().expect("calls lock"),
        vec!["uninstall settings=false cache=false".to_string()],
        "spec section 32: neither flag is set by default"
    );
    let error = events.last().error.expect("verification failed");
    assert_eq!(error.code, ErrorCode::UninstallFailed);
    assert_eq!(error.message_key, "error.tool.uninstallIncomplete");
}

#[tokio::test]
async fn uninstall_flags_reach_the_adapter_untouched() {
    let (service, _events, calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::NotInstalled));
    let request = LifecycleRequest::Uninstall(
        ToolId::Codex,
        UninstallOptions {
            remove_settings: true,
            remove_cache: true,
        },
    );
    let id = service.begin(&request).expect("uninstall begins");
    service.run(id, request).await;
    assert_eq!(
        *calls.lock().expect("calls lock"),
        vec!["uninstall settings=true cache=true".to_string()]
    );
}

#[tokio::test]
async fn an_action_the_tool_cannot_do_is_refused_by_capability_not_by_tool_id() {
    let mut adapter = StubAdapter::new(ToolId::GeminiCli, ToolStatus::Installed);
    adapter.capabilities = ToolCapabilities::default();
    let (service, _events, calls) = service_with(adapter);
    let error = service
        .begin(&LifecycleRequest::Install(ToolId::GeminiCli))
        .expect_err("install is not a capability of this tool");
    assert_eq!(error.message_key, "error.tool.actionUnsupported");
    assert_eq!(
        error.code,
        ErrorCode::InstallFailed,
        "capability rejection is a classified condition, not INTERNAL"
    );
    assert_eq!(
        error.remediation.as_deref(),
        Some("error.remediation.installManually")
    );
    assert!(calls.lock().expect("calls lock").is_empty());
}

#[tokio::test]
async fn an_unknown_tool_never_starts_an_operation() {
    let (service, events, _calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::Installed));
    let error = service
        .begin(&LifecycleRequest::Install(ToolId::OpenCode))
        .expect_err("no adapter is registered for this tool");
    assert_eq!(error.code, ErrorCode::ToolNotFound);
    assert!(events.seen.lock().expect("events lock").is_empty());
}

#[tokio::test]
async fn an_internal_executor_error_is_relabelled_with_the_action_specific_code() {
    let mut adapter =
        StubAdapter::new(ToolId::Codex, ToolStatus::Installed).with_update_target("2.0.0");
    adapter.action = Err(
        AppError::new(ErrorCode::Internal, "error.command.timeout").with_technical("timed out")
    );
    let (service, events, _calls) = service_with(adapter);
    let request = authorized_update(&service, &"a".repeat(64)).await;
    let id = service.begin(&request).expect("update begins");
    service.run(id, request).await;

    let error = events.last().error.expect("a failure carries an error");
    assert_eq!(
        error.code,
        ErrorCode::UpdateFailed,
        "the UI branches on code"
    );
    assert_eq!(
        error.message_key, "error.command.timeout",
        "the specific reason stays translatable"
    );
}

#[tokio::test]
async fn an_actionable_error_code_survives_the_relabelling() {
    let mut adapter =
        StubAdapter::new(ToolId::Codex, ToolStatus::Installed).with_update_target("2.0.0");
    adapter.action = Err(AppError::new(
        ErrorCode::PermissionDenied,
        "error.command.permissionDenied",
    ));
    let (service, events, _calls) = service_with(adapter);
    let request = authorized_update(&service, &"a".repeat(64)).await;
    let id = service.begin(&request).expect("update begins");
    service.run(id, request).await;
    assert_eq!(
        events.last().error.expect("error").code,
        ErrorCode::PermissionDenied
    );
}

/// Finding #3(a): re-running the same operation id must not repeat the real
/// adapter action or rewrite the terminal state the first run recorded.
#[tokio::test]
async fn running_the_same_operation_id_twice_only_calls_the_adapter_once() {
    let (service, events, calls) =
        service_with(StubAdapter::new(ToolId::Codex, ToolStatus::Installed));
    let request = LifecycleRequest::Install(ToolId::Codex);
    let id = service.begin(&request).expect("install begins");
    service.run(id.clone(), request.clone()).await;
    let first_final = events.last();
    assert_eq!(first_final.status, OperationStatus::Success);

    service.run(id, request).await;

    assert_eq!(
        *calls.lock().expect("calls lock"),
        vec!["install".to_string()]
    );
    assert_eq!(events.last(), first_final);
}

/// Finding #3(b): a request for the wrong tool must be refused before any
/// adapter runs, and must not finish the operation the id really belongs to.
#[tokio::test]
async fn a_request_mismatched_with_the_operations_tool_never_reaches_an_adapter() {
    let events = Arc::new(RecordingEvents::default());
    let operations = Arc::new(OperationManager::new(Box::new(events.clone())));
    let codex = Arc::new(StubAdapter::new(ToolId::Codex, ToolStatus::Installed));
    let claude = Arc::new(StubAdapter::new(ToolId::ClaudeCode, ToolStatus::Installed));
    let codex_calls = codex.calls.clone();
    let claude_calls = claude.calls.clone();
    let resolver: AdapterResolver = {
        let codex = codex.clone();
        let claude = claude.clone();
        Arc::new(move |id: ToolId| {
            let adapter: Box<dyn ToolAdapter> = match id {
                ToolId::Codex => Box::new(SharedAdapter(codex.clone())),
                ToolId::ClaudeCode => Box::new(SharedAdapter(claude.clone())),
                _ => return None,
            };
            Some(adapter)
        })
    };
    let service = ToolLifecycleService::new(
        operations.clone(),
        Arc::new(NeverExecutor),
        resolver,
        ToolVersionHistoryService::discarding(),
    );
    let codex_request = LifecycleRequest::Install(ToolId::Codex);
    let id = service.begin(&codex_request).expect("codex install begins");

    // Same id, but a request for a different tool: the A-id/B-request mismatch.
    let mismatched = LifecycleRequest::Install(ToolId::ClaudeCode);
    service.run(id.clone(), mismatched).await;

    assert!(codex_calls.lock().expect("calls lock").is_empty());
    assert!(claude_calls.lock().expect("calls lock").is_empty());
    let still_running = operations.get(&id).expect("operation still exists");
    assert_eq!(still_running.status, OperationStatus::Running);
}

#[test]
fn an_unsupported_action_points_at_the_matching_manual_fallback() {
    assert_eq!(
        LifecycleRequest::Install(ToolId::ClaudeCode).manual_remediation(),
        "error.remediation.installManually"
    );
    assert_eq!(
        LifecycleRequest::Update(
            ToolId::ClaudeCode,
            Box::new(authorization_for(ToolId::ClaudeCode, "2.0.0"))
        )
        .manual_remediation(),
        "error.remediation.installManually"
    );
    assert_eq!(
        LifecycleRequest::Repair(ToolId::Codex).manual_remediation(),
        "error.remediation.installManually"
    );
    assert_eq!(
        LifecycleRequest::Uninstall(ToolId::ClaudeCode, UninstallOptions::default())
            .manual_remediation(),
        "error.remediation.uninstallManually"
    );
}

// Split into a further subfile: the panic-payload fixture and its tests
// (Finding #2, B2) pushed this file past the 500-line-per-file limit; see
// AI_RULES.
#[path = "tests_panics.rs"]
mod tests_panics;

#[path = "tests_history.rs"]
mod tests_history;

#[path = "tests_recovery.rs"]
mod tests_recovery;

#[path = "tests_cancellation.rs"]
mod tests_cancellation;
