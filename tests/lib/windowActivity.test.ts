import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => false,
}));

const originalHasFocus = document.hasFocus;
const originalVisibilityState = document.visibilityState;

describe("window activity", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.useFakeTimers();
    delete document.documentElement.dataset.windowActive;
    delete document.documentElement.dataset.statusHeartbeat;
    Object.defineProperty(document, "hasFocus", {
      configurable: true,
      value: () => true,
    });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
    Object.defineProperty(document, "hasFocus", {
      configurable: true,
      value: originalHasFocus,
    });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: originalVisibilityState,
    });
    delete document.documentElement.dataset.windowActive;
    delete document.documentElement.dataset.statusHeartbeat;
  });

  it("keeps a hidden desktop window inactive until it is visible and focused", async () => {
    const { initializeWindowActivity } = await import("@/lib/windowActivity");
    initializeWindowActivity();

    expect(document.documentElement.dataset.windowActive).toBe("true");
    vi.advanceTimersByTime(3000);
    expect(document.documentElement.dataset.statusHeartbeat).toBe("true");

    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "hidden",
    });
    document.dispatchEvent(new Event("visibilitychange"));

    expect(document.documentElement.dataset.windowActive).toBe("false");
    expect(document.documentElement.dataset.statusHeartbeat).toBeUndefined();

    window.dispatchEvent(new Event("focus"));
    expect(document.documentElement.dataset.windowActive).toBe("false");

    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
    document.dispatchEvent(new Event("visibilitychange"));

    expect(document.documentElement.dataset.windowActive).toBe("true");
    expect(document.documentElement.dataset.statusHeartbeat).toBeUndefined();
  });
});
