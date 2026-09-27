import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  native,
  NativeError,
  onQuitRequested,
  onRoutingTrace,
  QUIT_REQUESTED_EVENT,
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

describe("native.routing trace and quit", () => {
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

  it("confirms a held quit through the product command", async () => {
    const calls: string[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_quit_confirmed`, ({ request }) => {
        calls.push(new URL(request.url).pathname);
        return HttpResponse.json(null);
      }),
    );
    await expect(native.routing.confirmQuit()).resolves.toBeNull();
    expect(calls).toEqual(["/app_quit_confirmed"]);
  });

  it("delivers a held quit with each routed tool and drops malformed ones", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const received: unknown[] = [];
    const unlisten = await onQuitRequested((payload) => received.push(payload));
    const payload = {
      tools: [
        { tool: "claude-code", pickup: "live" },
        { tool: "codex", pickup: "atStart" },
      ],
    };

    emitTauriEvent(QUIT_REQUESTED_EVENT, payload);
    emitTauriEvent(QUIT_REQUESTED_EVENT, { tools: [] });
    emitTauriEvent(QUIT_REQUESTED_EVENT, {
      tools: [{ tool: "codex", pickup: "sometimes" }],
    });

    expect(received).toEqual([payload]);
    unlisten();
  });

  it("accepts a try that is still in progress", () => {
    const inProgress = {
      ...traceEntry,
      attempts: [{ ...traceEntry.attempts[1], outcome: "pending", ms: 0 }],
      status: "pending",
      totalMs: null,
      failedOver: false,
    };
    expect(
      routingTraceUpdateSchema.safeParse({
        revision: 13,
        counts: snapshot.counts,
        entry: inProgress,
      }).success,
    ).toBe(true);
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
