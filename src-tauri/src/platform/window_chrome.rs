//! Window chrome the operating system draws for us.
//!
//! The product canvas is one saturated colour and the application has no
//! light/dark pair. macOS handles this through `titleBarStyle: "Overlay"`: the
//! webview reaches the top of the window and paints the canvas behind the
//! traffic lights. Windows has no equivalent — the caption is drawn by DWM, not
//! by the webview — so the colour has to be handed to DWM directly. Without
//! this the caption follows the dark-mode system colour, a near-black bar that
//! shares no hue with the canvas under it.

/// `--bg-primary`, the base of the product canvas, as a Win32 `COLORREF`.
/// `COLORREF` is `0x00BBGGRR`, so `#2a2740` reverses to `0x0040272a`.
#[cfg(target_os = "windows")]
const CAPTION_COLOR: u32 = 0x0040_272a;

/// `--text-primary`, `#f6f5fb`, reversed the same way.
#[cfg(target_os = "windows")]
const CAPTION_TEXT_COLOR: u32 = 0x00fb_f5f6;

/// Paint the system window chrome in the product's own colours.
///
/// Windows 11 build 22000 introduced these attributes. Earlier builds reject
/// them and keep the default caption, which is the behaviour this replaces, so
/// a failure needs no handling beyond not propagating it.
#[cfg(target_os = "windows")]
pub fn apply_product_chrome(window: &tauri::WebviewWindow) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    };

    let Ok(handle) = window.hwnd() else {
        log::debug!("no native window handle yet; leaving the system caption colour alone");
        return;
    };

    // The border takes the caption colour rather than a lighter outline: a
    // contrasting edge would draw a second rectangle around a window whose
    // whole design is one continuous field.
    for (attribute, value) in [
        (DWMWA_CAPTION_COLOR, CAPTION_COLOR),
        (DWMWA_BORDER_COLOR, CAPTION_COLOR),
        (DWMWA_TEXT_COLOR, CAPTION_TEXT_COLOR),
    ] {
        // SAFETY: `handle` is a live window handle owned by the caller for the
        // duration of the call, and the attribute is a `u32` matching the size
        // passed. Every colour attribute here takes a `COLORREF`.
        let result = unsafe {
            DwmSetWindowAttribute(
                handle.0 as _,
                attribute as u32,
                std::ptr::addr_of!(value).cast(),
                std::mem::size_of::<u32>() as u32,
            )
        };
        if result < 0 {
            log::debug!(
                "this Windows build does not accept DWM attribute {attribute}; keeping its default caption"
            );
            return;
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_product_chrome(_window: &tauri::WebviewWindow) {}
