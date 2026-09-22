use super::{
    parse_kind, parse_tool, parse_tools, start, start_mcp, start_mcp_removal, start_skill,
    start_skill_removal,
};
use crate::adapters::{AdapterRegistry, AuthorizedUpdate, LifecycleContext, ToolAdapter};
use crate::application::mcp_installation::{McpInstallationService, McpInstaller};
use crate::application::mcp_removal::{McpRemovalRequest, McpRemovalService, McpRemover};
use crate::application::skill_installation::{
    SkillInstallRequest, SkillInstallationService, SkillInstaller,
};
use crate::application::skill_removal::{SkillRemovalRequest, SkillRemovalService, SkillRemover};
use crate::application::tool_lifecycle::{LifecycleRequest, ToolLifecycleService};
use crate::application::tool_version_history::ToolVersionHistoryService;
use crate::domain::{
    ErrorCode, Extension, ExtensionKind, McpConnectionDraft, McpInstallDraft, OperationKind,
    OperationStatus, SkillCatalogItem, SkillSource, Tool, ToolCapabilities, ToolId,
    ToolInstallSource, ToolStatus, UninstallOptions,
};
use crate::infrastructure::{OperationEvents, OperationManager};
use crate::platform::executor::{CommandExecutor, CommandOutput};
use futures::future::BoxFuture;
use std::sync::Arc;

struct SilentEvents;

impl OperationEvents for SilentEvents {
    fn emit(&self, _operation: &crate::domain::Operation) {}
}

struct NeverExecutor;

impl CommandExecutor for NeverExecutor {
    fn execute(
        &self,
        _spec: crate::platform::command::CommandSpec,
    ) -> BoxFuture<'static, Result<CommandOutput, crate::domain::AppError>> {
        Box::pin(async { panic!("no command may run in the command-layer tests") })
    }
}

#[test]
fn tools_command_is_backed_by_the_adapter_registry() {
    let ids: Vec<ToolId> = AdapterRegistry::all()
        .iter()
        .map(|adapter| adapter.id())
        .collect();
    assert_eq!(ids, ToolId::ALL.to_vec());
}

#[test]
fn operations_command_reads_from_the_manager() {
    let manager = OperationManager::new(Box::new(SilentEvents));
    assert!(manager.list().is_empty());
    manager
        .begin(OperationKind::Scan, None)
        .expect("scan begins");
    let operations = super::list_operations(&manager);
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].kind, OperationKind::Scan);
}

#[test]
fn every_domain_tool_id_round_trips_and_upstream_names_are_rejected() {
    for id in ToolId::ALL {
        assert_eq!(parse_tool(id.as_str()).expect("known tool"), id);
    }
    for raw in ["claude", "gemini", "open-code", "", "Codex"] {
        let error = parse_tool(raw).expect_err("{raw} is not a product tool id");
        assert_eq!(error.code, ErrorCode::ToolNotFound);
        assert_eq!(error.message_key, "error.tool.notFound");
    }
}

#[test]
fn update_preview_request_accepts_only_product_tool_ids() {
    assert_eq!(
        parse_tools(vec!["claude-code".into(), "codex".into()]).unwrap(),
        vec![ToolId::ClaudeCode, ToolId::Codex]
    );
    assert_eq!(
        parse_tools(vec!["codex".into(), "codex".into()]).unwrap(),
        vec![ToolId::Codex]
    );
    assert!(parse_tools(vec!["claude".into()]).is_err());
    assert!(parse_tools(Vec::new()).is_err());
}

#[tokio::test]
async fn starting_an_action_for_an_unregistered_tool_never_spawns_anything() {
    let manager = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    let service = ToolLifecycleService::new(
        manager.clone(),
        Arc::new(NeverExecutor),
        Arc::new(|_id: ToolId| None::<Box<dyn ToolAdapter>>),
        ToolVersionHistoryService::discarding(),
    );
    let error = start(service, LifecycleRequest::Install(ToolId::Codex))
        .expect_err("no adapter means no operation");
    assert_eq!(error.code, ErrorCode::ToolNotFound);
    assert!(manager.list().is_empty());
}

