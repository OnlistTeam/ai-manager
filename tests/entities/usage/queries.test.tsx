import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  USAGE_AUTO_SYNC_GAP_MS,
  usageKeys,
  useRefreshUsage,
  useUsageOverview,
} from "@/entities/usage";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const overview = {
  periodDays: 30,
  startDate: "2026-07-26",
  endDate: "2026-08-24",
  summary: {
    requests: 2,
    estimatedCostUsd: "0.125000",
    tokens: 1_024,
    successRatePercent: 100,
    cacheHitRatePercent: 50,
  },
  byTool: [],
  trend: [],
};

afterEach(() => vi.restoreAllMocks());

describe("usage queries", () => {
  it("loads the aggregate overview under a dedicated cache key", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
        HttpResponse.json(overview),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useUsageOverview(), {
      wrapper: withQueryClient(client),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(overview);
    expect(client.getQueryData(usageKeys.overview())).toEqual(overview);
  });

  it("replaces the cached snapshot with the explicitly refreshed overview", async () => {
    const refreshed = {
      ...overview,
      summary: { ...overview.summary, requests: 7, tokens: 4_096 },
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, () =>
        HttpResponse.json({
          overview: refreshed,
          sync: {
            filesScanned: 3,
            recordsImported: 5,
            recordsSkipped: 0,
            sourceIssues: 0,
          },
        }),
      ),
    );
    const client = createTestQueryClient();
    client.setQueryData(usageKeys.overview(), overview);
    const { result } = renderHook(() => useRefreshUsage(), {
      wrapper: withQueryClient(client),
    });

    result.current.mutate();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(usageKeys.overview())).toEqual(refreshed);
  });

  it("starts an automatic sync only when none began within the gap", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, () => {
        calls += 1;
        return HttpResponse.json({
          overview,
          sync: {
            filesScanned: 1,
            recordsImported: 0,
            recordsSkipped: 0,
            sourceIssues: 0,
          },
        });
      }),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useRefreshUsage(), {
      wrapper: withQueryClient(client),
    });

    act(() => result.current.syncIfDue());
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    act(() => result.current.syncIfDue());
    expect(client.isMutating()).toBe(0);
    expect(calls).toBe(1);

    const later = Date.now() + USAGE_AUTO_SYNC_GAP_MS;
    vi.spyOn(Date, "now").mockReturnValue(later);
    act(() => result.current.syncIfDue());
    await waitFor(() => expect(calls).toBe(2));
  });
});
