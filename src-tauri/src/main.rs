// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Set WebKit env vars on Linux to work around DMA-BUF rendering issues.
    // On some Linux systems (e.g. Debian 13.2, Nvidia GPUs), WebKitGTK's DMA-BUF
    // renderer can cause a white/black screen.
    // Reference: https://github.com/tauri-apps/tauri/issues/9394
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        // Disable WebKitGTK compositing mode to avoid webview crashes on resize
        // and surface-negotiation issues under some Wayland compositors (the whole
        // window becomes unresponsive to clicks until maximize/restore).
        // Reference: https://github.com/tauri-apps/tauri/issues/9394
        if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }

        // AppImage's GTK launch hook (linuxdeploy-plugin-gtk.sh) unconditionally
        // does `export GDK_BACKEND=x11` to force XWayland, working around a historical
        // Wayland crash (tauri-apps/tauri#8541). But on newer Wayland + NVIDIA setups,
        // forcing XWayland instead makes WebKitGTK's webview stop receiving pointer
        // events (the title bar is clickable but page content isn't) and causes a
        // black screen after resize; switching back to native Wayland fixes it, and
        // that crash no longer reproduces on WebKitGTK 2.52.
        // Since the hook overrides any user-set GDK_BACKEND, this provides an escape
        // hatch the hook won't touch: setting AI_MANAGER_GDK_BACKEND=wayland forces
        // an override, while the default behavior stays unchanged (zero regression).
        if let Ok(backend) = std::env::var("AI_MANAGER_GDK_BACKEND") {
            if !backend.is_empty() {
                std::env::set_var("GDK_BACKEND", backend);
            }
        }
    }

    ai_manager_lib::run();
}