/// Trivially-successful adapter: `install`/`update`/`uninstall` resolve
/// immediately without touching the executor, so `run()` reaches a
/// terminal state on its first poll.
struct SucceedingAdapter {
    id: ToolId,
}

impl SucceedingAdapter {
    fn tool(&self, status: ToolStatus, latest_version: Option<&str>) -> Tool {
        Tool {
            id: self.id,
            name: "Stub".to_string(),
            description_key: "tool.stub.description".to_string(),
            discovery: crate::domain::ToolDiscovery::for_tool(self.id),
            status,
            version: Some("1.0.0".to_string()),
            latest_version: latest_version.map(str::to_string),
            capabilities: self.capabilities(),
            sessions_inside_settings: false,
            environment: Some("macos".to_string()),
            configuration_shared_with: None,
        }
    }
}

impl ToolAdapter for SucceedingAdapter {
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

    fn detect(&self) -> BoxFuture<'_, Result<Tool, crate::domain::AppError>> {
        Box::pin(async move { Ok(self.tool(ToolStatus::Installed, None)) })
    }

    fn install(
        &self,
        _ctx: LifecycleContext,
    ) -> BoxFuture<'_, Result<(), crate::domain::AppError>> {
        Box::pin(async { Ok(()) })
    }

    fn validate_update(
        &self,
        _preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, crate::domain::AppError>> {
        let authorized = AuthorizedUpdate {
            observed: self.tool(ToolStatus::UpdateAvailable, Some("2.0.0")),
            target_version: "2.0.0".to_string(),
            source: ToolInstallSource::Unmanaged,
            plan: None,
        };
        Box::pin(async move { Ok(authorized) })
    }

    fn update(
        &self,
        _ctx: LifecycleContext,
        _authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), crate::domain::AppError>> {
        Box::pin(async { Ok(()) })
    }

    fn uninstall(
        &self,
        _ctx: LifecycleContext,
        _options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), crate::domain::AppError>> {
        Box::pin(async { Ok(()) })
    }
}

/// T8 review's acceptance precondition #1: between a successful `begin`
/// and `tokio::spawn`, `start()` has zero fallible statements, so a
/// successful return must guarantee `run()` was scheduled and will drive
/// the operation to a terminal state — not just hand back an id that
/// silently never gets executed. This pins that guarantee down at the
/// command layer (T8's own suite proves it for `run()` called directly).
#[tokio::test]
async fn a_successful_start_guarantees_run_is_scheduled_and_reaches_a_terminal_state() {
    let manager = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    let service = ToolLifecycleService::new(
        manager.clone(),
        Arc::new(NeverExecutor),
        Arc::new(|id: ToolId| Some(Box::new(SucceedingAdapter { id }) as Box<dyn ToolAdapter>)),
        ToolVersionHistoryService::discarding(),
    );
    let id = start(service, LifecycleRequest::Install(ToolId::Codex))
        .expect("a resolvable adapter lets start() succeed");

    // start() only guarantees the spawn was *issued*; yield so the
    // scheduler gets a chance to actually drive it to completion before
    // we assert on the outcome.
    for _ in 0..200 {
        let still_running = manager
            .get(&id)
            .map(|operation| operation.status == OperationStatus::Running)
            .unwrap_or(false);
        if !still_running {
            break;
        }
        tokio::task::yield_now().await;
    }

    let operation = manager.get(&id).expect("operation exists");
    assert_eq!(
        operation.status,
        OperationStatus::Success,
        "start() must actually schedule run() to completion, not just return an id"
    );
}

