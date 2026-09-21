import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { OPERATION_CHANGED_EVENT } from "@/native";
import { operationKeys, useOperationEvents } from "@/entities/operation";
import { toolKeys } from "@/entities/tool";
import { extensionKeys } from "@/entities/extension";
import { healthKeys } from "@/entities/health";
import { skillCatalogKeys } from "@/entities/skill-catalog";
import { skillBackupKeys } from "@/entities/skill-backup";
import { skillUpdateKeys, type SkillUpdate } from "@/entities/skill-update";
import type { Operation } from "@/entities/operation";
import type { Tool } from "@/entities/tool";
import { emitTauriEvent } from "../../msw/tauriMocks";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const CAPABILITIES = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: false,
  canLaunch: true,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: true,
};

function makeTool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "tools.description.claudeCode",
    status: "installed",
    version: "2.3.1",
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: null,
    ...overrides,
  } as Tool;
}

function makeOperation(overrides: Partial<Operation> = {}): Operation {
  return {
    id: "op-1",
    kind: "install",
    tool: "claude-code",
    desktopApp: null,
    extension: null,
    status: "running",
    progress: 20,
    messageKey: "operation.phase.installing",
    canCancel: false,
    logs: [],
    updateRecovery: null,
    output: null,
    error: null,
    startedAt: 1_000,
    finishedAt: null,
    ...overrides,
  };
}

