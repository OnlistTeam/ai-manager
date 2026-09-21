//! Turning a confirmed deep link into the product's ordinary write paths
//! (ADR-0029 decision 5).
//!
//! This service owns no write of its own. Every resource is handed to the same
//! application entry point the user interface uses, so capability gates,
//! validation, the eight-step managed-file path and the task lock all apply
//! unchanged.
//!
//! Two deliberate omissions: the link's `enabled=true` is parsed but never
//! acted on. Switching the active service or turning on a server is a change to
//! what the tool does right now, and an import never makes that decision for
//! the user (the same rule the existing-setup import follows).

use std::sync::Arc;

use crate::compat::ccswitch::provider::deep_link::inspect_config;
use crate::compat::ccswitch::tools::{app_type_to_tool_id, capabilities_for, display_name_for};
use crate::domain::{
    AppError, DeepLinkBlockReason, DeepLinkCredentialField, DeepLinkImportOutcome, DeepLinkIntent,
    DeepLinkMcp, DeepLinkMcpConnection, DeepLinkMcpServer, DeepLinkPreview, DeepLinkPrompt,
    DeepLinkProvider, DeepLinkResource, DeepLinkSkill, DeepLinkTarget, ErrorCode, ExtensionScope,
    LinkOrigin, McpConnectionDraft, McpInstallDraft, PromptDraft, ProviderCustomCreateDraft,
    SkillCatalogItem, SkillSource, ToolCapabilities, ToolId,
};
use crate::infrastructure::OperationManager;

use super::mcp_installation::McpInstallationService;
use super::prompt_directory::PromptDirectory;
use super::provider_directory::ProviderDirectory;
use super::skill_installation::{SkillInstallRequest, SkillInstallationService};

mod queue;

pub use queue::{now_seconds, DeepLinkQueue, PendingDeepLink};

pub struct DeepLinkImportService;

impl DeepLinkImportService {
    /// Parses one link and queues it. Nothing is written, and the raw link is
    /// dropped here — only the validated intent survives.
    pub fn submit(
        queue: &DeepLinkQueue,
        raw: &str,
        origin: LinkOrigin,
    ) -> Result<DeepLinkPreview, AppError> {
        let intent = crate::domain::deep_link::parse(raw, origin)?;
        let now = now_seconds();
        let id = queue.push(origin, intent, now);
        queue.with(&id, now, project)
    }

    pub fn list(queue: &DeepLinkQueue) -> Vec<DeepLinkPreview> {
        queue.project(now_seconds(), project)
    }

    pub fn preview(queue: &DeepLinkQueue, id: &str) -> Result<DeepLinkPreview, AppError> {
        queue.with(id, now_seconds(), project)
    }

    pub fn dismiss(queue: &DeepLinkQueue, id: &str) -> Result<(), AppError> {
        queue.dismiss(id, now_seconds())
    }

    /// Consumes the entry, then performs the import. The entry is gone before
    /// any write starts, so a second press cannot import a second time.
    pub async fn confirm(
        app_handle: tauri::AppHandle,
        queue: &DeepLinkQueue,
        operations: Arc<OperationManager>,
        id: &str,
    ) -> Result<DeepLinkImportOutcome, AppError> {
        let pending = queue.take(id, now_seconds())?;
        if let Some(reason) = block_reason(&pending.intent) {
            return Err(blocked(reason));
        }
        let tools = supported_tools(&pending.intent);

        match pending.intent {
            DeepLinkIntent::Provider(provider) => {
                confirm_provider(app_handle, &pending.id, provider, first(&tools)?).await
            }
            DeepLinkIntent::Prompt(prompt) => {
                confirm_prompt(app_handle, prompt, first(&tools)?).await
            }
            DeepLinkIntent::Skill(skill) => {
                confirm_skill(app_handle, operations, skill, first(&tools)?)
            }
            DeepLinkIntent::Mcp(mcp) => confirm_mcp(app_handle, operations, mcp, &tools),
        }
    }
}

