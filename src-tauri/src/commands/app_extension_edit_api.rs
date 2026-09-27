//! Product commands for showing and editing a Skill or MCP row in place
//! (ADR-0062): the MCP edit form's read and save, and the folder actions for
//! a managed Skill. The renderer sends ids and actions only, never a path.

use crate::application::extension_directory::ExtensionDirectory;
use crate::application::mcp_editing::McpEditing;
use crate::domain::{
    AppError, DetectedSkillResourceAction, DetectedSkillResourceOpenOutcome, Extension,
    ExtensionScope, McpEditForm, McpInstallDraft,
};

use super::app_api::blocking;

/// The saved connection, values included, to prefill the edit form.
#[tauri::command]
pub async fn app_mcp_get(
    app_handle: tauri::AppHandle,
    scope: ExtensionScope,
    mcp: String,
) -> Result<McpEditForm, AppError> {
    blocking(move || McpEditing::form(&app_handle, scope, &mcp)).await
}

/// Save an edited connection under the same id, keeping every app's switch,
/// and return this scope's refreshed list.
#[tauri::command]
pub async fn app_mcp_update(
    app_handle: tauri::AppHandle,
    scope: ExtensionScope,
    mcp: String,
    draft: McpInstallDraft,
) -> Result<Vec<Extension>, AppError> {
    blocking(move || McpEditing::update(&app_handle, scope, &mcp, &draft)).await
}

/// Show a managed Skill's stored folder, or open its SKILL.md.
#[tauri::command]
pub async fn app_skill_resource_open(
    app_handle: tauri::AppHandle,
    skill: String,
    action: DetectedSkillResourceAction,
) -> Result<DetectedSkillResourceOpenOutcome, AppError> {
    blocking(move || ExtensionDirectory::open_managed_skill_resource(&app_handle, &skill, action))
        .await
}
