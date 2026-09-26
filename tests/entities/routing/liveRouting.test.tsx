import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  mergeRoutingTraceSnapshot,
  mergeRoutingTraceUpdate,
  routingKeys,
  useRoutingTrace,
  useSetLiveRoutingMode,
  type RoutingTraceEntry,
  type RoutingTraceSnapshot,
} from "@/entities/routing";
import { MAX_ROUTING_TRACE_ENTRIES, ROUTING_TRACE_EVENT } from "@/native";
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

describe("useSetLiveRoutingMode", () => {
  it("commits the returned overview", async () => {
    const overview = {
      running: false,
      address: null,
      port: null,
      activeConnections: 0,
      totalRequests: 0,
      successRequests: 0,
      failedRequests: 0,
      failoverCount: 0,
      targets: ["claude-code", "codex", "gemini-cli", "grok-build"].map(
        (tool) => ({
          tool,
          takeoverEnabled: false,
          autoFailoverEnabled: false,
          currentProvider: null,
          queue: [],
          available: [],
        }),
      ),
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_set_live_mode`, () =>
        HttpResponse.json({ overview, failures: [] }),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useSetLiveRoutingMode(), {
      wrapper: withQueryClient(client),
    });
    await act(async () => {
      await result.current.mutateAsync(false);
    });
    expect(client.getQueryData(routingKeys.overview())).toEqual(overview);
  });
});