describe("useOperationEvents", () => {
  it("writes every push into the operation list cache", async () => {
    const client = createTestQueryClient();
    renderHook(() => useOperationEvents(), {
      wrapper: withQueryClient(client),
    });

    emitTauriEvent(OPERATION_CHANGED_EVENT, makeOperation());
    await waitFor(() =>
      expect(
        client.getQueryData<Operation[]>(operationKeys.list()),
      ).toHaveLength(1),
    );

    emitTauriEvent(OPERATION_CHANGED_EVENT, makeOperation({ progress: 70 }));
    await waitFor(() =>
      expect(
        client.getQueryData<Operation[]>(operationKeys.list())?.[0].progress,
      ).toBe(70),
    );
  });

  it("refreshes the tool list only once an operation is final", async () => {
    const client = createTestQueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    renderHook(() => useOperationEvents(), {
      wrapper: withQueryClient(client),
    });

    emitTauriEvent(
      OPERATION_CHANGED_EVENT,
      makeOperation({ status: "running" }),
    );
    await waitFor(() =>
      expect(
        client.getQueryData<Operation[]>(operationKeys.list()),
      ).toHaveLength(1),
    );
    expect(
      invalidate.mock.calls.filter(([arg]) => arg?.queryKey === toolKeys.all),
    ).toHaveLength(0);

    emitTauriEvent(
      OPERATION_CHANGED_EVENT,
      makeOperation({ status: "success", progress: 100, finishedAt: 2_000 }),
    );
    await waitFor(() =>
      expect(
        invalidate.mock.calls.filter(([arg]) => arg?.queryKey === toolKeys.all),
      ).toHaveLength(1),
    );
  });

  it("replays provider-test output into connectivity without rescanning tools", async () => {
    const client = createTestQueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    renderHook(() => useOperationEvents(), {
      wrapper: withQueryClient(client),
    });
    const checked = {
      providerId: "relay",
      reachability: "degraded" as const,
      responseTimeMs: 900,
      httpStatus: 429,
    };

    emitTauriEvent(
      OPERATION_CHANGED_EVENT,
      makeOperation({
        kind: "testProviders",
        progress: 55,
        output: { kind: "providerTests", results: [checked] },
      }),
    );
    await waitFor(() =>
      expect(
        client.getQueryData<Record<string, Record<string, unknown>>>(
          healthKeys.connectivity(),
        )?.["claude-code"]?.relay,
      ).toEqual(checked),
    );

    emitTauriEvent(
      OPERATION_CHANGED_EVENT,
      makeOperation({
        kind: "testProviders",
        status: "success",
        progress: 100,
        finishedAt: 2_000,
        output: { kind: "providerTests", results: [checked] },
      }),
    );
    await waitFor(() =>
      expect(
        client.getQueryData<Operation[]>(operationKeys.list())?.[0]?.status,
      ).toBe("success"),
    );
    expect(
      invalidate.mock.calls.some(([arg]) => arg?.queryKey === toolKeys.all),
    ).toBe(false);
  });

  it("puts a finished action's verified detection on the card without rescanning every tool", async () => {
    const client = createTestQueryClient();
    client.setQueryData<Tool[]>(
      toolKeys.list(),
      [
        makeTool({
          status: "updateAvailable",
          version: "2.3.1",
          latestVersion: "2.4.0",
        }),
        makeTool({ id: "codex", name: "Codex CLI", version: "1.0.0" }),
      ],
      // Older than the task below, so the card is still holding its "done"
      // panel over the stale row when the event arrives.
      { updatedAt: 1_000 },
    );
    const invalidate = vi.spyOn(client, "invalidateQueries");
    renderHook(() => useOperationEvents(), {
      wrapper: withQueryClient(client),
    });

    emitTauriEvent(
      OPERATION_CHANGED_EVENT,
      makeOperation({
        kind: "update",
        status: "success",
        progress: 100,
        finishedAt: 2_000,
        output: {
          kind: "toolInventory",
          tool: makeTool({ version: "2.4.0", latestVersion: "2.4.0" }),
        },
      }),
    );

    await waitFor(() =>
      expect(
        client
          .getQueryData<Tool[]>(toolKeys.list())
          ?.find((tool) => tool.id === "claude-code")?.version,
      ).toBe("2.4.0"),
    );
    const cached = client.getQueryData<Tool[]>(toolKeys.list());
    expect(cached?.find((tool) => tool.id === "claude-code")?.status).toBe(
      "installed",
    );
    // Untouched tools keep their place and their state.
    expect(cached?.map((tool) => tool.id)).toEqual(["claude-code", "codex"]);
    expect(cached?.[1].version).toBe("1.0.0");
    // The whole point: no full re-scan, which would ask every registry for its
    // latest version before the card could stop showing the old one.
    expect(
      invalidate.mock.calls.some(([arg]) => arg?.queryKey === toolKeys.all),
    ).toBe(false);
    // The success panel stays up until the inventory is newer than the task
    // that finished. Landing the result has to move that clock forward, or the
    // card shows a correct version under a panel that never goes away.
    expect(
      client.getQueryState(toolKeys.list())?.dataUpdatedAt ?? 0,
    ).toBeGreaterThan(2_000);
  });

  it("falls back to a rescan when the verified detection has no cached list to land in", async () => {
    const client = createTestQueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    renderHook(() => useOperationEvents(), {
      wrapper: withQueryClient(client),
    });

    emitTauriEvent(
      OPERATION_CHANGED_EVENT,
      makeOperation({
        status: "success",
        progress: 100,
        finishedAt: 2_000,
        output: { kind: "toolInventory", tool: makeTool() },
      }),
    );

    await waitFor(() =>
      expect(
        invalidate.mock.calls.some(([arg]) => arg?.queryKey === toolKeys.all),
      ).toBe(true),
    );
    expect(client.getQueryData<Tool[]>(toolKeys.list())).toBeUndefined();
  });

  it("refreshes the shared Skill inventory, not the expensive tool scan, after a Skill task", async () => {
    const client = createTestQueryClient();
    client.setQueryData<SkillUpdate[]>(skillUpdateKeys.list(), [
      { id: "anthropics/skills:code-review", name: "Code review" },
      { id: "example-org/skills:release", name: "Release" },
    ]);
    const invalidate = vi.spyOn(client, "invalidateQueries");
    renderHook(() => useOperationEvents(), {
      wrapper: withQueryClient(client),
    });

    emitTauriEvent(
      OPERATION_CHANGED_EVENT,
      makeOperation({
        status: "success",
        progress: 100,
        finishedAt: 2_000,
        extension: {
          kind: "skill",
          id: "anthropics/skills:code-review",
          name: "Code review",
        },
      }),
    );

    await waitFor(() =>
      expect(
        invalidate.mock.calls.some(
          ([arg]) => arg?.queryKey === skillCatalogKeys.all,
        ),
      ).toBe(true),
    );
    expect(
      invalidate.mock.calls.some(
        ([arg]) => arg?.queryKey === extensionKeys.all,
      ),
    ).toBe(true);
    expect(
      invalidate.mock.calls.some(
        ([arg]) => arg?.queryKey === skillBackupKeys.all,
      ),
    ).toBe(true);
    expect(
      invalidate.mock.calls.some(([arg]) => arg?.queryKey === toolKeys.all),
    ).toBe(false);
    expect(client.getQueryData(skillUpdateKeys.list())).toEqual([
      { id: "example-org/skills:release", name: "Release" },
    ]);
  });

  it("refreshes the scope-keyed prompt list after a prompt task", async () => {
    const client = createTestQueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    renderHook(() => useOperationEvents(), {
      wrapper: withQueryClient(client),
    });

    emitTauriEvent(
      OPERATION_CHANGED_EVENT,
      makeOperation({
        status: "success",
        progress: 100,
        finishedAt: 2_000,
        extension: { kind: "prompt", id: "prompt-1", name: "Review" },
      }),
    );

    // The list readers key by extension scope ("tool:<id>"), never by bare id.
    await waitFor(() =>
      expect(invalidate).toHaveBeenCalledWith({
        queryKey: extensionKeys.list("tool:claude-code", "prompt"),
      }),
    );
    expect(
      invalidate.mock.calls.some(
        ([arg]) => arg?.queryKey === extensionKeys.all,
      ),
    ).toBe(false);
  });

  it("keeps a Skill update available when its operation fails", async () => {
    const client = createTestQueryClient();
    const update = {
      id: "anthropics/skills:code-review",
      name: "Code review",
    };
    client.setQueryData<SkillUpdate[]>(skillUpdateKeys.list(), [update]);
    renderHook(() => useOperationEvents(), {
      wrapper: withQueryClient(client),
    });

    emitTauriEvent(
      OPERATION_CHANGED_EVENT,
      makeOperation({
        kind: "update",
        status: "failed",
        finishedAt: 2_000,
        extension: { kind: "skill", id: update.id, name: update.name },
      }),
    );
    await waitFor(() =>
      expect(
        client.getQueryData<Operation[]>(operationKeys.list())?.[0]?.status,
      ).toBe("failed"),
    );
    expect(client.getQueryData(skillUpdateKeys.list())).toEqual([update]);
  });

  it("refreshes every MCP projection and Quick Check after a global MCP task finishes", async () => {
    const client = createTestQueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    renderHook(() => useOperationEvents(), {
      wrapper: withQueryClient(client),
    });

    emitTauriEvent(
      OPERATION_CHANGED_EVENT,
      makeOperation({
        status: "success",
        progress: 100,
        finishedAt: 2_000,
        extension: {
          kind: "mcp",
          id: "filesystem-a1b2c3d4",
          name: "Filesystem",
        },
      }),
    );

    await waitFor(() =>
      expect(
        invalidate.mock.calls.some(
          ([arg]) => arg?.queryKey === healthKeys.snapshots,
        ),
      ).toBe(true),
    );
    expect(
      invalidate.mock.calls.some(
        ([arg]) => arg?.queryKey === extensionKeys.all,
      ),
    ).toBe(true);
    expect(
      invalidate.mock.calls.some(([arg]) => arg?.queryKey === toolKeys.all),
    ).toBe(false);
  });

  it("drops a payload that does not match the operation schema", async () => {
    const client = createTestQueryClient();
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => {});
    renderHook(() => useOperationEvents(), {
      wrapper: withQueryClient(client),
    });

    emitTauriEvent(OPERATION_CHANGED_EVENT, {
      id: "op-1",
      status: "who knows",
    });
    await waitFor(() => expect(consoleError).toHaveBeenCalled());
    expect(
      client.getQueryData<Operation[]>(operationKeys.list()),
    ).toBeUndefined();
    consoleError.mockRestore();
  });
});
