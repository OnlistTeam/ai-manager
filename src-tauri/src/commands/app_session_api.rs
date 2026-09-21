//! Thin session product commands.

use crate::application::session_directory::SessionDirectory;
use crate::domain::{AppError, SessionList, SessionThread};

use super::app_api::{blocking, parse_tool};

#[tauri::command]
pub async fn app_sessions_list(
    query: Option<String>,
    tool: Option<String>,
) -> Result<SessionList, AppError> {
    let tool = tool.as_deref().map(parse_tool).transpose()?;
    blocking(move || SessionDirectory::list(query.as_deref(), tool)).await
}

#[tauri::command]
pub async fn app_session_thread(reference: String) -> Result<SessionThread, AppError> {
    blocking(move || SessionDirectory::thread(&reference)).await
}

#[tauri::command]
pub async fn app_session_resume(
    app_handle: tauri::AppHandle,
    reference: String,
) -> Result<(), AppError> {
    SessionDirectory::system()
        .resume(app_handle, reference)
        .await
}

#[tauri::command]
pub async fn app_session_reveal(
    app_handle: tauri::AppHandle,
    reference: String,
) -> Result<(), AppError> {
    blocking(move || SessionDirectory::reveal(&app_handle, &reference)).await
}
