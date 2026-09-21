use futures::future::join_all;
use std::sync::Arc;

use crate::domain::{
    AppError, DesktopApp, DesktopAppConfigurationRelationship, DesktopAppId,
    DesktopAppInstallerHandoff, DesktopAppLaunchOutcome, DesktopAppStatus,
    DesktopAppUninstallHandoff, DesktopAppUninstallOutcome, ToolId,
};
use crate::platform::{DesktopAppPlatform, DesktopAppPlatformState, SystemDesktopAppPlatform};

/// Product use case for desktop application inventory and safe OS handoff.
/// It deliberately does not expose install/update actions: those remain owned by
/// each vendor's signed distribution channel.
pub struct DesktopAppDirectory {
    platform: Arc<dyn DesktopAppPlatform>,
}

impl DesktopAppDirectory {
    pub fn new(platform: Arc<dyn DesktopAppPlatform>) -> Self {
        Self { platform }
    }

    pub fn system() -> Self {
        Self::new(Arc::new(SystemDesktopAppPlatform::default()))
    }

    pub async fn list(&self) -> Result<Vec<DesktopApp>, AppError> {
        let inspections = DesktopAppId::ALL.into_iter().map(|id| {
            let platform = self.platform.clone();
            async move { (id, platform.inspect(id).await) }
        });

        // One probe failing says nothing about the other four. Collecting into
        // a single Result used to turn any of them into an error card that
        // replaced the whole list, and on Windows every probe runs PowerShell,
        // so a restricted execution policy or a slow Appx query hid apps that
        // are plainly installed. A failed probe now reports only itself as
        // unknown, which is a state the renderer already draws.
        Ok(join_all(inspections)
            .await
            .into_iter()
            .map(|(id, inspection)| match inspection {
                Ok(state) => project(id, state),
                Err(error) => {
                    log::warn!("desktop app probe failed for {id:?}: {error}");
                    project(id, unknown_state())
                }
            })
            .collect())
    }

    pub async fn launch(&self, id: DesktopAppId) -> Result<DesktopAppLaunchOutcome, AppError> {
        // The platform boundary re-inspects the fixed native identity immediately
        // before handoff, so a stale renderer card can never supply a path.
        self.platform.launch(id).await?;
        Ok(DesktopAppLaunchOutcome::Launched)
    }

    pub async fn open_uninstall_handoff(
        &self,
        id: DesktopAppId,
    ) -> Result<DesktopAppUninstallOutcome, AppError> {
        // The native platform re-inspects the fixed app identity. Renderer input
        // cannot choose a bundle path, package name, settings URI, or command.
        self.platform.open_uninstall_handoff(id).await?;
        Ok(DesktopAppUninstallOutcome::Opened)
    }
}

/// What a row looks like when its probe could not answer. It deliberately
/// offers no action: claiming the app is missing would be a guess, and
/// offering launch or uninstall would act on a state nobody established.
fn unknown_state() -> DesktopAppPlatformState {
    DesktopAppPlatformState {
        status: DesktopAppStatus::Unknown,
        version: None,
        latest_version: None,
        can_launch: false,
        environment: crate::platform::Platform::current().as_str().to_string(),
        installer_handoff: DesktopAppInstallerHandoff::Unsupported,
        uninstall_handoff: DesktopAppUninstallHandoff::Unsupported,
        updates_managed_by_vendor: true,
    }
}

fn project(id: DesktopAppId, state: DesktopAppPlatformState) -> DesktopApp {
    let (name, related_tool, configuration_relationship) = product_identity(id);
    let installed = matches!(
        state.status,
        DesktopAppStatus::Installed | DesktopAppStatus::UpdateAvailable
    );
    DesktopApp {
        id,
        name: name.to_string(),
        can_launch: installed && state.can_launch,
        can_manage_mcp: installed
            && id == DesktopAppId::ClaudeDesktop
            && matches!(state.environment.as_str(), "macos" | "windows"),
        status: state.status,
        version: state.version,
        latest_version: state.latest_version,
        related_tool,
        configuration_relationship,
        environment: state.environment,
        installer_handoff: state.installer_handoff,
        uninstall_handoff: state.uninstall_handoff,
        updates_managed_by_vendor: state.updates_managed_by_vendor,
        can_rollback: false,
    }
}

