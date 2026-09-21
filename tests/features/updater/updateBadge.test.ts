import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { UPDATE_BADGES, updateTone } from "@/features/updater";

describe("updateTone", () => {
  it("says it is checking before the first answer arrives", () => {
    expect(
      updateTone({
        checking: true,
        failed: false,
        channelReady: undefined,
        availableVersion: null,
      }),
    ).toBe("checking");
  });

  it("never claims up to date when the check itself failed", () => {
    // Saying "up to date" while offline would be a lie — and that's exactly what happens without this check.
    expect(
      updateTone({
        checking: false,
        failed: true,
        channelReady: undefined,
        availableVersion: undefined,
      }),
    ).toBe("unknown");
  });

  it("reports an available version", () => {
    expect(
      updateTone({
        checking: false,
        failed: false,
        channelReady: true,
        availableVersion: "1.1.0",
      }),
    ).toBe("available");
  });

  it("distinguishes a background download from a restart-ready update", () => {
    expect(
      updateTone({
        checking: false,
        failed: false,
        phase: "downloading",
        channelReady: true,
        availableVersion: "1.1.0",
      }),
    ).toBe("downloading");
    expect(
      updateTone({
        checking: false,
        failed: false,
        phase: "ready",
        channelReady: true,
        availableVersion: "1.1.0",
      }),
    ).toBe("ready");
  });

  it("reports up to date only after a successful check with nothing newer", () => {
    expect(
      updateTone({
        checking: false,
        failed: false,
        channelReady: true,
        availableVersion: null,
      }),
    ).toBe("upToDate");
  });

  it("reports an unconfigured channel instead of claiming up to date", () => {
    expect(
      updateTone({
        checking: false,
        failed: false,
        channelReady: false,
        availableVersion: null,
      }),
    ).toBe("unconfigured");
  });

  it("gives every state an icon and a word, not just a colour", () => {
    // Spec §98: state must not be conveyed by color alone.
    for (const tone of [
      "checking",
      "downloading",
      "ready",
      "available",
      "upToDate",
      "unknown",
      "unconfigured",
    ] as const) {
      const meta = UPDATE_BADGES[tone];
      // lucide icons are the result of forwardRef (an object, not a function), so
      // this asserts that an icon actually renders, not what its implementation looks like.
      expect(renderToStaticMarkup(createElement(meta.icon))).toContain("<svg");
      expect(meta.labelKey.startsWith("preferences.updates.")).toBe(true);
    }
  });
});
