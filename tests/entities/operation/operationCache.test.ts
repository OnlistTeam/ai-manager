import { describe, expect, it } from "vitest";
import {
  OPERATION_HISTORY_LIMIT,
  isTerminal,
  mergeOperation,
  reconcileOperations,
  sortByRecency,
} from "@/entities/operation";
import type { Operation } from "@/entities/operation";

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

describe("operation cache", () => {
  it("replaces an operation in place when the id is already known", () => {
    const current = [makeOperation(), makeOperation({ id: "op-2" })];
    const merged = mergeOperation(
      current,
      makeOperation({ progress: 80, messageKey: "operation.phase.checking" }),
    );
    expect(merged).toHaveLength(2);
    expect(merged[0].progress).toBe(80);
    expect(merged[0].messageKey).toBe("operation.phase.checking");
    expect(merged[1].id).toBe("op-2");
  });

  it("never lets an out-of-order cancellation snapshot regress a terminal event", () => {
    const terminal = makeOperation({
      status: "cancelled",
      progress: 20,
      messageKey: "operation.phase.cancelled",
      finishedAt: 2_000,
    });
    const staleRequestSnapshot = makeOperation({
      status: "running",
      progress: 20,
      messageKey: "operation.phase.cancelling",
      finishedAt: null,
    });

    expect(mergeOperation([terminal], staleRequestSnapshot)).toEqual([
      terminal,
    ]);
  });

  it("puts a brand new operation at the front", () => {
    const merged = mergeOperation(
      [makeOperation()],
      makeOperation({ id: "op-9" }),
    );
    expect(merged.map((item) => item.id)).toEqual(["op-9", "op-1"]);
  });

  it("caps the history so a long session cannot grow without bound", () => {
    // Terminal status ("success"): eviction only trims terminal entries, and the
    // default "running" status never triggers eviction (see the "never evicts a
    // non-terminal operation" case below), so we must set a terminal status
    // explicitly to actually exercise the OPERATION_HISTORY_LIMIT.
    let list: Operation[] = [];
    for (let index = 0; index < OPERATION_HISTORY_LIMIT + 5; index += 1) {
      list = mergeOperation(
        list,
        makeOperation({ id: `op-${index}`, status: "success" }),
      );
    }
    expect(list).toHaveLength(OPERATION_HISTORY_LIMIT);
    expect(list[0].id).toBe(`op-${OPERATION_HISTORY_LIMIT + 4}`);
  });

  it("never evicts a non-terminal operation even past the history limit", () => {
    // 20 terminal entries plus 1 running entry at the tail (oldest position).
    const terminalOps = Array.from(
      { length: OPERATION_HISTORY_LIMIT },
      (_, i) => makeOperation({ id: `op-terminal-${i}`, status: "success" }),
    );
    const runningOp = makeOperation({ id: "op-running", status: "running" });
    const current = [...terminalOps, runningOp];

    const merged = mergeOperation(
      current,
      makeOperation({ id: "op-new", status: "queued" }),
    );

    // The running entry is never evicted, even if that means trimming one extra terminal entry.
    expect(merged.some((op) => op.id === "op-running")).toBe(true);
    expect(merged).toHaveLength(OPERATION_HISTORY_LIMIT);
    // The two evicted entries are the oldest terminal ones at the tail; terminal entries closer to the head are kept as-is.
    expect(merged.some((op) => op.id === "op-terminal-19")).toBe(false);
    expect(merged.some((op) => op.id === "op-terminal-18")).toBe(false);
    expect(merged.some((op) => op.id === "op-terminal-17")).toBe(true);
  });

  it("never mutates the array it was handed", () => {
    const current = [makeOperation()];
    const frozen = Object.freeze(current.slice());
    expect(() =>
      mergeOperation(frozen, makeOperation({ progress: 99 })),
    ).not.toThrow();
    expect(current[0].progress).toBe(20);
  });

  it("sorts newest first and keeps operations without a start time last", () => {
    const sorted = sortByRecency([
      makeOperation({ id: "a", startedAt: 10 }),
      makeOperation({ id: "b", startedAt: null }),
      makeOperation({ id: "c", startedAt: 30 }),
    ]);
    expect(sorted.map((item) => item.id)).toEqual(["c", "a", "b"]);
  });

  it("knows which statuses are final", () => {
    expect(isTerminal("success")).toBe(true);
    expect(isTerminal("failed")).toBe(true);
    expect(isTerminal("cancelled")).toBe(true);
    expect(isTerminal("queued")).toBe(false);
    expect(isTerminal("running")).toBe(false);
  });
});

describe("reconcileOperations", () => {
  it("returns the fetched snapshot untouched when there is no cache yet", () => {
    const fetched = [makeOperation({ id: "op-1" })];
    expect(reconcileOperations(fetched, undefined)).toEqual(fetched);
    expect(reconcileOperations(fetched, [])).toEqual(fetched);
  });

  it("keeps the cached terminal value when the fetched snapshot is a stale running copy", () => {
    // Reproduces Finding 2: after a restart, the terminal event lands in the
    // cache before the fetch resolves, and the fetch then returns an earlier
    // "running" snapshot.
    const fetched = [
      makeOperation({ id: "op-1", status: "running", progress: 40 }),
    ];
    const cached = [
      makeOperation({
        id: "op-1",
        status: "success",
        progress: 100,
        finishedAt: 2_000,
      }),
    ];
    const reconciled = reconcileOperations(fetched, cached);
    expect(reconciled).toHaveLength(1);
    expect(reconciled[0].status).toBe("success");
    expect(reconciled[0].finishedAt).toBe(2_000);
  });

  it("prefers the fetched value once it has caught up to (or past) the cache", () => {
    const fetched = [
      makeOperation({ id: "op-1", status: "success", finishedAt: 3_000 }),
    ];
    const cached = [
      makeOperation({ id: "op-1", status: "running", progress: 50 }),
    ];
    const reconciled = reconcileOperations(fetched, cached);
    expect(reconciled[0].status).toBe("success");
    expect(reconciled[0].finishedAt).toBe(3_000);
  });

  it("keeps a cache-only non-terminal operation that fetch has not seen yet", () => {
    const fetched: Operation[] = [];
    const cached = [makeOperation({ id: "op-2", status: "running" })];
    const reconciled = reconcileOperations(fetched, cached);
    expect(reconciled.map((op) => op.id)).toEqual(["op-2"]);
  });

  it("keeps a cache-only terminal operation that finished before the fetch snapshot was taken", () => {
    const fetched: Operation[] = [];
    const cached = [makeOperation({ id: "op-3", status: "success" })];
    const reconciled = reconcileOperations(fetched, cached);
    expect(reconciled.map((op) => op.id)).toEqual(["op-3"]);
  });

  it("does not mutate either input array", () => {
    const fetched = Object.freeze([
      makeOperation({ id: "op-1", status: "running" }),
    ]);
    const cached = Object.freeze([
      makeOperation({ id: "op-1", status: "success" }),
      makeOperation({ id: "op-2", status: "success" }),
    ]);
    expect(() => reconcileOperations(fetched, cached)).not.toThrow();
    expect(fetched[0].status).toBe("running");
    expect(cached[0].status).toBe("success");
  });
});
