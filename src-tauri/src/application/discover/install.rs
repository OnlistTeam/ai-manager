//! Adding a Discover item, and opening its pages.
//!
//! An MCP server becomes an ordinary guided install draft (ADR-0047) and goes
//! through `McpInstallationService` for the first app; a Skill is found in its
//! repository and goes through `SkillInstallationService`. The same task then
//! switches the item on for every other app chosen, through the same write the
//! list's own switches use, so one task adds it everywhere.

use std::sync::Arc;

use tauri_plugin_opener::OpenerExt;

use super::catalog::MarketServer;
use super::{blocking, mcp_scopes, skills_sh, DiscoverService};
use crate::application::extension_directory::supports;
use crate::application::mcp_installation::{McpInstallationService, McpInstaller};
use crate::application::skill_installation::{
    SkillInstallRequest, SkillInstallationService, SkillInstaller,
};
use crate::compat::ccswitch::discover::{mcp_holdings, mcp_transports, resolve_skill};
use crate::compat::ccswitch::extension::ExtensionStore;
use crate::compat::ccswitch::skill_catalog::SkillCatalogStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{
    AppError, DiscoverInputTarget, DiscoverInputValue, DiscoverLink, DiscoverTransport, ErrorCode,
    Extension, ExtensionKind, ExtensionScope, McpConnectionDraft, McpInstallDraft,
    McpVariableDraft, OperationId, ToolId,
};
use crate::infrastructure::OperationManager;

const MAX_NAME_CHARS: usize = 80;
const MAX_DESCRIPTION_CHARS: usize = 240;

