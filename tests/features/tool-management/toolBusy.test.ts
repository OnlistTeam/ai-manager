import { describe, expect, it } from "vitest";
import {
  activeToolOperations,
  busyTools,
  retryActionForOperation,
  toolCardOperations,
  toolsAwaitingInventory,
} from "@/features/tool-management";
import type { Operation } from "@/entities/operation";

function makeOperation(overrides: Partial<Operation> = {}): Operation {
  return {
    id: "op-1",
    kind: "install",
    tool: "claude-code",
    desktopApp: null,
    extension: null,
    status: "running",
    progress: 10,
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

describe("busyTools", () => {
  it("locks a tool while an operation is queued or running (spec section 41)", () => {
    const busy = busyTools([
      makeOperation({ id: "a", status: "queued" }),
      makeOperation({ id: "b", tool: "codex", status: "running" }),
    ]);
    expect(busy.has("claude-code")).toBe(true);
    expect(busy.has("codex")).toBe(true);
  });

  it("releases a tool once its operation reaches a final state", () => {
    const busy = busyTools([
      makeOperation({ status: "success" }),
      makeOperation({ id: "b", tool: "codex", status: "failed" }),
      makeOperation({ id: "c", tool: "opencode", status: "cancelled" }),
    ]);
    expect(busy.size).toBe(0);
  });

  it("leaves other tools free so they can run in parallel", () => {
    const busy = busyTools([makeOperation()]);
    expect(busy.has("opencode")).toBe(false);
  });

  it("ignores operations that are not bound to a tool", () => {
    expect(busyTools([makeOperation({ tool: null, kind: "scan" })]).size).toBe(
      0,
    );
  });

  it("does not turn read-only provider batches into tool mutation locks", () => {
    const running = makeOperation({ kind: "testProviders" });
    const finished = makeOperation({
      id: "finished-checks",
      kind: "testProviders",
      status: "success",
      progress: 100,
      finishedAt: 200,
    });
    expect(activeToolOperations([running])).toEqual(new Map());
    expect(busyTools([running])).toEqual(new Set());
    expect(toolCardOperations([finished], new Set(), new Set())).toEqual(
      new Map(),
    );
    expect(toolsAwaitingInventory([finished], 100)).toEqual(new Set());
    expect(retryActionForOperation(finished)).toBeNull();
  });

  it("provides the active operation each card should present", () => {
    const claude = makeOperation({ id: "claude", startedAt: 10 });
    const codex = makeOperation({
      id: "codex",
      tool: "codex",
      status: "queued",
      startedAt: null,
    });

    const active = activeToolOperations([claude, codex]);

    expect(active.get("claude-code")).toBe(claude);
    expect(active.get("codex")).toBe(codex);
  });

  it("keeps a tool busy while one of its Skills is being installed", () => {
    const skillInstall = makeOperation({
      extension: {
        kind: "skill",
        id: "anthropics/skills:code-review",
        name: "Code review",
      },
    });
    expect(busyTools([skillInstall]).has("claude-code")).toBe(true);
    expect(activeToolOperations([skillInstall]).get("claude-code")).toBe(
      skillInstall,
    );
  });

  it("selects the newest active record if an inconsistent cache has two", () => {
    const older = makeOperation({ id: "older", startedAt: 10 });
    const newer = makeOperation({ id: "newer", startedAt: 20, progress: 70 });

    expect(activeToolOperations([older, newer]).get("claude-code")).toBe(newer);
  });

  it("never presents terminal or tool-less work on a tool card", () => {
    const active = activeToolOperations([
      makeOperation({ status: "success" }),
      makeOperation({ id: "scan", kind: "scan", tool: null }),
    ]);

    expect(active.size).toBe(0);
  });

  it("keeps the newest failed operation visible until the user dismisses it", () => {
    const older = makeOperation({
      id: "older",
      status: "failed",
      finishedAt: 20,
    });
    const newest = makeOperation({
      id: "newest",
      kind: "update",
      status: "failed",
      finishedAt: 30,
    });

    expect(
      toolCardOperations([older, newest], new Set(), new Set()).get(
        "claude-code",
      ),
    ).toBe(newest);
    expect(
      toolCardOperations([older, newest], new Set(["newest"]), new Set()).has(
        "claude-code",
      ),
    ).toBe(false);
  });

  it("lets active work replace retained failure feedback", () => {
    const failure = makeOperation({
      id: "failure",
      status: "failed",
      finishedAt: 30,
    });
    const retry = makeOperation({ id: "retry", startedAt: 40 });

    expect(
      toolCardOperations([failure, retry], new Set(), new Set()).get(
        "claude-code",
      ),
    ).toBe(retry);
  });

  it("does not let unrelated extension success hide a tool failure", () => {
    const failure = makeOperation({
      id: "tool-failure",
      kind: "update",
      status: "failed",
      finishedAt: 30,
    });
    const extensionSuccess = makeOperation({
      id: "skill-success",
      status: "success",
      finishedAt: 40,
      extension: {
        kind: "skill",
        id: "example-org/skills:review",
        name: "Review",
      },
    });

    expect(
      toolCardOperations([failure, extensionSuccess], new Set(), new Set()).get(
        "claude-code",
      ),
    ).toBe(failure);
  });

  it("does not resurrect an older failure after newer tool success reconciles", () => {
    const failure = makeOperation({
      id: "tool-failure",
      status: "failed",
      finishedAt: 30,
    });
    const success = makeOperation({
      id: "tool-success",
      status: "success",
      finishedAt: 40,
    });

    expect(
      toolCardOperations([failure, success], new Set(), new Set()).has(
        "claude-code",
      ),
    ).toBe(false);
  });

  it("shows success only during the detector handoff", () => {
    const success = makeOperation({
      status: "success",
      finishedAt: 30,
    });

    expect(
      toolCardOperations([success], new Set(), new Set(["claude-code"])).get(
        "claude-code",
      ),
    ).toBe(success);
    expect(toolCardOperations([success], new Set(), new Set()).size).toBe(0);
  });

  it("reconciles only tools whose terminal result is newer than inventory", () => {
    const awaiting = toolsAwaitingInventory(
      [
        makeOperation({ status: "failed", finishedAt: 101 }),
        makeOperation({
          id: "codex",
          tool: "codex",
          status: "success",
          finishedAt: 99,
        }),
      ],
      100,
    );

    expect([...awaiting]).toEqual(["claude-code"]);
  });

  it("does not wait for tool inventory after nested extension work", () => {
    const awaiting = toolsAwaitingInventory(
      [
        makeOperation({
          status: "success",
          finishedAt: 101,
          extension: {
            kind: "skill",
            id: "example-org/skills:review",
            name: "Review",
          },
        }),
      ],
      100,
    );

    expect(awaiting.size).toBe(0);
  });

  it("maps retries back to safe product actions without replaying commands", () => {
    expect(retryActionForOperation(makeOperation({ kind: "update" }))).toBe(
      "update",
    );
    expect(retryActionForOperation(makeOperation({ kind: "uninstall" }))).toBe(
      "remove",
    );
    expect(
      retryActionForOperation(makeOperation({ kind: "changeVersion" })),
    ).toBe("version");
    expect(
      retryActionForOperation(makeOperation({ kind: "scan", tool: null })),
    ).toBeNull();
    expect(
      retryActionForOperation(
        makeOperation({
          extension: {
            kind: "skill",
            id: "example-org/skills:review",
            name: "Review",
          },
        }),
      ),
    ).toBeNull();
  });
});
