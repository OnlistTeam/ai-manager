//! Shared "show this in the file manager" sequence: reveal the target if it
//! still exists, otherwise open the nearest existing ancestor folder. Used by
//! both the raw-path reveal command and the reference-based session reveal so
//! the fallback rule lives in exactly one place.

use std::path::{Path, PathBuf};

use tauri_plugin_opener::OpenerExt;

pub fn open_in_file_manager(app_handle: &tauri::AppHandle, target: &Path) -> Result<(), String> {
    if target.exists() {
        return app_handle
            .opener()
            .reveal_item_in_dir(target)
            .map_err(|error| format!("system opener failed: {error}"));
    }

    let folder = nearest_existing_ancestor(target)
        .ok_or_else(|| "no existing folder contains the path".to_string())?;
    app_handle
        .opener()
        .open_path(folder.to_string_lossy().to_string(), None::<String>)
        .map_err(|error| format!("system opener failed: {error}"))
}

fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .skip(1)
        .find(|candidate| candidate.is_dir())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_files_fall_back_without_creating_directories() {
        let home = tempfile::TempDir::new().expect("temporary home");
        let missing = home.path().join("tool").join("config.json");
        assert_eq!(
            super::nearest_existing_ancestor(&missing).as_deref(),
            Some(home.path())
        );
        assert!(!missing.parent().expect("parent").exists());
    }
}
