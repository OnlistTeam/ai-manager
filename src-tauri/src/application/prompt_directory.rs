//! Prompt Basic Management use cases (spec sections 36 and 82).

use crate::compat::ccswitch::extension::ExtensionStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{
    AppError, ErrorCode, Extension, ExtensionKind, PromptDetail, PromptDraft, ToolId,
};

use super::extension_directory::supports;

pub struct PromptDirectory;

impl PromptDirectory {
    pub fn detail(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<PromptDetail, AppError> {
        open(app_handle, tool)?.prompt_detail(tool, id)
    }

    pub fn save(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: Option<&str>,
        draft: &PromptDraft,
    ) -> Result<Vec<Extension>, AppError> {
        open(app_handle, tool)?.save_prompt(tool, id, draft)
    }

    pub fn remove(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<Vec<Extension>, AppError> {
        open(app_handle, tool)?.remove_prompt(tool, id)
    }

    pub fn import_current(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
    ) -> Result<Vec<Extension>, AppError> {
        open(app_handle, tool)?.import_current_prompt(tool)
    }
}

fn open(app_handle: &tauri::AppHandle, tool: ToolId) -> Result<ExtensionStore, AppError> {
    ensure_supported(tool)?;
    ExtensionStore::open(app_handle)
}

fn ensure_supported(tool: ToolId) -> Result<(), AppError> {
    if supports(ExtensionKind::Prompt, &capabilities_for(tool)) {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::ExtensionNotFound, "error.prompt.unsupportedTool")
            .with_technical(format!("{} cannot manage Prompts", tool.as_str())),
    )
}

#[cfg(test)]
mod tests {
    use super::ensure_supported;
    use crate::domain::{ErrorCode, ToolId};

    #[test]
    fn prompt_management_is_gated_by_the_product_capability_table() {
        for tool in [
            ToolId::ClaudeCode,
            ToolId::Codex,
            ToolId::OpenCode,
            ToolId::GeminiCli,
            ToolId::GrokBuild,
            ToolId::OpenClaw,
            ToolId::Hermes,
            ToolId::Pi,
        ] {
            assert!(ensure_supported(tool).is_ok(), "{tool:?}");
        }
        for tool in [ToolId::KimiCode, ToolId::DeepSeekDsh] {
            let error = ensure_supported(tool).expect_err("Prompt capability is off");
            assert_eq!(error.code, ErrorCode::ExtensionNotFound);
            assert_eq!(error.message_key, "error.prompt.unsupportedTool");
        }
    }
}
