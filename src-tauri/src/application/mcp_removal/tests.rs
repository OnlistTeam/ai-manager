use super::{McpRemovalRequest, McpRemovalService, McpRemover};
use crate::domain::{AppError, ErrorCode, ExtensionScope, Operation, OperationStatus, ToolId};
use crate::infrastructure::{OperationEvents, OperationManager};
use std::sync::Arc;

struct SilentEvents;

impl OperationEvents for SilentEvents {
    fn emit(&self, _operation: &Operation) {}
}

fn request() -> McpRemovalRequest {
    McpRemovalRequest {
        scope: ExtensionScope::tool(ToolId::Codex),
        id: "docs-a1b2c3d4".to_string(),
        name: "Project docs".to_string(),
    }
}

fn service(remover: McpRemover) -> (McpRemovalService, Arc<OperationManager>) {
    let operations = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    (
        McpRemovalService::new(operations.clone(), remover),
        operations,
    )
}

#[test]
fn incomplete_targets_never_create_an_operation() {
    let remover: McpRemover = Arc::new(|_, _| Box::pin(async { unreachable!() }));
    let (service, operations) = service(remover);

    let mut incomplete = request();
    incomplete.name = "  ".to_string();
    assert_eq!(
        service
            .begin(&incomplete)
            .expect_err("an incomplete target is unsafe")
            .code,
        ErrorCode::UninstallFailed
    );
    assert!(operations.list().is_empty());
}

#[tokio::test]
async fn successful_removal_reaches_removed_with_the_authoritative_target() {
    let remover: McpRemover = Arc::new(|_, _| Box::pin(async { Ok(()) }));
    let (service, operations) = service(remover);
    let request = request();
    let id = service.begin(&request).expect("begin removal");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation remains in history");
    assert_eq!(operation.status, OperationStatus::Success);
    assert_eq!(operation.progress, 100);
    assert_eq!(
        operation.message_key.as_deref(),
        Some("operation.phase.removed")
    );
    assert_eq!(
        operation
            .extension
            .as_ref()
            .map(|target| (target.id.as_str(), target.name.as_str())),
        Some(("docs-a1b2c3d4", "Project docs"))
    );
}

#[tokio::test]
async fn removal_failure_releases_the_lock_and_keeps_details_in_the_task() {
    let remover: McpRemover = Arc::new(|_, _| {
        Box::pin(async {
            Err(
                AppError::new(ErrorCode::UninstallFailed, "error.mcp.removeFailed")
                    .with_technical("live projection could not be updated"),
            )
        })
    });
    let (service, operations) = service(remover);
    let request = request();
    let id = service.begin(&request).expect("begin removal");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("failed operation remains");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation.error.as_ref().map(|error| error.code),
        Some(ErrorCode::UninstallFailed)
    );
    assert_eq!(
        operation
            .error
            .as_ref()
            .and_then(|error| error.context_id.as_deref()),
        Some(id.as_str())
    );
    operations
        .begin(crate::domain::OperationKind::Update, Some(ToolId::Codex))
        .expect("failed MCP removal releases the tool lock");
}

#[tokio::test]
async fn an_async_panic_finishes_without_leaking_secrets() {
    let remover: McpRemover =
        Arc::new(|_, _| Box::pin(async { panic!("token=remove_async_secret_123456") }));
    let (service, operations) = service(remover);
    let request = request();
    let id = service.begin(&request).expect("begin removal");
    service.run(id.clone(), request).await;

    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("panic becomes a structured failure");
    assert_eq!(error.message_key, "error.mcp.removePanicked");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("remove_async_secret_123456"));
}

#[tokio::test]
async fn a_panic_before_the_future_also_finishes_and_releases_the_lock() {
    let remover: McpRemover = Arc::new(|_, _| panic!("token=remove_sync_secret_123456"));
    let (service, operations) = service(remover);
    let request = request();
    let id = service.begin(&request).expect("begin removal");
    service.run(id.clone(), request).await;

    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("synchronous panic becomes a structured failure");
    assert_eq!(error.message_key, "error.mcp.removePanicked");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("remove_sync_secret_123456"));
    operations
        .begin(crate::domain::OperationKind::Update, Some(ToolId::Codex))
        .expect("a synchronous panic releases the tool lock");
}

#[tokio::test]
async fn a_mismatched_request_never_reaches_the_remover() {
    let remover: McpRemover = Arc::new(|_, _| {
        Box::pin(async { panic!("a mismatched request must never reach the remover") })
    });
    let (service, operations) = service(remover);
    let original = request();
    let id = service.begin(&original).expect("begin removal");
    let mut mismatched = original;
    mismatched.id = "other-00000000".to_string();

    service.run(id.clone(), mismatched).await;

    let operation = operations.get(&id).expect("failed operation remains");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation.error.as_ref().map(|error| error.code),
        Some(ErrorCode::Internal)
    );
    operations
        .begin(crate::domain::OperationKind::Update, Some(ToolId::Codex))
        .expect("a mismatch releases the tool lock");
}
