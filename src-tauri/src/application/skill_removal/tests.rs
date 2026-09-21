use super::{SkillRemovalRequest, SkillRemovalService, SkillRemover};
use crate::domain::{AppError, ErrorCode, Operation, OperationStatus, ToolId};
use crate::infrastructure::{OperationEvents, OperationManager};
use std::sync::Arc;

struct SilentEvents;

impl OperationEvents for SilentEvents {
    fn emit(&self, _operation: &Operation) {}
}

fn request() -> SkillRemovalRequest {
    SkillRemovalRequest {
        tool: ToolId::ClaudeCode,
        id: "anthropics/skills:skills/code-review".to_string(),
        name: "Code review".to_string(),
    }
}

fn service(remover: SkillRemover) -> (SkillRemovalService, Arc<OperationManager>) {
    let operations = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    (
        SkillRemovalService::new(operations.clone(), remover),
        operations,
    )
}

#[test]
fn unsupported_or_incomplete_targets_never_create_an_operation() {
    let remover: SkillRemover = Arc::new(|_, _| Box::pin(async { unreachable!() }));
    let (service, operations) = service(remover);

    let mut unsupported = request();
    unsupported.tool = ToolId::OpenClaw;
    assert_eq!(
        service
            .begin(&unsupported)
            .expect_err("OpenClaw has no Skill capability")
            .message_key,
        "error.skill.unsupportedTool"
    );

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
async fn successful_removal_reaches_removed_and_preserves_the_skill_target() {
    let remover: SkillRemover = Arc::new(|_, _| Box::pin(async { Ok(()) }));
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
            .map(|target| target.name.as_str()),
        Some("Code review")
    );
}

#[tokio::test]
async fn removal_failure_releases_the_lock_and_keeps_details_in_the_task() {
    let remover: SkillRemover = Arc::new(|_, _| {
        Box::pin(async {
            Err(
                AppError::new(ErrorCode::UninstallFailed, "error.skill.removeFailed")
                    .with_technical("backup could not be created"),
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
        .begin(
            crate::domain::OperationKind::Update,
            Some(ToolId::ClaudeCode),
        )
        .expect("failed Skill removal releases the tool lock");
}

#[tokio::test]
async fn a_panicking_remover_still_finishes_without_leaking_secrets() {
    let remover: SkillRemover =
        Arc::new(|_, _| Box::pin(async { panic!("token=ghp_do_not_leak") }));
    let (service, operations) = service(remover);
    let request = request();
    let id = service.begin(&request).expect("begin removal");
    service.run(id.clone(), request).await;

    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("panic becomes a structured failure");
    assert_eq!(error.message_key, "error.skill.removePanicked");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("ghp_do_not_leak"));
}

#[tokio::test]
async fn a_remover_that_panics_before_returning_a_future_also_finishes() {
    let remover: SkillRemover = Arc::new(|_, _| panic!("token=sync_secret_do_not_leak"));
    let (service, operations) = service(remover);
    let request = request();
    let id = service.begin(&request).expect("begin removal");
    service.run(id.clone(), request).await;

    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("synchronous panic becomes a structured failure");
    assert_eq!(error.message_key, "error.skill.removePanicked");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("sync_secret_do_not_leak"));
    operations
        .begin(
            crate::domain::OperationKind::Update,
            Some(ToolId::ClaudeCode),
        )
        .expect("a synchronous panic releases the tool lock");
}

#[tokio::test]
async fn a_mismatched_request_still_fails_and_releases_the_tool_lock() {
    let remover: SkillRemover = Arc::new(|_, _| {
        Box::pin(async { panic!("a mismatched request must never reach the remover") })
    });
    let (service, operations) = service(remover);
    let original = request();
    let id = service.begin(&original).expect("begin removal");
    let mut mismatched = original;
    mismatched.id = "anthropics/skills:skills/other".to_string();

    service.run(id.clone(), mismatched).await;

    let operation = operations.get(&id).expect("failed operation remains");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation.error.as_ref().map(|error| error.code),
        Some(ErrorCode::Internal)
    );
    operations
        .begin(
            crate::domain::OperationKind::Update,
            Some(ToolId::ClaudeCode),
        )
        .expect("a mismatched request releases the tool lock");
}