#[tokio::test]
async fn a_successful_skill_start_is_scheduled_and_reaches_a_terminal_state() {
    let manager = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    let item = SkillCatalogItem {
        id: "anthropics/skills:code-review".to_string(),
        name: "Code review".to_string(),
        description: None,
        source: SkillSource {
            owner: "anthropics".to_string(),
            repository: "skills".to_string(),
            branch: "main".to_string(),
            directory: "code-review".to_string(),
        },
        installed: false,
        mirror_used: false,
    };
    let installed_id = item.id.clone();
    let installer: SkillInstaller = Arc::new(move |tool, _| {
        let installed_id = installed_id.clone();
        Box::pin(async move {
            Ok(Extension {
                kind: ExtensionKind::Skill,
                id: installed_id,
                scope: crate::domain::ExtensionScope::tool(tool),
                name: "Code review".to_string(),
                description: None,
                management: crate::domain::ExtensionManagement::Managed,
                enabled: true,
                can_disable: true,
            })
        })
    });
    let service = SkillInstallationService::new(manager.clone(), installer);
    let id = start_skill(
        service,
        SkillInstallRequest {
            tool: ToolId::ClaudeCode,
            item,
        },
    )
    .expect("Skill start succeeds");

    for _ in 0..200 {
        if manager
            .get(&id)
            .is_some_and(|operation| operation.status != OperationStatus::Running)
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        manager.get(&id).expect("operation exists").status,
        OperationStatus::Success
    );
}

#[tokio::test]
async fn a_successful_mcp_start_is_scheduled_and_reaches_a_terminal_state() {
    let manager = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    let draft = McpInstallDraft {
        name: "Docs".to_string(),
        description: None,
        connection: McpConnectionDraft::Http {
            url: "https://mcp.example.test".to_string(),
        },
    };
    let request =
        McpInstallationService::prepare(crate::domain::ExtensionScope::tool(ToolId::Codex), draft)
            .expect("valid request");
    let installed_id = request.id.clone();
    let installer: McpInstaller = Arc::new(move |scope, _, draft| {
        let installed_id = installed_id.clone();
        Box::pin(async move {
            Ok(Extension {
                kind: ExtensionKind::Mcp,
                id: installed_id,
                scope,
                name: draft.normalized_name(),
                description: draft.normalized_description(),
                management: crate::domain::ExtensionManagement::Managed,
                enabled: true,
                can_disable: true,
            })
        })
    });
    let service = McpInstallationService::new(manager.clone(), installer);
    let id = start_mcp(service, request).expect("MCP start succeeds");

    for _ in 0..200 {
        if manager
            .get(&id)
            .is_some_and(|operation| operation.status != OperationStatus::Running)
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        manager.get(&id).expect("operation exists").status,
        OperationStatus::Success
    );
}

#[tokio::test]
async fn a_successful_mcp_removal_is_scheduled_and_reaches_a_terminal_state() {
    let manager = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    let remover: McpRemover = Arc::new(|_, _| Box::pin(async { Ok(()) }));
    let service = McpRemovalService::new(manager.clone(), remover);
    let id = start_mcp_removal(
        service,
        McpRemovalRequest {
            scope: crate::domain::ExtensionScope::tool(ToolId::Codex),
            id: "docs-a1b2c3d4".to_string(),
            name: "Project docs".to_string(),
        },
    )
    .expect("MCP removal start succeeds");

    for _ in 0..200 {
        if manager
            .get(&id)
            .is_some_and(|operation| operation.status != OperationStatus::Running)
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        manager.get(&id).expect("operation exists").status,
        OperationStatus::Success
    );
}

