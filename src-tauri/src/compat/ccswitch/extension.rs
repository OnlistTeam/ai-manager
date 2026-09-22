//! Compatibility layer for CC Switch's three engines: MCP / skills / prompts (ADR-0003 /
//! spec §91).
//!
//! This is the **only** place that knows about `AppState` / `AppType` / the upstream
//! `McpServer` / `InstalledSkill` / `Prompt` / the upstream error types. The signatures the
//! layers above see contain only `crate::domain` types.
//!
//! Upstream files are unmodified by this layer: `mod app_config;` / `mod prompt;` /
//! `mod services;` / `mod store;` / `mod database;` are modules private to the crate root,
//! and this module is a descendant of the crate root, so they are visible naturally (the same
//! reasoning as the Phase 4 provider facade).
//!
//! The three extension kinds are implemented under `extension/`, split by kind: the read
//! model is unified, the write paths are not — the three upstream enable/disable interfaces
//! do not even share an error type. All message_key literals are concentrated in this file
//! and the submodules only call the constructors here.

mod location;
mod mcp;
pub use location::{describe_location, location_path};
mod prompt;
mod skill;

use std::path::PathBuf;

use crate::compat::ccswitch::provider::app_type_for;
use crate::domain::{
    AppError, DesktopAppId, ErrorCode, Extension, ExtensionKind, ExtensionManagement,
    ExtensionScope, McpInstallDraft, PromptDetail, PromptDraft, ToolId,
};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::store::AppState;

/// Upstream error strings may be bare localized text and may carry file paths (the skills
/// side is an `anyhow::Error`). Never hand them over as is: redact and truncate first, and
/// only put them into `technical_message` (View Details in §42). They are funnelled through
/// `Display` because the three upstream services have mutually different error types
/// (decision 4).
pub(super) fn detail<E: std::fmt::Display>(error: E) -> String {
    truncate_tail(&redact_secrets(&error.to_string()), 20, 2000)
}

/// A blank description is treated as "no description": an empty line of text on a card looks
/// worse than no line at all, and it would also distort the Empty state detection in §97.
pub(super) fn non_empty(value: Option<String>) -> Option<String> {
    let trimmed = value?.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(super) fn list_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::UpstreamError, "error.extension.listFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

pub(super) fn toggle_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.extension.toggleFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

pub(super) fn resource_open_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(
        ErrorCode::LaunchFailed,
        "error.extension.resourceOpenFailed",
    )
    .with_technical(detail(error))
    .with_remediation("error.remediation.checkLocalFileAccess")
}

pub(super) fn copy_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.extension.copyFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

pub(super) fn adopt_failed<E: std::fmt::Display>(error: E) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.extension.adoptFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.retryOrViewDetails")
}

pub(super) fn reject_adopt_unsupported(kind: ExtensionKind) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.extension.adoptUnsupported",
    )
    .with_technical(format!(
        "detected adoption is unavailable for {} extensions",
        kind.as_str()
    ))
}

/// The upstream `McpService::toggle_app` succeeds silently for a non-existent id
/// (services/mcp.rs:78), so "does this entry still exist" must be answered here first,
/// otherwise the user sees a fake success.
pub(super) fn not_found(scope: ExtensionScope, kind: ExtensionKind, id: &str) -> AppError {
    AppError::new(ErrorCode::ExtensionNotFound, "error.extension.notFound").with_technical(format!(
        "{}/{}/{id}",
        scope.stable_key(),
        kind.as_str()
    ))
}

/// "Turn off" does not apply to prompts: turning off the last one would write the tool's
/// instruction file as an empty file (services/prompt.rs:82-93). The UI never renders that
/// control thanks to `Extension::can_disable`; this is the same rule on the backend side, so
/// the command cannot be called directly to get around it.
pub(super) fn reject_disable() -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.extension.cannotDisable",
    )
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn reject_detected_write(scope: ExtensionScope, kind: ExtensionKind, id: &str) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.extension.detectedReadOnly",
    )
    .with_technical(format!(
        "refusing to modify detected-only {}/{}/{}",
        scope.stable_key(),
        kind.as_str(),
        id
    ))
    .with_remediation("error.remediation.retryOrViewDetails")
}

/// Handle to the upstream extension store. The fields are private, so the layers above can never reach `AppState`.
pub struct ExtensionStore {
    state: AppState,
}

impl ExtensionStore {
    pub fn open(app_handle: &tauri::AppHandle) -> Result<Self, AppError> {
        let state = tauri::Manager::try_state::<AppState>(app_handle).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.extension.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
        Ok(Self {
            state: state.inner().clone(),
        })
    }

    /// "This kind of extension for this tool". All three kinds return exactly the same shape,
    /// so callers (the application layer and the whole frontend) never need to know the
    /// differences between kinds.
    pub fn list(&self, tool: ToolId, kind: ExtensionKind) -> Result<Vec<Extension>, AppError> {
        self.list_scope(ExtensionScope::tool(tool), kind)
    }

