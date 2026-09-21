//! Import-an-existing-setup use case (spec §17 / §58).

use crate::compat::ccswitch::import::ImportStore;
use crate::domain::{AppError, ImportOutcome, ImportPreview};

pub struct ImportExisting;

impl ImportExisting {
    pub fn preview(app_handle: &tauri::AppHandle) -> Result<ImportPreview, AppError> {
        ImportStore::open(app_handle)?.preview()
    }

    pub async fn run(app_handle: &tauri::AppHandle) -> Result<ImportOutcome, AppError> {
        ImportStore::open(app_handle)?.run().await
    }
}
