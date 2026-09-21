use std::path::PathBuf;

use futures::future::BoxFuture;

use crate::adapters::lifecycle_runner::{precheck_removable, run_plan};
use crate::adapters::tool_adapter::{
    AuthorizedUpdate, LaunchContext, LifecycleContext, ToolAdapter, VersionQueryContext,
};
use crate::adapters::uninstall_runner::run_uninstall;
use crate::adapters::version_catalog_runner::load_version_catalog;
use crate::compat::ccswitch::install_probe::{probe, InstallSource, LifecycleProbe};
use crate::compat::ccswitch::lifecycle::{
    install_plan, install_version_plan, repair_plan, uninstall_plan,
};
use crate::compat::ccswitch::tool_launch::launch_spec;
use crate::compat::ccswitch::tool_paths::{cache_paths, settings_paths};
use crate::compat::ccswitch::tools::{capabilities_for, detect_tool, detect_tool_local};
use crate::compat::ccswitch::update_preview::checked_update_plan;
use crate::compat::ccswitch::versioning::source_for_probe;
use crate::domain::operation::{log_message, phase};
use crate::domain::{
    AppError, ErrorCode, OperationLogKind, Tool, ToolCapabilities, ToolId, ToolInstallSource,
    ToolVersionCatalog, UninstallOptions,
};
use crate::platform::executor::CommandCancellation;

/// Spec §32: cache/settings are computed once per batch from the options the user ticked
/// (including the reported progress value), and the pre-check and the real deletion share the same
/// data — do not call `cache_paths`/`settings_paths` twice, or the list the pre-check saw may
/// differ from the one actually deleted (a TOCTOU-style inconsistency).
/// The order is fixed as "cache first, settings second": the caches of claude / codex live inside
/// the config directory, so the reverse order would make the cache deletion a no-op.
fn removal_batches(id: ToolId, options: &UninstallOptions) -> Vec<(u8, Vec<PathBuf>)> {
    let mut batches = Vec::new();
    if options.remove_cache {
        batches.push((85, cache_paths(id)));
    }
    if options.remove_settings {
        batches.push((90, settings_paths(id)));
    }
    batches
}

/// Pre-check the whole batch of cache/settings paths; a single rejected entry fails the batch and
/// deletes nothing. It must be called before the uninstall plan (which removes the app), so that a
/// failed pre-check leaves the app untouched byte for byte.
fn precheck_removal_batches(batches: &[(u8, Vec<PathBuf>)]) -> Result<(), AppError> {
    for (_, paths) in batches {
        precheck_removable(paths, ErrorCode::UninstallFailed)?;
    }
    Ok(())
}

/// ADR-0033: after a failed official self-update, continue only when this platform can supply the
/// official layout of that native installation from the registry; once a cancellation is confirmed
/// or the cancel check fails, no further write operation is started.
fn native_supply_follows(
    id: ToolId,
    source: InstallSource,
    cancellation: &CommandCancellation,
) -> bool {
    source == InstallSource::Native
        && crate::compat::ccswitch::native_supply::supplies(id)
        && !cancellation.is_confirmed()
        && !cancellation.is_failed()
}

/// Whether the official self-update is already doomed, so the supply path can be taken directly.
///
/// The tool's own updater retries for a long time before failing on an unreachable channel, while
/// the supply path that follows succeeds in seconds. So the channel is probed once first and the
/// tool's update command is skipped when the probe fails. Supplying writes files, so
/// `native_supply_follows` is confirmed both before and after the probe.
async fn official_update_is_already_lost(
    id: ToolId,
    source: InstallSource,
    cancellation: &CommandCancellation,
) -> bool {
    if !native_supply_follows(id, source, cancellation) {
        return false;
    }
    if crate::adapters::native_supply::official_channel_reachable(id).await {
        return false;
    }
    native_supply_follows(id, source, cancellation)
}

/// Every P0/P0.5 tool shares the same adapter implementation: all detection capabilities come from
/// the upstream facade and the differences are expressed by ToolCapabilities, with no per-tool
/// branching (AI_RULES rule 8).
pub struct UpstreamToolAdapter {
    id: ToolId,
}

impl UpstreamToolAdapter {
    pub fn new(id: ToolId) -> Self {
        Self { id }
    }
}

impl ToolAdapter for UpstreamToolAdapter {
    fn id(&self) -> ToolId {
        self.id
    }

    fn capabilities(&self) -> ToolCapabilities {
        capabilities_for(self.id)
    }

    fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        Box::pin(async move { Ok(detect_tool(self.id).await) })
    }

    fn detect_local(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        Box::pin(async move { Ok(detect_tool_local(self.id).await) })
    }

    fn install_source(&self) -> BoxFuture<'_, Result<ToolInstallSource, AppError>> {
        let id = self.id;
        Box::pin(async move {
            let probed = probe(id).await?;
            Ok(source_for_probe(&probed))
        })
    }

    fn install(&self, ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        let id = self.id;
        Box::pin(async move {
            let probed = probe(id).await?;
            let plan = install_plan(id, &probed)?;
            ctx.report(20, phase::DOWNLOADING);
            run_plan(
                &plan,
                &ctx,
                ErrorCode::InstallFailed,
                "error.tool.installFailed",
                phase::INSTALLING,
            )
            .await
        })
    }

    fn update_preview(&self) -> BoxFuture<'_, Result<crate::domain::ToolUpdatePreview, AppError>> {
        let id = self.id;
        Box::pin(
            async move { crate::compat::ccswitch::update_preview::inspect_and_preview(id).await },
        )
    }

    fn validate_update(
        &self,
        preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
        let id = self.id;
        Box::pin(async move {
            let checked = checked_update_plan(id, &preview_fingerprint).await?;
            Ok(AuthorizedUpdate {
                observed: checked.observed.clone(),
                target_version: checked.target_version.clone(),
                source: source_for_probe(&LifecycleProbe {
                    entry: Some(checked.entry.clone()),
                    path_env: None,
                }),
                plan: Some(checked),
            })
        })
    }

    /// Runs the plan `validate_update` checked; nothing is re-derived here, so
    /// the registry and the installation enumeration are not consulted again.
    fn update(
        &self,
        ctx: LifecycleContext,
        authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        let id = self.id;
        Box::pin(async move {
            let checked = authorized.plan.ok_or_else(|| {
                AppError::new(ErrorCode::UpdateFailed, "error.tool.actionUnsupported")
                    .with_technical("update authorization carries no checked plan")
                    .with_remediation("error.remediation.recheckUpdate")
            })?;
            ctx.report(20, phase::DOWNLOADING);
            if official_update_is_already_lost(id, checked.entry.source, &ctx.cancellation).await {
                // The supply path itself logs "the official update channel is unavailable, fetching
                // the same official program from the software download source", which holds here too,
                // so there is no need for a second, near-synonymous log line.
                return crate::adapters::native_supply::run(
                    id,
                    &checked.target_version,
                    &checked.entry,
                    &ctx,
                )
                .await;
            }
            let outcome = run_plan(
                &checked.plan,
                &ctx,
                ErrorCode::UpdateFailed,
                "error.tool.updateFailed",
                phase::INSTALLING,
            )
            .await;
            match outcome {
                Ok(()) => Ok(()),
                Err(_) if native_supply_follows(id, checked.entry.source, &ctx.cancellation) => {
                    ctx.log_message(OperationLogKind::System, log_message::TRYING_FALLBACK);
                    crate::adapters::native_supply::run(
                        id,
                        &checked.target_version,
                        &checked.entry,
                        &ctx,
                    )
                    .await
                }
                Err(error) => Err(error),
            }
        })
    }

    fn install_version(
        &self,
        ctx: LifecycleContext,
        version: String,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        let id = self.id;
        Box::pin(async move {
            let probed = probe(id).await?;
            let plan = install_version_plan(id, &probed, &version)?;
            ctx.report(20, phase::DOWNLOADING);
            run_plan(
                &plan,
                &ctx,
                ErrorCode::UpdateFailed,
                "error.tool.updateFailed",
                phase::INSTALLING,
            )
            .await
        })
    }

    fn version_catalog(
        &self,
        ctx: VersionQueryContext,
    ) -> BoxFuture<'_, Result<ToolVersionCatalog, AppError>> {
        let id = self.id;
        Box::pin(async move {
            let probed = probe(id).await?;
            let target = crate::compat::ccswitch::versioning::catalog_target(id, &probed)?;
            load_version_catalog(id, &target, &ctx).await
        })
    }

    fn repair(&self, ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        let id = self.id;
        Box::pin(async move {
            let probed = probe(id).await?;
            let plan = repair_plan(id, &probed)?;
            ctx.report(20, phase::DOWNLOADING);
            run_plan(
                &plan,
                &ctx,
                ErrorCode::UpdateFailed,
                "error.tool.updateFailed",
                phase::INSTALLING,
            )
            .await
        })
    }

    fn uninstall(
        &self,
        ctx: LifecycleContext,
        options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        let id = self.id;
        Box::pin(async move {
            let probed = probe(id).await?;
            let plan = uninstall_plan(id, &probed)?;

            // Validate the exact cache/settings batches before touching the app.
            let removals = removal_batches(id, &options);
            precheck_removal_batches(&removals)?;

            run_uninstall(plan, removals, &ctx).await
        })
    }

    fn launch(
        &self,
        ctx: LaunchContext,
        working_directory: PathBuf,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        let id = self.id;
        Box::pin(async move {
            let spec = launch_spec(id, working_directory)
                .await?
                .with_terminal_app(ctx.terminal_app);
            ctx.launcher.launch(spec).await
        })
    }
}

