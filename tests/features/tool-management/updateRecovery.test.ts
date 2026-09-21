import { describe, expect, it } from "vitest";
import type { Operation } from "@/entities/operation";
import type { Tool } from "@/entities/tool";
import { updateRecoveryPresentation } from "@/features/tool-management";

const TOOL: Tool = {
  id: "codex",
  name: "Codex",
  descriptionKey: "tool.codex.description",
  status: "broken",
  version: "1.2.3",
  latestVersion: "2.0.0",
  capabilities: {
    canInstall: true,
    canUpdate: true,
    canUninstall: true,
    canRepair: true,
    canLaunch: false,
    canManageProvider: true,
    canManageMcp: true,
    canManageSkills: true,
    canManagePrompts: true,
    canManageVersion: true,
  },
  sessionsInsideSettings: false,
  environment: "macos",
};

function operation(updateRecovery: Operation["updateRecovery"]): Operation {
  return {
    id: "failed-update",
    kind: "update",
    tool: "codex",
    desktopApp: null,
    extension: null,
    status: "failed",
    progress: 50,
    messageKey: "operation.phase.installing",
    canCancel: false,
    logs: [],
    updateRecovery,
    output: null,
    error: null,
    startedAt: 1,
    finishedAt: 2,
  };
}

describe("updateRecoveryPresentation", () => {
  it("requires both native recovery evidence and a currently Broken tool", () => {
    const failed = operation({ kind: "available", targetVersion: "1.2.3" });
    expect(updateRecoveryPresentation(failed, TOOL)).toEqual({
      kind: "available",
      version: "1.2.3",
      actionable: true,
    });
    expect(updateRecoveryPresentation(failed, null)).toEqual({
      kind: "available",
      version: "1.2.3",
      actionable: false,
    });
    expect(
      updateRecoveryPresentation(failed, { ...TOOL, status: "installed" }),
    ).toBeNull();
  });

  it("keeps ownership and history failures advisory-only", () => {
    expect(
      updateRecoveryPresentation(operation({ kind: "ownershipChanged" }), TOOL),
    ).toEqual({ kind: "advisory", reason: "ownershipChanged" });
    expect(
      updateRecoveryPresentation(
        operation({ kind: "historyUnavailable" }),
        TOOL,
      ),
    ).toEqual({ kind: "advisory", reason: "historyUnavailable" });
  });

  it("never turns a non-update or non-failure into a restore action", () => {
    const failed = operation({ kind: "available", targetVersion: "1.2.3" });
    expect(
      updateRecoveryPresentation({ ...failed, kind: "install" }, TOOL),
    ).toBeNull();
    expect(
      updateRecoveryPresentation({ ...failed, status: "success" }, TOOL),
    ).toBeNull();
  });
});