#[tokio::test]
async fn a_successful_skill_removal_is_scheduled_and_reaches_a_terminal_state() {
    let manager = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    let remover: SkillRemover = Arc::new(|_, _| Box::pin(async { Ok(()) }));
    let service = SkillRemovalService::new(manager.clone(), remover);
    let id = start_skill_removal(
        service,
        SkillRemovalRequest {
            tool: ToolId::ClaudeCode,
            id: "anthropics/skills:code-review".to_string(),
            name: "Code review".to_string(),
        },
    )
    .expect("Skill removal start succeeds");

    for _ in 0..200 {
        if manager
            .get(&id)
            .is_some_and(|operation| operation.status != OperationStatus::Running)
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        manager.get(&id).expect("operation exists").status,
        OperationStatus::Success
    );
}

#[test]
fn the_uninstall_payload_defaults_to_app_only() {
    let options: UninstallOptions = serde_json::from_str("{}").expect("an absent options object");
    assert_eq!(options, UninstallOptions::default());
}

#[test]
fn the_provider_commands_reject_upstream_tool_names_the_same_way() {
    for raw in ["claude", "gemini", "grokbuild", ""] {
        let error = parse_tool(raw).expect_err("{raw} is not a product tool id");
        assert_eq!(error.code, ErrorCode::ToolNotFound);
    }
    for id in ToolId::ALL {
        assert_eq!(parse_tool(id.as_str()).expect("known tool"), id);
    }
}

#[test]
fn a_draft_without_a_key_deserializes_to_keeping_the_stored_one() {
    let draft: crate::domain::ProviderDraft =
        serde_json::from_str(r#"{"name":"Anthropic","apiKey":null}"#).expect("draft");
    assert_eq!(draft.api_key, None);
}

#[test]
fn every_extension_kind_round_trips_and_upstream_words_are_rejected() {
    for kind in crate::domain::ExtensionKind::ALL {
        assert_eq!(parse_kind(kind.as_str()).expect("known kind"), kind);
    }
    for raw in ["mcpServer", "skills", "prompts", "Mcp", ""] {
        let error = parse_kind(raw).expect_err("not a product extension kind");
        assert_eq!(error.code, ErrorCode::ExtensionNotFound);
        assert_eq!(error.message_key, "error.extension.unknownKind");
    }
}

/// If a command isn't registered in `lib.rs`'s `invoke_handler` list, it
/// simply doesn't exist at runtime: neither compilation nor tests catch it —
/// only clicking the button for real reveals it. The handover doc lists this
/// as one of the "three easiest things to forget", so this test cross-checks
/// the list against `#[tauri::command]` in every product command file. Update
/// the count below when adding a command (same as `message_keys.rs`'s `len()`).
#[test]
fn every_product_command_is_registered_in_the_invoke_handler() {
    let api = [
        "src/commands/app_api.rs",
        "src/commands/app_desktop_api.rs",
        "src/commands/app_network_api.rs",
        "src/commands/app_routing_api.rs",
        "src/commands/app_session_api.rs",
        "src/commands/app_skill_api.rs",
        "src/commands/app_system_api.rs",
        "src/commands/app_update_api.rs",
        "src/commands/app_usage_api.rs",
        "src/commands/app_workspace_api.rs",
    ]
    .into_iter()
    .map(|path| std::fs::read_to_string(path).unwrap_or_else(|_| panic!("read {path}")))
    .collect::<Vec<_>>()
    .join("\n");
    let lib = std::fs::read_to_string("src/lib.rs").expect("read lib.rs");

    let mut declared: Vec<&str> = Vec::new();
    let mut lines = api.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() != "#[tauri::command]" {
            continue;
        }
        let signature = lines.peek().copied().unwrap_or_default();
        let name = signature
            .trim_start()
            .trim_start_matches("pub async fn ")
            .trim_start_matches("pub fn ")
            .split('(')
            .next()
            .unwrap_or_default();
        assert!(
            name.starts_with("app_"),
            "unreadable command signature: {signature}"
        );
        declared.push(name);
    }

    assert_eq!(declared.len(), 114, "found {declared:?}");
    for name in declared {
        assert!(
            lib.contains(&format!("commands::{name},")),
            "{name} is missing from the invoke_handler list in lib.rs"
        );
    }
}
