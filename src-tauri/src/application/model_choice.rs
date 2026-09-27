//! Choosing the model and the thinking effort from Home (ADR-0055).
//!
//! The gate is the capability table, never a tool name: a tool Home cannot
//! choose a model for is refused before any file is read. What is logged is
//! the tool and whether a value was set, never a file's content.

use crate::compat::ccswitch::provider::ProviderStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{AppError, ErrorCode, ToolId, ToolModelChoice};

fn gate(tool: ToolId, effort: bool) -> Result<(), AppError> {
    let capabilities = capabilities_for(tool);
    let allowed = if effort {
        capabilities.can_choose_effort
    } else {
        capabilities.can_choose_model
    };
    if !allowed {
        return Err(AppError::new(
            ErrorCode::ProviderNotFound,
            "error.provider.unsupportedTool",
        )
        .with_technical(format!("{} has no model choice", tool.as_str())));
    }
    Ok(())
}

pub struct ModelChoiceService;

impl ModelChoiceService {
    pub fn read(app_handle: &tauri::AppHandle, tool: ToolId) -> Result<ToolModelChoice, AppError> {
        gate(tool, false)?;
        ProviderStore::open(app_handle)?.model_choice(tool)
    }

    pub fn set_model(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> Result<ToolModelChoice, AppError> {
        gate(tool, false)?;
        ProviderStore::open(app_handle)?.set_model(tool, provider, model)
    }

    pub fn set_effort(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        effort: Option<&str>,
    ) -> Result<ToolModelChoice, AppError> {
        gate(tool, true)?;
        ProviderStore::open(app_handle)?.set_effort(tool, effort)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_without_a_model_choice_is_refused_before_any_read() {
        for tool in [ToolId::OpenCode, ToolId::Pi, ToolId::KimiCode] {
            let error = gate(tool, false).expect_err("refuse");
            assert_eq!(error.message_key, "error.provider.unsupportedTool");
        }
        assert!(gate(ToolId::GeminiCli, false).is_ok());
        assert!(gate(ToolId::GeminiCli, true).is_err());
        assert!(gate(ToolId::ClaudeCode, true).is_ok());
    }
}
