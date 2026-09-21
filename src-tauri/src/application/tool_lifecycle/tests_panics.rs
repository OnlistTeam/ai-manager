use super::{authorization_for, NeverExecutor, RecordingEvents};
use super::{AdapterResolver, LifecycleContext, LifecycleRequest, ToolLifecycleService};
use crate::adapters::{AuthorizedUpdate, ToolAdapter};
use crate::application::tool_version_history::ToolVersionHistoryService;
use crate::domain::{
    AppError, ErrorCode, OperationStatus, Tool, ToolCapabilities, ToolId, UninstallOptions,
};
use crate::infrastructure::OperationManager;
use futures::future::BoxFuture;
use std::sync::Arc;

/// Simulates a panic inside the adapter/upstream plan construction code (Task 8 review, finding #2).
/// `message` is parameterized so B2's redaction test can reuse this same
/// adapter with a panic payload that contains a fake secret.
struct PanickingAdapter {
    id: ToolId,
    message: &'static str,
    /// Also panic while checking the update, not only while running it.
    panics_while_authorizing: bool,
}

impl PanickingAdapter {
    fn new(id: ToolId) -> Self {
        Self {
            id,
            message: "simulated adapter panic",
            panics_while_authorizing: false,
        }
    }
}

impl ToolAdapter for PanickingAdapter {
    fn id(&self) -> ToolId {
        self.id
    }
    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            can_install: true,
            can_update: true,
            can_uninstall: true,
            ..ToolCapabilities::default()
        }
    }
    fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        Box::pin(async move {
            Ok(Tool {
                id: self.id,
                name: "Stub".to_string(),
                description_key: "tool.stub.description".to_string(),
                discovery: crate::domain::ToolDiscovery::for_tool(self.id),
                status: crate::domain::ToolStatus::Installed,
                version: Some("1.0.0".to_string()),
                latest_version: None,
                capabilities: self.capabilities(),
                sessions_inside_settings: false,
                environment: Some("test".to_string()),
                configuration_shared_with: None,
            })
        })
    }
    fn install(&self, _ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        let message = self.message;
        Box::pin(async move { panic!("{message}") })
    }
    fn validate_update(
        &self,
        _preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
        let message = self.message;
        let panics = self.panics_while_authorizing;
        let authorized = authorization_for(self.id, "2.0.0");
        Box::pin(async move {
            if panics {
                panic!("{message}");
            }
            Ok(authorized)
        })
    }

    fn update(
        &self,
        _ctx: LifecycleContext,
        _authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        let message = self.message;
        Box::pin(async move { panic!("{message}") })
    }
    fn uninstall(
        &self,
        _ctx: LifecycleContext,
        _options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        let message = self.message;
        Box::pin(async move { panic!("{message}") })
    }
}

/// Finding #2: a panic anywhere in `execute` must still reach a terminal
/// state and release the per-tool lock, not leave it Running forever.
#[tokio::test]
async fn a_panic_in_the_adapter_action_still_finishes_the_operation_and_releases_the_lock() {
    let events = Arc::new(RecordingEvents::default());
    let operations = Arc::new(OperationManager::new(Box::new(events.clone())));
    let resolver: AdapterResolver = Arc::new(|id: ToolId| {
        (id == ToolId::Codex).then(|| Box::new(PanickingAdapter::new(id)) as Box<dyn ToolAdapter>)
    });
    let service = ToolLifecycleService::new(
        operations,
        Arc::new(NeverExecutor),
        resolver,
        ToolVersionHistoryService::discarding(),
    );
    let request = LifecycleRequest::Install(ToolId::Codex);
    let id = service.begin(&request).expect("install begins");
    service.run(id, request).await;

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Failed);
    let error = final_operation
        .error
        .expect("panic must surface as an error");
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(error.message_key, "error.tool.actionPanicked");
    assert!(error.technical_message.is_some());

    // A leaked per-tool lock would turn this into OPERATION_CONFLICT.
    service
        .begin(&LifecycleRequest::Update(
            ToolId::Codex,
            Box::new(authorization_for(ToolId::Codex, "2.0.0")),
        ))
        .expect("per-tool lock must be released even after a panic");
}

/// The update check runs before the lock, in the command's own future: a
/// panic there must come back as an error the renderer can show, not as a
/// command that never answers.
#[tokio::test]
async fn a_panic_while_authorizing_an_update_is_an_error_not_a_hung_command() {
    let events = Arc::new(RecordingEvents::default());
    let operations = Arc::new(OperationManager::new(Box::new(events.clone())));
    let resolver: AdapterResolver = Arc::new(|id: ToolId| {
        (id == ToolId::Codex).then(|| {
            Box::new(PanickingAdapter {
                id,
                message: "simulated inspection panic",
                panics_while_authorizing: true,
            }) as Box<dyn ToolAdapter>
        })
    });
    let service = ToolLifecycleService::new(
        operations,
        Arc::new(NeverExecutor),
        resolver,
        ToolVersionHistoryService::discarding(),
    );
    let error = service
        .authorize_update(ToolId::Codex, &"a".repeat(64))
        .await
        .expect_err("the panic must surface as an error");

    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(error.message_key, "error.tool.actionPanicked");
    assert!(events.seen.lock().expect("events lock").is_empty());
}

/// B2: a panic payload containing a secret must come out redacted — not just
/// caught. `panic_summary` has to run the same `redact_secrets` +
/// `truncate_tail` pipeline as adapter failure `technical_message`s.
#[tokio::test]
async fn a_panic_payload_containing_a_secret_is_redacted_before_it_becomes_technical_message() {
    let events = Arc::new(RecordingEvents::default());
    let operations = Arc::new(OperationManager::new(Box::new(events.clone())));
    let resolver: AdapterResolver = Arc::new(|id: ToolId| {
        (id == ToolId::Codex).then(|| {
            Box::new(PanickingAdapter {
                id,
                message: "npm ERR! auth sk-ant-leakedsecretvalue0123 failed",
                panics_while_authorizing: false,
            }) as Box<dyn ToolAdapter>
        })
    });
    let service = ToolLifecycleService::new(
        operations,
        Arc::new(NeverExecutor),
        resolver,
        ToolVersionHistoryService::discarding(),
    );
    let request = LifecycleRequest::Install(ToolId::Codex);
    let id = service.begin(&request).expect("install begins");
    service.run(id, request).await;

    let final_operation = events.last();
    assert_eq!(final_operation.status, OperationStatus::Failed);
    let error = final_operation
        .error
        .expect("panic must surface as an error");
    assert_eq!(error.message_key, "error.tool.actionPanicked");
    let technical = error.technical_message.expect("panic summary is present");
    assert!(
        !technical.contains("sk-ant-leakedsecretvalue0123"),
        "the raw secret must not survive into technical_message: {technical}"
    );
    assert!(
        technical.contains("***"),
        "the secret must be redacted, not dropped: {technical}"
    );
}
