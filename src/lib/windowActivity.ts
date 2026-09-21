import { isTauri } from "@tauri-apps/api/core";

const HEARTBEAT_INTERVAL_MS = 3000;
const HEARTBEAT_DIM_MS = 300;

let initialized = false;
let heartbeatInterval: number | undefined;
let heartbeatReset: number | undefined;
let currentWindowActive: boolean | undefined;

function stopHeartbeat() {
  if (heartbeatInterval !== undefined) {
    window.clearInterval(heartbeatInterval);
    heartbeatInterval = undefined;
  }
  if (heartbeatReset !== undefined) {
    window.clearTimeout(heartbeatReset);
    heartbeatReset = undefined;
  }
  delete document.documentElement.dataset.statusHeartbeat;
}

function startHeartbeat() {
  stopHeartbeat();
  heartbeatInterval = window.setInterval(() => {
    document.documentElement.dataset.statusHeartbeat = "true";
    heartbeatReset = window.setTimeout(() => {
      delete document.documentElement.dataset.statusHeartbeat;
      heartbeatReset = undefined;
    }, HEARTBEAT_DIM_MS);
  }, HEARTBEAT_INTERVAL_MS);
}

function setWindowActive(active: boolean) {
  if (currentWindowActive === active) return;
  currentWindowActive = active;
  document.documentElement.dataset.windowActive = String(active);

  if (active) {
    startHeartbeat();
  } else {
    stopHeartbeat();
  }
}

export function initializeWindowActivity() {
  if (initialized) return;
  initialized = true;

  const syncBrowserActivity = () => {
    setWindowActive(
      document.visibilityState === "visible" && document.hasFocus(),
    );
  };

  syncBrowserActivity();

  // Browser focus events are a fallback for non-Tauri renderer tests and dev mode.
  window.addEventListener("focus", syncBrowserActivity);
  window.addEventListener("blur", () => setWindowActive(false));
  document.addEventListener("visibilitychange", syncBrowserActivity);

  if (isTauri()) {
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) =>
        getCurrentWindow().onFocusChanged(({ payload }) =>
          setWindowActive(payload && document.visibilityState === "visible"),
        ),
      )
      .catch((error) => {
        console.error("Failed to observe window focus changes", error);
      });
  }
}
