import { describe, expect, it } from "vitest";
import { describeUninstall } from "@/features/tool-management";

const NESTED = { sessionsInsideSettings: true };
const SEPARATE = { sessionsInsideSettings: false };

describe("describeUninstall", () => {
  it("removes only the app by default (spec section 32)", () => {
    const impact = describeUninstall(
      { removeSettings: false, removeCache: false },
      NESTED,
    );
    expect(impact.removedKeys).toEqual(["tools.uninstall.impact.app"]);
    expect(impact.warningKeys).toEqual([]);
    expect(impact.destructive).toBe(false);
  });

  it("says out loud that removing settings also deletes saved conversations", () => {
    const impact = describeUninstall(
      { removeSettings: true, removeCache: false },
      NESTED,
    );
    expect(impact.removedKeys).toContain("tools.uninstall.impact.settings");
    expect(impact.warningKeys).toContain(
      "tools.uninstall.warning.sessionsInsideSettings",
    );
    expect(impact.destructive).toBe(true);
  });

  it("stays quiet about conversations when the tool keeps them elsewhere", () => {
    // This is the correctness fix for this task: previously all four tools showed
    // this "maybe" wording, even though OpenCode's session directory isn't inside the settings directory at all.
    const impact = describeUninstall(
      { removeSettings: true, removeCache: false },
      SEPARATE,
    );
    expect(impact.removedKeys).toContain("tools.uninstall.impact.settings");
    expect(impact.warningKeys).not.toContain(
      "tools.uninstall.warning.sessionsInsideSettings",
    );
    expect(impact.destructive).toBe(true);
  });

  it("never shows the conversation warning without the settings box", () => {
    expect(
      describeUninstall({ removeSettings: false, removeCache: true }, NESTED)
        .warningKeys,
    ).not.toContain("tools.uninstall.warning.sessionsInsideSettings");
  });

  it("warns about files outside the home folder whenever anything is deleted", () => {
    for (const options of [
      { removeSettings: true, removeCache: false },
      { removeSettings: false, removeCache: true },
    ]) {
      expect(describeUninstall(options, SEPARATE).warningKeys).toContain(
        "tools.uninstall.warning.outsideHome",
      );
    }
    expect(
      describeUninstall({ removeSettings: false, removeCache: false }, SEPARATE)
        .warningKeys,
    ).not.toContain("tools.uninstall.warning.outsideHome");
  });

  it("lists cache before settings, the order the backend deletes in", () => {
    const impact = describeUninstall(
      { removeSettings: true, removeCache: true },
      NESTED,
    );
    expect(impact.removedKeys).toEqual([
      "tools.uninstall.impact.app",
      "tools.uninstall.impact.cache",
      "tools.uninstall.impact.settings",
    ]);
  });
});
