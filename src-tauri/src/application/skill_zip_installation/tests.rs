use std::sync::Arc;

use tempfile::NamedTempFile;

use super::{
    SkillZipInstallRequest, SkillZipInstallationService, SkillZipInstaller, MAX_ARCHIVE_NAME_CHARS,
};
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, ExtensionManagement, ExtensionScope, Operation,
    OperationKind, OperationStatus, ToolId,
};
use crate::infrastructure::{OperationEvents, OperationManager};

struct SilentEvents;

impl OperationEvents for SilentEvents {
    fn emit(&self, _operation: &Operation) {}
}

fn selected(name: &str) -> (NamedTempFile, SkillZipInstallRequest) {
    let archive = tempfile::Builder::new()
        .prefix(name)
        .suffix(".zip")
        .tempfile()
        .expect("temporary ZIP selection");
    let request =
        SkillZipInstallRequest::selected(ToolId::ClaudeCode, archive.path().to_path_buf())
            .expect("valid native selection");
    (archive, request)
}

fn installed(tool: ToolId, id: &str) -> Extension {
    Extension {
        kind: ExtensionKind::Skill,
        id: id.to_string(),
        scope: ExtensionScope::tool(tool),
        name: id.trim_start_matches("local:").to_string(),
        description: None,
        management: ExtensionManagement::Managed,
        enabled: true,
        can_disable: true,
    }
}

fn service(installer: SkillZipInstaller) -> (SkillZipInstallationService, Arc<OperationManager>) {
    let operations = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    (
        SkillZipInstallationService::new(operations.clone(), installer),
        operations,
    )
}

#[test]
fn selection_rejects_non_zip_paths_and_unsupported_tools_before_starting() {
    let other = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temporary text file");
    let invalid = SkillZipInstallRequest::selected(ToolId::ClaudeCode, other.path().to_path_buf())
        .expect_err("non-ZIP selection must fail");
    assert_eq!(invalid.message_key, "error.skill.zipInvalid");
    assert!(!invalid
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains(&other.path().to_string_lossy().to_string()));

    let archive = tempfile::Builder::new()
        .suffix(".zip")
        .tempfile()
        .expect("temporary ZIP");
    let unsupported =
        SkillZipInstallRequest::selected(ToolId::OpenClaw, archive.path().to_path_buf())
            .expect_err("unsupported tool must fail before task creation");
    assert_eq!(unsupported.message_key, "error.skill.unsupportedTool");
}

#[test]
fn begin_uses_a_bounded_archive_label_without_exposing_the_path() {
    let long_name = "a".repeat(MAX_ARCHIVE_NAME_CHARS + 40);
    let (_archive, request) = selected(&long_name);
    let installer: SkillZipInstaller = Arc::new(|_, _| Box::pin(async { unreachable!() }));
    let (service, operations) = service(installer);
    let id = service.begin(&request).expect("begin local ZIP install");
    let operation = operations.get(&id).expect("operation");
    let target = operation.extension.expect("Skill task target");
    assert_eq!(operation.kind, OperationKind::Install);
    assert_eq!(target.kind, ExtensionKind::Skill);
    assert!(target.name.chars().count() <= MAX_ARCHIVE_NAME_CHARS);
    assert!(!target.name.contains(std::path::MAIN_SEPARATOR));
}

#[tokio::test]
async fn successful_multi_skill_install_reaches_ready_and_releases_the_lock() {
    let (_archive, request) = selected("skills");
    let installer: SkillZipInstaller = Arc::new(|tool, _| {
        Box::pin(async move {
            Ok(vec![
                installed(tool, "local:review"),
                installed(tool, "local:release"),
            ])
        })
    });
    let (service, operations) = service(installer);
    let id = service.begin(&request).expect("begin local ZIP install");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation");
    assert_eq!(operation.status, OperationStatus::Success);
    assert_eq!(operation.progress, 100);
    operations
        .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
        .expect("successful ZIP task releases the tool lock");
}

#[tokio::test]
async fn an_archive_with_only_conflicts_fails_honestly() {
    let (_archive, request) = selected("duplicates");
    let installer: SkillZipInstaller = Arc::new(|_, _| Box::pin(async { Ok(Vec::new()) }));
    let (service, operations) = service(installer);
    let id = service.begin(&request).expect("begin local ZIP install");
    service.run(id.clone(), request).await;

    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("empty result must be a task failure");
    assert_eq!(error.message_key, "error.skill.zipNoNewSkills");
    assert_eq!(error.context_id.as_deref(), Some(id.as_str()));
}

#[tokio::test]
async fn a_mismatched_installed_row_fails_verification() {
    let (_archive, request) = selected("wrong-tool");
    let installer: SkillZipInstaller =
        Arc::new(|_, _| Box::pin(async { Ok(vec![installed(ToolId::Codex, "local:review")]) }));
    let (service, operations) = service(installer);
    let id = service.begin(&request).expect("begin local ZIP install");
    service.run(id.clone(), request).await;

    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("verification error");
    assert_eq!(error.message_key, "error.skill.zipVerifyFailed");
}

#[tokio::test]
async fn installer_failure_keeps_a_scoped_task_and_releases_the_lock() {
    let (_archive, request) = selected("failed");
    let installer: SkillZipInstaller = Arc::new(|_, _| {
        Box::pin(async {
            Err(AppError::new(
                ErrorCode::InstallFailed,
                "error.skill.zipInstallFailed",
            ))
        })
    });
    let (service, operations) = service(installer);
    let id = service.begin(&request).expect("begin local ZIP install");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("failed operation remains");
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
        .expect("failed ZIP task releases the tool lock");
}

#[tokio::test]
async fn a_panicking_installer_is_redacted_and_finishes_the_task() {
    let (_archive, request) = selected("panic");
    let installer: SkillZipInstaller =
        Arc::new(|_, _| Box::pin(async { panic!("token=ghp_zip_secret local archive panic") }));
    let (service, operations) = service(installer);
    let id = service.begin(&request).expect("begin local ZIP install");
    service.run(id.clone(), request).await;

    let error = operations
        .get(&id)
        .and_then(|operation| operation.error)
        .expect("panic becomes structured failure");
    assert_eq!(error.message_key, "error.skill.actionPanicked");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("ghp_zip_secret"));
}

#[tokio::test]
async fn a_mismatched_request_never_reaches_the_installer() {
    let (_archive, request) = selected("original");
    let installer: SkillZipInstaller =
        Arc::new(|_, _| Box::pin(async { panic!("mismatched ZIP request reached installer") }));
    let (service, operations) = service(installer);
    let id = service.begin(&request).expect("begin local ZIP install");
    let mut mismatched = request;
    mismatched.archive_name = "different.zip".to_string();
    service.run(id.clone(), mismatched).await;

    let operation = operations.get(&id).expect("failed operation remains");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation.error.as_ref().map(|error| error.code),
        Some(ErrorCode::Internal)
    );
}