async fn confirm_provider(
    app_handle: tauri::AppHandle,
    request_id: &str,
    provider: DeepLinkProvider,
    tool: ToolId,
) -> Result<DeepLinkImportOutcome, AppError> {
    let evidence = provider
        .config
        .as_ref()
        .map(|config| inspect_config(tool, config));
    let base_url = provider
        .endpoint
        .clone()
        .or_else(|| evidence.as_ref().and_then(|found| found.base_url.clone()))
        .ok_or_else(|| {
            invalid(
                "error.deepLink.endpointRequired",
                "the link names no address for the service",
            )
        })?;
    let api_key = provider
        .api_key
        .clone()
        .or_else(|| evidence.as_ref().and_then(|found| found.api_key.clone()))
        .ok_or_else(|| blocked(DeepLinkBlockReason::CredentialRequired))?;

    let draft = ProviderCustomCreateDraft {
        name: provider.name.clone(),
        api_key,
        model: provider.model.clone().unwrap_or_default(),
        base_url,
    };
    let request_id = request_id.to_string();
    blocking(move || ProviderDirectory::create_custom(&app_handle, tool, &request_id, &draft))
        .await?;

    Ok(DeepLinkImportOutcome {
        resource: DeepLinkResource::Provider,
        tools: vec![tool],
        operations: Vec::new(),
        applied: 1,
    })
}

async fn confirm_prompt(
    app_handle: tauri::AppHandle,
    prompt: DeepLinkPrompt,
    tool: ToolId,
) -> Result<DeepLinkImportOutcome, AppError> {
    let draft = PromptDraft {
        name: prompt.name,
        description: prompt.description,
        content: prompt.content,
    };
    draft.validate()?;
    blocking(move || PromptDirectory::save(&app_handle, tool, None, &draft)).await?;

    Ok(DeepLinkImportOutcome {
        resource: DeepLinkResource::Prompt,
        tools: vec![tool],
        operations: Vec::new(),
        applied: 1,
    })
}

fn confirm_skill(
    app_handle: tauri::AppHandle,
    operations: Arc<OperationManager>,
    skill: DeepLinkSkill,
    tool: ToolId,
) -> Result<DeepLinkImportOutcome, AppError> {
    // The GitHub coordinates, the archive transport and the zip-slip guards all
    // belong to the Skill service; this only hands it the catalog identity.
    let request = SkillInstallRequest {
        tool,
        item: SkillCatalogItem {
            id: skill.catalog_id(),
            name: skill.display_name(),
            description: None,
            source: SkillSource {
                owner: skill.owner,
                repository: skill.repository,
                branch: skill.branch,
                directory: skill.directory,
            },
            installed: false,
            mirror_used: false,
        },
    };
    let service = SkillInstallationService::system(app_handle, operations);
    let id = service.begin(&request)?;
    let spawned = id.clone();
    tokio::spawn(async move { service.run(spawned, request).await });

    Ok(DeepLinkImportOutcome {
        resource: DeepLinkResource::Skill,
        tools: vec![tool],
        operations: vec![id],
        applied: 1,
    })
}

/// One link can name several servers and several tools. Every pair is validated
/// and gated before the first one starts, so a refusal leaves nothing behind.
///
/// They then run one at a time: `begin` takes the per-tool task lock, so
/// starting them together would make all but the first fail as a conflict. The
/// tasks appear in the Task Center as each one starts, which is why this
/// outcome reports a count rather than a list of task ids.
fn confirm_mcp(
    app_handle: tauri::AppHandle,
    operations: Arc<OperationManager>,
    mcp: DeepLinkMcp,
    tools: &[ToolId],
) -> Result<DeepLinkImportOutcome, AppError> {
    let mut requests = Vec::new();
    for tool in tools {
        for server in &mcp.servers {
            requests.push(McpInstallationService::prepare(
                ExtensionScope::tool(*tool),
                mcp_draft(server),
            )?);
        }
    }
    let applied = requests.len() as u32;

    tokio::spawn(async move {
        for request in requests {
            let service = McpInstallationService::system(app_handle.clone(), operations.clone());
            match service.begin(&request) {
                Ok(id) => service.run(id, request).await,
                Err(error) => log::warn!(
                    "a deep-linked MCP server could not start: {}",
                    error.message_key
                ),
            }
        }
    });

    Ok(DeepLinkImportOutcome {
        resource: DeepLinkResource::Mcp,
        tools: tools.to_vec(),
        operations: Vec::new(),
        applied,
    })
}

fn mcp_draft(server: &DeepLinkMcpServer) -> McpInstallDraft {
    McpInstallDraft {
        name: server.name.clone(),
        description: None,
        connection: match &server.connection {
            DeepLinkMcpConnection::Stdio { command, args } => McpConnectionDraft::Stdio {
                command: command.clone(),
                arguments: args.clone(),
            },
            DeepLinkMcpConnection::Http { url } => McpConnectionDraft::Http { url: url.clone() },
            DeepLinkMcpConnection::Sse { url } => McpConnectionDraft::Sse { url: url.clone() },
        },
    }
}

