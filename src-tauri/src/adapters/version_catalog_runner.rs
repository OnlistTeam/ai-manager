use crate::adapters::lifecycle_runner::{
    community_mirror_can_help, download_command_failure, prepare_download_spec,
};
use crate::adapters::tool_adapter::VersionQueryContext;
use crate::compat::ccswitch::versioning::{
    blocked_catalog, parse_catalog, VersionCatalogRequest, VersionCatalogTarget,
};
use crate::domain::{AppError, DownloadStrategy, ErrorCode, ToolId, ToolVersionCatalog};
use crate::platform::command::CommandSpec;

/// The historical version catalog shares the official-first strategy with install/update: only a
/// transport-level network failure, a command timeout, or a 403 / region restriction from the
/// public npm registry may append a community mirror to the npm subprocess once.
/// Account/proxy authentication, permissions, 404, a missing version and a malformed successful
/// response all fail as is, so switching mirrors never masks the real problem.
pub(crate) async fn load_version_catalog(
    id: ToolId,
    target: &VersionCatalogTarget,
    ctx: &VersionQueryContext,
) -> Result<ToolVersionCatalog, AppError> {
    let Some(request) = target.request.clone() else {
        return Ok(blocked_catalog(id, target));
    };

    let VersionCatalogRequest::Npm(base_spec) = request else {
        return load_hermes_pypi(id, target.source).await;
    };

    let official = load_attempt(id, target, ctx, base_spec.clone(), false).await;
    match official {
        Ok(catalog) => Ok(catalog),
        Err(error)
            if ctx.network.download_strategy == DownloadStrategy::Automatic
                && community_mirror_can_help(&error) =>
        {
            load_attempt(id, target, ctx, base_spec, true).await
        }
        Err(error) => Err(error),
    }
}

async fn load_hermes_pypi(
    id: ToolId,
    source: crate::domain::ToolInstallSource,
) -> Result<ToolVersionCatalog, AppError> {
    crate::compat::ccswitch::versioning::pypi::fetch_catalog(
        id,
        source,
        std::time::Duration::from_secs(120),
    )
    .await
}

