use std::sync::Arc;

use crate::adapters::{AdapterRegistry, LifecycleNetworkPolicy, ToolAdapter, VersionQueryContext};
use crate::application::tool_lifecycle::AdapterResolver;
use crate::domain::{AppError, DownloadStrategy, ErrorCode, ToolId, ToolVersionCatalog};
use crate::platform::executor::{CommandExecutor, SystemExecutor};

pub struct ToolVersionDirectory {
    executor: Arc<dyn CommandExecutor>,
    adapters: AdapterResolver,
    network: LifecycleNetworkPolicy,
}

impl ToolVersionDirectory {
    pub fn new(executor: Arc<dyn CommandExecutor>, adapters: AdapterResolver) -> Self {
        Self {
            executor,
            adapters,
            network: LifecycleNetworkPolicy::default(),
        }
    }

    pub fn system(strategy: DownloadStrategy) -> Self {
        let mut directory = Self::new(Arc::new(SystemExecutor), Arc::new(AdapterRegistry::get));
        directory.network = LifecycleNetworkPolicy::new(
            strategy,
            crate::compat::ccswitch::lifecycle::configured_proxy_url(),
        );
        directory
    }

    pub async fn catalog(&self, tool: ToolId) -> Result<ToolVersionCatalog, AppError> {
        let adapter = self.resolve(tool)?;
        if !adapter.capabilities().can_manage_version {
            return Err(
                AppError::new(ErrorCode::UpdateFailed, "error.tool.actionUnsupported")
                    .with_technical(format!(
                        "{} has no version catalog capability",
                        tool.as_str()
                    ))
                    .with_remediation("error.remediation.installManually"),
            );
        }
        adapter
            .version_catalog(VersionQueryContext {
                executor: self.executor.clone(),
                network: self.network.clone(),
            })
            .await
    }

    fn resolve(&self, id: ToolId) -> Result<Box<dyn ToolAdapter>, AppError> {
        (self.adapters)(id).ok_or_else(|| {
            AppError::new(ErrorCode::ToolNotFound, "error.tool.notFound")
                .with_technical(id.as_str().to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ToolVersionDirectory;
    use crate::adapters::{
        AuthorizedUpdate, LaunchContext, LifecycleContext, ToolAdapter, VersionQueryContext,
    };
    use crate::domain::{
        AppError, Tool, ToolCapabilities, ToolId, ToolInstallSource, ToolVersionCatalog,
        UninstallOptions,
    };
    use crate::platform::executor::{CommandExecutor, CommandOutput};
    use futures::future::BoxFuture;
    use std::sync::Arc;

    struct NoopExecutor;
    impl CommandExecutor for NoopExecutor {
        fn execute(
            &self,
            _spec: crate::platform::command::CommandSpec,
        ) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
            Box::pin(async { unreachable!("stub adapter owns the result") })
        }
    }

    struct StubAdapter {
        allowed: bool,
    }
    impl ToolAdapter for StubAdapter {
        fn id(&self) -> ToolId {
            ToolId::ClaudeCode
        }
        fn capabilities(&self) -> ToolCapabilities {
            ToolCapabilities {
                can_manage_version: self.allowed,
                ..ToolCapabilities::default()
            }
        }
        fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
            Box::pin(async { unreachable!() })
        }
        fn install(&self, _ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
            Box::pin(async { unreachable!() })
        }
        fn validate_update(
            &self,
            _preview_fingerprint: String,
        ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
            Box::pin(async { unreachable!() })
        }

        fn update(
            &self,
            _ctx: LifecycleContext,
            _authorized: AuthorizedUpdate,
        ) -> BoxFuture<'_, Result<(), AppError>> {
            Box::pin(async { unreachable!() })
        }
        fn uninstall(
            &self,
            _ctx: LifecycleContext,
            _options: UninstallOptions,
        ) -> BoxFuture<'_, Result<(), AppError>> {
            Box::pin(async { unreachable!() })
        }
        fn launch(
            &self,
            _ctx: LaunchContext,
            _working_directory: std::path::PathBuf,
        ) -> BoxFuture<'_, Result<(), AppError>> {
            Box::pin(async { unreachable!() })
        }
        fn version_catalog(
            &self,
            _ctx: VersionQueryContext,
        ) -> BoxFuture<'_, Result<ToolVersionCatalog, AppError>> {
            Box::pin(async {
                Ok(ToolVersionCatalog {
                    tool: ToolId::ClaudeCode,
                    source: ToolInstallSource::Npm,
                    can_change_version: true,
                    restriction: None,
                    latest_version: Some("2.0.0".to_string()),
                    dist_tags: Vec::new(),
                    versions: vec!["2.0.0".to_string(), "1.0.0".to_string()],
                    mirror_used: false,
                })
            })
        }
    }

    fn directory(allowed: bool) -> ToolVersionDirectory {
        ToolVersionDirectory::new(
            Arc::new(NoopExecutor),
            Arc::new(move |_| Some(Box::new(StubAdapter { allowed }) as Box<dyn ToolAdapter>)),
        )
    }

    #[tokio::test]
    async fn capability_gate_precedes_the_adapter_query() {
        let error = directory(false)
            .catalog(ToolId::ClaudeCode)
            .await
            .unwrap_err();
        assert_eq!(error.message_key, "error.tool.actionUnsupported");
    }

    #[tokio::test]
    async fn safe_catalog_crosses_the_application_boundary() {
        let catalog = directory(true).catalog(ToolId::ClaudeCode).await.unwrap();
        assert_eq!(catalog.versions, vec!["2.0.0", "1.0.0"]);
        assert!(catalog.can_change_version);
    }
}
