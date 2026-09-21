// Lightweight platform detection that avoids throwing in SSR or environments without navigator.
export const isMac = (): boolean => {
  try {
    const ua = navigator.userAgent || "";
    const plat = (navigator.platform || "").toLowerCase();
    return /mac/i.test(ua) || plat.includes("mac");
  } catch {
    return false;
  }
};

export const isWindows = (): boolean => {
  try {
    const ua = navigator.userAgent || "";
    return /windows|win32|win64/i.test(ua);
  } catch {
    return false;
  }
};

export const isLinux = (): boolean => {
  try {
    const ua = navigator.userAgent || "";
    // WebKitGTK/Chromium's UA on Linux/Wayland/X11 usually includes "Linux" or "X11".
    return (
      /linux|x11/i.test(ua) && !/android/i.test(ua) && !isMac() && !isWindows()
    );
  } catch {
    return false;
  }
};

// Disable all drag regions on Linux to avoid window-event glitches around
// gtk_window_begin_move_drag under Wayland (Tauri #13440). macOS keeps the original drag
// behavior; Windows never depended on this to begin with.
//
// These constants are designed to be consumed via JSX prop spread
// (`{...DRAG_REGION_ATTR}`), because `data-tauri-drag-region` is an attribute-presence
// check on the wry side, so the attribute must not be rendered at all to disable it —
// an empty string or "false" would still trigger it.
export const DRAG_REGION_ENABLED = !isLinux();

export const DRAG_REGION_ATTR: Record<string, unknown> = DRAG_REGION_ENABLED
  ? { "data-tauri-drag-region": true }
  : {};

export const DRAG_REGION_STYLE: Record<string, unknown> = DRAG_REGION_ENABLED
  ? { WebkitAppRegion: "drag" }
  : {};
