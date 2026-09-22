//! Use-case orchestration for AI services (spec §9).
//!
//! This layer does two things only: decide whether an action is allowed based on
//! `ToolCapabilities`, then hand the request to the compatibility layer. It **must not** contain
//! any upstream type — boundary rule R2 blocks that in CI.

use crate::compat::ccswitch::provider::{connection_profile_for, ProviderStore};
use crate::compat::ccswitch::provider_endpoints::{
    test_provider_endpoints, test_reviewed_provider_presets,
};
use crate::compat::ccswitch::provider_runtime::{resolve_resource, RuntimeResourceTarget};
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{
    AppError, ErrorCode, Provider, ProviderConnectionProfile, ProviderCreateDraft,
    ProviderCreateResult, ProviderCustomCreateDraft, ProviderDraft, ProviderEditProfile,
    ProviderEndpointCandidate, ProviderEndpointTestResult, ProviderRuntimeContext,
    ProviderRuntimeResourceAction, ProviderRuntimeResourceOpenOutcome, ProviderTestResult, ToolId,
};
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tauri_plugin_opener::OpenerExt;

fn unsupported(tool: ToolId) -> AppError {
    AppError::new(
        ErrorCode::ProviderNotFound,
        "error.provider.unsupportedTool",
    )
    .with_technical(format!("{} cannot manage services", tool.as_str()))
}

fn ensure_supported(tool: ToolId) -> Result<(), AppError> {
    if !capabilities_for(tool).can_manage_provider {
        return Err(unsupported(tool));
    }
    Ok(())
}

fn gate(app_handle: &tauri::AppHandle, tool: ToolId) -> Result<ProviderStore, AppError> {
    ensure_supported(tool)?;
    ProviderStore::open(app_handle)
}

pub struct ProviderDirectory;

impl ProviderDirectory {
    pub fn runtime_context(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
    ) -> Result<ProviderRuntimeContext, AppError> {
        ensure_supported(tool)?;
        crate::compat::ccswitch::provider_runtime::runtime_context(app_handle, tool)
    }

    pub fn open_runtime_resource(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        resource_id: &str,
    ) -> Result<ProviderRuntimeResourceOpenOutcome, AppError> {
        ensure_supported(tool)?;
        let resolved = resolve_resource(tool, resource_id)?;

        let (path, outcome) = match resolved.resource.action {
            ProviderRuntimeResourceAction::Edit => {
                if resolved.target != RuntimeResourceTarget::File {
                    return Err(resource_open_error(
                        "an editable runtime resource did not resolve to a file",
                    ));
                }
                ensure_editable_file(&resolved.path)?;
                (
                    resolved.path,
                    ProviderRuntimeResourceOpenOutcome::EditorOpened,
                )
            }
            ProviderRuntimeResourceAction::Browse => {
                let desired = match resolved.target {
                    RuntimeResourceTarget::File => resolved.path.parent().map(Path::to_path_buf),
                    RuntimeResourceTarget::Directory => Some(resolved.path),
                }
                .ok_or_else(|| resource_open_error("runtime resource has no parent directory"))?;
                (
                    nearest_existing_directory(desired).ok_or_else(|| {
                        resource_open_error("no existing folder contains the runtime resource")
                    })?,
                    ProviderRuntimeResourceOpenOutcome::FolderOpened,
                )
            }
        };

        app_handle
            .opener()
            .open_path(path.to_string_lossy().to_string(), None::<String>)
            .map_err(|error| resource_open_error(format!("system opener failed: {error}")))?;
        Ok(outcome)
    }

    pub fn connection_profile(tool: ToolId) -> Result<ProviderConnectionProfile, AppError> {
        ensure_supported(tool)?;
        connection_profile_for(tool)
    }

    /// Open the page where a reviewed preset's service hands out API keys.
    ///
    /// The renderer passes a preset id, never a URL. That is not decoration:
    /// tauri-plugin-opener injects a document-wide click handler that turns
    /// every `<a target="_blank">` into `plugin:opener|open_url`, so shipping
    /// the link as an anchor would mean granting `opener:allow-open-url` to the
    /// whole window — an IPC path any renderer code could use with any address.
    /// Going through a command keeps the address native-side, where the
    /// catalogue has already validated it.
    pub fn open_preset_key_page(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        preset_id: &str,
    ) -> Result<(), AppError> {
        ensure_supported(tool)?;
        let url = crate::compat::ccswitch::provider::preset_key_page(tool, preset_id)?;
        app_handle
            .opener()
            .open_url(url, None::<String>)
            .map_err(|error| {
                // Same sentence, same situation as the About panel's notices;
                // a second key would be the same copy under another name.
                AppError::new(ErrorCode::LaunchFailed, "error.about.openLinkFailed")
                    .with_technical(error.to_string())
                    .with_remediation("error.remediation.retryOrViewDetails")
            })
    }

