import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { native, NativeError } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const overview = {
  running: true,
  address: "127.0.0.1",
  port: 15_721,
  activeConnections: 1,
  totalRequests: 8,
  successRequests: 7,
  failedRequests: 1,
  failoverCount: 1,
  targets: ["claude-code", "codex", "gemini-cli", "grok-build"].map(
    (tool, index) => ({
      tool,
      takeoverEnabled: index === 0,
      autoFailoverEnabled: false,
      currentProvider: null,
      queue: [],
      available: [],
    }),
  ),
};

describe("native.routing", () => {
  let bodies: unknown[];

  beforeEach(() => {
    bodies = [];
  });

  it.each([
    ["app_routing_overview", () => native.routing.overview(), {}],
    [
      "app_routing_set_takeover",
      () => native.routing.setTakeover("codex", true),
      { tool: "codex", enabled: true },
    ],
    [
      "app_routing_set_failover",
      () => native.routing.setFailover("codex", true),
      { tool: "codex", enabled: true },
    ],
    [
      "app_routing_queue_add",
      () => native.routing.addToQueue("codex", "provider-a"),
      { tool: "codex", providerId: "provider-a" },
    ],
    [
      "app_routing_queue_remove",
      () => native.routing.removeFromQueue("codex", "provider-a"),
      { tool: "codex", providerId: "provider-a" },
    ],
    [
      "app_routing_switch_provider",
      () => native.routing.switchProvider("codex", "provider-a"),
      { tool: "codex", providerId: "provider-a" },
    ],
    ["app_routing_stop_all", () => native.routing.stopAll(), {}],
  ] as const)(
    "validates %s and sends only the product payload",
    async (command, call, expected) => {
      server.use(
        http.post(`${TAURI_ENDPOINT}/${command}`, async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json(overview);
        }),
      );

      await expect(call()).resolves.toEqual(overview);
      expect(bodies).toEqual([expected]);
    },
  );

  it.each([
    ["raw request error", { ...overview, lastError: "Bearer private" }],
    [
      "provider settings",
      {
        ...overview,
        targets: overview.targets.map((target, index) =>
          index === 0
            ? { ...target, settingsConfig: { apiKey: "private" } }
            : target,
        ),
      },
    ],
    ["endpoint while stopped", { ...overview, running: false }],
    [
      "duplicate target",
      {
        ...overview,
        targets: overview.targets.map((target) => ({
          ...target,
          tool: "codex",
        })),
      },
    ],
  ])("fails closed when the response contains %s", async (_label, response) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
        HttpResponse.json(response),
      ),
    );

    await expect(native.routing.overview()).rejects.toBeInstanceOf(NativeError);
  });
});
