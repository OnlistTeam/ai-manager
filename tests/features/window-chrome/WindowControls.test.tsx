import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";

import en from "@/i18n/locales/en.json";
import { WindowControls } from "@/features/window-chrome";
import { native } from "@/native";

vi.mock("@/native", async () => {
  const actual = await vi.importActual<typeof import("@/native")>("@/native");
  return {
    ...actual,
    native: {
      ...actual.native,
      windowControls: {
        minimize: vi.fn(() => Promise.resolve()),
        toggleMaximize: vi.fn(() => Promise.resolve()),
        close: vi.fn(() => Promise.resolve()),
        isMaximized: vi.fn(() => Promise.resolve(false)),
        onResized: vi.fn(() => Promise.resolve(() => {})),
      },
    },
  };
});

const controls = native.windowControls as unknown as Record<
  string,
  ReturnType<typeof vi.fn>
>;

describe("WindowControls", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    controls.isMaximized.mockResolvedValue(false);
    controls.onResized.mockResolvedValue(() => {});
    i18n.addResourceBundle("en", "translation", en, true, true);
    await i18n.changeLanguage("en");
  });

  /**
   * A window with no system caption has no system buttons either. If any of
   * these three is missing or miswired, the window cannot be minimized or
   * closed from its own chrome at all.
   */
  it("drives all three window actions", async () => {
    render(<WindowControls />);

    for (const [label, call] of [
      [en.nav.window.minimize, "minimize"],
      [en.nav.window.maximize, "toggleMaximize"],
      [en.nav.window.close, "close"],
    ] as const) {
      await userEvent.click(screen.getByRole("button", { name: label }));
      expect(controls[call]).toHaveBeenCalledTimes(1);
    }
  });

  /**
   * Aero Snap and the Win+Arrow shortcuts maximize the window without going
   * through our button, so the glyph is driven by the window's own state
   * rather than by what was last clicked.
   */
  it("follows the real window state rather than the last click", async () => {
    controls.isMaximized.mockResolvedValue(true);
    render(<WindowControls />);

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: en.nav.window.restore }),
      ).toBeVisible(),
    );
    expect(
      screen.queryByRole("button", { name: en.nav.window.maximize }),
    ).toBeNull();
  });

  it("keeps a drag region so the window can still be moved", () => {
    const { container } = render(<WindowControls />);
    expect(container.querySelector("[data-window-drag-region]")).not.toBeNull();
  });

  /** A window state that cannot be read must not take the buttons with it. */
  it("still renders when the window state cannot be read", async () => {
    controls.isMaximized.mockRejectedValue(new Error("no handle"));
    render(<WindowControls />);

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: en.nav.window.close }),
      ).toBeVisible(),
    );
  });
});