fn product_identity(
    id: DesktopAppId,
) -> (
    &'static str,
    Option<ToolId>,
    DesktopAppConfigurationRelationship,
) {
    match id {
        DesktopAppId::CodexApp => (
            "ChatGPT / Codex",
            Some(ToolId::Codex),
            DesktopAppConfigurationRelationship::SharedConfiguration,
        ),
        DesktopAppId::ClaudeDesktop => (
            "Claude Desktop",
            Some(ToolId::ClaudeCode),
            DesktopAppConfigurationRelationship::SeparateConfiguration,
        ),
        DesktopAppId::Cursor => (
            "Cursor",
            None,
            DesktopAppConfigurationRelationship::StandaloneApplication,
        ),
        DesktopAppId::ZCode => (
            "ZCode",
            None,
            DesktopAppConfigurationRelationship::StandaloneApplication,
        ),
        DesktopAppId::CherryStudio => (
            "Cherry Studio",
            None,
            DesktopAppConfigurationRelationship::StandaloneApplication,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::DesktopAppDirectory;
    use crate::domain::{
        AppError, DesktopAppConfigurationRelationship, DesktopAppId, DesktopAppInstallerHandoff,
        DesktopAppStatus, DesktopAppUninstallHandoff, ToolId,
    };
    use crate::platform::{DesktopAppPlatform, DesktopAppPlatformState};
    use futures::future::BoxFuture;
    use std::sync::{Arc, Mutex};

    struct FakePlatform {
        launched: Arc<Mutex<Vec<DesktopAppId>>>,
        uninstall_handoffs: Arc<Mutex<Vec<DesktopAppId>>>,
    }

    impl DesktopAppPlatform for FakePlatform {
        fn inspect(
            &self,
            id: DesktopAppId,
        ) -> BoxFuture<'static, Result<DesktopAppPlatformState, AppError>> {
            Box::pin(async move {
                Ok(match id {
                    DesktopAppId::CodexApp => DesktopAppPlatformState {
                        status: DesktopAppStatus::UpdateAvailable,
                        version: Some("1.2.3".to_string()),
                        latest_version: Some("1.3.0".to_string()),
                        can_launch: true,
                        environment: "macos".to_string(),
                        installer_handoff: DesktopAppInstallerHandoff::DirectOfficialPackage,
                        uninstall_handoff: DesktopAppUninstallHandoff::RevealApplication,
                        updates_managed_by_vendor: true,
                    },
                    DesktopAppId::ClaudeDesktop => DesktopAppPlatformState {
                        status: DesktopAppStatus::NotInstalled,
                        version: None,
                        latest_version: None,
                        can_launch: false,
                        environment: "macos".to_string(),
                        installer_handoff: DesktopAppInstallerHandoff::DirectOfficialPackage,
                        uninstall_handoff: DesktopAppUninstallHandoff::RevealApplication,
                        updates_managed_by_vendor: true,
                    },
                    DesktopAppId::Cursor | DesktopAppId::ZCode | DesktopAppId::CherryStudio => {
                        DesktopAppPlatformState {
                            status: DesktopAppStatus::Unsupported,
                            version: None,
                            latest_version: None,
                            can_launch: false,
                            environment: "windows".to_string(),
                            installer_handoff: DesktopAppInstallerHandoff::OfficialDownloadPage,
                            uninstall_handoff: DesktopAppUninstallHandoff::Unsupported,
                            updates_managed_by_vendor: true,
                        }
                    }
                })
            })
        }

        fn launch(&self, id: DesktopAppId) -> BoxFuture<'static, Result<(), AppError>> {
            let launched = self.launched.clone();
            Box::pin(async move {
                launched.lock().expect("launch lock").push(id);
                Ok(())
            })
        }

        fn open_uninstall_handoff(
            &self,
            id: DesktopAppId,
        ) -> BoxFuture<'static, Result<(), AppError>> {
            let uninstall_handoffs = self.uninstall_handoffs.clone();
            Box::pin(async move {
                uninstall_handoffs
                    .lock()
                    .expect("uninstall handoff lock")
                    .push(id);
                Ok(())
            })
        }
    }

    type RecordedAppIds = Arc<Mutex<Vec<DesktopAppId>>>;
    type DirectoryFixture = (DesktopAppDirectory, RecordedAppIds, RecordedAppIds);

    fn directory() -> DirectoryFixture {
        let launched = Arc::new(Mutex::new(Vec::new()));
        let uninstall_handoffs = Arc::new(Mutex::new(Vec::new()));
        (
            DesktopAppDirectory::new(Arc::new(FakePlatform {
                launched: launched.clone(),
                uninstall_handoffs: uninstall_handoffs.clone(),
            })),
            launched,
            uninstall_handoffs,
        )
    }

    /// Every Windows probe shells out to PowerShell. A restricted execution
    /// policy or one slow Appx query used to replace the entire list with an
    /// error card, hiding apps the user can see running.
    struct OneFailingPlatform;

    impl DesktopAppPlatform for OneFailingPlatform {
        fn inspect(
            &self,
            id: DesktopAppId,
        ) -> BoxFuture<'static, Result<DesktopAppPlatformState, AppError>> {
            Box::pin(async move {
                if id == DesktopAppId::CodexApp {
                    return Err(AppError::new(
                        crate::domain::ErrorCode::Internal,
                        "error.desktopApp.inspectFailed",
                    ));
                }
                Ok(DesktopAppPlatformState {
                    status: DesktopAppStatus::Installed,
                    version: Some("1.0.0".to_string()),
                    latest_version: None,
                    can_launch: true,
                    environment: "windows".to_string(),
                    installer_handoff: DesktopAppInstallerHandoff::OfficialDownloadPage,
                    uninstall_handoff: DesktopAppUninstallHandoff::SystemSettings,
                    updates_managed_by_vendor: true,
                })
            })
        }

        fn launch(&self, _id: DesktopAppId) -> BoxFuture<'static, Result<(), AppError>> {
            Box::pin(async { Ok(()) })
        }

        fn open_uninstall_handoff(
            &self,
            _id: DesktopAppId,
        ) -> BoxFuture<'static, Result<(), AppError>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn a_failed_probe_reports_only_itself_as_unknown() {
        let directory = DesktopAppDirectory::new(Arc::new(OneFailingPlatform));

        let apps = directory.list().await.expect("a failed probe is not fatal");

        assert_eq!(apps.len(), 5);
        assert_eq!(apps[0].id, DesktopAppId::CodexApp);
        assert_eq!(apps[0].status, DesktopAppStatus::Unknown);
        assert!(!apps[0].can_launch, "an unknown app offers no action");
        assert!(!apps[0].can_manage_mcp);
        for app in &apps[1..] {
            assert_eq!(app.status, DesktopAppStatus::Installed, "{:?}", app.id);
        }
    }

    #[tokio::test]
    async fn list_keeps_apps_separate_from_cli_identity_and_in_stable_order() {
        let (directory, _, _) = directory();
        let apps = directory.list().await.expect("list");
        assert_eq!(apps.len(), 5);
        assert_eq!(apps[0].id, DesktopAppId::CodexApp);
        assert_eq!(apps[0].related_tool, Some(ToolId::Codex));
        assert_eq!(
            apps[0].configuration_relationship,
            DesktopAppConfigurationRelationship::SharedConfiguration
        );
        assert!(apps[0].can_launch);
        assert_eq!(apps[0].status, DesktopAppStatus::UpdateAvailable);
        assert_eq!(apps[0].latest_version.as_deref(), Some("1.3.0"));
        assert!(!apps[0].can_manage_mcp);
        assert_eq!(
            apps[0].installer_handoff,
            DesktopAppInstallerHandoff::DirectOfficialPackage
        );
        assert_eq!(
            apps[0].uninstall_handoff,
            DesktopAppUninstallHandoff::RevealApplication
        );
        assert!(apps[0].updates_managed_by_vendor);
        assert!(!apps[0].can_rollback);
        assert_eq!(apps[1].id, DesktopAppId::ClaudeDesktop);
        assert_eq!(apps[1].related_tool, Some(ToolId::ClaudeCode));
        assert_eq!(
            apps[1].configuration_relationship,
            DesktopAppConfigurationRelationship::SeparateConfiguration
        );
        assert!(!apps[1].can_launch);
        assert!(!apps[1].can_manage_mcp);
        assert_eq!(apps[2].id, DesktopAppId::Cursor);
        assert_eq!(apps[2].related_tool, None);
        assert_eq!(
            apps[2].configuration_relationship,
            DesktopAppConfigurationRelationship::StandaloneApplication
        );
        assert_eq!(apps[3].id, DesktopAppId::ZCode);
        assert_eq!(apps[3].related_tool, None);
        assert_eq!(apps[4].id, DesktopAppId::CherryStudio);
        assert_eq!(apps[4].related_tool, None);
    }

    #[tokio::test]
    async fn launch_passes_only_the_stable_desktop_app_id() {
        let (directory, launched, _) = directory();
        let outcome = directory
            .launch(DesktopAppId::ClaudeDesktop)
            .await
            .expect("launch");
        assert_eq!(
            serde_json::to_string(&outcome).expect("serialize"),
            r#""launched""#
        );
        assert_eq!(
            *launched.lock().expect("launch lock"),
            vec![DesktopAppId::ClaudeDesktop]
        );
    }

    #[tokio::test]
    async fn uninstall_handoff_passes_only_the_stable_desktop_app_id() {
        let (directory, _, uninstall_handoffs) = directory();
        let outcome = directory
            .open_uninstall_handoff(DesktopAppId::CodexApp)
            .await
            .expect("uninstall handoff");
        assert_eq!(
            serde_json::to_string(&outcome).expect("serialize"),
            r#""opened""#
        );
        assert_eq!(
            *uninstall_handoffs.lock().expect("uninstall handoff lock"),
            vec![DesktopAppId::CodexApp]
        );
    }
}
