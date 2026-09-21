//! The tool list the product shows, with configuration ownership resolved.
//!
//! The adapter registry answers one question: is this command line installed
//! on this machine? That is not the same question as: does this tool have a
//! configuration worth managing. A desktop application can own the same
//! configuration directory as a command line — the Codex application and the
//! Codex CLI both read `~/.codex` — so a machine with the application and
//! without the CLI has services, MCP servers and prompts that are in use by
//! something the user can see running.
//!
//! Detection is anchored to the *application's* installed state, never to the
//! presence of its configuration directory. A directory outlives an uninstall,
//! and a tool listed on the strength of leftover files is a tool the product
//! cannot actually act on.

use crate::adapters::registry::AdapterRegistry;
use crate::application::desktop_app_directory::DesktopAppDirectory;
use crate::domain::{
    AppError, DesktopApp, DesktopAppConfigurationRelationship, DesktopAppStatus, Tool,
};

/// The local-only list used for the first paint.
pub async fn list_local() -> Result<Vec<Tool>, AppError> {
    let tools = AdapterRegistry::detect_all_local().await?;
    Ok(with_configuration_owners(tools).await)
}

/// The same list once the latest versions have been looked up over the network.
pub async fn list_with_latest_versions() -> Result<Vec<Tool>, AppError> {
    let tools = AdapterRegistry::detect_all().await?;
    Ok(with_configuration_owners(tools).await)
}

async fn with_configuration_owners(tools: Vec<Tool>) -> Vec<Tool> {
    // A failed desktop inventory must not cost the user their tool list. The
    // worst case is the list this function was given, which is what shipped
    // before any of this existed.
    let apps = match DesktopAppDirectory::system().list().await {
        Ok(apps) => apps,
        Err(error) => {
            log::warn!("desktop inventory unavailable; listing tools without it: {error}");
            return tools;
        }
    };
    annotate(tools, &apps)
}

fn annotate(tools: Vec<Tool>, apps: &[DesktopApp]) -> Vec<Tool> {
    tools
        .into_iter()
        .map(|mut tool| {
            tool.configuration_shared_with = apps
                .iter()
                .find(|app| {
                    app.related_tool == Some(tool.id)
                        && app.configuration_relationship
                            == DesktopAppConfigurationRelationship::SharedConfiguration
                        && matches!(
                            app.status,
                            DesktopAppStatus::Installed | DesktopAppStatus::UpdateAvailable
                        )
                })
                .map(|app| app.id);
            tool
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::annotate;
    use crate::domain::{
        DesktopApp, DesktopAppConfigurationRelationship, DesktopAppId, DesktopAppInstallerHandoff,
        DesktopAppStatus, DesktopAppUninstallHandoff, Tool, ToolCapabilities, ToolDiscovery,
        ToolId, ToolStatus,
    };

    fn tool(id: ToolId, status: ToolStatus) -> Tool {
        Tool {
            id,
            name: id.as_str().to_string(),
            description_key: String::new(),
            discovery: ToolDiscovery::for_tool(id),
            status,
            version: None,
            latest_version: None,
            capabilities: ToolCapabilities::default(),
            sessions_inside_settings: false,
            environment: None,
            configuration_shared_with: None,
        }
    }

    fn app(
        id: DesktopAppId,
        related: Option<ToolId>,
        relationship: DesktopAppConfigurationRelationship,
        status: DesktopAppStatus,
    ) -> DesktopApp {
        DesktopApp {
            id,
            name: String::new(),
            can_launch: false,
            can_manage_mcp: false,
            status,
            version: None,
            latest_version: None,
            related_tool: related,
            configuration_relationship: relationship,
            environment: "windows".to_string(),
            installer_handoff: DesktopAppInstallerHandoff::Unsupported,
            uninstall_handoff: DesktopAppUninstallHandoff::Unsupported,
            updates_managed_by_vendor: true,
            can_rollback: false,
        }
    }

    #[test]
    fn an_installed_application_vouches_for_the_configuration_of_its_command_line() {
        let annotated = annotate(
            vec![tool(ToolId::Codex, ToolStatus::NotInstalled)],
            &[app(
                DesktopAppId::CodexApp,
                Some(ToolId::Codex),
                DesktopAppConfigurationRelationship::SharedConfiguration,
                DesktopAppStatus::Installed,
            )],
        );

        assert_eq!(
            annotated[0].configuration_shared_with,
            Some(DesktopAppId::CodexApp)
        );
        assert_eq!(
            annotated[0].status,
            ToolStatus::NotInstalled,
            "the command line really is absent and the list must keep saying so"
        );
    }

    #[test]
    fn an_uninstalled_application_vouches_for_nothing() {
        // The configuration directory outlives the uninstall. Anchoring to the
        // application's state is what keeps a removed app from leaving the
        // product claiming a tool nobody has.
        for status in [DesktopAppStatus::NotInstalled, DesktopAppStatus::Unknown] {
            let annotated = annotate(
                vec![tool(ToolId::Codex, ToolStatus::NotInstalled)],
                &[app(
                    DesktopAppId::CodexApp,
                    Some(ToolId::Codex),
                    DesktopAppConfigurationRelationship::SharedConfiguration,
                    status,
                )],
            );
            assert_eq!(annotated[0].configuration_shared_with, None, "{status:?}");
        }
    }

    #[test]
    fn an_application_with_its_own_configuration_vouches_for_nothing() {
        // Claude Desktop is installed and related to Claude Code, but it keeps
        // a separate configuration, so it says nothing about the CLI's.
        let annotated = annotate(
            vec![tool(ToolId::ClaudeCode, ToolStatus::NotInstalled)],
            &[app(
                DesktopAppId::ClaudeDesktop,
                Some(ToolId::ClaudeCode),
                DesktopAppConfigurationRelationship::SeparateConfiguration,
                DesktopAppStatus::Installed,
            )],
        );

        assert_eq!(annotated[0].configuration_shared_with, None);
    }

    #[test]
    fn an_unrelated_application_never_attaches_itself_to_a_tool() {
        let annotated = annotate(
            vec![tool(ToolId::Codex, ToolStatus::Installed)],
            &[app(
                DesktopAppId::CherryStudio,
                None,
                DesktopAppConfigurationRelationship::StandaloneApplication,
                DesktopAppStatus::Installed,
            )],
        );

        assert_eq!(annotated[0].configuration_shared_with, None);
    }
}
