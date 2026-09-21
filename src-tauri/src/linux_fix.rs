//! Linux-specific main-window recovery patch.
//!
//! Works around a Tauri 2.x issue on some Linux distros (especially Wayland /
//! certain WebKitGTK versions) where the UI stops responding to clicks after
//! startup:
//!
//! - **Failure mode A** (Tauri #10746 / wry #637): the webview does not get
//!   keyboard focus after `show()`, so the first click is consumed by
//!   X11/Wayland as click-to-activate instead of being passed to the webview.
//! - **Failure mode B**: size negotiation between the GTK surface and the
//!   WebKitWebView's input region fails on the `visible:false` → `show()`
//!   path, so the whole window never responds to clicks until a fresh
//!   `size_allocate` (e.g. maximize-then-restore) happens.
//!
//! This module exports [`nudge_main_window`], which reproduces the
//! maximize-then-restore workaround precisely via an "explicit set_focus +
//! visually imperceptible ±1px fake resize", without the user noticing.
//! Every path that "brings the main window in front of the user" (normal
//! startup, deeplink activation, single_instance callback, tray show_main,
//! lightweight exit) should append a call to this right after the existing
//! `set_focus()`.

use std::time::Duration;

use tauri::{PhysicalSize, WebviewWindow};

/// Delay after the webview realizes, to let the GTK main loop finish
/// processing the realize event. 200ms is a community-tested value; too
/// short and set_focus still fails, too long and the first-interactive
/// time becomes visually perceptible.
const REALIZE_WAIT: Duration = Duration::from_millis(200);

/// Gap between the two steps of the ±1px fake resize, ensuring GTK processes
/// the first `size_allocate` before receiving the second resize. Widened to
/// 100ms because Tao's size API on Linux is asynchronous (it goes through
/// `gtk_window_resize` → compositor configure under the hood); too short and
/// the compositor coalesces the two consecutive resizes into one.
const RESIZE_GAP: Duration = Duration::from_millis(100);

/// Extra wait before reading back the size for reconciliation. The three
/// delays (200ms, 100ms, 500ms) add up to roughly 800ms before checking
/// whether the window size is back to its original value. That is long enough
/// for every compositor to drain its resize message queue.
const RECONCILE_WAIT: Duration = Duration::from_millis(500);

/// Runs the Linux-specific "focus + surface reactivation" sequence on the
/// main window.
///
/// This call is fire-and-forget: it spawns an async task internally that
/// completes after ~250ms. The calling thread returns immediately, not
/// blocking the UI.
pub(crate) fn nudge_main_window(window: WebviewWindow) {
    // First set_focus: the webview may not have realized yet, so this call
    // usually has no effect, but it's essentially free (thread-safe, runs via
    // run_on_main_thread internally), so we do it anyway.
    let _ = window.set_focus();

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(REALIZE_WAIT).await;

        // Second set_focus: the webview has realized by now, so on most
        // distros this call actually takes effect, eliminating failure mode A.
        let _ = window.set_focus();

        // Fake resize: read the current inner_size, bump it by 1px, then
        // restore it. This triggers GTK's size-allocate →
        // WebKitWebViewBase::size_allocate → re-attaching the input surface,
        // eliminating failure mode B.
        //
        // Use PhysicalSize to avoid logical-coordinate drift across DPIs;
        // saturating_add guards against overflow at extreme sizes.
        match window.inner_size() {
            Ok(original) => {
                let bumped = PhysicalSize::new(original.width.saturating_add(1), original.height);
                let _ = window.set_size(bumped);
                tokio::time::sleep(RESIZE_GAP).await;
                let _ = window.set_size(original);
                log::info!("Linux: ran focus + surface reactivation on the main window");

                // Reconciliation read-back: Tao's size API on Linux is asynchronous —
                // `set_size` just pushes a resize request into the GTK main loop queue,
                // and the compositor may coalesce two consecutive requests (especially
                // the second `set_size(original)`), leaving the window permanently at
                // width+1. Here we wait for the compositor to drain its queue, then read
                // the actual size once; if there's drift, we issue one more
                // `set_size(original)` as a fallback.
                //
                // Known limitation: tiling Wayland compositors (sway/river/hyprland)
                // ignore `set_size` entirely, so the reconciliation always sees drift=0
                // (both set_size calls are no-ops), which looks "fine" but failure mode B
                // is actually not fixed; this is a known limitation that requires the user
                // to work around with GDK_BACKEND=x11 — the README should mention it.
                tokio::time::sleep(RECONCILE_WAIT).await;
                match window.inner_size() {
                    Ok(after) => {
                        if after.width != original.width || after.height != original.height {
                            log::info!(
                                "Linux nudge size drift: expected={}x{}, got={}x{}, compensated",
                                original.width,
                                original.height,
                                after.width,
                                after.height
                            );
                            let _ = window.set_size(original);
                            // Final check: if it's still inconsistent after compensating,
                            // log a warning so the user/developer knows reconciliation
                            // failed. The window will be stuck at an unexpected size
                            // (usually +1px) — this is an extreme fallback scenario.
                            if let Ok(final_size) = window.inner_size() {
                                if final_size.width != original.width
                                    || final_size.height != original.height
                                {
                                    log::warn!(
                                        "Linux nudge size drift still inconsistent after compensation: expected={}x{}, got={}x{}",
                                        original.width,
                                        original.height,
                                        final_size.width,
                                        final_size.height
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Linux nudge: failed to read back inner_size for reconciliation: {e}"
                        );
                    }
                }
            }
            Err(e) => {
                // Extremely rare failure path; having done just set_focus is still
                // better than nothing — don't let a resize failure swallow the whole patch.
                log::warn!("Linux nudge: failed to read inner_size, skipping fake resize: {e}");
            }
        }
    });
}
