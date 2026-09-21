//! Thin product commands for desktop application inventory and launch.

use crate::application::desktop_app_directory::DesktopAppDirectory;
use crate::domain::{
    AppError, DesktopApp, DesktopAppId, DesktopAppInstallerHandoff, DesktopAppLaunchOutcome,
    DesktopAppOfficialDownloadOutcome, DesktopAppUninstallOutcome, ErrorCode,
};
use crate::platform::desktop_app::current_official_download_target;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn app_desktop_apps_list() -> Result<Vec<DesktopApp>, AppError> {
    DesktopAppDirectory::system().list().await
}

#[tauri::command]
pub async fn app_desktop_app_launch(app: String) -> Result<DesktopAppLaunchOutcome, AppError> {
    let id = parse_desktop_app(&app)?;
    DesktopAppDirectory::system().launch(id).await
}

#[tauri::command]
pub async fn app_desktop_app_open_official_download(
    handle: AppHandle,
    app: String,
) -> Result<DesktopAppOfficialDownloadOutcome, AppError> {
    let id = parse_desktop_app(&app)?;
    let target = current_official_download_target(id);
    if target.handoff == DesktopAppInstallerHandoff::Unsupported {
        return Err(AppError::new(
            ErrorCode::LaunchFailed,
            "error.desktopApp.officialDownloadUnsupported",
        )
        .with_remediation("error.remediation.installManually"));
    }
    handle
        .opener()
        .open_url(target.url, None::<String>)
        .map_err(|error| {
            AppError::new(
                ErrorCode::LaunchFailed,
                "error.desktopApp.officialDownloadFailed",
            )
            .with_technical(error.to_string())
            .with_remediation("error.remediation.installManually")
        })?;
    Ok(DesktopAppOfficialDownloadOutcome {
        handoff: target.handoff,
    })
}

#[tauri::command]
pub async fn app_desktop_app_open_uninstall(
    app: String,
) -> Result<DesktopAppUninstallOutcome, AppError> {
    let id = parse_desktop_app(&app)?;
    DesktopAppDirectory::system()
        .open_uninstall_handoff(id)
        .await
}

fn parse_desktop_app(value: &str) -> Result<DesktopAppId, AppError> {
    DesktopAppId::from_str_id(value).ok_or_else(|| {
        AppError::new(ErrorCode::ToolNotFound, "error.desktopApp.notFound")
            .with_technical(value.to_string())
            .with_remediation("error.remediation.installManually")
    })
}

#[cfg(test)]
mod tests {
    use super::parse_desktop_app;
    use crate::domain::{DesktopAppId, ErrorCode};

    #[test]
    fn desktop_app_parser_accepts_only_product_ids() {
        for id in DesktopAppId::ALL {
            assert_eq!(parse_desktop_app(id.as_str()).expect("known id"), id);
        }
        for raw in [
            "codex",
            "claude",
            "Codex.app",
            "",
            "/Applications/Codex.app",
        ] {
            let error = parse_desktop_app(raw).expect_err("not a product desktop app id");
            assert_eq!(error.code, ErrorCode::ToolNotFound);
            assert_eq!(error.message_key, "error.desktopApp.notFound");
        }
    }
}
