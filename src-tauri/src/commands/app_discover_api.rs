//! Product commands for the Discover section of the Skills and MCP pages
//! (ADR-0063). Thin: parse, then hand over to `application::discover`.

use std::collections::BTreeMap;
use std::sync::Arc;

use tauri::State;

use crate::application::discover::{self, DiscoverService};
use crate::domain::{
    AppError, DiscoverInputValue, DiscoverLink, DiscoverMcpList, DiscoverSkillList, ErrorCode,
    ExtensionKind, ExtensionScope, OperationId,
};
use crate::infrastructure::OperationManager;

use super::app_api::{blocking, parse_tool};

#[tauri::command]
pub async fn app_discover_mcp_list(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<DiscoverMcpList, AppError> {
    discover::list_mcp(app_handle, query).await
}

#[tauri::command]
pub async fn app_discover_skill_list(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<DiscoverSkillList, AppError> {
    discover::list_skills(app_handle, query).await
}

#[tauri::command]
pub async fn app_discover_skill_descriptions(
    ids: Vec<String>,
) -> Result<BTreeMap<String, String>, AppError> {
    Ok(DiscoverService::system().descriptions(ids).await)
}

#[tauri::command]
pub async fn app_discover_icon(url: String) -> Result<String, AppError> {
    DiscoverService::system().icon(&url).await
}

#[tauri::command]
pub async fn app_discover_mcp_install(
    app_handle: tauri::AppHandle,
    server: String,
    values: Vec<DiscoverInputValue>,
    description: Option<String>,
    scopes: Vec<ExtensionScope>,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<OperationId, AppError> {
    discover::install_mcp(
        app_handle,
        operations.inner().clone(),
        server,
        values,
        description,
        scopes,
    )
    .await
}

#[tauri::command]
pub async fn app_discover_skill_install(
    app_handle: tauri::AppHandle,
    skill: String,
    tools: Vec<String>,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<OperationId, AppError> {
    let tools = tools
        .iter()
        .map(|tool| parse_tool(tool))
        .collect::<Result<Vec<_>, _>>()?;
    discover::install_skill(app_handle, operations.inner().clone(), skill, tools).await
}

#[tauri::command]
pub async fn app_discover_link_open(
    app_handle: tauri::AppHandle,
    kind: String,
    id: String,
    link: DiscoverLink,
) -> Result<(), AppError> {
    let kind = ExtensionKind::from_str_id(&kind).ok_or_else(|| {
        AppError::new(ErrorCode::ExtensionNotFound, "error.extension.unknownKind")
            .with_technical(kind.clone())
    })?;
    blocking(move || discover::open_link(&app_handle, kind, &id, link)).await
}
