import { describe, expect, it } from "vitest";
import type { Tool } from "@/entities/tool";
import {
  firstLaunchableTool,
  isToolLaunchable,
} from "@/features/tool-management";

const CAPABILITIES: Tool["capabilities"] = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: false,
  canLaunch: true,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: false,
};

function tool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "tool.claude-code.description",
    status: "installed",
    version: "1.0.0",
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: null,
    ...overrides,
  };
}

describe("tool launch availability", () => {
  it.each(["installed", "updateAvailable"] as const)(
    "allows a capability-approved %s tool",
    (status) => {
      expect(isToolLaunchable(tool({ status }))).toBe(true);
    },
  );

  it.each(["notInstalled", "broken", "unknown"] as const)(
    "refuses a %s tool even when the platform supports launch",
    (status) => {
      expect(isToolLaunchable(tool({ status }))).toBe(false);
    },
  );

  it("refuses an installed tool when the platform blocks launch", () => {
    expect(
      isToolLaunchable(
        tool({
          capabilities: { ...CAPABILITIES, canLaunch: false },
        }),
      ),
    ).toBe(false);
  });

  it("preserves backend order while skipping unavailable tools", () => {
    const unavailable = tool({
      capabilities: { ...CAPABILITIES, canLaunch: false },
    });
    const codex = tool({ id: "codex", name: "Codex" });
    const opencode = tool({ id: "opencode", name: "OpenCode" });

    expect(firstLaunchableTool([unavailable, codex, opencode])).toBe(codex);
    expect(firstLaunchableTool([unavailable])).toBeNull();
  });
});
