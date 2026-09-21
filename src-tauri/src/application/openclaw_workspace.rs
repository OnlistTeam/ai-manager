//! OpenClaw workspace use cases (ADR-0014).

use tauri_plugin_opener::OpenerExt;

use crate::compat::ccswitch::workspace::WorkspaceStore;
use crate::domain::{
    AppError, ErrorCode, OpenClawDailyMemoryDocument, OpenClawDailyMemoryList,
    OpenClawWorkspaceDirectory, OpenClawWorkspaceDocument, OpenClawWorkspaceFileId,
    OpenClawWorkspaceOverview, OpenClawWorkspaceWriteOutcome,
};

pub struct OpenClawWorkspaceService;

impl OpenClawWorkspaceService {
    pub fn overview() -> Result<OpenClawWorkspaceOverview, AppError> {
        WorkspaceStore::open().overview()
    }

    pub fn document(file: OpenClawWorkspaceFileId) -> Result<OpenClawWorkspaceDocument, AppError> {
        WorkspaceStore::open().document(file)
    }

    pub fn save_document(
        file: OpenClawWorkspaceFileId,
        content: &str,
    ) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
        WorkspaceStore::open().save_document(file, content)
    }

    pub fn memories(query: Option<&str>) -> Result<OpenClawDailyMemoryList, AppError> {
        WorkspaceStore::open().memories(query)
    }

    pub fn memory_document(date: &str) -> Result<OpenClawDailyMemoryDocument, AppError> {
        WorkspaceStore::open().memory_document(date)
    }

    pub fn save_memory(
        date: &str,
        content: &str,
    ) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
        WorkspaceStore::open().save_memory(date, content)
    }

    pub fn delete_memory(date: &str) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
        WorkspaceStore::open().delete_memory(date)
    }

    pub fn open_directory(
        app_handle: &tauri::AppHandle,
        directory: OpenClawWorkspaceDirectory,
    ) -> Result<(), AppError> {
        let path = WorkspaceStore::open().directory(directory)?;
        app_handle
            .opener()
            .open_path(path.to_string_lossy().to_string(), None::<String>)
            .map_err(|error| {
                log::warn!("The system file manager refused an OpenClaw workspace target: {error}");
                AppError::new(
                    ErrorCode::LaunchFailed,
                    "error.openclawWorkspace.directoryOpenFailed",
                )
                .with_technical("the system file manager rejected the fixed workspace target")
                .with_remediation("error.remediation.retryOrViewDetails")
            })?;
        Ok(())
    }
}
