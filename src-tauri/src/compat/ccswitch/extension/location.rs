//! Resolve native live files only; never open the database or create files.
use super::resource_open_failed;
use crate::domain::{
    AppError, DesktopAppId, ExtensionKind, ExtensionLocation, ExtensionScope, ToolId,
};
use std::path::{Path, PathBuf};

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

/// The same path, in the form a person reads.
///
/// The home directory is abbreviated so the line stays short and does not put
/// the account name on screen; a file outside home keeps its full path,
/// because there the location is the whole point.
fn display_path(path: &Path) -> String {
    let home = crate::config::get_home_dir();
    match path.strip_prefix(&home) {
        Ok(relative) => format!("~/{}", relative.to_string_lossy()),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// Which file this scope's MCP servers or instructions are written in.
///
/// `None` for Skills: each Skill is its own directory, so there is no one
/// shared file to name, and claiming otherwise would point the reader at a
/// file that does not decide anything.
pub fn describe_location(
    scope: ExtensionScope,
    kind: ExtensionKind,
) -> Result<Option<ExtensionLocation>, AppError> {
    if kind == ExtensionKind::Skill {
        return Ok(None);
    }
    match location_path(scope, kind) {
        Ok(path) => Ok(Some(ExtensionLocation {
            kind,
            path: display_path(&path),
            // A tool that has never written its config yet is the normal
            // first-run state, not a failure, so this is reported rather than
            // turned into an error.
            exists: path.is_file(),
        })),
        // A scope with no shared file is simply not described.
        Err(_) => Ok(None),
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

    /// Skills keep one directory per entry, so naming a single shared file
    /// would point the reader at something that decides nothing.
    #[test]
    #[serial_test::serial]
    fn skills_have_no_single_file_to_name() {
        assert_eq!(
            describe_location(
                ExtensionScope::tool(ToolId::ClaudeCode),
                ExtensionKind::Skill
            )
            .expect("a describable scope"),
            None
        );
    }

    /// The line exists so someone can check which file is about to change, so
    /// it has to be the file itself and readable as a path, not an account
    /// name followed by an absolute path.
    #[test]
    #[serial_test::serial]
    fn a_shared_file_is_described_by_a_home_relative_path() {
        let location =
            describe_location(ExtensionScope::tool(ToolId::ClaudeCode), ExtensionKind::Mcp)
                .expect("a describable scope")
                .expect("Claude Code keeps its MCP servers in one file");
        assert_eq!(location.kind, ExtensionKind::Mcp);
        assert!(
            location.path.starts_with("~/"),
            "a path under home is shown relative to it: {}",
            location.path
        );
        assert!(!location.path.contains(
            crate::config::get_home_dir()
                .to_string_lossy()
                .trim_end_matches('/')
        ));
    }

    /// A tool that has never written its config is the normal first-run state.
    /// Reporting it as absent lets the UI say so; erroring would make an empty
    /// machine look broken.
    #[test]
    #[serial_test::serial]
    fn a_file_the_tool_has_not_written_yet_is_absent_rather_than_an_error() {
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

        let location =
            describe_location(ExtensionScope::tool(ToolId::ClaudeCode), ExtensionKind::Mcp)
                .expect("a describable scope")
                .expect("the path is known even before the file exists");
        assert!(!location.exists);
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
