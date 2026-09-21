use super::{AdapterResolver, NeverExecutor, RecordingEvents};
use crate::adapters::{AuthorizedUpdate, LifecycleContext, ToolAdapter};
use crate::application::tool_lifecycle::{LifecycleRequest, ToolLifecycleService};
use crate::application::tool_version_history::{
    ToolVersionEventRepository, ToolVersionHistoryService,
};
use crate::domain::{
    AppError, ErrorCode, OperationKind, OperationStatus, Tool, ToolCapabilities, ToolId,
    ToolInstallSource, ToolStatus, ToolUpdateRecovery, ToolVersionEvent, UninstallOptions,
};
use crate::infrastructure::OperationManager;
use futures::future::BoxFuture;
use std::sync::Arc;

/// The authorization carries the pre-update observation and owner; every
/// later `detect`/`install_source` is the post-failure state.
struct FailedUpdateAdapter {
    final_status: ToolStatus,
    initial_source: ToolInstallSource,
    final_source: ToolInstallSource,
}

impl FailedUpdateAdapter {
    fn new(
        final_status: ToolStatus,
        initial_source: ToolInstallSource,
        final_source: ToolInstallSource,
    ) -> Self {
        Self {
            final_status,
            initial_source,
            final_source,
        }
    }

    fn tool(status: ToolStatus) -> Tool {
        Tool {
            id: ToolId::Codex,
            name: "Codex".to_string(),
            description_key: "tool.codex.description".to_string(),
            discovery: crate::domain::ToolDiscovery::for_tool(ToolId::Codex),
            status,
            version: Some("1.2.3".to_string()),
            latest_version: Some("2.0.0".to_string()),
            capabilities: FailedUpdateAdapter::capabilities(),
            sessions_inside_settings: false,
            environment: Some("test".to_string()),
            configuration_shared_with: None,
        }
    }

    fn capabilities() -> ToolCapabilities {
        ToolCapabilities {
            can_update: true,
            can_manage_version: true,
            ..ToolCapabilities::default()
        }
    }
}

impl ToolAdapter for Arc<FailedUpdateAdapter> {
    fn id(&self) -> ToolId {
        ToolId::Codex
    }

    fn capabilities(&self) -> ToolCapabilities {
        FailedUpdateAdapter::capabilities()
    }

    fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        let status = self.final_status;
        Box::pin(async move { Ok(FailedUpdateAdapter::tool(status)) })
    }

    fn install_source(&self) -> BoxFuture<'_, Result<ToolInstallSource, AppError>> {
        let source = self.final_source;
        Box::pin(async move { Ok(source) })
    }

    fn install(&self, _ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { Ok(()) })
    }

    fn validate_update(
        &self,
        _preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
        let authorized = AuthorizedUpdate {
            observed: FailedUpdateAdapter::tool(ToolStatus::UpdateAvailable),
            target_version: "2.0.0".to_string(),
            source: self.initial_source,
            plan: None,
        };
        Box::pin(async move { Ok(authorized) })
    }

    fn update(
        &self,
        _ctx: LifecycleContext,
        _authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async {
            Err(AppError::new(
                ErrorCode::UpdateFailed,
                "error.tool.updateFailed",
            ))
        })
    }

    fn uninstall(
        &self,
        _ctx: LifecycleContext,
        _options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { Ok(()) })
    }
}

struct RecoveryHistory {
    events: Vec<ToolVersionEvent>,
}

impl ToolVersionEventRepository for RecoveryHistory {
    fn append(&self, _event: ToolVersionEvent) -> BoxFuture<'static, Result<(), AppError>> {
        Box::pin(async { Ok(()) })
    }

    fn list_newest(
        &self,
        tool: ToolId,
        limit: usize,
    ) -> BoxFuture<'static, Result<Vec<ToolVersionEvent>, AppError>> {
        let events = self
            .events
            .iter()
            .filter(|event| event.tool == tool)
            .take(limit)
            .cloned()
            .collect();
        Box::pin(async move { Ok(events) })
    }
}