pub struct AdapterRegistry;

impl AdapterRegistry {
    pub fn all() -> Vec<Box<dyn ToolAdapter>> {
        ToolId::ALL
            .into_iter()
            .map(|id| Box::new(UpstreamToolAdapter::new(id)) as Box<dyn ToolAdapter>)
            .collect()
    }

    pub fn get(id: ToolId) -> Option<Box<dyn ToolAdapter>> {
        ToolId::ALL
            .into_iter()
            .find(|candidate| *candidate == id)
            .map(|found| Box::new(UpstreamToolAdapter::new(found)) as Box<dyn ToolAdapter>)
    }

    pub async fn detect_all() -> Result<Vec<Tool>, AppError> {
        futures::future::join_all(
            Self::all()
                .into_iter()
                .map(|adapter| async move { adapter.detect().await }),
        )
        .await
        .into_iter()
        .collect()
    }

    /// Like `detect_all`, but local-only: the page uses it for the first paint while the latest version is fetched in the background by a separate command.
    pub async fn detect_all_local() -> Result<Vec<Tool>, AppError> {
        futures::future::join_all(
            Self::all()
                .into_iter()
                .map(|adapter| async move { adapter.detect_local().await }),
        )
        .await
        .into_iter()
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{precheck_removal_batches, removal_batches, AdapterRegistry, UpstreamToolAdapter};
    use crate::adapters::tool_adapter::{AuthorizedUpdate, LifecycleContext};
    use crate::adapters::ToolAdapter;
    use crate::compat::ccswitch::tool_paths::{cache_paths, settings_paths};
    use crate::compat::ccswitch::tools::capabilities_for;
    use crate::domain::{
        AppError, ErrorCode, Tool, ToolCapabilities, ToolId, ToolStatus, UninstallOptions,
    };
    use futures::future::BoxFuture;

    struct StubAdapter {
        id: ToolId,
        outcome: Result<Tool, AppError>,
    }

    impl ToolAdapter for StubAdapter {
        fn id(&self) -> ToolId {
            self.id
        }

        fn capabilities(&self) -> ToolCapabilities {
            capabilities_for(self.id)
        }

        fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
            Box::pin(async move { self.outcome.clone() })
        }

        fn install(&self, _ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
            Box::pin(async { Ok(()) })
        }

        fn validate_update(
            &self,
            _preview_fingerprint: String,
        ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
            Box::pin(async { unreachable!("update is not part of these tests") })
        }

        fn update(
            &self,
            _ctx: LifecycleContext,
            _authorized: AuthorizedUpdate,
        ) -> BoxFuture<'_, Result<(), AppError>> {
            Box::pin(async { Ok(()) })
        }

