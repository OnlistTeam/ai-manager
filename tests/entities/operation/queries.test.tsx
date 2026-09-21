import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  operationKeys,
  useCachedOperations,
  useOperations,
} from "@/entities/operation";
import type { Operation } from "@/entities/operation";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

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

describe("useOperations", () => {
  it("observes the shared cache without making a native baseline request", () => {
    const client = createTestQueryClient();
    const { result } = renderHook(() => useCachedOperations(), {
      wrapper: withQueryClient(client),
    });
    expect(result.current).toEqual([]);
    act(() => {
      client.setQueryData(operationKeys.list(), [makeOperation()]);
    });
    expect(result.current).toHaveLength(1);
  });

  it("returns the fetched list as-is when the cache is empty", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([makeOperation()]),
      ),
    );
    const { result } = renderHook(() => useOperations(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([makeOperation()]);
  });

  it("keeps a terminal event that beat a stale fetch snapshot to the cache", async () => {
    // Reproduces Finding 2: before the list() request resolves, a terminal
    // `operation://changed` event lands in the cache first; the fetch that
    // follows then returns an earlier "running" snapshot. After reconcile,
    // this op should stay terminal in the cache instead of regressing to
    // "running".
    let releaseResponse: (() => void) | undefined;
    const responseGate = new Promise<void>((resolve) => {
      releaseResponse = resolve;
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, async () => {
        await responseGate;
        return HttpResponse.json([
          makeOperation({ id: "op-1", status: "running", progress: 40 }),
        ]);
      }),
    );

    const client = createTestQueryClient();
    const { result } = renderHook(() => useOperations(), {
      wrapper: withQueryClient(client),
    });

    // While the fetch is still stuck on responseGate, the event bridge writes the terminal status into the cache first.
    client.setQueryData<Operation[]>(operationKeys.list(), [
      makeOperation({
        id: "op-1",
        status: "success",
        progress: 100,
        finishedAt: 2_000,
      }),
    ]);

    releaseResponse?.();

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toHaveLength(1);
    expect(result.current.data?.[0].status).toBe("success");
    expect(result.current.data?.[0].finishedAt).toBe(2_000);
  });
});
