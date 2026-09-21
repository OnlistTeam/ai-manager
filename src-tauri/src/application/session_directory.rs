//! Session browsing and safe terminal resume orchestration.

use std::sync::Arc;

use crate::compat::ccswitch::session::SessionStore;
use crate::domain::{AppError, ErrorCode, SessionList, SessionThread, ToolId};
use crate::platform::{SystemTerminalLauncher, TerminalLauncher};

use super::product_settings::ProductSettingsService;
use super::reveal::open_in_file_manager;

pub struct SessionDirectory {
    launcher: Arc<dyn TerminalLauncher>,
}

impl SessionDirectory {
    pub fn system() -> Self {
        Self {
            launcher: Arc::new(SystemTerminalLauncher),
        }
    }

    pub fn list(query: Option<&str>, tool: Option<ToolId>) -> Result<SessionList, AppError> {
        SessionStore::list(query, tool)
    }

    pub fn thread(reference: &str) -> Result<SessionThread, AppError> {
        SessionStore::thread(reference)
    }

    /// Opens the folder containing a session's source file in the system
    /// file manager. The path never leaves this function: it is resolved
    /// from the opaque `reference` and handed straight to the opener, and
    /// any failure is reported back without the resolved path attached
    /// (`AppError` is `Serialize` and would otherwise carry it across the
    /// native boundary, defeating the reference's opacity).
    pub fn reveal(app_handle: &tauri::AppHandle, reference: &str) -> Result<(), AppError> {
        let target = SessionStore::reveal_target(reference)?;
        open_in_file_manager(app_handle, &target).map_err(|technical| {
            log::warn!("Session reveal failed for reference={reference}: {technical}");
            reveal_failed_error()
        })
    }

    pub async fn resume(
        &self,
        app_handle: tauri::AppHandle,
        reference: String,
    ) -> Result<(), AppError> {
        let (target, terminal_app) = tokio::task::spawn_blocking(move || {
            let settings = ProductSettingsService::load(&app_handle)?;
            Ok::<_, AppError>((
                SessionStore::resume_target(&reference)?,
                settings.terminal_app,
            ))
        })
        .await
        .map_err(|error| {
            AppError::new(ErrorCode::Internal, "error.command.joinFailed")
                .with_technical(error.to_string())
                .with_remediation("error.remediation.retryOrViewDetails")
        })??;

        let mut launch = crate::compat::ccswitch::tool_launch::launch_spec(
            target.tool,
            target.working_directory,
        )
        .await?
        .with_terminal_app(terminal_app);
        launch.command.args = target.args;
        launch.validate()?;
        self.launcher.launch(launch).await
    }
}

/// The error returned to the renderer when opening a session's folder fails.
/// Deliberately carries no `technical_message`: the resolved path lives only
/// in the `log::warn!` call at the call site, never in this `Serialize`d
/// value that crosses the native boundary.
fn reveal_failed_error() -> AppError {
    AppError::new(ErrorCode::LaunchFailed, "error.system.revealFailed")
        .with_remediation("error.remediation.checkPermissions")
}

#[cfg(test)]
mod tests {
    use super::reveal_failed_error;
    use crate::domain::ErrorCode;

    #[test]
    fn reveal_failure_error_carries_no_technical_detail_or_path() {
        let error = reveal_failed_error();
        assert_eq!(error.code, ErrorCode::LaunchFailed);
        assert_eq!(error.message_key, "error.system.revealFailed");
        assert!(error.technical_message.is_none());

        let encoded = serde_json::to_string(&error).expect("serialize AppError");
        for forbidden in ["/Users/", "sourcePath", ".jsonl", "\\Users\\"] {
            assert!(
                !encoded.contains(forbidden),
                "reveal error leaked {forbidden}"
            );
        }
    }
}
