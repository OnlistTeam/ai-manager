use std::sync::Arc;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::application::extension_directory::ExtensionDirectory;
use crate::application::mcp_installation::{McpInstallRequest, McpInstallationService};
use crate::application::mcp_removal::{McpRemovalRequest, McpRemovalService};
use crate::application::product_settings::ProductSettingsService;
use crate::application::prompt_directory::PromptDirectory;
use crate::application::provider_batch_test::ProviderBatchTestService;
use crate::application::provider_directory::ProviderDirectory;
use crate::application::provider_preflight::ProviderPreflightService;
use crate::application::skill_installation::{SkillInstallRequest, SkillInstallationService};
use crate::application::skill_removal::{SkillRemovalRequest, SkillRemovalService};
use crate::application::tool_launch::ToolLaunchService;
use crate::application::tool_lifecycle::{LifecycleRequest, ToolLifecycleService};
use crate::application::tool_uninstall_preview::ToolUninstallPreviewService;
use crate::application::tool_update_preview::ToolUpdatePreviewService;
use crate::application::tool_version_directory::ToolVersionDirectory;
use crate::application::tool_version_history::ToolVersionHistoryService;
use crate::compat::ccswitch::tool_version_storage::ToolVersionStore;
use crate::database::Database;
use crate::domain::{
    AppError, DetectedSkillResourceAction, DetectedSkillResourceOpenOutcome, ErrorCode, Extension,
    ExtensionKind, ExtensionScope, LocalExtensionInventory, McpInstallDraft, Operation,
    OperationId, PromptDetail, PromptDraft, Provider, ProviderConnectionProfile,
    ProviderCreateDraft, ProviderCreateResult, ProviderCustomCreateDraft, ProviderDraft,
    ProviderEditProfile, ProviderEndpointCandidate, ProviderEndpointTestResult,
    ProviderPreflightOutcome, ProviderPreflightStatus, ProviderRuntimeContext,
    ProviderRuntimeResourceOpenOutcome, ProviderTestResult, SkillCatalogItem, Tool, ToolId,
    ToolLaunchOutcome, ToolUninstallPreview, ToolUpdatePreview, ToolVersionCatalog,
    UninstallOptions,
};
use crate::infrastructure::OperationManager;
use crate::repositories::tool_version_events::SqliteToolVersionEventRepository;
use crate::store::AppState;

/// AI Manager product API: keep the command layer thin — only argument
/// validation and forwarding.
///
/// Local read only: whether it's installed, its version, whether it runs.
/// The latest version is checked separately by `app_tools_check_versions` so
/// this page doesn't have to wait on the network.
#[tauri::command]
pub async fn app_tools_list() -> Result<Vec<Tool>, AppError> {
    crate::application::tool_directory::list_local().await
}

/// Same list, but with the latest version looked up over the network; the
/// frontend overrides the local entry by id.
/// When the latest version can't be determined, `latestVersion` stays null —
/// that means "unknown", not "already up to date".
#[tauri::command]
pub async fn app_tools_check_versions() -> Result<Vec<Tool>, AppError> {
    crate::application::tool_directory::list_with_latest_versions().await
}

#[tauri::command]
pub async fn app_tools_update_preview(
    tools: Vec<String>,
) -> Result<Vec<ToolUpdatePreview>, AppError> {
    ToolUpdatePreviewService::system()
        .preview(parse_tools(tools)?)
        .await
}

#[tauri::command]
pub async fn app_operations_list(
    operations: State<'_, Arc<OperationManager>>,
) -> Result<Vec<Operation>, AppError> {
    Ok(list_operations(operations.inner()))
}

