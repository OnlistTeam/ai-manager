import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * The three window controls a self-drawn title bar has to provide.
 *
 * On Windows and Linux the product draws its own title bar so the window has
 * one continuous surface instead of a system bar sitting above the gradient
 * (ADR-0044). That removes the system's buttons, so these take their place.
 *
 * macOS keeps its traffic lights — `titleBarStyle: "Overlay"` already draws
 * them inside the client area — and never calls any of this.
 */
export const windowControls = {
  minimize(): Promise<void> {
    return getCurrentWindow().minimize();
  },

  /** Maximize or restore, matching what a double-click on the bar does. */
  toggleMaximize(): Promise<void> {
    return getCurrentWindow().toggleMaximize();
  },

  close(): Promise<void> {
    return getCurrentWindow().close();
  },

  isMaximized(): Promise<boolean> {
    return getCurrentWindow().isMaximized();
  },

  /**
   * Fires whenever the window is resized, which is the only signal that the
   * maximized state may have changed. Returns its own unsubscribe.
   */
  onResized(handler: () => void): Promise<() => void> {
    return getCurrentWindow().onResized(() => handler());
  },
};
