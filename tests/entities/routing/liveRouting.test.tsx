import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  mergeRoutingTraceSnapshot,
  mergeRoutingTraceUpdate,
  routingKeys,
  useConfirmQuit,
  useQuitRequest,
  useRoutingTrace,
  type RoutingTraceEntry,
  type RoutingTraceSnapshot,
} from "@/entities/routing";
import {
  MAX_ROUTING_TRACE_ENTRIES,
  QUIT_REQUESTED_EVENT,
  ROUTING_TRACE_EVENT,
} from "@/native";
import { emitTauriEvent } from "../../msw/tauriMocks";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const COUNTS = { requests: 0, rerouted: 0, failed: 0 };

function entry(
  seq: number,
  revision = seq,
  status: RoutingTraceEntry["status"] = "ok",
): RoutingTraceEntry {
  return {
    seq,
    revision,
    startedAt: 1_790_000_000_000 + seq,
    tool: "claude-code",
    model: "sonnet",
    attempts: [],
    status,
    error: null,
    totalMs: status === "pending" ? null : 10,
    failedOver: false,
  };
}

function snapshot(
  entries: RoutingTraceEntry[],
  revision: number,
  requests = entries.length,
): RoutingTraceSnapshot {
  return { revision, counts: { ...COUNTS, requests }, entries };
}

describe("routing trace merging", () => {
  it("keeps the newer copy of an entry whichever arrives last", () => {
    const pending = entry(3, 5, "pending");
    const finished = entry(3, 6, "ok");
    const fromUpdate = mergeRoutingTraceUpdate(snapshot([pending], 5), {
      revision: 6,
      counts: { ...COUNTS, requests: 3 },
      entry: finished,
    });
    // A stale snapshot read that raced the push must not roll it back.
    const merged = mergeRoutingTraceSnapshot(
      fromUpdate,
      snapshot([pending], 5, 2),
    );
    expect(merged.entries).toEqual([finished]);
    expect(merged.counts.requests).toBe(3);
    expect(merged.revision).toBe(6);
  });

  it("orders newest first and caps like the native ring", () => {
    const many = Array.from(
      { length: MAX_ROUTING_TRACE_ENTRIES + 5 },
      (_, index) => entry(index + 1),
    );
    const merged = mergeRoutingTraceSnapshot(undefined, snapshot(many, 99));
    expect(merged.entries).toHaveLength(MAX_ROUTING_TRACE_ENTRIES);
    expect(merged.entries[0]?.seq).toBe(MAX_ROUTING_TRACE_ENTRIES + 5);
    expect(merged.entries.at(-1)?.seq).toBe(6);
  });
});

describe("useRoutingTrace", () => {
  it("seeds from the native ring and folds pushed changes into the cache", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_trace`, () =>
        HttpResponse.json(snapshot([entry(2), entry(1)], 2)),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useRoutingTrace(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(result.current.data?.entries).toHaveLength(2));

    act(() => {
      emitTauriEvent(ROUTING_TRACE_EVENT, {
        revision: 3,
        counts: { ...COUNTS, requests: 3 },
        entry: entry(3, 3, "pending"),
      });
    });
    await waitFor(() =>
      expect(result.current.data?.entries.map(({ seq }) => seq)).toEqual([
        3, 2, 1,
      ]),
    );
    expect(result.current.data?.counts.requests).toBe(3);
    expect(client.getQueryData(routingKeys.trace())).toEqual(
      result.current.data,
    );
  });
});

describe("useRoutingTrace while the seed read is in flight", () => {
  it("keeps a change pushed before the read resolves", async () => {
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_trace`, async () => {
        await gate;
        return HttpResponse.json(snapshot([entry(1)], 1));
      }),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useRoutingTrace(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(result.current.isFetching).toBe(true));

    act(() => {
      client.setQueryData(
        routingKeys.trace(),
        mergeRoutingTraceUpdate(undefined, {
          revision: 2,
          counts: { ...COUNTS, requests: 2 },
          entry: entry(2, 2, "pending"),
        }),
      );
    });
    release();
    await waitFor(() => expect(result.current.isFetching).toBe(false));
    expect(result.current.data?.entries.map(({ seq }) => seq)).toEqual([2, 1]);
  });
});

describe("useQuitRequest and useConfirmQuit", () => {
  it("holds the routed tools until dismissed and confirms through native", async () => {
    let confirmed = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_quit_confirmed`, () => {
        confirmed += 1;
        return HttpResponse.json(null);
      }),
    );
    const { result } = renderHook(
      () => ({ request: useQuitRequest(), confirm: useConfirmQuit() }),
      { wrapper: withQueryClient(createTestQueryClient()) },
    );
    expect(result.current.request.tools).toBeNull();

    const tools = [{ tool: "codex", pickup: "atStart" }] as const;
    await waitFor(() => {
      act(() => emitTauriEvent(QUIT_REQUESTED_EVENT, { tools }));
      expect(result.current.request.tools).toEqual(tools);
    });

    act(() => result.current.request.dismiss());
    expect(result.current.request.tools).toBeNull();

    await act(async () => {
      await result.current.confirm.mutateAsync();
    });
    expect(confirmed).toBe(1);
  });
});
