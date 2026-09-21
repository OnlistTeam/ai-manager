import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  routingKeys,
  useRoutingOverview,
  useSetRoutingTakeover,
} from "@/entities/routing";
import { providerKeys } from "@/entities/provider";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

function overview(takeoverEnabled = false) {
  return {
    running: takeoverEnabled,
    address: takeoverEnabled ? "127.0.0.1" : null,
    port: takeoverEnabled ? 15_721 : null,
    activeConnections: 0,
    totalRequests: 0,
    successRequests: 0,
    failedRequests: 0,
    failoverCount: 0,
    targets: ["claude-code", "codex", "gemini-cli", "grok-build"].map(
      (tool, index) => ({
        tool,
        takeoverEnabled: takeoverEnabled && index === 0,
        autoFailoverEnabled: false,
        currentProvider: null,
        queue: [],
        available: [],
      }),
    ),
  };
}

describe("routing queries", () => {
  it("loads the sanitized overview under its own cache key", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
        HttpResponse.json(overview()),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useRoutingOverview(), {
      wrapper: withQueryClient(client),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(routingKeys.overview())).toEqual(overview());
  });

  it("commits a mutation snapshot and invalidates service lists", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_set_takeover`, () =>
        HttpResponse.json(overview(true)),
      ),
    );
    const client = createTestQueryClient();
    client.setQueryData(routingKeys.overview(), overview());
    client.setQueryData(providerKeys.list("claude-code"), []);
    const { result } = renderHook(() => useSetRoutingTakeover(), {
      wrapper: withQueryClient(client),
    });

    result.current.mutate({ tool: "claude-code", enabled: true });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(routingKeys.overview())).toEqual(overview(true));
    expect(
      client.getQueryState(providerKeys.list("claude-code"))?.isInvalidated,
    ).toBe(true);
  });
});
