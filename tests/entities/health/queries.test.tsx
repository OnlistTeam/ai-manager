import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  canonicalHealthTools,
  connectivityFor,
  healthKeys,
  useHealthSnapshot,
  clearProviderConnectivityBatch,
  writeProviderConnectivityBatch,
  writeProviderConnectivity,
} from "@/entities/health";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const EMPTY_SNAPSHOT = {
  providers: [],
  configs: [],
  mcp: { total: 0, enabled: 0 },
};

describe("health queries", () => {
  it("deduplicates and canonicalizes the installed set for key and request", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_health_snapshot`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json(EMPTY_SNAPSHOT);
        },
      ),
    );
    const input = ["codex", "claude-code", "codex"] as const;
    const { result } = renderHook(() => useHealthSnapshot(input), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(canonicalHealthTools(input)).toEqual(["claude-code", "codex"]);
    expect(healthKeys.snapshot(input)).toEqual([
      "health",
      "snapshot",
      "claude-code",
      "codex",
    ]);
    expect(seen).toEqual([{ tools: ["claude-code", "codex"] }]);
  });

  it("stores independent manual results by tool and provider id", () => {
    const client = createTestQueryClient();
    const first = {
      providerId: "shared",
      reachability: "operational" as const,
      responseTimeMs: 120,
      httpStatus: 200,
    };
    const second = {
      providerId: "shared",
      reachability: "failed" as const,
      responseTimeMs: null,
      httpStatus: null,
    };
    writeProviderConnectivity(client, "claude-code", "shared", first);
    writeProviderConnectivity(client, "codex", "shared", second);

    const cache = client.getQueryData(healthKeys.connectivity()) ?? {};
    expect(connectivityFor(cache, "claude-code", "shared")).toEqual(first);
    expect(connectivityFor(cache, "codex", "shared")).toEqual(second);
  });

  it("applies and clears a provider batch atomically without touching other tools", () => {
    const client = createTestQueryClient();
    const first = {
      providerId: "first",
      reachability: "operational" as const,
      responseTimeMs: 80,
      httpStatus: 200,
    };
    const second = {
      providerId: "second",
      reachability: "failed" as const,
      responseTimeMs: null,
      httpStatus: null,
    };
    writeProviderConnectivity(client, "codex", "kept", first);
    writeProviderConnectivityBatch(client, "claude-code", [first, second]);
    clearProviderConnectivityBatch(client, "claude-code", ["first"]);

    const cache = client.getQueryData(healthKeys.connectivity()) ?? {};
    expect(connectivityFor(cache, "claude-code", "first")).toBeUndefined();
    expect(connectivityFor(cache, "claude-code", "second")).toEqual(second);
    expect(connectivityFor(cache, "codex", "kept")).toEqual(first);
  });
});