fn verified_event(source: ToolInstallSource) -> ToolVersionEvent {
    ToolVersionEvent::verified(
        ToolId::Codex,
        None,
        "1.2.3".to_string(),
        source,
        OperationKind::Install,
        1,
    )
    .unwrap()
}

fn service(
    adapter: FailedUpdateAdapter,
    events: Vec<ToolVersionEvent>,
) -> (ToolLifecycleService, Arc<RecordingEvents>) {
    let recorded = Arc::new(RecordingEvents::default());
    let operations = Arc::new(OperationManager::new(Box::new(recorded.clone())));
    let adapter = Arc::new(adapter);
    let resolver: AdapterResolver = Arc::new(move |tool| {
        (tool == ToolId::Codex).then(|| Box::new(adapter.clone()) as Box<dyn ToolAdapter>)
    });
    (
        ToolLifecycleService::new(
            operations,
            Arc::new(NeverExecutor),
            resolver,
            ToolVersionHistoryService::new(Arc::new(RecoveryHistory { events })),
        ),
        recorded,
    )
}

async fn failed_operation(
    adapter: FailedUpdateAdapter,
    history: Vec<ToolVersionEvent>,
) -> crate::domain::Operation {
    let (service, events) = service(adapter, history);
    let request = LifecycleRequest::Update(
        ToolId::Codex,
        Box::new(
            service
                .authorize_update(ToolId::Codex, &"a".repeat(64))
                .await
                .expect("update authorized"),
        ),
    );
    let id = service.begin(&request).unwrap();
    service.run(id, request).await;
    events.last()
}

#[tokio::test]
async fn broken_update_offers_only_the_matching_verified_owned_version() {
    let operation = failed_operation(
        FailedUpdateAdapter::new(
            ToolStatus::Broken,
            ToolInstallSource::Npm,
            ToolInstallSource::Npm,
        ),
        vec![verified_event(ToolInstallSource::Npm)],
    )
    .await;

    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation.update_recovery,
        Some(ToolUpdateRecovery::Available {
            target_version: "1.2.3".to_string(),
        })
    );
}

#[tokio::test]
async fn missing_history_and_changed_ownership_are_advisory_only() {
    let no_history = failed_operation(
        FailedUpdateAdapter::new(
            ToolStatus::Broken,
            ToolInstallSource::Npm,
            ToolInstallSource::Npm,
        ),
        Vec::new(),
    )
    .await;
    assert_eq!(
        no_history.update_recovery,
        Some(ToolUpdateRecovery::HistoryUnavailable)
    );

    let changed_owner = failed_operation(
        FailedUpdateAdapter::new(
            ToolStatus::Broken,
            ToolInstallSource::Npm,
            ToolInstallSource::Pnpm,
        ),
        vec![verified_event(ToolInstallSource::Npm)],
    )
    .await;
    assert_eq!(
        changed_owner.update_recovery,
        Some(ToolUpdateRecovery::OwnershipChanged)
    );
}

#[tokio::test]
async fn unsupported_owner_is_advisory_and_a_non_broken_failure_has_no_recovery() {
    let unsupported = failed_operation(
        FailedUpdateAdapter::new(
            ToolStatus::Broken,
            ToolInstallSource::Brew,
            ToolInstallSource::Brew,
        ),
        vec![verified_event(ToolInstallSource::Brew)],
    )
    .await;
    assert_eq!(
        unsupported.update_recovery,
        Some(ToolUpdateRecovery::OwnerUnsupported)
    );

    let still_installed = failed_operation(
        FailedUpdateAdapter::new(
            ToolStatus::Installed,
            ToolInstallSource::Npm,
            ToolInstallSource::Npm,
        ),
        vec![verified_event(ToolInstallSource::Npm)],
    )
    .await;
    assert_eq!(still_installed.update_recovery, None);
}
