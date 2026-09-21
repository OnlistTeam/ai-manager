use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Open the product data directory in the system file manager.
#[tauri::command]
pub async fn open_app_config_folder(handle: AppHandle) -> Result<bool, String> {
    let config_dir = crate::infrastructure::paths::product_data_dir();

    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| format!("Failed to create directory: {e}"))?;
    }

    handle
        .opener()
        .open_path(config_dir.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| format!("Failed to open folder: {e}"))?;

    Ok(true)
}
