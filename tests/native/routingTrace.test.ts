import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  native,
  NativeError,
  onRoutingTrace,
  ROUTING_TRACE_EVENT,
  routingTraceUpdateSchema,
} from "@/native";
import { emitTauriEvent } from "../msw/tauriMocks";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const traceEntry = {
  seq: 7,
  revision: 12,
  startedAt: 1_790_000_000_000,
  tool: "codex",
  model: "gpt-5",
  attempts: [
    {
      providerId: "a",
      providerName: "Service A",
      outcome: "failed",
      httpStatus: 429,
      error: "rateLimited",
      ms: 120,
    },
    {
      providerId: "b",
      providerName: "Service B",
      outcome: "ok",
      httpStatus: null,
      error: null,
      ms: 800,
    },
  ],
  status: "ok",
  error: null,
  totalMs: 950,
  failedOver: true,
};

const snapshot = {
  revision: 12,
  counts: { requests: 7, rerouted: 1, failed: 0 },
  entries: [traceEntry],
};

const overview = {
  running: true,
  address: "127.0.0.1",
  port: 15_721,
  activeConnections: 0,
  totalRequests: 0,
  successRequests: 0,
  failedRequests: 0,
  failoverCount: 0,
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

describe("native.routing live mode and trace", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("reads the trace snapshot through the product command", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_trace`, () =>
        HttpResponse.json(snapshot),
      ),
    );
    await expect(native.routing.trace()).resolves.toEqual(snapshot);
  });

  it("sends only the switch position and parses per-tool failures", async () => {
    const bodies: unknown[] = [];
    const outcome = {
      overview,
      failures: [
        {
          tool: "gemini-cli",
          error: {
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.routing.liveTakeoverFailed",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          },
        },
      ],
    };
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_routing_set_live_mode`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json(outcome);
        },
      ),
    );
    await expect(native.routing.setLiveMode(true)).resolves.toEqual(outcome);
    expect(bodies).toEqual([{ enabled: true }]);
  });

  it.each([
    ["a request body", { ...traceEntry, requestBody: '{"messages":[]}' }],
    [
      "an upstream error body",
      {
        ...traceEntry,
        attempts: [{ ...traceEntry.attempts[0], body: "Bearer sk-private" }],
      },
    ],
    ["a provider URL", { ...traceEntry, baseUrl: "https://relay.example" }],
    ["an unknown category", { ...traceEntry, error: "Bearer sk-private" }],
  ])("fails closed when an entry carries %s", async (_label, entry) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_trace`, () =>
        HttpResponse.json({ ...snapshot, entries: [entry] }),
      ),
    );
    await expect(native.routing.trace()).rejects.toBeInstanceOf(NativeError);
    expect(
      routingTraceUpdateSchema.safeParse({
        revision: 1,
        counts: snapshot.counts,
        entry,
      }).success,
    ).toBe(false);
  });

  it("delivers valid pushed updates and drops malformed ones", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const received: unknown[] = [];
    const unlisten = await onRoutingTrace((update) => received.push(update));
    const update = { revision: 12, counts: snapshot.counts, entry: traceEntry };

    emitTauriEvent(ROUTING_TRACE_EVENT, update);
    emitTauriEvent(ROUTING_TRACE_EVENT, { ...update, entry: { seq: 1 } });

    expect(received).toEqual([update]);
    unlisten();
  });
});
