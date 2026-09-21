import { describe, expect, it } from "vitest";
import {
  canOpenCompletedTool,
  countActive,
  formatOperationTime,
  presentOperation,
} from "@/features/task-center";
import type { Operation } from "@/entities/operation";
import type { Tool } from "@/entities/tool";

function makeOperation(overrides: Partial<Operation> = {}): Operation {
  return {
    id: "op-1",
    kind: "install",
    tool: "claude-code",
    desktopApp: null,
    extension: null,
    status: "running",
    progress: 40,
    messageKey: "operation.phase.installing",
    canCancel: false,
    logs: [],
    updateRecovery: null,
    output: null,
    error: null,
    startedAt: 1,
    finishedAt: null,
    ...overrides,
  };
}

function makeTool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "tool.claude-code.description",
    status: "installed",
    version: "1.0.0",
    latestVersion: null,
    capabilities: {
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
    },
    sessionsInsideSettings: false,
    environment: null,
    ...overrides,
  };
}

describe("presentOperation", () => {
  it("names the task after what it is doing", () => {
    expect(presentOperation(makeOperation()).titleKey).toBe(
      "taskCenter.kind.install",
    );
    expect(
      presentOperation(makeOperation({ kind: "uninstall" })).titleKey,
    ).toBe("taskCenter.kind.uninstall");
    expect(
      presentOperation(makeOperation({ kind: "testProviders" })).titleKey,
    ).toBe("taskCenter.kind.testProviders");
  });

  it("names extension work after the Skill instead of the locking tool", () => {
    const view = presentOperation(
      makeOperation({
        extension: {
          kind: "skill",
          id: "anthropics/skills:code-review",
          name: "Code review",
        },
      }),
    );
    expect(view.titleKey).toBe("taskCenter.extension.skill.install");
    expect(view.targetName).toBe("Code review");
  });

  it("uses the Skill removal task title for an extension uninstall", () => {
    const view = presentOperation(
      makeOperation({
        kind: "uninstall",
        extension: {
          kind: "skill",
          id: "anthropics/skills:code-review",
          name: "Code review",
        },
      }),
    );
    expect(view.titleKey).toBe("taskCenter.extension.skill.uninstall");
    expect(view.targetName).toBe("Code review");
  });

  it("uses the Skill update task title for a managed update", () => {
    const view = presentOperation(
      makeOperation({
        kind: "update",
        extension: {
          kind: "skill",
          id: "anthropics/skills:code-review",
          name: "Code review",
        },
      }),
    );
    expect(view.titleKey).toBe("taskCenter.extension.skill.update");
    expect(view.targetName).toBe("Code review");
  });

  it("names MCP installation after the connection instead of the locking tool", () => {
    const view = presentOperation(
      makeOperation({
        messageKey: "operation.phase.configuring",
        extension: {
          kind: "mcp",
          id: "filesystem-a1b2c3d4",
          name: "Filesystem",
        },
      }),
    );
    expect(view.titleKey).toBe("taskCenter.extension.mcp.install");
    expect(view.targetName).toBe("Filesystem");
    expect(view.detailKey).toBe("operation.phase.configuring");
  });

  it("uses the MCP removal title and global connection name", () => {
    const view = presentOperation(
      makeOperation({
        kind: "uninstall",
        messageKey: "operation.phase.removing",
        extension: {
          kind: "mcp",
          id: "filesystem-a1b2c3d4",
          name: "Filesystem",
        },
      }),
    );
    expect(view.titleKey).toBe("taskCenter.extension.mcp.uninstall");
    expect(view.targetName).toBe("Filesystem");
    expect(view.detailKey).toBe("operation.phase.removing");
  });

  it("shows the current phase while the task is alive", () => {
    const view = presentOperation(makeOperation());
    expect(view.detailKey).toBe("operation.phase.installing");
    expect(view.error).toBeNull();
    expect(view.active).toBe(true);
  });

  it("prefers the error over the phase key left behind by a failure", () => {
    const view = presentOperation(
      makeOperation({
        status: "failed",
        // The backend doesn't clear this: it's the last phase before the task stopped.
        messageKey: "operation.phase.installing",
        error: {
          code: "INSTALL_FAILED",
          messageKey: "error.tool.installFailed",
          technicalMessage: "exit code 127",
          remediation: "error.remediation.retryOrViewDetails",
          contextId: "op-1",
        },
        finishedAt: 5,
      }),
    );
    expect(view.detailKey).toBe("error.tool.installFailed");
    expect(view.error?.code).toBe("INSTALL_FAILED");
    expect(view.active).toBe(false);
  });

  it("still says something when a failure arrives with no error payload", () => {
    const view = presentOperation(
      makeOperation({ status: "failed", error: null, finishedAt: 5 }),
    );
    expect(view.detailKey).toBe("operation.phase.installing");
    expect(view.error).toBeNull();
  });

  it("does not show a stale running phase after cancellation", () => {
    const view = presentOperation(
      makeOperation({ status: "cancelled", finishedAt: 5 }),
    );
    expect(view.detailKey).toBeNull();
    expect(view.active).toBe(false);
  });

  it("counts only the tasks that are still going (spec section 40)", () => {
    expect(
      countActive([
        makeOperation({ id: "a", status: "running" }),
        makeOperation({ id: "b", status: "queued" }),
        makeOperation({ id: "c", status: "success" }),
        makeOperation({ id: "d", status: "failed" }),
      ]),
    ).toBe(2);
  });

  it("formats recent work relatively and older work as a compact date", () => {
    const now = Date.UTC(2026, 7, 20, 12);
    expect(formatOperationTime(now - 10_000, now, "en")).toBe("now");
    expect(formatOperationTime(now - 120_000, now, "en")).toBe("2 minutes ago");
    expect(formatOperationTime(Date.UTC(2026, 7, 1), now, "en")).toBe("Aug 1");
    expect(formatOperationTime(Number.NaN, now, "en")).toBeNull();
  });
});