    pub fn list_scope(
        &self,
        scope: ExtensionScope,
        kind: ExtensionKind,
    ) -> Result<Vec<Extension>, AppError> {
        let app_type = app_type_for_scope(scope)?;
        match kind {
            ExtensionKind::Skill => {
                let tool = require_tool_scope(scope, kind)?;
                skill::list(&self.state, tool, &app_type)
            }
            ExtensionKind::Mcp => mcp::list(&self.state, scope, &app_type),
            ExtensionKind::Prompt => {
                let tool = require_tool_scope(scope, kind)?;
                prompt::list(&self.state, tool, &app_type)
            }
        }
    }

    /// Re-resolve a detected Skill from the current native inventory. The
    /// renderer provides a stable Skill id only; no path crosses IPC.
    pub fn detected_skill_path(
        &self,
        scope: ExtensionScope,
        id: &str,
    ) -> Result<PathBuf, AppError> {
        let app_type = app_type_for_scope(scope)?;
        require_tool_scope(scope, ExtensionKind::Skill)?;
        skill::detected_path(&self.state, scope, &app_type, id)
    }

    /// Enable / disable one extension and return the refreshed full list.
    ///
    /// All three write paths have cross-entry side effects (switching to one prompt turns off
    /// every other prompt of the same tool; toggling MCP / skills changes that tool's live
    /// config), so returning only `Ok(())` would force the frontend to guess the new state.
    /// Returning the authoritative list is the cheapest option.
    pub fn set_enabled(
        &self,
        tool: ToolId,
        kind: ExtensionKind,
        id: &str,
        enabled: bool,
    ) -> Result<Vec<Extension>, AppError> {
        self.set_enabled_scope(ExtensionScope::tool(tool), kind, id, enabled)
    }

    pub fn set_enabled_scope(
        &self,
        scope: ExtensionScope,
        kind: ExtensionKind,
        id: &str,
        enabled: bool,
    ) -> Result<Vec<Extension>, AppError> {
        let current = self.list_scope(scope, kind)?;
        let target = current
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| not_found(scope, kind, id))?;
        if target.management == ExtensionManagement::Detected {
            return Err(reject_detected_write(scope, kind, id));
        }

        let app_type = app_type_for_scope(scope)?;
        match kind {
            ExtensionKind::Skill => {
                require_tool_scope(scope, kind)?;
                skill::set_enabled(&self.state, &app_type, id, enabled)?;
            }
            ExtensionKind::Mcp => mcp::set_enabled(&self.state, &app_type, id, enabled)?,
            ExtensionKind::Prompt => {
                let tool = require_tool_scope(scope, kind)?;
                prompt::set_enabled(&self.state, tool, &app_type, id, enabled)?;
            }
        }

