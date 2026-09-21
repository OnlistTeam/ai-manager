use super::{SkillInstallRequest, SkillInstallationService, SkillInstaller};
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, ExtensionScope, Operation, OperationStatus,
    SkillCatalogItem, SkillSource, ToolId,
};
use crate::infrastructure::{OperationEvents, OperationManager};
use std::sync::Arc;

struct SilentEvents;

impl OperationEvents for SilentEvents {
    fn emit(&self, _operation: &Operation) {}
}

fn item() -> SkillCatalogItem {
    SkillCatalogItem {
        id: "anthropics/skills:skills/code-review".to_string(),
        name: "Code review".to_string(),
        description: Some("Reviews changes.".to_string()),
        source: SkillSource {
            owner: "anthropics".to_string(),
            repository: "skills".to_string(),
            branch: "main".to_string(),
            directory: "skills/code-review".to_string(),
        },
        installed: false,
        mirror_used: false,
    }
}

fn installed(request: &SkillInstallRequest) -> Extension {
    Extension {
        kind: ExtensionKind::Skill,
        id: request.item.id.clone(),
        scope: ExtensionScope::tool(request.tool),
        name: request.item.name.clone(),
        description: request.item.description.clone(),
        management: crate::domain::ExtensionManagement::Managed,
        enabled: true,
        can_disable: true,
    }
}

fn service(installer: SkillInstaller) -> (SkillInstallationService, Arc<OperationManager>) {
    let operations = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    (
        SkillInstallationService::new(operations.clone(), installer),
        operations,
    )
}

#[test]
fn unsupported_tools_and_installed_rows_never_create_an_operation() {
    let installer: SkillInstaller = Arc::new(|_, _| Box::pin(async { unreachable!() }));
    let (service, operations) = service(installer);

    let unsupported = SkillInstallRequest {
        tool: ToolId::OpenClaw,
        item: item(),
    };
    assert_eq!(
        service
            .begin(&unsupported)
            .expect_err("OpenClaw has no Skill capability")
            .message_key,
        "error.skill.unsupportedTool"
    );

    let mut already = item();
    already.installed = true;
    let duplicate = SkillInstallRequest {
        tool: ToolId::ClaudeCode,
        item: already,
    };
    assert_eq!(
        service
            .begin(&duplicate)
            .expect_err("already installed")
            .code,
        ErrorCode::OperationConflict
    );
    assert!(operations.list().is_empty());
}

#[tokio::test]
async fn successful_install_reaches_ready_and_preserves_the_skill_target() {
    let request = SkillInstallRequest {
        tool: ToolId::ClaudeCode,
        item: item(),
    };
    let expected = installed(&request);
    let installer: SkillInstaller = Arc::new(move |_, _| {
        let expected = expected.clone();
        Box::pin(async move { Ok(expected) })
    });
    let (service, operations) = service(installer);
    let id = service.begin(&request).expect("begin install");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation remains in history");
    assert_eq!(operation.status, OperationStatus::Success);
    assert_eq!(operation.progress, 100);
    assert_eq!(
        operation.message_key.as_deref(),
        Some("operation.phase.ready")
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
async fn installer_failure_releases_the_lock_and_keeps_details_in_the_task() {
    let installer: SkillInstaller = Arc::new(|_, _| {
        Box::pin(async {
            Err(
                AppError::new(ErrorCode::NetworkError, "error.skill.downloadFailed")
                    .with_technical("connection reset"),
            )
        })
    });
    let (service, operations) = service(installer);
    let request = SkillInstallRequest {
        tool: ToolId::ClaudeCode,
        item: item(),
    };
    let id = service.begin(&request).expect("begin install");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("failed operation remains");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation.error.as_ref().map(|error| error.code),
        Some(ErrorCode::NetworkError)
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
        .expect("failed Skill install releases the tool lock");
}

#[tokio::test]
async fn a_panicking_installer_still_finishes_the_operation() {
    let installer: SkillInstaller =
        Arc::new(|_, _| Box::pin(async { panic!("token=ghp_do_not_leak") }));
    let (service, operations) = service(installer);
    let request = SkillInstallRequest {
        tool: ToolId::ClaudeCode,
        item: item(),
    };
    let id = service.begin(&request).expect("begin install");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation exists");
    assert_eq!(operation.status, OperationStatus::Failed);
    let error = operation.error.expect("panic becomes structured error");
    assert_eq!(error.message_key, "error.skill.actionPanicked");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("ghp_do_not_leak"));
}

#[tokio::test]
async fn a_mismatched_install_result_fails_verification() {
    let installer: SkillInstaller = Arc::new(|tool, _| {
        Box::pin(async move {
            Ok(Extension {
                kind: ExtensionKind::Skill,
                id: "different".to_string(),
                scope: ExtensionScope::tool(tool),
                name: "Different".to_string(),
                description: None,
                management: crate::domain::ExtensionManagement::Managed,
                enabled: true,
                can_disable: true,
            })
        })
    });
    let (service, operations) = service(installer);
    let request = SkillInstallRequest {
        tool: ToolId::ClaudeCode,
        item: item(),
    };
    let id = service.begin(&request).expect("begin install");
    service.run(id.clone(), request).await;

    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("verification error");
    assert_eq!(error.message_key, "error.skill.verifyFailed");
}

#[tokio::test]
async fn a_mismatched_request_still_fails_and_releases_the_tool_lock() {
    let installer: SkillInstaller = Arc::new(|_, _| {
        Box::pin(async { panic!("a mismatched request must never reach the installer") })
    });
    let (service, operations) = service(installer);
    let original = SkillInstallRequest {
        tool: ToolId::ClaudeCode,
        item: item(),
    };
    let id = service.begin(&original).expect("begin install");
    let mut mismatched = original;
    mismatched.item.id = "anthropics/skills:skills/other".to_string();
    mismatched.item.source.directory = "skills/other".to_string();

    service.run(id.clone(), mismatched).await;

    let operation = operations.get(&id).expect("failed operation remains");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation.error.as_ref().map(|error| error.code),
        Some(ErrorCode::Internal)
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
        .expect("a mismatched request releases the tool lock");
}
