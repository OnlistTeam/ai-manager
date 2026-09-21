use super::{AdapterResolver, NeverExecutor, RecordingEvents};
use crate::adapters::{AuthorizedUpdate, LifecycleContext, ToolAdapter};
use crate::application::tool_lifecycle::{LifecycleRequest, ToolLifecycleService};
use crate::application::tool_version_history::{
    history_write_failed, ToolVersionEventRepository, ToolVersionHistoryService,
};
use crate::domain::{
    AppError, OperationKind, OperationStatus, Tool, ToolCapabilities, ToolId, ToolInstallSource,
    ToolStatus, ToolVersionEvent, UninstallOptions,
};
use crate::infrastructure::OperationManager;
use futures::future::BoxFuture;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct TransitionAdapter {
    detect_count: AtomicUsize,
    before: Option<String>,
    after: Option<String>,
    source: ToolInstallSource,
}

impl TransitionAdapter {
    fn new(before: Option<&str>, after: Option<&str>, source: ToolInstallSource) -> Self {
        Self {
            detect_count: AtomicUsize::new(0),
            before: before.map(str::to_string),
            after: after.map(str::to_string),
            source,
        }
    }

    fn tool(&self, version: Option<String>, update_check: bool) -> Tool {
        Tool {
            id: ToolId::Codex,
            name: "Codex".to_string(),
            description_key: "tool.codex.description".to_string(),
            discovery: crate::domain::ToolDiscovery::for_tool(ToolId::Codex),
            status: if version.is_none() {
                ToolStatus::NotInstalled
            } else if update_check {
                ToolStatus::UpdateAvailable
            } else {
                ToolStatus::Installed
            },
            version,
            latest_version: update_check.then(|| {
                self.after
                    .clone()
                    .expect("update fixture has a target version")
            }),
            capabilities: Self::capabilities_value(),
            sessions_inside_settings: false,
            environment: Some("test".to_string()),
            configuration_shared_with: None,
        }
    }

    fn capabilities_value() -> ToolCapabilities {
        ToolCapabilities {
            can_install: true,
            can_update: true,
            can_uninstall: true,
            can_manage_version: true,
            ..ToolCapabilities::default()
        }
    }
}

impl ToolAdapter for Arc<TransitionAdapter> {
    fn id(&self) -> ToolId {
        ToolId::Codex
    }

    fn capabilities(&self) -> ToolCapabilities {
        TransitionAdapter::capabilities_value()
    }

    fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        let update_check = self.detect_count.fetch_add(1, Ordering::SeqCst) == 0;
        let version = if update_check {
            self.before.clone()
        } else {
            self.after.clone()
        };
        Box::pin(async move { Ok(self.tool(version, update_check)) })
    }

    fn install_source(&self) -> BoxFuture<'_, Result<ToolInstallSource, AppError>> {
        let source = self.source;
        Box::pin(async move { Ok(source) })
    }

    fn install(&self, _ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { Ok(()) })
    }

    /// Consumes the pre-update observation: the lifecycle never observes
    /// again before the action, so the next `detect` is the post-action one.
    fn validate_update(
        &self,
        _preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
        self.detect_count.fetch_add(1, Ordering::SeqCst);
        let authorized = AuthorizedUpdate {
            observed: self.tool(self.before.clone(), true),
            target_version: "2.0.0".to_string(),
            source: self.source,
            plan: None,
        };
        Box::pin(async move { Ok(authorized) })
    }

    fn update(
        &self,
        _ctx: LifecycleContext,
        _authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { Ok(()) })
    }

    fn install_version(
        &self,
        _ctx: LifecycleContext,
        _version: String,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { Ok(()) })
    }

    fn uninstall(
        &self,
        _ctx: LifecycleContext,
        _options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Default)]
struct RecordingHistory {
    events: Mutex<Vec<ToolVersionEvent>>,
    fail_write: bool,
}

impl ToolVersionEventRepository for RecordingHistory {
    fn append(&self, event: ToolVersionEvent) -> BoxFuture<'static, Result<(), AppError>> {
        if self.fail_write {
            return Box::pin(async { Err(history_write_failed("simulated database failure")) });
        }
        self.events.lock().expect("history lock").push(event);
        Box::pin(async { Ok(()) })
    }

    fn list_newest(
        &self,
        _tool: ToolId,
        _limit: usize,
    ) -> BoxFuture<'static, Result<Vec<ToolVersionEvent>, AppError>> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

async fn update_request(service: &ToolLifecycleService) -> LifecycleRequest {
    LifecycleRequest::Update(
        ToolId::Codex,
        Box::new(
            service
                .authorize_update(ToolId::Codex, &"a".repeat(64))
                .await
                .expect("update authorized"),
        ),
    )
}

fn service(
    adapter: TransitionAdapter,
    history: Arc<RecordingHistory>,
) -> (ToolLifecycleService, Arc<RecordingEvents>) {
    let events = Arc::new(RecordingEvents::default());
    let operations = Arc::new(OperationManager::new(Box::new(events.clone())));
    let adapter = Arc::new(adapter);
    let resolver: AdapterResolver = Arc::new(move |id| {
        (id == ToolId::Codex).then(|| Box::new(adapter.clone()) as Box<dyn ToolAdapter>)
    });
    (
        ToolLifecycleService::new(
            operations,
            Arc::new(NeverExecutor),
            resolver,
            ToolVersionHistoryService::new(history),
        ),
        events,
    )
}

#[tokio::test]
async fn verified_update_records_exact_versions_owner_and_kind() {
    let history = Arc::new(RecordingHistory::default());
    let (service, events) = service(
        TransitionAdapter::new(Some("1.0.0"), Some("2.0.0"), ToolInstallSource::Pnpm),
        history.clone(),
    );
    let request = update_request(&service).await;
    let id = service.begin(&request).unwrap();
    service.run(id, request).await;

    assert_eq!(events.last().status, OperationStatus::Success);
    let recorded = history.events.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].from_version.as_deref(), Some("1.0.0"));
    assert_eq!(recorded[0].to_version, "2.0.0");
    assert_eq!(recorded[0].source, ToolInstallSource::Pnpm);
    assert_eq!(recorded[0].operation_kind, OperationKind::Update);
}

#[tokio::test]
async fn failed_target_verification_never_writes_history() {
    let history = Arc::new(RecordingHistory::default());
    let (service, events) = service(
        TransitionAdapter::new(Some("1.0.0"), Some("2.0.0"), ToolInstallSource::Npm),
        history.clone(),
    );
    let request = LifecycleRequest::InstallVersion(ToolId::Codex, "3.0.0".to_string());
    let id = service.begin(&request).unwrap();
    service.run(id, request).await;

    assert_eq!(events.last().status, OperationStatus::Failed);
    assert!(history.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_history_write_failure_cannot_be_reported_as_full_success() {
    let history = Arc::new(RecordingHistory {
        fail_write: true,
        ..RecordingHistory::default()
    });
    let (service, events) = service(
        TransitionAdapter::new(Some("1.0.0"), Some("2.0.0"), ToolInstallSource::Npm),
        history,
    );
    let request = update_request(&service).await;
    let id = service.begin(&request).unwrap();
    service.run(id, request).await;

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Failed);
    assert_eq!(
        final_operation.error.unwrap().message_key,
        "error.tool.versionHistoryWriteFailed"
    );
}