        self.list_scope(scope, kind)
    }

    /// Bring every detected item in one visible tool/kind scope under product
    /// management, then return the authoritative refreshed inventory. The
    /// upstream import paths copy/read existing state but do not rewrite the
    /// source tool config during adoption.
    pub fn adopt_detected(
        &self,
        tool: ToolId,
        kind: ExtensionKind,
    ) -> Result<Vec<Extension>, AppError> {
        self.adopt_detected_scope(ExtensionScope::tool(tool), kind)
    }

    pub fn adopt_detected_scope(
        &self,
        scope: ExtensionScope,
        kind: ExtensionKind,
    ) -> Result<Vec<Extension>, AppError> {
        let app_type = app_type_for_scope(scope)?;
        match kind {
            ExtensionKind::Skill => {
                require_tool_scope(scope, kind)?;
                skill::adopt_detected(&self.state, &app_type)?;
            }
            ExtensionKind::Mcp => mcp::adopt_detected(&self.state, &app_type)?,
            ExtensionKind::Prompt => return Err(reject_adopt_unsupported(kind)),
        }

        self.list_scope(scope, kind)
    }

    /// Copy one detected Skill into another tool's Skills directory and return
    /// that tool's authoritative inventory. The copy is a real, independent
    /// folder: the original stays where the user put it, and neither side
    /// tracks the other afterwards.
    pub fn copy_detected_skill(
        &self,
        source: ExtensionScope,
        target: ToolId,
        id: &str,
    ) -> Result<Vec<Extension>, AppError> {
        let source_app = app_type_for_scope(source)?;
        require_tool_scope(source, ExtensionKind::Skill)?;
        let target_app = app_type_for(target);
        skill::copy_detected_to(&self.state, source, &source_app, &target_app, id)?;
        self.list_scope(ExtensionScope::tool(target), ExtensionKind::Skill)
    }

    /// Install one product-shaped MCP draft for a single tool. The caller owns
    /// capability gating and operation locking; this compatibility boundary
    /// owns upstream format synthesis, rollback, and authoritative verification.
    pub fn install_mcp(
        &self,
        tool: ToolId,
        id: &str,
        draft: &McpInstallDraft,
    ) -> Result<Extension, AppError> {
        self.install_mcp_in_scope(ExtensionScope::tool(tool), id, draft)
    }

    pub fn install_mcp_in_scope(
        &self,
        scope: ExtensionScope,
        id: &str,
        draft: &McpInstallDraft,
    ) -> Result<Extension, AppError> {
        let app_type = app_type_for_scope(scope)?;
        mcp::install(&self.state, scope, &app_type, id, draft)
    }

    /// Resolve a destructive MCP target from the authoritative global row set.
    /// The product renderer supplies only an id and never chooses the task name.
    pub fn installed_mcp(&self, tool: ToolId, id: &str) -> Result<Extension, AppError> {
        self.installed_mcp_in_scope(ExtensionScope::tool(tool), id)
    }

    pub fn installed_mcp_in_scope(
        &self,
        scope: ExtensionScope,
        id: &str,
    ) -> Result<Extension, AppError> {
        let app_type = app_type_for_scope(scope)?;
        mcp::installed(&self.state, scope, &app_type, id)
    }

    /// Remove one global MCP row and every enabled live projection. The
    /// compatibility layer owns rollback and post-write verification.
    pub fn remove_mcp(&self, tool: ToolId, id: &str) -> Result<(), AppError> {
        self.remove_mcp_in_scope(ExtensionScope::tool(tool), id)
    }

    pub fn remove_mcp_in_scope(&self, scope: ExtensionScope, id: &str) -> Result<(), AppError> {
        app_type_for_scope(scope)?;
        mcp::remove(&self.state, scope, id)
    }

    /// Read Prompt content only for the explicit edit-detail flow.
    pub fn prompt_detail(&self, tool: ToolId, id: &str) -> Result<PromptDetail, AppError> {
        let app_type = app_type_for(tool);
        prompt::get(&self.state, tool, &app_type, id)
    }

    /// Create or edit one Prompt and return the authoritative small inventory.
    pub fn save_prompt(
        &self,
        tool: ToolId,
        id: Option<&str>,
        draft: &PromptDraft,
    ) -> Result<Vec<Extension>, AppError> {
        let app_type = app_type_for(tool);
        prompt::save(&self.state, tool, &app_type, id, draft)
    }

    /// Remove one inactive Prompt. Active instructions must be switched first.
    pub fn remove_prompt(&self, tool: ToolId, id: &str) -> Result<Vec<Extension>, AppError> {
        let app_type = app_type_for(tool);
        prompt::remove(&self.state, tool, &app_type, id)
    }

    /// Import the tool's current instruction file as an inactive managed item.
    pub fn import_current_prompt(&self, tool: ToolId) -> Result<Vec<Extension>, AppError> {
        let app_type = app_type_for(tool);
        prompt::import_current(&self.state, tool, &app_type)
    }
}

fn app_type_for_scope(scope: ExtensionScope) -> Result<crate::app_config::AppType, AppError> {
    match scope {
        ExtensionScope::Tool { id } => Ok(app_type_for(id)),
        ExtensionScope::DesktopApp {
            id: DesktopAppId::ClaudeDesktop,
        } => Ok(crate::app_config::AppType::ClaudeDesktop),
        ExtensionScope::DesktopApp { id } => Err(AppError::new(
            ErrorCode::McpUnavailable,
            "error.mcp.unsupportedScope",
        )
        .with_technical(format!("{} cannot manage MCP connections", id.as_str()))),
    }
}

fn require_tool_scope(scope: ExtensionScope, kind: ExtensionKind) -> Result<ToolId, AppError> {
    scope.tool_id().ok_or_else(|| {
        AppError::new(
            ErrorCode::ExtensionNotFound,
            "error.extension.unsupportedScope",
        )
        .with_technical(format!(
            "{} cannot manage {} extensions",
            scope.stable_key(),
            kind.as_str()
        ))
    })
}

/// Shared projection for the Skill installation compatibility path. Keeping
/// this conversion here guarantees newly installed rows and normal list rows
/// use exactly the same product wire model.
pub(super) fn project_installed_skill(
    tool: ToolId,
    raw: &crate::app_config::InstalledSkill,
    app_type: &crate::app_config::AppType,
) -> Extension {
    skill::extension_from_skill(tool, raw, app_type)
}

// The production-code scanner in message_keys.rs stops at the first `#[cfg(test)]` in a file —
// the test module declarations must come after all production code that emits a message_key.
// Test file names start with `tests` so the scanner excludes them wholesale
// (message_keys.rs:101).
#[cfg(test)]
#[path = "extension/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "extension/tests_write.rs"]
mod tests_write;
