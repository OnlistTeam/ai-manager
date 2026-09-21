use super::{McpInstallRequest, McpInstallationService, McpInstaller};
use crate::domain::{
    Extension, ExtensionKind, ExtensionScope, McpConnectionDraft, McpInstallDraft, OperationKind,
    OperationStatus, ToolId,
};
use crate::infrastructure::{OperationEvents, OperationManager};
use std::sync::Arc;

struct SilentEvents;

impl OperationEvents for SilentEvents {
    fn emit(&self, _operation: &crate::domain::Operation) {}
}

fn manager() -> Arc<OperationManager> {
    Arc::new(OperationManager::new(Box::new(SilentEvents)))
}

fn draft(name: &str) -> McpInstallDraft {
    McpInstallDraft {
        name: name.to_string(),
        description: Some("A trusted local connection.".to_string()),
        connection: McpConnectionDraft::Stdio {
            command: "npx".to_string(),
            arguments: vec!["-y".to_string(), "server-package".to_string()],
        },
    }
}

fn installed(request: &McpInstallRequest) -> Extension {
    Extension {
        kind: ExtensionKind::Mcp,
        id: request.id.clone(),
        scope: request.scope,
        name: request.draft.normalized_name(),
        description: request.draft.normalized_description(),
        management: crate::domain::ExtensionManagement::Managed,
        enabled: true,
        can_disable: true,
    }
}

#[test]
fn prepare_generates_a_private_safe_id_and_validates_before_starting() {
    let request = McpInstallationService::prepare(
        ExtensionScope::tool(ToolId::ClaudeCode),
        draft("My Files"),
    )
    .expect("valid draft");
    assert!(request.id.starts_with("my-files-"));
    assert_eq!(request.id.len(), "my-files-".len() + 8);
    assert!(request
        .id
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'));

    let error =
        McpInstallationService::prepare(ExtensionScope::tool(ToolId::ClaudeCode), draft(" "))
            .err()
            .expect("blank name is rejected before an operation exists");
    assert_eq!(error.message_key, "error.mcp.nameRequired");
}

#[test]
fn begin_records_mcp_metadata_and_uses_the_shared_tool_lock() {
    let operations = manager();
    let installer: McpInstaller = Arc::new(|_, _, _| unreachable!());
    let service = McpInstallationService::new(operations.clone(), installer);
    let request =
        McpInstallationService::prepare(ExtensionScope::tool(ToolId::Codex), draft("Docs"))
            .expect("prepare request");
    let id = service.begin(&request).expect("begin MCP install");

    let operation = operations.get(&id).expect("operation exists");
    assert_eq!(operation.kind, OperationKind::Install);
    assert_eq!(operation.tool, Some(ToolId::Codex));
    let target = operation.extension.expect("MCP metadata");
    assert_eq!(target.kind, ExtensionKind::Mcp);
    assert_eq!(target.id, request.id);
    assert_eq!(target.name, "Docs");

    let conflict = operations
        .begin(OperationKind::Update, Some(ToolId::Codex))
        .expect_err("tool lifecycle cannot race its MCP write");
    assert_eq!(conflict.message_key, "error.operation.toolBusy");
}

#[tokio::test]
async fn a_verified_install_finishes_successfully() {
    let operations = manager();
    let request =
        McpInstallationService::prepare(ExtensionScope::tool(ToolId::OpenCode), draft("Browser"))
            .expect("prepare request");
    let expected = installed(&request);
    let installer: McpInstaller = Arc::new(move |_, _, _| {
        let expected = expected.clone();
        Box::pin(async move { Ok(expected) })
    });
    let service = McpInstallationService::new(operations.clone(), installer);
    let id = service.begin(&request).expect("begin");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation exists");
    assert_eq!(operation.status, OperationStatus::Success);
    assert_eq!(operation.progress, 100);
    assert_eq!(
        operation.message_key.as_deref(),
        Some("operation.phase.ready")
    );
}

#[tokio::test]
async fn verification_failure_is_structured_and_releases_the_tool() {
    let operations = manager();
    let request =
        McpInstallationService::prepare(ExtensionScope::tool(ToolId::GeminiCli), draft("Search"))
            .expect("prepare request");
    let installer: McpInstaller = Arc::new(|scope, _, _| {
        Box::pin(async move {
            Ok(Extension {
                kind: ExtensionKind::Mcp,
                id: "wrong".to_string(),
                scope,
                name: "Wrong".to_string(),
                description: None,
                management: crate::domain::ExtensionManagement::Managed,
                enabled: true,
                can_disable: true,
            })
        })
    });
    let service = McpInstallationService::new(operations.clone(), installer);
    let id = service.begin(&request).expect("begin");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation exists");
    assert_eq!(operation.status, OperationStatus::Failed);
    let error = operation.error.expect("structured error");
    assert_eq!(error.message_key, "error.mcp.verifyFailed");
    assert_eq!(error.context_id.as_deref(), Some(id.as_str()));
    operations
        .begin(OperationKind::Update, Some(ToolId::GeminiCli))
        .expect("failed MCP install released the tool lock");
}

#[tokio::test]
async fn a_panic_before_future_creation_is_redacted_and_terminal() {
    let operations = manager();
    let request =
        McpInstallationService::prepare(ExtensionScope::tool(ToolId::ClaudeCode), draft("Files"))
            .expect("prepare request");
    let installer: McpInstaller = Arc::new(|_, _, _| {
        panic!("Authorization: Bearer sk-secret-value-123456");
    });
    let service = McpInstallationService::new(operations.clone(), installer);
    let id = service.begin(&request).expect("begin");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation exists");
    assert_eq!(operation.status, OperationStatus::Failed);
    let error = operation.error.expect("panic becomes an error");
    assert_eq!(error.message_key, "error.mcp.actionPanicked");
    let technical = error.technical_message.expect("safe panic detail");
    assert!(!technical.contains("sk-secret-value-123456"));
    assert!(technical.contains("***"));
}

#[tokio::test]
async fn a_panic_while_the_install_future_runs_is_redacted_and_terminal() {
    let operations = manager();
    let request =
        McpInstallationService::prepare(ExtensionScope::tool(ToolId::ClaudeCode), draft("Files"))
            .expect("prepare request");
    let installer: McpInstaller =
        Arc::new(|_, _, _| Box::pin(async { panic!("x-api-key: sk-async-secret-value-123456") }));
    let service = McpInstallationService::new(operations.clone(), installer);
    let id = service.begin(&request).expect("begin");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation exists");
    assert_eq!(operation.status, OperationStatus::Failed);
    let error = operation.error.expect("panic becomes an error");
    assert_eq!(error.message_key, "error.mcp.actionPanicked");
    let technical = error.technical_message.expect("safe panic detail");
    assert!(!technical.contains("sk-async-secret-value-123456"));
    assert!(technical.contains("***"));
}

#[tokio::test]
async fn mismatched_operation_and_request_never_call_the_installer() {
    let operations = manager();
    let installer: McpInstaller = Arc::new(|_, _, _| panic!("installer must not run"));
    let service = McpInstallationService::new(operations.clone(), installer);
    let request =
        McpInstallationService::prepare(ExtensionScope::tool(ToolId::ClaudeCode), draft("Files"))
            .expect("prepare request");
    let id = service.begin(&request).expect("begin");
    let mut mismatched = request;
    mismatched.id = "different-id".to_string();
    service.run(id.clone(), mismatched).await;

    let operation = operations.get(&id).expect("operation exists");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation.error.expect("error").message_key,
        "error.mcp.installFailed"
    );
}
