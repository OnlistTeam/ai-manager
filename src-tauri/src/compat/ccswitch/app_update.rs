//! Compatibility facade for the inherited native exit/restart lifecycle.
//!
//! Product update orchestration stays in `application`; these functions are
//! the only bridge to shell cleanup still owned by the inherited crate root.

#[cfg(target_os = "windows")]
pub async fn prepare_for_update_install(app: &tauri::AppHandle) {
    crate::save_window_state_before_exit(app);
    crate::cleanup_before_exit(app).await;
    crate::remove_tray_icon_before_exit(app);
    crate::destroy_single_instance_lock(app);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

#[cfg(not(target_os = "windows"))]
pub async fn restart_after_update_install(app: &tauri::AppHandle) -> ! {
    crate::save_window_state_before_exit(app);
    crate::cleanup_before_exit(app).await;
    log::info!("signed product update installed; restarting AI Manager");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    crate::restart_process(app);
}
