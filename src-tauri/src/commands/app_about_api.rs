//! Thin product commands for the About panel's Appropriate Legal Notices.
//!
//! AGPL-3.0 §5 requires an interactive program to tell the user where the
//! corresponding source lives and which licence applies. Both destinations are
//! fixed here rather than passed in from the renderer: the UI names *which*
//! notice to open, the native side owns the address, so a compromised renderer
//! cannot turn the legal notice into an arbitrary URL launcher.

use crate::domain::{AppError, ErrorCode};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Where the corresponding source for this binary is published (AGPL-3.0 §5).
pub const SOURCE_CODE_URL: &str = "https://github.com/OnlistTeam/ai-manager";

/// The canonical licence text. The UI links to it instead of embedding it.
pub const LICENSE_URL: &str = "https://www.gnu.org/licenses/agpl-3.0.html";

#[tauri::command]
pub async fn app_about_open_source_code(app_handle: AppHandle) -> Result<bool, AppError> {
    open_legal_notice_url(&app_handle, SOURCE_CODE_URL)
}

#[tauri::command]
pub async fn app_about_open_license(app_handle: AppHandle) -> Result<bool, AppError> {
    open_legal_notice_url(&app_handle, LICENSE_URL)
}

fn open_legal_notice_url(app_handle: &AppHandle, url: &'static str) -> Result<bool, AppError> {
    app_handle
        .opener()
        .open_url(url, None::<String>)
        .map_err(|error| {
            AppError::new(ErrorCode::LaunchFailed, "error.about.openLinkFailed")
                .with_technical(error.to_string())
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{LICENSE_URL, SOURCE_CODE_URL};

    #[test]
    fn the_legal_notice_targets_are_fixed_https_destinations() {
        assert_eq!(SOURCE_CODE_URL, "https://github.com/OnlistTeam/ai-manager");
        assert_eq!(LICENSE_URL, "https://www.gnu.org/licenses/agpl-3.0.html");
        for url in [SOURCE_CODE_URL, LICENSE_URL] {
            assert!(
                url.starts_with("https://"),
                "a legal notice link must not be downgraded to plain http: {url}"
            );
        }
    }
}