        fn uninstall(
            &self,
            _ctx: LifecycleContext,
            _options: UninstallOptions,
        ) -> BoxFuture<'_, Result<(), AppError>> {
            Box::pin(async { Ok(()) })
        }
    }

    fn stub_tool(id: ToolId) -> Tool {
        Tool {
            id,
            name: "Stub".to_string(),
            description_key: format!("tool.{}.description", id.as_str()),
            discovery: crate::domain::ToolDiscovery::for_tool(id),
            status: ToolStatus::Installed,
            version: Some("1.0.0".to_string()),
            latest_version: Some("1.0.0".to_string()),
            capabilities: capabilities_for(id),
            sessions_inside_settings: false,
            environment: Some("macos".to_string()),
            configuration_shared_with: None,
        }
    }

    #[test]
    fn registry_exposes_every_domain_tool_with_facade_capabilities() {
        let adapters = AdapterRegistry::all();
        assert_eq!(adapters.len(), ToolId::ALL.len());
        let ids: Vec<ToolId> = adapters.iter().map(|adapter| adapter.id()).collect();
        assert_eq!(ids, ToolId::ALL.to_vec());
        for adapter in &adapters {
            assert_eq!(adapter.capabilities(), capabilities_for(adapter.id()));
        }
        for id in ToolId::ALL {
            assert_eq!(AdapterRegistry::get(id).expect("registered").id(), id);
        }
        let direct = UpstreamToolAdapter::new(ToolId::GeminiCli);
        assert_eq!(direct.id(), ToolId::GeminiCli);
        assert_eq!(direct.capabilities(), capabilities_for(ToolId::GeminiCli));
    }

    #[tokio::test]
    async fn trait_is_object_safe_and_detect_returns_domain_tool() {
        let adapter: Box<dyn ToolAdapter> = Box::new(StubAdapter {
            id: ToolId::Codex,
            outcome: Ok(stub_tool(ToolId::Codex)),
        });
        let tool = adapter.detect().await.expect("stub detect succeeds");
        assert_eq!(tool.id, ToolId::Codex);
        assert_eq!(tool.status, ToolStatus::Installed);
    }

    #[tokio::test]
    async fn detect_failures_surface_as_domain_errors() {
        let adapter: Box<dyn ToolAdapter> = Box::new(StubAdapter {
            id: ToolId::ClaudeCode,
            outcome: Err(AppError::new(
                ErrorCode::ToolNotFound,
                "error.tool.notFound",
            )),
        });
        let error = adapter.detect().await.expect_err("stub detect fails");
        assert_eq!(error.code, ErrorCode::ToolNotFound);
    }

    /// ADR-0033: only a native Claude Code install on a platform the registry
    /// can serve falls through to the layout supply, and never after a cancel.
    #[test]
    fn native_supply_follows_only_a_native_claude_update_that_was_not_cancelled() {
        use super::native_supply_follows;
        use crate::compat::ccswitch::install_probe::InstallSource;
        use crate::compat::ccswitch::native_supply::supplies;
        use crate::platform::executor::CommandCancellation;

        let idle = CommandCancellation::default();
        assert_eq!(
            native_supply_follows(ToolId::ClaudeCode, InstallSource::Native, &idle),
            supplies(ToolId::ClaudeCode)
        );
        assert!(!native_supply_follows(
            ToolId::ClaudeCode,
            InstallSource::NodeManagerNpm,
            &idle
        ));
        assert!(!native_supply_follows(
            ToolId::OpenCode,
            InstallSource::Native,
            &idle
        ));

        let cancelled = CommandCancellation::default();
        assert!(cancelled.request());
        assert!(cancelled.confirm_if_requested());
        assert!(!native_supply_follows(
            ToolId::ClaudeCode,
            InstallSource::Native,
            &cancelled
        ));
    }

    /// Cache is removed before settings because some cache paths are nested.
    #[test]
    #[serial_test::serial]
    fn removal_batches_reflect_the_checked_options_in_cache_then_settings_order() {
        let neither = UninstallOptions {
            remove_settings: false,
            remove_cache: false,
        };
        assert!(removal_batches(ToolId::ClaudeCode, &neither).is_empty());

        let cache_only = UninstallOptions {
            remove_settings: false,
            remove_cache: true,
        };
        assert_eq!(
            removal_batches(ToolId::ClaudeCode, &cache_only),
            vec![(85, cache_paths(ToolId::ClaudeCode))]
        );

        let settings_only = UninstallOptions {
            remove_settings: true,
            remove_cache: false,
        };
        assert_eq!(
            removal_batches(ToolId::ClaudeCode, &settings_only),
            vec![(90, settings_paths(ToolId::ClaudeCode))]
        );

        let both = UninstallOptions {
            remove_settings: true,
            remove_cache: true,
        };
        assert_eq!(
            removal_batches(ToolId::ClaudeCode, &both),
            vec![
                (85, cache_paths(ToolId::ClaudeCode)),
                (90, settings_paths(ToolId::ClaudeCode)),
            ]
        );
    }

    /// A refused path blocks all optional removal batches.
    #[test]
    #[serial_test::serial]
    fn precheck_removal_batches_refuses_the_whole_set_when_any_path_is_outside_home() {
        let all_inside = vec![
            (85u8, cache_paths(ToolId::ClaudeCode)),
            (90u8, settings_paths(ToolId::ClaudeCode)),
        ];
        assert!(precheck_removal_batches(&all_inside).is_ok());

        let one_outside = vec![
            (85u8, cache_paths(ToolId::ClaudeCode)),
            (
                90u8,
                vec![std::path::PathBuf::from("/usr/local/bin/not-ours")],
            ),
        ];
        let error = precheck_removal_batches(&one_outside)
            .expect_err("a path outside the home directory must refuse the whole set");
        assert_eq!(error.code, ErrorCode::UninstallFailed);
        assert_eq!(error.message_key, "error.tool.removePathRefused");
    }
}
