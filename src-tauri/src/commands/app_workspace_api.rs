//! Thin OpenClaw workspace product commands.

use crate::application::openclaw_workspace::OpenClawWorkspaceService;
use crate::domain::{
    AppError, ErrorCode, OpenClawDailyMemoryDocument, OpenClawDailyMemoryList,
    OpenClawWorkspaceDirectory, OpenClawWorkspaceDocument, OpenClawWorkspaceFileId,
    OpenClawWorkspaceOverview, OpenClawWorkspaceWriteOutcome,
};

use super::app_api::blocking;

#[tauri::command]
pub async fn app_openclaw_workspace_overview() -> Result<OpenClawWorkspaceOverview, AppError> {
    blocking(OpenClawWorkspaceService::overview).await
}

#[tauri::command]
pub async fn app_openclaw_workspace_document(
    file: String,
) -> Result<OpenClawWorkspaceDocument, AppError> {
    let file = parse_file(&file)?;
    blocking(move || OpenClawWorkspaceService::document(file)).await
}

#[tauri::command]
pub async fn app_openclaw_workspace_save_document(
    file: String,
    content: String,
) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
    let file = parse_file(&file)?;
    blocking(move || OpenClawWorkspaceService::save_document(file, &content)).await
}

#[tauri::command]
pub async fn app_openclaw_daily_memories(
    query: Option<String>,
) -> Result<OpenClawDailyMemoryList, AppError> {
    blocking(move || OpenClawWorkspaceService::memories(query.as_deref())).await
}

#[tauri::command]
pub async fn app_openclaw_daily_memory(
    date: String,
) -> Result<OpenClawDailyMemoryDocument, AppError> {
    blocking(move || OpenClawWorkspaceService::memory_document(&date)).await
}

#[tauri::command]
pub async fn app_openclaw_daily_memory_save(
    date: String,
    content: String,
) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
    blocking(move || OpenClawWorkspaceService::save_memory(&date, &content)).await
}

#[tauri::command]
pub async fn app_openclaw_daily_memory_delete(
    date: String,
) -> Result<OpenClawWorkspaceWriteOutcome, AppError> {
    blocking(move || OpenClawWorkspaceService::delete_memory(&date)).await
}

#[tauri::command]
pub async fn app_openclaw_workspace_open_directory(
    app_handle: tauri::AppHandle,
    directory: String,
) -> Result<(), AppError> {
    let directory = parse_directory(&directory)?;
    blocking(move || OpenClawWorkspaceService::open_directory(&app_handle, directory)).await
}

fn parse_file(raw: &str) -> Result<OpenClawWorkspaceFileId, AppError> {
    match raw {
        "agents" => Ok(OpenClawWorkspaceFileId::Agents),
        "soul" => Ok(OpenClawWorkspaceFileId::Soul),
        "user" => Ok(OpenClawWorkspaceFileId::User),
        "identity" => Ok(OpenClawWorkspaceFileId::Identity),
        "tools" => Ok(OpenClawWorkspaceFileId::Tools),
        "memory" => Ok(OpenClawWorkspaceFileId::Memory),
        "heartbeat" => Ok(OpenClawWorkspaceFileId::Heartbeat),
        "bootstrap" => Ok(OpenClawWorkspaceFileId::Bootstrap),
        "boot" => Ok(OpenClawWorkspaceFileId::Boot),
        _ => Err(invalid_target("unknown workspace file id")),
    }
}

fn parse_directory(raw: &str) -> Result<OpenClawWorkspaceDirectory, AppError> {
    match raw {
        "workspace" => Ok(OpenClawWorkspaceDirectory::Workspace),
        "daily-memory" => Ok(OpenClawWorkspaceDirectory::DailyMemory),
        _ => Err(invalid_target("unknown workspace directory id")),
    }
}

fn invalid_target(technical: &'static str) -> AppError {
    AppError::new(
        ErrorCode::ConfigParseFailed,
        "error.openclawWorkspace.invalidTarget",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
mod tests {
    use super::{parse_directory, parse_file};
    use crate::domain::{OpenClawWorkspaceDirectory, OpenClawWorkspaceFileId};

    #[test]
    fn product_targets_are_closed_enums_instead_of_paths() {
        assert_eq!(
            parse_file("agents").expect("file"),
            OpenClawWorkspaceFileId::Agents
        );
        assert_eq!(
            parse_directory("daily-memory").expect("directory"),
            OpenClawWorkspaceDirectory::DailyMemory
        );
        for raw in ["../AGENTS.md", "/tmp", "memory/2026-08-26.md", ""] {
            assert_eq!(
                parse_file(raw)
                    .expect_err("raw target rejected")
                    .message_key,
                "error.openclawWorkspace.invalidTarget"
            );
            assert_eq!(
                parse_directory(raw)
                    .expect_err("raw directory rejected")
                    .message_key,
                "error.openclawWorkspace.invalidTarget"
            );
        }
    }
}
