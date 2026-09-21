import { describe, expect, it } from "vitest";
import { summarizeEnvironment } from "@/features/health";
import type { Tool, ToolStatus } from "@/entities/tool";

function makeTool(
  status: ToolStatus,
  id: Tool["id"] = "claude-code",
  latestVersion: string | null = status === "updateAvailable"
    ? "1.1.0"
    : status === "installed"
      ? "1.0.0"
      : null,
): Tool {
  return {
    id,
    name: id,
    descriptionKey: `tools.description.${id}`,
    status,
    version: status === "notInstalled" ? null : "1.0.0",
    latestVersion,
    capabilities: {
      canInstall: true,
      canUpdate: true,
      canUninstall: true,
      canRepair: false,
      canManageProvider: true,
      canManageMcp: true,
      canManageSkills: true,
      canManagePrompts: true,
      canManageVersion: false,
      canLaunch: true,
    },
    sessionsInsideSettings: false,
    environment: null,
  };
}

describe("summarizeEnvironment", () => {
  it("is Ready when every installed tool is current and nothing is broken", () => {
    const summary = summarizeEnvironment([
      makeTool("installed", "claude-code"),
      makeTool("installed", "codex"),
      makeTool("notInstalled", "opencode"),
    ]);
    expect(summary.status).toBe("ready");
    expect(summary.installed).toBe(2);
    expect(summary.updatable).toBe(0);
    expect(summary.unverified).toBe(0);
    expect(summary.attention).toBe(0);
    expect(summary.total).toBe(3);
  });

  /// When the online version check hasn't come back yet (or never found anything),
  /// `updatable === 0` only means "no update was found." It must be distinguishable
  /// from "everything really is up to date," or callers will tell the user something unverified.
  it("separates a tool whose latest version is unknown from a verified one", () => {
    const summary = summarizeEnvironment([
      makeTool("installed", "claude-code", null),
      makeTool("installed", "codex"),
    ]);
    expect(summary.updatable).toBe(0);
    expect(summary.unverified).toBe(1);
    // Not knowing the latest version isn't something the user needs to act on, so it must not trigger an alert.
    expect(summary.attention).toBe(0);
    expect(summary.status).toBe("ready");
  });

  it("counts a tool with an update as installed and as needing attention", () => {
    const summary = summarizeEnvironment([makeTool("updateAvailable")]);
    expect(summary.status).toBe("attention");
    expect(summary.installed).toBe(1);
    expect(summary.updatable).toBe(1);
    expect(summary.attention).toBe(1);
  });

  it("escalates to Action Required as soon as one tool is broken", () => {
    const summary = summarizeEnvironment([
      makeTool("installed", "claude-code"),
      makeTool("broken", "codex"),
      makeTool("updateAvailable", "opencode"),
    ]);
    expect(summary.status).toBe("action");
    expect(summary.attention).toBe(2);
  });

  it("treats an unchecked tool as something to look at", () => {
    expect(summarizeEnvironment([makeTool("unknown")]).status).toBe(
      "attention",
    );
  });

  it("asks for attention when nothing is installed at all", () => {
    const summary = summarizeEnvironment([
      makeTool("notInstalled", "claude-code"),
      makeTool("notInstalled", "codex"),
    ]);
    expect(summary.status).toBe("attention");
    expect(summary.installed).toBe(0);
    expect(summary.attention).toBe(0);
  });

  it("handles an empty list without dividing by zero", () => {
    expect(summarizeEnvironment([])).toEqual({
      status: "attention",
      total: 0,
      installed: 0,
      updatable: 0,
      unverified: 0,
      attention: 0,
    });
  });
});