    pub fn list(app_handle: &tauri::AppHandle, tool: ToolId) -> Result<Vec<Provider>, AppError> {
        gate(app_handle, tool)?.list(tool)
    }

    pub fn edit_profile(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<ProviderEditProfile, AppError> {
        gate(app_handle, tool)?.edit_profile(tool, id)
    }

    pub fn create(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        request_id: &str,
        draft: &ProviderCreateDraft,
    ) -> Result<ProviderCreateResult, AppError> {
        gate(app_handle, tool)?.create(tool, request_id, draft)
    }

    pub fn create_custom(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        request_id: &str,
        draft: &ProviderCustomCreateDraft,
    ) -> Result<ProviderCreateResult, AppError> {
        gate(app_handle, tool)?.create_custom(tool, request_id, draft)
    }

    pub fn switch(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<Vec<Provider>, AppError> {
        gate(app_handle, tool)?.switch(tool, id)
    }

    pub fn save(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
        draft: &ProviderDraft,
    ) -> Result<Vec<Provider>, AppError> {
        gate(app_handle, tool)?.save(tool, id, draft)
    }

    pub fn remove(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<Vec<Provider>, AppError> {
        gate(app_handle, tool)?.remove(tool, id)
    }

    pub async fn test(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<ProviderTestResult, AppError> {
        gate(app_handle, tool)?.test(tool, id).await
    }

    pub async fn test_for_preflight(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<ProviderTestResult, AppError> {
        gate(app_handle, tool)?.test_for_preflight(tool, id).await
    }

    pub async fn test_failover_candidates(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        ids: &[String],
    ) -> Result<Vec<ProviderTestResult>, AppError> {
        gate(app_handle, tool)?.test_for_failover(tool, ids).await
    }

    pub async fn test_endpoints(
        tool: ToolId,
        candidates: &[ProviderEndpointCandidate],
    ) -> Result<Vec<ProviderEndpointTestResult>, AppError> {
        ensure_supported(tool)?;
        test_provider_endpoints(candidates).await
    }

    pub async fn test_presets(tool: ToolId) -> Result<Vec<ProviderEndpointTestResult>, AppError> {
        ensure_supported(tool)?;
        test_reviewed_provider_presets(tool).await
    }
}

fn ensure_editable_file(path: &Path) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| resource_open_error("editable runtime resource has no parent"))?;
    std::fs::create_dir_all(parent).map_err(|error| {
        resource_open_error(format!(
            "failed to create runtime resource directory: {error}"
        ))
    })?;
    if path.exists() {
        if path.is_file() {
            return Ok(());
        }
        return Err(resource_open_error(
            "editable runtime resource exists but is not a file",
        ));
    }

    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists && path.is_file() => Ok(()),
        Err(error) => Err(resource_open_error(format!(
            "failed to create editable runtime resource: {error}"
        ))),
    }
}

fn nearest_existing_directory(mut path: PathBuf) -> Option<PathBuf> {
    loop {
        if path.is_dir() {
            return Some(path);
        }
        path = path.parent()?.to_path_buf();
    }
}

fn resource_open_error(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::LaunchFailed,
        "error.provider.runtimeResourceOpenFailed",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.checkPermissions")
}

#[cfg(test)]
mod tests {
    use crate::compat::ccswitch::tools::capabilities_for;
    use crate::domain::ToolId;

    #[test]
    fn the_gate_is_a_capability_and_not_a_list_of_tool_names() {
        for tool in ToolId::ALL {
            assert_eq!(
                super::ensure_supported(tool).is_ok(),
                capabilities_for(tool).can_manage_provider,
                "{tool:?} must be gated by its capability"
            );
        }
    }

    #[test]
    fn lifecycle_only_tools_do_not_enter_the_reviewed_provider_boundary() {
        assert!(super::ensure_supported(ToolId::KimiCode).is_err());
        assert!(super::ensure_supported(ToolId::DeepSeekDsh).is_err());
        assert_eq!(
            ToolId::ALL
                .into_iter()
                .filter(|tool| capabilities_for(*tool).can_manage_provider)
                .count(),
            8
        );
    }

    #[test]
    fn browsing_a_missing_resource_stops_at_the_nearest_existing_directory() {
        let existing = std::env::temp_dir();
        let requested = existing
            .join("ai-manager-resource-does-not-exist")
            .join("nested");
        assert_eq!(super::nearest_existing_directory(requested), Some(existing));
    }

    #[test]
    fn arbitrary_frontend_paths_are_not_runtime_resource_ids() {
        let error = crate::compat::ccswitch::provider_runtime::resolve_resource(
            ToolId::Codex,
            "/Users/someone/private.txt",
        )
        .expect_err("a path must never be accepted as a resource ID");
        assert_eq!(error.code, crate::domain::ErrorCode::ProviderNotFound);
        assert_eq!(error.message_key, "error.provider.runtimeResourceNotFound");
    }
}