#[tauri::command]
pub async fn app_operation_cancel(
    operation_id: String,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<Operation, AppError> {
    let id = OperationId::parse(&operation_id)?;
    operations.request_cancel(&id)
}

#[tauri::command]
pub async fn app_tool_install(
    app_handle: tauri::AppHandle,
    tool: String,
    operations: State<'_, Arc<OperationManager>>,
    app_state: State<'_, AppState>,
) -> Result<OperationId, AppError> {
    let request = LifecycleRequest::Install(parse_tool(&tool)?);
    let settings = blocking(move || ProductSettingsService::load(&app_handle)).await?;
    start(
        system_lifecycle(
            operations.inner().clone(),
            settings.download_strategy,
            app_state.db.clone(),
        ),
        request,
    )
}

#[tauri::command]
pub async fn app_tool_update(
    app_handle: tauri::AppHandle,
    tool: String,
    preview_fingerprint: String,
    operations: State<'_, Arc<OperationManager>>,
    app_state: State<'_, AppState>,
) -> Result<OperationId, AppError> {
    let tool = parse_tool(&tool)?;
    let preview_fingerprint =
        crate::domain::validate_update_preview_fingerprint(&preview_fingerprint)?;
    let settings = blocking(move || ProductSettingsService::load(&app_handle)).await?;
    let service = system_lifecycle(
        operations.inner().clone(),
        settings.download_strategy,
        app_state.db.clone(),
    );
    // One click, one check: this step runs the local `--version` once and
    // asks the registry for the latest version once, then hands the
    // resulting plan as-is to the background task. On a restricted network,
    // failure to look it up is reported directly here rather than starting a
    // task first and letting the user watch it spin and fail.
    let authorized = service.authorize_update(tool, &preview_fingerprint).await?;
    start(
        service,
        LifecycleRequest::Update(tool, Box::new(authorized)),
    )
}

#[tauri::command]
pub async fn app_tool_version_catalog(
    app_handle: tauri::AppHandle,
    tool: String,
) -> Result<ToolVersionCatalog, AppError> {
    let tool = parse_tool(&tool)?;
    let settings_handle = app_handle.clone();
    let settings = blocking(move || ProductSettingsService::load(&settings_handle)).await?;
    ToolVersionDirectory::system(settings.download_strategy)
        .catalog(tool)
        .await
}

#[tauri::command]
pub async fn app_tool_install_version(
    app_handle: tauri::AppHandle,
    tool: String,
    version: String,
    operations: State<'_, Arc<OperationManager>>,
    app_state: State<'_, AppState>,
) -> Result<OperationId, AppError> {
    let tool = parse_tool(&tool)?;
    let version = crate::domain::validate_tool_version(&version)?;
    let request = LifecycleRequest::InstallVersion(tool, version);
    let settings = blocking(move || ProductSettingsService::load(&app_handle)).await?;
    start(
        system_lifecycle(
            operations.inner().clone(),
            settings.download_strategy,
            app_state.db.clone(),
        ),
        request,
    )
}

#[tauri::command]
pub async fn app_tool_repair(
    app_handle: tauri::AppHandle,
    tool: String,
    operations: State<'_, Arc<OperationManager>>,
    app_state: State<'_, AppState>,
) -> Result<OperationId, AppError> {
    let request = LifecycleRequest::Repair(parse_tool(&tool)?);
    let settings = blocking(move || ProductSettingsService::load(&app_handle)).await?;
    start(
        system_lifecycle(
            operations.inner().clone(),
            settings.download_strategy,
            app_state.db.clone(),
        ),
        request,
    )
}

#[tauri::command]
pub async fn app_tool_uninstall(
    tool: String,
    options: UninstallOptions,
    operations: State<'_, Arc<OperationManager>>,
    app_state: State<'_, AppState>,
) -> Result<OperationId, AppError> {
    let request = LifecycleRequest::Uninstall(parse_tool(&tool)?, options);
    start(
        system_lifecycle(
            operations.inner().clone(),
            crate::domain::DownloadStrategy::OfficialOnly,
            app_state.db.clone(),
        ),
        request,
    )
}

#[tauri::command]
pub async fn app_tool_uninstall_preview(tool: String) -> Result<ToolUninstallPreview, AppError> {
    ToolUninstallPreviewService::preview(parse_tool(&tool)?).await
}

#[tauri::command]
pub async fn app_tool_launch(
    app_handle: tauri::AppHandle,
    tool: String,
    directory_mode: String,
) -> Result<ToolLaunchOutcome, AppError> {
    let tool = parse_tool(&tool)?;
    let selected = match directory_mode.as_str() {
        "default" => {
            let home = crate::config::get_home_dir();
            if home.as_os_str().is_empty() {
                return Err(AppError::new(
                    ErrorCode::LaunchFailed,
                    "error.tool.projectFolderUnavailable",
                )
                .with_technical("default launch directory is unavailable")
                .with_remediation("error.remediation.chooseAnotherFolder"));
            }
            Some(home)
        }
        "choose" => {
            let app_handle = app_handle.clone();
            blocking(move || {
                let picked = app_handle.dialog().file().blocking_pick_folder();
                picked
                    .map(|path| {
                        path.simplified().into_path().map_err(|_| {
                            AppError::new(
                                ErrorCode::LaunchFailed,
                                "error.tool.projectFolderUnavailable",
                            )
                            .with_technical("folder picker returned a non-filesystem location")
                            .with_remediation("error.remediation.chooseAnotherFolder")
                        })
                    })
                    .transpose()
            })
            .await?
        }
        _ => {
            return Err(AppError::new(
                ErrorCode::LaunchFailed,
                "error.tool.projectFolderUnavailable",
            )
            .with_technical("invalid launch directory mode")
            .with_remediation("error.remediation.chooseAnotherFolder"))
        }
    };

    let Some(directory) = selected else {
        return Ok(ToolLaunchOutcome::Cancelled);
    };
    let settings = blocking(move || ProductSettingsService::load(&app_handle)).await?;
    ToolLaunchService::system()
        .launch(tool, directory, settings.terminal_app)
        .await?;
    Ok(ToolLaunchOutcome::Launched)
}

#[tauri::command]
pub async fn app_providers_list(
    app_handle: tauri::AppHandle,
    tool: String,
) -> Result<Vec<Provider>, AppError> {
    let tool = parse_tool(&tool)?;
    blocking(move || ProviderDirectory::list(&app_handle, tool)).await
}

#[tauri::command]
pub async fn app_provider_runtime_context(
    app_handle: tauri::AppHandle,
    tool: String,
) -> Result<ProviderRuntimeContext, AppError> {
    let tool = parse_tool(&tool)?;
    blocking(move || ProviderDirectory::runtime_context(&app_handle, tool)).await
}

#[tauri::command]
pub async fn app_provider_runtime_resource_open(
    app_handle: tauri::AppHandle,
    tool: String,
    resource: String,
) -> Result<ProviderRuntimeResourceOpenOutcome, AppError> {
    let tool = parse_tool(&tool)?;
    blocking(move || ProviderDirectory::open_runtime_resource(&app_handle, tool, resource.trim()))
        .await
}

#[tauri::command]
pub async fn app_provider_connection_profile(
    tool: String,
) -> Result<ProviderConnectionProfile, AppError> {
    ProviderDirectory::connection_profile(parse_tool(&tool)?)
}

#[tauri::command]
pub async fn app_provider_edit_profile(
    app_handle: tauri::AppHandle,
    tool: String,
    provider: String,
) -> Result<ProviderEditProfile, AppError> {
    let tool = parse_tool(&tool)?;
    blocking(move || ProviderDirectory::edit_profile(&app_handle, tool, &provider)).await
}

#[tauri::command]
pub async fn app_provider_create(
    app_handle: tauri::AppHandle,
    tool: String,
    request_id: String,
    draft: ProviderCreateDraft,
) -> Result<ProviderCreateResult, AppError> {
    let tool = parse_tool(&tool)?;
    let worker_handle = app_handle.clone();
    let result =
        blocking(move || ProviderDirectory::create(&worker_handle, tool, &request_id, &draft))
            .await?;
    crate::tray::provider_changed(&app_handle, tool);
    Ok(result)
}

#[tauri::command]
pub async fn app_provider_custom_create(
    app_handle: tauri::AppHandle,
    tool: String,
    request_id: String,
    draft: ProviderCustomCreateDraft,
) -> Result<ProviderCreateResult, AppError> {
    let tool = parse_tool(&tool)?;
    let worker_handle = app_handle.clone();
    let result = blocking(move || {
        ProviderDirectory::create_custom(&worker_handle, tool, &request_id, &draft)
    })
    .await?;
    crate::tray::provider_changed(&app_handle, tool);
    Ok(result)
}

#[tauri::command]
pub async fn app_provider_switch(
    app_handle: tauri::AppHandle,
    tool: String,
    provider: String,
) -> Result<Vec<Provider>, AppError> {
    let tool = parse_tool(&tool)?;
    let worker_handle = app_handle.clone();
    let result =
        blocking(move || ProviderDirectory::switch(&worker_handle, tool, &provider)).await?;
    crate::tray::provider_changed(&app_handle, tool);
    Ok(result)
}

#[tauri::command]
pub async fn app_provider_save(
    app_handle: tauri::AppHandle,
    tool: String,
    provider: String,
    draft: ProviderDraft,
) -> Result<Vec<Provider>, AppError> {
    let tool = parse_tool(&tool)?;
    let worker_handle = app_handle.clone();
    let result =
        blocking(move || ProviderDirectory::save(&worker_handle, tool, &provider, &draft)).await?;
    crate::tray::provider_changed(&app_handle, tool);
    Ok(result)
}

#[tauri::command]
pub async fn app_provider_remove(
    app_handle: tauri::AppHandle,
    tool: String,
    provider: String,
) -> Result<Vec<Provider>, AppError> {
    let tool = parse_tool(&tool)?;
    let worker_handle = app_handle.clone();
    let result =
        blocking(move || ProviderDirectory::remove(&worker_handle, tool, &provider)).await?;
    crate::tray::provider_changed(&app_handle, tool);
    Ok(result)
}

#[tauri::command]
pub async fn app_provider_test(
    app_handle: tauri::AppHandle,
    tool: String,
    provider: String,
) -> Result<ProviderTestResult, AppError> {
    let tool = parse_tool(&tool)?;
    // The probe itself is an async network call; the DB read is find_raw's
    // full SELECT by app_type + in-memory filtering (provider counts are
    // small), inlined identically to upstream's stream_check_provider. If the
    // provider table might grow large in the future, this read should also
    // move to spawn_blocking like the three write commands.
    // spawn_blocking。
    ProviderDirectory::test(&app_handle, tool, &provider).await
}

#[tauri::command]
pub async fn app_provider_test_all(
    app_handle: tauri::AppHandle,
    tool: String,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<OperationId, AppError> {
    let tool = parse_tool(&tool)?;
    let resolver_handle = app_handle.clone();
    let request =
        blocking(move || ProviderBatchTestService::resolve(&resolver_handle, tool)).await?;
    let service = ProviderBatchTestService::system(app_handle, operations.inner().clone());
    let id = service.begin(&request)?;
    let spawned = id.clone();
    tokio::spawn(async move { service.run(spawned, request).await });
    Ok(id)
}

#[tauri::command]
pub async fn app_provider_endpoints_test(
    tool: String,
    candidates: Vec<ProviderEndpointCandidate>,
) -> Result<Vec<ProviderEndpointTestResult>, AppError> {
    ProviderDirectory::test_endpoints(parse_tool(&tool)?, &candidates).await
}

#[tauri::command]
pub async fn app_provider_presets_test(
    tool: String,
) -> Result<Vec<ProviderEndpointTestResult>, AppError> {
    ProviderDirectory::test_presets(parse_tool(&tool)?).await
}

#[tauri::command]
pub async fn app_provider_activation_prepare(
    app_handle: tauri::AppHandle,
    tool: String,
    provider: String,
) -> Result<ProviderPreflightOutcome, AppError> {
    let tool = parse_tool(&tool)?;
    let notify = app_handle.clone();
    let outcome = ProviderPreflightService::prepare_activation(app_handle, tool, provider).await?;
    if outcome.status != ProviderPreflightStatus::Unreachable {
        crate::tray::provider_changed(&notify, tool);
    }
    Ok(outcome)
}

#[tauri::command]
pub async fn app_provider_launch_prepare(
    app_handle: tauri::AppHandle,
    tool: String,
) -> Result<ProviderPreflightOutcome, AppError> {
    let tool = parse_tool(&tool)?;
    let notify = app_handle.clone();
    let outcome = ProviderPreflightService::prepare_launch(app_handle, tool).await?;
    if outcome.status == ProviderPreflightStatus::FailedOver {
        crate::tray::provider_changed(&notify, tool);
    }
    Ok(outcome)
}

#[tauri::command]
pub async fn app_provider_next_healthy(
    app_handle: tauri::AppHandle,
    tool: String,
    failed_provider: String,
) -> Result<ProviderPreflightOutcome, AppError> {
    let tool = parse_tool(&tool)?;
    let notify = app_handle.clone();
    let outcome =
        ProviderPreflightService::try_next_healthy(app_handle, tool, failed_provider).await?;
    if outcome.status == ProviderPreflightStatus::FailedOver {
        crate::tray::provider_changed(&notify, tool);
    }
    Ok(outcome)
}

#[tauri::command]
pub async fn app_extensions_local_inventory(
    app_handle: tauri::AppHandle,
) -> Result<LocalExtensionInventory, AppError> {
    blocking(move || ExtensionDirectory::local_inventory(&app_handle)).await
}

#[tauri::command]
pub async fn app_extensions_list(
    app_handle: tauri::AppHandle,
    scope: ExtensionScope,
    kind: String,
) -> Result<Vec<Extension>, AppError> {
    let kind = parse_kind(&kind)?;
    blocking(move || ExtensionDirectory::list(&app_handle, scope, kind)).await
}

#[tauri::command]
pub async fn app_extension_set_enabled(
    app_handle: tauri::AppHandle,
    scope: ExtensionScope,
    kind: String,
    extension: String,
    enabled: bool,
) -> Result<Vec<Extension>, AppError> {
    let kind = parse_kind(&kind)?;
    blocking(move || ExtensionDirectory::set_enabled(&app_handle, scope, kind, &extension, enabled))
        .await
}

#[tauri::command]
pub async fn app_extension_location_reveal(
    app_handle: tauri::AppHandle,
    scope: ExtensionScope,
    kind: String,
) -> Result<(), AppError> {
    let kind = parse_kind(&kind)?;
    blocking(move || ExtensionDirectory::reveal_location(&app_handle, scope, kind)).await
}

#[tauri::command]
pub async fn app_detected_skill_resource_open(
    app_handle: tauri::AppHandle,
    scope: ExtensionScope,
    skill: String,
    action: DetectedSkillResourceAction,
) -> Result<DetectedSkillResourceOpenOutcome, AppError> {
    blocking(move || {
        ExtensionDirectory::open_detected_skill_resource(&app_handle, scope, &skill, action)
    })
    .await
}

#[tauri::command]
pub async fn app_extensions_adopt_detected(
    app_handle: tauri::AppHandle,
    scope: ExtensionScope,
    kind: String,
) -> Result<Vec<Extension>, AppError> {
    let kind = parse_kind(&kind)?;
    blocking(move || ExtensionDirectory::adopt_detected(&app_handle, scope, kind)).await
}

#[tauri::command]
pub async fn app_mcp_install(
    app_handle: tauri::AppHandle,
    scope: ExtensionScope,
    draft: McpInstallDraft,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<OperationId, AppError> {
    let request = McpInstallationService::prepare(scope, draft)?;
    start_mcp(
        McpInstallationService::system(app_handle, operations.inner().clone()),
        request,
    )
}

#[tauri::command]
pub async fn app_mcp_remove(
    app_handle: tauri::AppHandle,
    scope: ExtensionScope,
    mcp: String,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<OperationId, AppError> {
    let resolved_handle = app_handle.clone();
    let request =
        blocking(move || McpRemovalService::resolve(&resolved_handle, scope, &mcp)).await?;
    start_mcp_removal(
        McpRemovalService::system(app_handle, operations.inner().clone()),
        request,
    )
}

#[tauri::command]
pub async fn app_skill_catalog_list(
    app_handle: tauri::AppHandle,
    tool: String,
) -> Result<Vec<SkillCatalogItem>, AppError> {
    SkillInstallationService::catalog(&app_handle, parse_tool(&tool)?).await
}

#[tauri::command]
pub async fn app_skill_install(
    app_handle: tauri::AppHandle,
    tool: String,
    skill: SkillCatalogItem,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<OperationId, AppError> {
    let request = SkillInstallRequest {
        tool: parse_tool(&tool)?,
        item: skill,
    };
    start_skill(
        SkillInstallationService::system(app_handle, operations.inner().clone()),
        request,
    )
}

#[tauri::command]
pub async fn app_skill_remove(
    app_handle: tauri::AppHandle,
    tool: String,
    skill: String,
    operations: State<'_, Arc<OperationManager>>,
) -> Result<OperationId, AppError> {
    let tool = parse_tool(&tool)?;
    let resolved_handle = app_handle.clone();
    let request =
        blocking(move || SkillRemovalService::resolve(&resolved_handle, tool, &skill)).await?;
    start_skill_removal(
        SkillRemovalService::system(app_handle, operations.inner().clone()),
        request,
    )
}

#[tauri::command]
pub async fn app_prompt_get(
    app_handle: tauri::AppHandle,
    tool: String,
    prompt: String,
) -> Result<PromptDetail, AppError> {
    let tool = parse_tool(&tool)?;
    blocking(move || PromptDirectory::detail(&app_handle, tool, &prompt)).await
}

#[tauri::command]
pub async fn app_prompt_save(
    app_handle: tauri::AppHandle,
    tool: String,
    prompt: Option<String>,
    draft: PromptDraft,
) -> Result<Vec<Extension>, AppError> {
    let tool = parse_tool(&tool)?;
    blocking(move || PromptDirectory::save(&app_handle, tool, prompt.as_deref(), &draft)).await
}

#[tauri::command]
pub async fn app_prompt_remove(
    app_handle: tauri::AppHandle,
    tool: String,
    prompt: String,
) -> Result<Vec<Extension>, AppError> {
    let tool = parse_tool(&tool)?;
    blocking(move || PromptDirectory::remove(&app_handle, tool, &prompt)).await
}

#[tauri::command]
pub async fn app_prompt_import_current(
    app_handle: tauri::AppHandle,
    tool: String,
) -> Result<Vec<Extension>, AppError> {
    let tool = parse_tool(&tool)?;
    blocking(move || PromptDirectory::import_current(&app_handle, tool)).await
}

/// The upstream Provider engine is entirely synchronous, blocking SQLite +
/// file writes. Offload it to the blocking thread pool so it doesn't hog the
/// async runtime (upstream `switch_provider` does the same).
pub(super) async fn blocking<T, F>(work: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            AppError::new(ErrorCode::Internal, "error.command.joinFailed")
                .with_technical(error.to_string())
                .with_remediation("error.remediation.retryOrViewDetails")
        })?
}

/// Synchronously call `begin` first (acquire the per-tool lock, pass
/// capability checks), then hand execution off to the background.
/// The order matters: reversed, a second click would also get Ok, and the
/// conflict wouldn't surface until the background task ran (§41).
fn start(
    service: ToolLifecycleService,
    request: LifecycleRequest,
) -> Result<OperationId, AppError> {
    let id = service.begin(&request)?;
    let spawned = id.clone();
    tokio::spawn(async move { service.run(spawned, request).await });
    Ok(id)
}

fn system_lifecycle(
    operations: Arc<OperationManager>,
    strategy: crate::domain::DownloadStrategy,
    database: Arc<Database>,
) -> ToolLifecycleService {
    let history = ToolVersionHistoryService::new(Arc::new(SqliteToolVersionEventRepository::new(
        ToolVersionStore::new(database),
    )));
    ToolLifecycleService::system(operations, strategy, history)
}

fn start_skill(
    service: SkillInstallationService,
    request: SkillInstallRequest,
) -> Result<OperationId, AppError> {
    let id = service.begin(&request)?;
    let spawned = id.clone();
    tokio::spawn(async move { service.run(spawned, request).await });
    Ok(id)
}

fn start_mcp(
    service: McpInstallationService,
    request: McpInstallRequest,
) -> Result<OperationId, AppError> {
    let id = service.begin(&request)?;
    let spawned = id.clone();
    tokio::spawn(async move { service.run(spawned, request).await });
    Ok(id)
}

fn start_mcp_removal(
    service: McpRemovalService,
    request: McpRemovalRequest,
) -> Result<OperationId, AppError> {
    let id = service.begin(&request)?;
    let spawned = id.clone();
    tokio::spawn(async move { service.run(spawned, request).await });
    Ok(id)
}

fn start_skill_removal(
    service: SkillRemovalService,
    request: SkillRemovalRequest,
) -> Result<OperationId, AppError> {
    let id = service.begin(&request)?;
    let spawned = id.clone();
    tokio::spawn(async move { service.run(spawned, request).await });
    Ok(id)
}

pub(super) fn parse_tools(raw: Vec<String>) -> Result<Vec<ToolId>, AppError> {
    let mut seen = std::collections::HashSet::new();
    let tools = raw
        .into_iter()
        .map(|value| parse_tool(&value))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|tool| seen.insert(*tool))
        .collect::<Vec<_>>();
    if tools.is_empty() || tools.len() > ToolId::ALL.len() {
        return Err(AppError::new(
            ErrorCode::UpdateFailed,
            "error.tool.updatePreviewUnavailable",
        )
        .with_remediation("error.remediation.recheckUpdate"));
    }
    Ok(tools)
}

pub(super) fn parse_tool(raw: &str) -> Result<ToolId, AppError> {
    ToolId::from_str_id(raw).ok_or_else(|| {
        AppError::new(ErrorCode::ToolNotFound, "error.tool.notFound")
            .with_technical(raw.to_string())
    })
}

fn parse_kind(raw: &str) -> Result<ExtensionKind, AppError> {
    ExtensionKind::from_str_id(raw).ok_or_else(|| {
        AppError::new(ErrorCode::ExtensionNotFound, "error.extension.unknownKind")
            .with_technical(raw.to_string())
    })
}

fn list_operations(manager: &OperationManager) -> Vec<Operation> {
    manager.list()
}

// The test module is split into a sub-file: adding full tests here would
// break the 500-line limit (AI_RULES). The declaration must stay at the
// **end** of the file — `message_keys.rs`'s scanner stops at the first
// `#[cfg(test)]`, so putting it in the middle would hide the commands after
// it from the guard.
#[cfg(test)]
#[path = "app_api/tests.rs"]
mod tests;
