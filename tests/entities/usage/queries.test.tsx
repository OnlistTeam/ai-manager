import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { usageKeys, useRefreshUsage, useUsageOverview } from "@/entities/usage";
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
});