/// The safe projection the renderer receives. It is built from the intent every
/// time it is asked for, so nothing derived from a credential can be cached.
fn project(pending: &PendingDeepLink) -> DeepLinkPreview {
    let intent = &pending.intent;
    let (name, endpoint, items) = match intent {
        DeepLinkIntent::Provider(provider) => (
            Some(provider.name.clone()),
            provider.endpoint.clone().or_else(|| {
                let tool = first(&supported_tools(intent)).ok()?;
                inspect_config(tool, provider.config.as_ref()?).base_url
            }),
            Vec::new(),
        ),
        DeepLinkIntent::Mcp(mcp) => (
            None,
            None,
            mcp.servers
                .iter()
                .map(|server| server.name.clone())
                .collect(),
        ),
        DeepLinkIntent::Prompt(prompt) => (Some(prompt.name.clone()), None, Vec::new()),
        DeepLinkIntent::Skill(skill) => (
            Some(skill.display_name()),
            None,
            vec![format!("{}/{}", skill.owner, skill.repository)],
        ),
    };

    DeepLinkPreview {
        id: pending.id.clone(),
        origin: pending.origin,
        resource: intent.resource(),
        name,
        endpoint,
        items,
        targets: targets(intent),
        credential_fields: intent.credential_fields(),
        blocked: block_reason(intent),
        expires_at: pending.expires_at,
    }
}

fn targets(intent: &DeepLinkIntent) -> Vec<DeepLinkTarget> {
    intent
        .app_tokens()
        .into_iter()
        .map(|token| {
            let tool = app_type_to_tool_id(token);
            DeepLinkTarget {
                tool,
                name: tool.map(|tool| display_name_for(tool).to_string()),
                supported: tool.is_some_and(|tool| can_manage(intent.resource(), tool)),
            }
        })
        .collect()
}

fn supported_tools(intent: &DeepLinkIntent) -> Vec<ToolId> {
    intent
        .app_tokens()
        .into_iter()
        .filter_map(app_type_to_tool_id)
        .filter(|tool| can_manage(intent.resource(), *tool))
        .collect()
}

/// Behaviour comes from the capability table, never from a tool name.
fn can_manage(resource: DeepLinkResource, tool: ToolId) -> bool {
    let ToolCapabilities {
        can_manage_provider,
        can_manage_mcp,
        can_manage_skills,
        can_manage_prompts,
        ..
    } = capabilities_for(tool);
    match resource {
        DeepLinkResource::Provider => can_manage_provider,
        DeepLinkResource::Mcp => can_manage_mcp,
        DeepLinkResource::Skill => can_manage_skills,
        DeepLinkResource::Prompt => can_manage_prompts,
    }
}

fn block_reason(intent: &DeepLinkIntent) -> Option<DeepLinkBlockReason> {
    let tools = supported_tools(intent);
    if tools.is_empty() {
        return Some(DeepLinkBlockReason::NoSupportedTool);
    }
    // A service is stored with its key; the product has no shape for a keyless
    // one. Saying so in the preview beats offering a button that can only fail.
    if let DeepLinkIntent::Provider(provider) = intent {
        let carries_key = intent
            .credential_fields()
            .contains(&DeepLinkCredentialField::ApiKey)
            || provider
                .config
                .as_ref()
                .and_then(|config| inspect_config(tools[0], config).api_key)
                .is_some();
        if !carries_key {
            return Some(DeepLinkBlockReason::CredentialRequired);
        }
    }
    None
}

fn first(tools: &[ToolId]) -> Result<ToolId, AppError> {
    tools
        .first()
        .copied()
        .ok_or_else(|| blocked(DeepLinkBlockReason::NoSupportedTool))
}

fn blocked(reason: DeepLinkBlockReason) -> AppError {
    match reason {
        DeepLinkBlockReason::NoSupportedTool => {
            AppError::new(ErrorCode::ToolNotFound, "error.deepLink.toolUnavailable")
                .with_technical("no managed application can accept this link")
                .with_remediation("error.remediation.retryOrViewDetails")
        }
        DeepLinkBlockReason::CredentialRequired => AppError::new(
            ErrorCode::ConfigWriteFailed,
            "error.deepLink.credentialRequired",
        )
        .with_technical("the link creates a service but carries no key")
        .with_remediation("error.remediation.checkServiceSettings"),
    }
}

fn invalid(message_key: &'static str, technical: &'static str) -> AppError {
    AppError::new(ErrorCode::ConfigParseFailed, message_key)
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

async fn blocking<T, F>(work: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            AppError::new(ErrorCode::Internal, "error.command.joinFailed")
                .with_technical(error.to_string())
                .with_remediation("error.remediation.retryOrViewDetails")
        })?
}

#[cfg(test)]
#[path = "deep_link_import/tests.rs"]
mod tests;
