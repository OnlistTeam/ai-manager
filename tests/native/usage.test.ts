import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { native, NativeError } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const overview = {
  periodDays: 30,
  startDate: "2026-07-26",
  endDate: "2026-08-24",
  summary: {
    requests: 12,
    estimatedCostUsd: "1.250000",
    tokens: 25_000,
    successRatePercent: 91.5,
    cacheHitRatePercent: 42,
  },
  byTool: [
    {
      tool: "codex",
      metrics: {
        requests: 12,
        estimatedCostUsd: "1.250000",
        tokens: 25_000,
        successRatePercent: 91.5,
        cacheHitRatePercent: 42,
      },
    },
  ],
  trend: [
    {
      date: "2026-08-24",
      requests: 12,
      estimatedCostUsd: "1.250000",
      tokens: 25_000,
    },
  ],
};

describe("native.usage", () => {
  let seen: unknown[];

  beforeEach(() => {
    seen = [];
  });

  it("reads the aggregate overview without sending identifiers", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json(overview);
      }),
    );

    await expect(native.usage.overview()).resolves.toEqual(overview);
    expect(seen).toEqual([{}]);
  });

  it("refreshes local records and returns only sanitized counts", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json({
          overview,
          sync: {
            filesScanned: 4,
            recordsImported: 8,
            recordsSkipped: 2,
            sourceIssues: 0,
          },
        });
      }),
    );

    await expect(native.usage.refresh()).resolves.toMatchObject({
      overview,
      sync: { filesScanned: 4, recordsImported: 8 },
    });
    expect(seen).toEqual([{}]);
  });

  it.each([
    ["raw prompt", { ...overview, prompt: "private prompt" }],
    [
      "raw path",
      {
        ...overview,
        trend: [{ ...overview.trend[0], sourcePath: "/Users/alice/.codex" }],
      },
    ],
    [
      "invalid percentage",
      {
        ...overview,
        summary: { ...overview.summary, successRatePercent: 101 },
      },
    ],
    [
      "unscaled cost",
      {
        ...overview,
        summary: { ...overview.summary, estimatedCostUsd: "1.25" },
      },
    ],
  ])("fails closed for %s in a response", async (_label, response) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
        HttpResponse.json(response),
      ),
    );

    await expect(native.usage.overview()).rejects.toBeInstanceOf(NativeError);
  });
});
