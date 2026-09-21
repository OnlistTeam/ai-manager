//! Editing a connection variable a shell profile sets (ADR-0042).
//!
//! The capability gate is the same one the services page uses, and the write
//! path adds nothing to what the compatibility layer already refuses: a
//! variable the tool does not read, a line that moved on, a value that is not a
//! plain literal, and a file outside the user's home are all rejected there.
//!
//! What it logs is the variable name and the line number. Not the path, not the
//! old value, not the new one.

use crate::compat::ccswitch::provider_runtime;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{
    AppError, ErrorCode, ShellVariableLocation, ShellVariableUpdate, ShellVariableWritten, ToolId,
};

fn gate(tool: ToolId) -> Result<(), AppError> {
    if !capabilities_for(tool).can_manage_provider {
        return Err(AppError::new(
            ErrorCode::ProviderNotFound,
            "error.provider.unsupportedTool",
        )
        .with_technical(format!("{} cannot manage services", tool.as_str())));
    }
    Ok(())
}

pub struct ShellVariableService;

impl ShellVariableService {
    /// The start-up lines behind this tool's connection, if any can be pinned
    /// down. An empty list is the ordinary answer for a tool configured through
    /// its own files rather than the shell.
    pub fn locate(tool: ToolId) -> Result<Vec<ShellVariableLocation>, AppError> {
        gate(tool)?;
        let located = provider_runtime::shell_variable_locations(tool);
        log::info!(
            "located {} shell variable(s) for {}: {:?}",
            located.len(),
            tool.as_str(),
            located
                .iter()
                .map(|entry| (entry.variable.as_str(), entry.line, entry.editable))
                .collect::<Vec<_>>()
        );
        Ok(located)
    }

    /// Replaces one value on one line. The caller must pass the value it last
    /// saw, so a line that changed in the meantime is refused rather than
    /// overwritten.
    pub fn write(
        tool: ToolId,
        update: ShellVariableUpdate,
    ) -> Result<ShellVariableWritten, AppError> {
        gate(tool)?;
        let written = provider_runtime::write_shell_variable(tool, &update)?;
        log::info!(
            "rewrote {} for {} at line {}",
            update.variable,
            tool.as_str(),
            written.line
        );
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_that_cannot_manage_services_is_refused_before_any_read() {
        for tool in [ToolId::KimiCode, ToolId::DeepSeekDsh] {
            let error = gate(tool).expect_err("refuse an unsupported tool");
            assert_eq!(error.message_key, "error.provider.unsupportedTool");
        }
    }

    #[test]
    fn the_tools_that_read_connection_variables_pass_the_gate() {
        for tool in [ToolId::ClaudeCode, ToolId::Codex, ToolId::GeminiCli] {
            assert!(gate(tool).is_ok(), "{tool:?}");
        }
    }
}
