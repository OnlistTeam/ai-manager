//! Resolve native live files only; never open the database or create files.
use super::resource_open_failed;
use crate::domain::{AppError, DesktopAppId, ExtensionKind, ExtensionScope, ToolId};
use std::path::PathBuf;

pub fn location_path(scope: ExtensionScope, kind: ExtensionKind) -> Result<PathBuf, AppError> {
    match (scope, kind) {
        (ExtensionScope::Tool { id }, ExtensionKind::Prompt) => {
            let app = crate::compat::ccswitch::tools::tool_id_to_app_type(id)
                .ok_or_else(|| resource_open_failed("tool has no native prompt file"))?
                .parse::<crate::app_config::AppType>()
                .map_err(resource_open_failed)?;
            crate::prompt_files::prompt_file_path(&app).map_err(resource_open_failed)
        }
        (ExtensionScope::Tool { id }, ExtensionKind::Mcp) => Ok(match id {
            ToolId::ClaudeCode => crate::config::get_claude_mcp_path(),
            ToolId::Codex => crate::codex_config::get_codex_config_path(),
            ToolId::OpenCode => crate::opencode_config::get_opencode_config_path(),
            ToolId::GeminiCli => crate::gemini_config::get_gemini_settings_path(),
            ToolId::GrokBuild => crate::grok_config::get_grok_config_path(),
            ToolId::Hermes => crate::hermes_config::get_hermes_config_path(),
            _ => return Err(resource_open_failed("tool has no native MCP file")),
        }),
        (
            ExtensionScope::DesktopApp {
                id: DesktopAppId::ClaudeDesktop,
            },
            ExtensionKind::Mcp,
        ) => crate::platform::claude_desktop_mcp_config_path()
            .ok_or_else(|| resource_open_failed("desktop MCP file unavailable on this platform")),
        _ => Err(resource_open_failed(
            "scope has no shared native extension file",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::ccswitch::tools::capabilities_for;

    #[test]
    #[serial_test::serial]
    fn every_supported_cli_resolves_a_live_file_without_creating_it() {
        struct RestoreHome(Option<std::ffi::OsString>);
        impl Drop for RestoreHome {
            fn drop(&mut self) {
                match &self.0 {
                    Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
                    None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
                }
            }
        }
        let home = tempfile::TempDir::new().expect("temporary home");
        let _restore = RestoreHome(std::env::var_os("AI_MANAGER_TEST_HOME"));
        std::env::set_var("AI_MANAGER_TEST_HOME", home.path());
        for tool in ToolId::ALL {
            let capabilities = capabilities_for(tool);
            for (kind, supported) in [
                (ExtensionKind::Mcp, capabilities.can_manage_mcp),
                (ExtensionKind::Prompt, capabilities.can_manage_prompts),
            ] {
                let path = location_path(ExtensionScope::tool(tool), kind);
                assert_eq!(path.is_ok(), supported, "{tool:?} {kind:?}");
                if let Ok(path) = path {
                    assert!(path.is_absolute());
                    assert!(path.file_name().is_some());
                }
            }
        }
        assert_eq!(
            std::fs::read_dir(home.path())
                .expect("read temporary home")
                .count(),
            0
        );
    }

    #[test]
    #[serial_test::serial]
    fn mcp_points_at_the_native_file_not_a_synthetic_mcp_directory() {
        for (tool, path) in [
            (ToolId::ClaudeCode, crate::config::get_claude_mcp_path()),
            (ToolId::Codex, crate::codex_config::get_codex_config_path()),
            (
                ToolId::OpenCode,
                crate::opencode_config::get_opencode_config_path(),
            ),
            (
                ToolId::GeminiCli,
                crate::gemini_config::get_gemini_settings_path(),
            ),
            (
                ToolId::GrokBuild,
                crate::grok_config::get_grok_config_path(),
            ),
            (
                ToolId::Hermes,
                crate::hermes_config::get_hermes_config_path(),
            ),
        ] {
            assert_eq!(
                location_path(ExtensionScope::tool(tool), ExtensionKind::Mcp)
                    .expect("native MCP file"),
                path
            );
        }
    }

    #[test]
    fn unsupported_scopes_fail_closed() {
        assert!(location_path(
            ExtensionScope::tool(ToolId::ClaudeCode),
            ExtensionKind::Skill
        )
        .is_err());
        assert!(location_path(
            ExtensionScope::tool(ToolId::KimiCode),
            ExtensionKind::Prompt
        )
        .is_err());
        assert!(location_path(
            ExtensionScope::DesktopApp {
                id: DesktopAppId::ClaudeDesktop
            },
            ExtensionKind::Prompt
        )
        .is_err());
    }
}