async fn load_attempt(
    id: ToolId,
    target: &VersionCatalogTarget,
    ctx: &VersionQueryContext,
    base_spec: CommandSpec,
    community_mirror: bool,
) -> Result<ToolVersionCatalog, AppError> {
    let spec = prepare_download_spec(base_spec, &ctx.network, community_mirror);
    match ctx.executor.execute(spec.clone()).await {
        Ok(output) if output.success => {
            parse_catalog(id, target.source, &output.stdout, community_mirror)
        }
        Ok(output) => Err(download_command_failure(
            &spec,
            &output,
            ErrorCode::UpdateFailed,
            "error.tool.versionCatalogFailed",
        )),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::load_version_catalog;
    use crate::adapters::tool_adapter::{LifecycleNetworkPolicy, VersionQueryContext};
    use crate::compat::ccswitch::versioning::{VersionCatalogRequest, VersionCatalogTarget};
    use crate::domain::{AppError, DownloadStrategy, ErrorCode, ToolId, ToolInstallSource};
    use crate::platform::command::{AllowedProgram, CommandSpec};
    use crate::platform::executor::{CommandExecutor, CommandOutput};
    use futures::future::BoxFuture;
    use std::sync::{Arc, Mutex};

    struct ScriptedExecutor {
        outcomes: Mutex<Vec<Result<CommandOutput, AppError>>>,
        seen: Mutex<Vec<CommandSpec>>,
    }

    impl ScriptedExecutor {
        fn new(outcomes: Vec<Result<CommandOutput, AppError>>) -> Arc<Self> {
            Arc::new(Self {
                outcomes: Mutex::new(outcomes),
                seen: Mutex::new(Vec::new()),
            })
        }

        fn specs(&self) -> Vec<CommandSpec> {
            self.seen.lock().expect("seen lock").clone()
        }
    }

    impl CommandExecutor for ScriptedExecutor {
        fn execute(
            &self,
            spec: CommandSpec,
        ) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
            self.seen.lock().expect("seen lock").push(spec);
            let next = self.outcomes.lock().expect("outcomes lock").remove(0);
            Box::pin(async move { next })
        }
    }

    fn target() -> VersionCatalogTarget {
        VersionCatalogTarget {
            source: ToolInstallSource::NotInstalled,
            restriction: None,
            request: Some(VersionCatalogRequest::Npm(CommandSpec::new(
                AllowedProgram::Npm,
                vec![
                    "view".to_string(),
                    "@anthropic-ai/claude-code".to_string(),
                    "dist-tags".to_string(),
                    "versions".to_string(),
                    "--json".to_string(),
                ],
            ))),
        }
    }

    fn context(executor: Arc<ScriptedExecutor>) -> VersionQueryContext {
        VersionQueryContext {
            executor,
            network: LifecycleNetworkPolicy::new(DownloadStrategy::Automatic, None),
        }
    }

    fn failed_output(stderr: &str) -> CommandOutput {
        CommandOutput {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }

    fn catalog_output() -> CommandOutput {
        CommandOutput {
            success: true,
            exit_code: Some(0),
            stdout: r#"{"dist-tags":{"latest":"2.0.0"},"versions":["1.0.0","2.0.0"]}"#.to_string(),
            stderr: String::new(),
        }
    }

    #[tokio::test]
    async fn transport_failure_retries_once_with_a_process_scoped_mirror() {
        let executor = ScriptedExecutor::new(vec![
            Ok(failed_output(
                "npm ERR! request ENOTFOUND registry.npmjs.org",
            )),
            Ok(catalog_output()),
        ]);

        let catalog =
            load_version_catalog(ToolId::ClaudeCode, &target(), &context(executor.clone()))
                .await
                .expect("the mirror query succeeds");

        assert!(catalog.mirror_used);
        let specs = executor.specs();
        assert_eq!(specs.len(), 2);
        assert!(specs[0].env.iter().any(|(key, value)| {
            key == "NPM_CONFIG_REGISTRY" && value == "https://registry.npmjs.org"
        }));
        assert!(specs[1].env.iter().any(|(key, value)| {
            key == "NPM_CONFIG_REGISTRY" && value == "https://registry.npmmirror.com"
        }));
    }

    #[tokio::test]
    async fn auth_and_package_metadata_failures_never_switch_registry() {
        let cases = [
            (
                "npm ERR! code E401\n401 Unauthorized",
                ErrorCode::NetworkError,
                "error.tool.downloadAccessDenied",
            ),
            (
                "npm ERR! code E407\n407 Proxy Authentication Required",
                ErrorCode::NetworkError,
                "error.tool.downloadAccessDenied",
            ),
            (
                "npm ERR! code E404\n404 Not Found",
                ErrorCode::UpdateFailed,
                "error.tool.versionCatalogFailed",
            ),
            (
                "npm ERR! code ETARGET\nnpm ERR! notarget No matching version found",
                ErrorCode::UpdateFailed,
                "error.tool.versionCatalogFailed",
            ),
        ];

        for (stderr, expected_code, expected_key) in cases {
            let executor = ScriptedExecutor::new(vec![Ok(failed_output(stderr))]);
            let error =
                load_version_catalog(ToolId::ClaudeCode, &target(), &context(executor.clone()))
                    .await
                    .expect_err("the registry failure must remain visible");

            assert_eq!(error.code, expected_code, "{stderr}");
            assert_eq!(error.message_key, expected_key, "{stderr}");
            assert_eq!(executor.specs().len(), 1, "{stderr}");
        }
    }

    #[tokio::test]
    async fn registry_403_retries_once_with_the_process_scoped_mirror() {
        let executor = ScriptedExecutor::new(vec![
            Ok(failed_output("npm ERR! code E403\n403 Forbidden")),
            Ok(catalog_output()),
        ]);

        let catalog =
            load_version_catalog(ToolId::ClaudeCode, &target(), &context(executor.clone()))
                .await
                .expect("the alternate registry supplies the public package catalog");

        assert!(catalog.mirror_used);
        let specs = executor.specs();
        assert_eq!(specs.len(), 2);
        assert!(specs[1].env.iter().any(|(key, value)| {
            key == "NPM_CONFIG_REGISTRY" && value == "https://registry.npmmirror.com"
        }));
    }

    #[tokio::test]
    async fn registry_region_restriction_retries_once_with_the_process_scoped_mirror() {
        let executor = ScriptedExecutor::new(vec![
            Ok(failed_output("Package unavailable in your region")),
            Ok(catalog_output()),
        ]);

        let catalog =
            load_version_catalog(ToolId::ClaudeCode, &target(), &context(executor.clone()))
                .await
                .expect("the alternate registry supplies the region-neutral public catalog");

        assert!(catalog.mirror_used);
        assert_eq!(executor.specs().len(), 2);
    }

    #[tokio::test]
    async fn malformed_success_response_is_not_retried_against_a_mirror() {
        let executor = ScriptedExecutor::new(vec![Ok(CommandOutput {
            success: true,
            exit_code: Some(0),
            stdout: "not-json".to_string(),
            stderr: String::new(),
        })]);

        let error = load_version_catalog(ToolId::ClaudeCode, &target(), &context(executor.clone()))
            .await
            .expect_err("malformed registry data must fail closed");

        assert_eq!(error.code, ErrorCode::UpdateFailed);
        assert_eq!(error.message_key, "error.tool.versionCatalogFailed");
        assert_eq!(
            error.remediation.as_deref(),
            Some("error.remediation.retryOrViewDetails")
        );
        assert_eq!(executor.specs().len(), 1);
    }

    #[tokio::test]
    async fn local_executor_permission_failure_is_preserved_without_a_mirror() {
        let executor = ScriptedExecutor::new(vec![Err(AppError::new(
            ErrorCode::PermissionDenied,
            "error.command.permissionDenied",
        ))]);

        let error = load_version_catalog(ToolId::ClaudeCode, &target(), &context(executor.clone()))
            .await
            .expect_err("local permission failures are not network failures");

        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert_eq!(error.message_key, "error.command.permissionDenied");
        assert_eq!(executor.specs().len(), 1);
    }

    #[tokio::test]
    async fn command_timeout_gets_one_bounded_mirror_retry() {
        let executor = ScriptedExecutor::new(vec![
            Err(AppError::new(ErrorCode::Internal, "error.command.timeout")),
            Ok(catalog_output()),
        ]);

        let catalog =
            load_version_catalog(ToolId::ClaudeCode, &target(), &context(executor.clone()))
                .await
                .expect("an npm timeout gets the bounded network recovery attempt");

        assert!(catalog.mirror_used);
        assert_eq!(executor.specs().len(), 2);
    }
}