describe("canOpenCompletedTool", () => {
  it.each(["install", "update"] as const)(
    "allows a verified launch after a successful %s",
    (kind) => {
      expect(
        canOpenCompletedTool(
          makeOperation({ kind, status: "success", finishedAt: 5 }),
          makeTool(),
        ),
      ).toBe(true);
    },
  );

  it("also allows opening when another update is already available", () => {
    expect(
      canOpenCompletedTool(
        makeOperation({ status: "success", finishedAt: 5 }),
        makeTool({ status: "updateAvailable" }),
      ),
    ).toBe(true);
  });

  it.each([
    ["operation still running", makeOperation(), makeTool()],
    [
      "failed install",
      makeOperation({ status: "failed", finishedAt: 5 }),
      makeTool(),
    ],
    [
      "successful uninstall",
      makeOperation({ kind: "uninstall", status: "success", finishedAt: 5 }),
      makeTool(),
    ],
    ["missing tool", makeOperation({ status: "success" }), null],
    [
      "different tool",
      makeOperation({ status: "success" }),
      makeTool({ id: "codex" }),
    ],
    [
      "tool not detected",
      makeOperation({ status: "success" }),
      makeTool({ status: "notInstalled" }),
    ],
    [
      "broken tool",
      makeOperation({ status: "success" }),
      makeTool({ status: "broken" }),
    ],
    [
      "launch capability absent",
      makeOperation({ status: "success" }),
      makeTool({
        capabilities: { ...makeTool().capabilities, canLaunch: false },
      }),
    ],
    [
      "Skill install rather than tool install",
      makeOperation({
        status: "success",
        extension: { kind: "skill", id: "skill-1", name: "Review" },
      }),
      makeTool(),
    ],
  ])("rejects %s", (_reason, operation, tool) => {
    expect(canOpenCompletedTool(operation, tool)).toBe(false);
  });
});