fn invalid(key: &'static str, technical: &'static str) -> AppError {
    AppError::new(ErrorCode::InstallFailed, key)
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

/// `~/code` is the user's folder: servers receive it as an argument, and not
/// every server expands a tilde itself.
fn expand_home(value: &str) -> String {
    let home = || dirs::home_dir().map(|home| home.to_string_lossy().into_owned());
    if value == "~" {
        return home().unwrap_or_else(|| value.to_string());
    }
    match (value.strip_prefix("~/"), home()) {
        (Some(rest), Some(home)) => format!("{}/{rest}", home.trim_end_matches('/')),
        _ => value.to_string(),
    }
}

fn clip(value: &str, max: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

/// The guided install draft for `server` with the values the user typed.
/// Errors name the rule that failed, never an input or its value.
pub(crate) fn draft_for(
    server: &MarketServer,
    values: &[DiscoverInputValue],
    description: Option<&str>,
) -> Result<McpInstallDraft, AppError> {
    for (index, value) in values.iter().enumerate() {
        let known = server
            .inputs
            .iter()
            .any(|input| input.view.key == value.key);
        let repeated = values[..index].iter().any(|other| other.key == value.key);
        if !known || repeated {
            return Err(invalid(
                "error.discover.inputInvalid",
                "a value was given for an input the server does not have",
            ));
        }
    }
    let spec = &server.spec;
    let mut env = Vec::new();
    let mut headers = spec
        .headers
        .iter()
        .map(|(name, value)| McpVariableDraft {
            name: name.clone(),
            value: value.clone(),
        })
        .collect::<Vec<_>>();
    let mut arguments = spec.args.clone();
    for input in &server.inputs {
        let value = values
            .iter()
            .find(|value| value.key == input.view.key)
            .map(|value| value.value.trim())
            .unwrap_or_default();
        if value.is_empty() {
            if input.view.required {
                return Err(invalid(
                    "error.discover.inputRequired",
                    "a required input is empty",
                ));
            }
            continue;
        }
        let name = input.view.key.clone();
        match input.view.target {
            DiscoverInputTarget::Env => env.push(McpVariableDraft {
                name,
                value: value.to_string(),
            }),
            DiscoverInputTarget::Header => headers.push(McpVariableDraft {
                name,
                value: input.format.replacen("{}", value, 1),
            }),
            DiscoverInputTarget::Argument => arguments.push(expand_home(value)),
        }
    }
    let connection = match spec.transport {
        DiscoverTransport::Stdio => McpConnectionDraft::Stdio {
            command: spec.command.clone().unwrap_or_default(),
            arguments,
            env,
        },
        DiscoverTransport::Http => McpConnectionDraft::Http {
            url: spec.url.clone().unwrap_or_default(),
            headers,
        },
        DiscoverTransport::Sse => McpConnectionDraft::Sse {
            url: spec.url.clone().unwrap_or_default(),
            headers,
        },
    };
    let description = if server.featured {
        description.map(str::to_string)
    } else {
        server.description.clone()
    }
    .map(|text| clip(&text, MAX_DESCRIPTION_CHARS))
    .filter(|text| !text.is_empty());
    Ok(McpInstallDraft {
        name: clip(&server.title, MAX_NAME_CHARS),
        description,
        connection,
    })
}

/// The apps asked for, in order, once each. Every one must take MCP servers
/// and be able to reach this one's transport.
pub(crate) fn mcp_targets(
    transport: DiscoverTransport,
    scopes: Vec<ExtensionScope>,
    reachable: &[ExtensionScope],
) -> Result<Vec<ExtensionScope>, AppError> {
    let mut chosen = Vec::new();
    for scope in scopes {
        if !chosen.contains(&scope) {
            chosen.push(scope);
        }
    }
    if chosen.is_empty() || chosen.iter().any(|scope| !reachable.contains(scope)) {
        return Err(invalid(
            "error.discover.targetsInvalid",
            "no app, or an app that cannot take MCP servers, was chosen",
        ));
    }
    if chosen
        .iter()
        .any(|scope| !mcp_transports(*scope).contains(&transport))
    {
        return Err(invalid(
            "error.discover.transportUnsupported",
            "a chosen app cannot reach this server's transport",
        ));
    }
    Ok(chosen)
}

pub(crate) fn skill_targets(tools: Vec<ToolId>) -> Result<Vec<ToolId>, AppError> {
    let mut chosen = Vec::new();
    for tool in tools {
        if !chosen.contains(&tool) {
            chosen.push(tool);
        }
    }
    if chosen.is_empty()
        || chosen
            .iter()
            .any(|tool| !supports(ExtensionKind::Skill, &capabilities_for(*tool)))
    {
        return Err(invalid(
            "error.discover.targetsInvalid",
            "no app, or an app that cannot take Skills, was chosen",
        ));
    }
    Ok(chosen)
}

/// Switches a just-added item on for the remaining apps. Every app is tried;
/// the task fails, naming the apps that missed out, when any write failed.
fn enable_rest(
    store: &ExtensionStore,
    kind: ExtensionKind,
    id: &str,
    rest: &[ExtensionScope],
) -> Result<(), AppError> {
    let failed = rest
        .iter()
        .filter(|scope| store.set_enabled_scope(**scope, kind, id, true).is_err())
        .map(|scope| scope.stable_key())
        .collect::<Vec<_>>();
    if failed.is_empty() {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::ConfigWriteFailed, "error.discover.enableFailed")
            .with_technical(format!(
                "added, but not switched on for {}",
                failed.join(", ")
            ))
            .with_remediation("error.remediation.retryOrViewDetails"),
    )
}

/// Adds a Discover MCP server for every app in `scopes`, as one task.
pub async fn install_mcp(
    app_handle: tauri::AppHandle,
    operations: Arc<OperationManager>,
    server: String,
    values: Vec<DiscoverInputValue>,
    description: Option<String>,
    scopes: Vec<ExtensionScope>,
) -> Result<OperationId, AppError> {
    let entry = DiscoverService::system().server(&server).ok_or_else(|| {
        AppError::new(ErrorCode::ExtensionNotFound, "error.discover.serverUnknown")
            .with_technical("the server is neither featured nor in this session's results")
            .with_remediation("error.remediation.retryOrViewDetails")
    })?;
    let scopes = mcp_targets(entry.spec.transport, scopes, &mcp_scopes())?;
    let draft = draft_for(&entry, &values, description.as_deref())?;

    let holdings_handle = app_handle.clone();
    let holdings = blocking(move || mcp_holdings(&holdings_handle)).await?;
    if super::matching::added_server(&entry.spec, &holdings).is_some() {
        return Err(
            AppError::new(ErrorCode::OperationConflict, "error.discover.alreadyAdded")
                .with_technical("a connection already runs this server"),
        );
    }

    let (first, rest) = scopes
        .split_first()
        .expect("mcp_targets rejects an empty list");
    let rest = rest.to_vec();
    let request = McpInstallationService::prepare(*first, draft)?;
    let installer: McpInstaller = Arc::new(move |scope, id, draft| {
        let app_handle = app_handle.clone();
        let rest = rest.clone();
        Box::pin(async move {
            blocking(move || {
                let store = ExtensionStore::open(&app_handle)?;
                let installed = store.install_mcp_in_scope(scope, &id, &draft)?;
                enable_rest(&store, ExtensionKind::Mcp, &installed.id, &rest)?;
                Ok(installed)
            })
            .await
        })
    });
    let service = McpInstallationService::new(operations, installer);
    let operation = service.begin(&request)?;
    let spawned = operation.clone();
    tokio::spawn(async move { service.run(spawned, request).await });
    Ok(operation)
}

/// Adds a skills.sh Skill (`owner/repo/skill`) for every tool in `tools`, as
/// one task. The repository is read first to find which folder holds it.
pub async fn install_skill(
    app_handle: tauri::AppHandle,
    operations: Arc<OperationManager>,
    skill: String,
    tools: Vec<ToolId>,
) -> Result<OperationId, AppError> {
    let Some((source, skill_id)) = skills_sh::split_id(&skill) else {
        return Err(invalid(
            "error.discover.skillInvalid",
            "the Skill id is not owner/repository/skill",
        ));
    };
    let (owner, repository) = source
        .split_once('/')
        .expect("split_id validated owner/repository");
    let tools = skill_targets(tools)?;
    let service = DiscoverService::system();
    let item = match service.resolved(&skill) {
        Some(item) => item,
        None => {
            let item = resolve_skill(owner, repository, skill_id).await?;
            service.remember_resolved(&skill, item.clone());
            item
        }
    };

    let (first, rest) = tools
        .split_first()
        .expect("skill_targets rejects an empty list");
    let rest = rest
        .iter()
        .map(|tool| ExtensionScope::tool(*tool))
        .collect::<Vec<_>>();
    let installer: SkillInstaller = Arc::new(move |tool, item| {
        let app_handle = app_handle.clone();
        let rest = rest.clone();
        Box::pin(async move {
            let installed: Extension = SkillCatalogStore::open(&app_handle)?
                .install(tool, &item)
                .await?;
            let id = installed.id.clone();
            blocking(move || {
                enable_rest(
                    &ExtensionStore::open(&app_handle)?,
                    ExtensionKind::Skill,
                    &id,
                    &rest,
                )
            })
            .await?;
            Ok(installed)
        })
    });
    let installation = SkillInstallationService::new(operations, installer);
    let request = SkillInstallRequest { tool: *first, item };
    let operation = installation.begin(&request)?;
    let spawned = operation.clone();
    tokio::spawn(async move { installation.run(spawned, request).await });
    Ok(operation)
}

/// The address of one of a Discover item's pages. Only addresses the native
/// side built or keeps are ever opened.
pub(crate) fn link_url(
    service: &DiscoverService,
    kind: ExtensionKind,
    id: &str,
    link: DiscoverLink,
) -> Option<String> {
    match (kind, link) {
        (ExtensionKind::Mcp, DiscoverLink::Homepage) => service
            .server(id)?
            .homepage
            .filter(|url| url.starts_with("https://")),
        (ExtensionKind::Skill, DiscoverLink::Page) => {
            let (source, skill_id) = skills_sh::split_id(id)?;
            Some(format!("https://skills.sh/{source}/{skill_id}"))
        }
        (ExtensionKind::Skill, DiscoverLink::Repository) => {
            let (source, _) = skills_sh::split_id(id)?;
            Some(format!("https://github.com/{source}"))
        }
        _ => None,
    }
}

pub fn open_link(
    app_handle: &tauri::AppHandle,
    kind: ExtensionKind,
    id: &str,
    link: DiscoverLink,
) -> Result<(), AppError> {
    let url = link_url(&DiscoverService::system(), kind, id, link).ok_or_else(|| {
        AppError::new(
            ErrorCode::ExtensionNotFound,
            "error.discover.linkUnavailable",
        )
        .with_technical("no page is known for this item")
    })?;
    app_handle
        .opener()
        .open_url(url, None::<String>)
        .map_err(|error| {
            AppError::new(ErrorCode::LaunchFailed, "error.discover.openLinkFailed")
                .with_technical(error.to_string())
                .with_remediation("error.remediation.retryOrViewDetails")
        })
}
