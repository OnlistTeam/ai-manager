//! Edit one MCP connection in place (ADR-0062).
//!
//! The capability gate is the same as every other MCP action in the scope;
//! the compatibility layer owns the read, the merge into the stored spec, the
//! write to every app that has the connection on, and the rollback. Editing a
//! found connection is two steps the renderer already knows: take it over
//! (`app_extensions_adopt_detected`, as the first switch click does), then
//! save.

use crate::application::extension_directory::gate;
use crate::domain::{
    AppError, Extension, ExtensionKind, ExtensionScope, McpEditForm, McpInstallDraft,
};

pub struct McpEditing;

impl McpEditing {
    /// The saved connection, values included, to prefill the edit form. Read
    /// only when a person opens that form; never logged.
    pub fn form(
        app_handle: &tauri::AppHandle,
        scope: ExtensionScope,
        id: &str,
    ) -> Result<McpEditForm, AppError> {
        gate(app_handle, scope, ExtensionKind::Mcp)?.mcp_edit_form(scope, id)
    }

    /// Save the edited connection and return this scope's refreshed list.
    pub fn update(
        app_handle: &tauri::AppHandle,
        scope: ExtensionScope,
        id: &str,
        draft: &McpInstallDraft,
    ) -> Result<Vec<Extension>, AppError> {
        draft.validate()?;
        gate(app_handle, scope, ExtensionKind::Mcp)?.update_mcp(scope, id, draft)
    }
}
