use std::sync::Arc;

use super::{SkillBackupRestoreRequest, SkillBackupRestoreService, SkillBackupRestorer};
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, ExtensionManagement, ExtensionScope, Operation,
    OperationKind, OperationStatus, ToolId,
};
use crate::infrastructure::{OperationEvents, OperationManager};

struct SilentEvents;

impl OperationEvents for SilentEvents {
    fn emit(&self, _operation: &Operation) {}
}

fn request() -> SkillBackupRestoreRequest {
    SkillBackupRestoreRequest {
        tool: ToolId::ClaudeCode,
        id: "a".repeat(64),
        name: "Code review".to_string(),
    }
}

fn restored(tool: ToolId) -> Extension {
    Extension {
        kind: ExtensionKind::Skill,
        id: "local:code-review".to_string(),
        scope: ExtensionScope::tool(tool),
        name: "Code review".to_string(),
        description: None,
        management: ExtensionManagement::Managed,
        enabled: true,
        can_disable: true,
    }
}

fn service(restorer: SkillBackupRestorer) -> (SkillBackupRestoreService, Arc<OperationManager>) {
    let operations = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    (
        SkillBackupRestoreService::new(operations.clone(), restorer),
        operations,
    )
}

#[test]
fn begin_records_an_install_task_for_the_opaque_backup() {
    let restorer: SkillBackupRestorer =
        Arc::new(|tool, _| Box::pin(async move { Ok(restored(tool)) }));
    let (service, operations) = service(restorer);
    let request = request();
    let id = service.begin(&request).expect("begin restore");
    let operation = operations.get(&id).expect("operation");
    assert_eq!(operation.kind, OperationKind::Install);
    assert_eq!(operation.tool, Some(ToolId::ClaudeCode));
    assert_eq!(operation.status, OperationStatus::Running);
    assert_eq!(
        operation
            .extension
            .as_ref()
            .map(|target| target.id.as_str()),
        Some(request.id.as_str())
    );
}

#[tokio::test]
async fn successful_restore_reaches_ready_and_releases_the_tool_lock() {
    let restorer: SkillBackupRestorer =
        Arc::new(|tool, _| Box::pin(async move { Ok(restored(tool)) }));
    let (service, operations) = service(restorer);
    let request = request();
    let id = service.begin(&request).expect("begin restore");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation");
    assert_eq!(operation.status, OperationStatus::Success);
    assert_eq!(operation.progress, 100);
    operations
        .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
        .expect("successful restore releases the tool lock");
}

#[tokio::test]
async fn restore_failure_keeps_context_and_releases_the_lock() {
    let restorer: SkillBackupRestorer = Arc::new(|_, _| {
        Box::pin(async {
            Err(AppError::new(
                ErrorCode::ConfigWriteFailed,
                "error.skill.backupRestoreFailed",
            ))
        })
    });
    let (service, operations) = service(restorer);
    let request = request();
    let id = service.begin(&request).expect("begin restore");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation
            .error
            .as_ref()
            .and_then(|error| error.context_id.as_deref()),
        Some(id.as_str())
    );
    operations
        .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
        .expect("failed restore releases the tool lock");
}

#[tokio::test]
async fn a_wrong_projection_fails_verification() {
    let restorer: SkillBackupRestorer =
        Arc::new(|_, _| Box::pin(async { Ok(restored(ToolId::Codex)) }));
    let (service, operations) = service(restorer);
    let request = request();
    let id = service.begin(&request).expect("begin restore");
    service.run(id.clone(), request).await;

    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("verification error");
    assert_eq!(error.message_key, "error.skill.backupVerifyFailed");
}

#[tokio::test]
async fn a_panicking_restorer_is_redacted() {
    let restorer: SkillBackupRestorer =
        Arc::new(|_, _| Box::pin(async { panic!("token=ghp_backup_secret restore panic") }));
    let (service, operations) = service(restorer);
    let request = request();
    let id = service.begin(&request).expect("begin restore");
    service.run(id.clone(), request).await;

    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("panic becomes failure");
    assert_eq!(error.message_key, "error.skill.actionPanicked");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("ghp_backup_secret"));
}

#[tokio::test]
async fn a_mismatched_request_never_reaches_the_restorer() {
    let restorer: SkillBackupRestorer =
        Arc::new(|_, _| Box::pin(async { panic!("mismatched recovery request reached restorer") }));
    let (service, operations) = service(restorer);
    let original = request();
    let id = service.begin(&original).expect("begin restore");
    let mut mismatched = original;
    mismatched.id = "b".repeat(64);
    service.run(id.clone(), mismatched).await;

    let operation = operations.get(&id).expect("failed operation");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation.error.as_ref().map(|error| error.code),
        Some(ErrorCode::Internal)
    );
}
