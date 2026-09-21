//! The deep-link import IPC surface (ADR-0029).
//!
//! Every command here is thin: it validates its argument, calls the one
//! application service, and returns. The queue lives in native memory, so the
//! renderer only ever sends an opaque pending identity back.

use std::sync::Arc;

use tauri::State;

use crate::application::deep_link_import::{DeepLinkImportService, DeepLinkQueue};
use crate::domain::{
    AppError, DeepLinkImportOutcome, DeepLinkPreview, DeepLinkResource, ErrorCode, LinkOrigin,
};
use crate::infrastructure::OperationManager;

/// The renderer is told only that the queue changed; it reads the safe preview
/// through the commands below.
pub const DEEP_LINK_PENDING_EVENT: &str = "deeplink://pending";

#[tauri::command]
pub async fn app_deeplink_pending_list(
    queue: State<'_, Arc<DeepLinkQueue>>,
) -> Result<Vec<DeepLinkPreview>, AppError> {
    Ok(DeepLinkImportService::list(queue.inner()))
}

#[tauri::command]
pub async fn app_deeplink_preview(
    queue: State<'_, Arc<DeepLinkQueue>>,
    pending: String,
) -> Result<DeepLinkPreview, AppError> {
    DeepLinkImportService::preview(queue.inner(), &pending)
}

/// The paste path. It accepts the full format, including a credential and the
/// upstream scheme, because a paste never travels through argv, a process list
/// or a system log (ADR-0029 decision 4).
#[tauri::command]
pub async fn app_deeplink_submit_pasted(
    queue: State<'_, Arc<DeepLinkQueue>>,
    link: String,
) -> Result<DeepLinkPreview, AppError> {
    if link.len() > crate::domain::deep_link::MAX_DEEP_LINK_BYTES {
        return Err(
            AppError::new(ErrorCode::ConfigParseFailed, "error.deepLink.tooLarge")
                .with_technical("the pasted link exceeds the accepted size")
                .with_remediation("error.remediation.retryOrViewDetails"),
        );
    }
    DeepLinkImportService::submit(queue.inner(), &link, LinkOrigin::Paste)
}

#[tauri::command]
pub async fn app_deeplink_dismiss(
    queue: State<'_, Arc<DeepLinkQueue>>,
    pending: String,
) -> Result<(), AppError> {
    DeepLinkImportService::dismiss(queue.inner(), &pending)
}

#[tauri::command]
pub async fn app_deeplink_confirm(
    app_handle: tauri::AppHandle,
    queue: State<'_, Arc<DeepLinkQueue>>,
    operations: State<'_, Arc<OperationManager>>,
    pending: String,
) -> Result<DeepLinkImportOutcome, AppError> {
    let queue = queue.inner().clone();
    let outcome = DeepLinkImportService::confirm(
        app_handle.clone(),
        &queue,
        operations.inner().clone(),
        &pending,
    )
    .await?;

    // A new service changes what the tray offers, exactly as the endpoint
    // editor's own create command does.
    if outcome.resource == DeepLinkResource::Provider {
        for tool in &outcome.tools {
            crate::tray::provider_changed(&app_handle, *tool);
        }
    }
    Ok(outcome)
}
