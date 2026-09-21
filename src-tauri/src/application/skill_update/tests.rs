use std::sync::Arc;

use super::{SkillUpdateRequest, SkillUpdateService, SkillUpdater};
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, ExtensionManagement, ExtensionScope,
    OperationKind, OperationStatus, ToolId,
};
use crate::infrastructure::{OperationEvents, OperationManager};

struct SilentEvents;

impl OperationEvents for SilentEvents {
    fn emit(&self, _operation: &crate::domain::Operation) {}
}

fn request() -> SkillUpdateRequest {
    SkillUpdateRequest {
        tool: ToolId::ClaudeCode,
        id: "anthropics/skills:skills/code-review".to_string(),
        name: "Code review".to_string(),
    }
}

fn updated() -> Extension {
    Extension {
        kind: ExtensionKind::Skill,
        id: request().id,
        scope: ExtensionScope::tool(ToolId::ClaudeCode),
        name: "Code review".to_string(),
        description: None,
        management: ExtensionManagement::Managed,
        enabled: true,
        can_disable: true,
    }
}

fn service(updater: SkillUpdater) -> (SkillUpdateService, Arc<OperationManager>) {
    let operations = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    (
        SkillUpdateService::new(operations.clone(), updater),
        operations,
    )
}

#[test]
fn begin_records_an_update_for_the_resolved_skill() {
    let updater: SkillUpdater = Arc::new(|_, _| Box::pin(async { Ok(updated()) }));
    let (service, operations) = service(updater);
    let id = service.begin(&request()).expect("begin update");
    let operation = operations.get(&id).expect("operation");
    assert_eq!(operation.kind, OperationKind::Update);
    assert_eq!(operation.tool, Some(ToolId::ClaudeCode));
    assert_eq!(operation.status, OperationStatus::Running);
    assert_eq!(
        operation
            .extension
            .as_ref()
            .map(|target| target.id.as_str()),
        Some("anthropics/skills:skills/code-review")
    );
}

#[tokio::test]
async fn successful_update_reaches_ready_and_releases_the_tool_lock() {
    let updater: SkillUpdater = Arc::new(|_, _| Box::pin(async { Ok(updated()) }));
    let (service, operations) = service(updater);
    let request = request();
    let id = service.begin(&request).expect("begin update");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation");
    assert_eq!(operation.status, OperationStatus::Success);
    assert_eq!(operation.progress, 100);
    operations
        .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
        .expect("lock released");
}

#[tokio::test]
async fn failed_update_keeps_a_scoped_error_and_releases_the_lock() {
    let updater: SkillUpdater = Arc::new(|_, _| {
        Box::pin(async {
            Err(AppError::new(
                ErrorCode::NetworkError,
                "error.skill.updateDownloadFailed",
            ))
        })
    });
    let (service, operations) = service(updater);
    let request = request();
    let id = service.begin(&request).expect("begin update");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation
            .error
            .as_ref()
            .map(|error| error.message_key.as_str()),
        Some("error.skill.updateDownloadFailed")
    );
    assert_eq!(
        operation
            .error
            .as_ref()
            .and_then(|error| error.context_id.as_deref()),
        Some(id.as_str())
    );
    operations
        .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
        .expect("lock released");
}

#[tokio::test]
async fn mismatched_request_never_reaches_the_updater() {
    let updater: SkillUpdater =
        Arc::new(|_, _| Box::pin(async { panic!("mismatched request reached updater") }));
    let (service, operations) = service(updater);
    let original = request();
    let id = service.begin(&original).expect("begin update");
    let mut mismatched = original;
    mismatched.id = "someone/else:other".to_string();
    service.run(id.clone(), mismatched).await;
    assert_eq!(
        operations.get(&id).expect("operation").status,
        OperationStatus::Failed
    );
}

#[tokio::test]
async fn panic_details_are_redacted() {
    let updater: SkillUpdater =
        Arc::new(|_, _| Box::pin(async { panic!("token=ghp_secret_value update panic") }));
    let (service, operations) = service(updater);
    let request = request();
    let id = service.begin(&request).expect("begin update");
    service.run(id.clone(), request).await;
    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("error");
    assert_eq!(error.message_key, "error.skill.updatePanicked");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("ghp_secret_value"));
}
