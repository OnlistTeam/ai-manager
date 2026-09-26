import i18n from "i18next";
import { beforeAll, describe, expect, it } from "vitest";
import type {
  RoutingProvider,
  RoutingTarget,
  RoutingTraceAttempt,
  RoutingTraceEntry,
} from "@/entities/routing";
import en from "@/i18n/locales/en.json";
import zh from "@/i18n/locales/zh.json";
import { planFlight } from "@/features/live-routing/flightPlan";
import { liveCaption } from "@/features/live-routing/liveCaption";
import {
  endpointMarks,
  stageEndpoints,
  stageTools,
} from "@/features/live-routing/stageModel";

function provider(id: string, name: string, healthy = true): RoutingProvider {
  return {
    id,
    name,
    priority: 1,
    current: false,
    healthy,
    consecutiveFailures: healthy ? 0 : 3,
  };
}

function attempt(
  id: string,
  name: string,
  outcome: RoutingTraceAttempt["outcome"],
  error: RoutingTraceAttempt["error"] = null,
  ms = 420,
): RoutingTraceAttempt {
  return {
    providerId: id,
    providerName: name,
    outcome,
    httpStatus: null,
    error,
    ms,
  };
}

function entry(
  attempts: RoutingTraceAttempt[],
  status: RoutingTraceEntry["status"],
  overrides: Partial<RoutingTraceEntry> = {},
): RoutingTraceEntry {
  return {
    seq: 1,
    revision: 1,
    startedAt: 0,
    tool: "claude-code",
    model: null,
    attempts,
    status,
    error: null,
    totalMs: status === "pending" ? null : 2_310,
    failedOver: attempts.length > 1,
    ...overrides,
  };
}

const target: RoutingTarget = {
  tool: "claude-code",
  takeoverEnabled: true,
  autoFailoverEnabled: true,
  currentProvider: provider("anthropic", "Anthropic"),
  queue: [
    provider("anthropic", "Anthropic"),
    provider("gateway", "Shared Team Gateway"),
    provider("jordan", "jordan.lee@example.com", false),
  ],
  available: [],
};

describe("stage model", () => {
  it("keeps the routing order however a request went", () => {
    const direct = entry([attempt("anthropic", "Anthropic", "ok")], "ok");
    const rerouted = entry(
      [
        attempt("jordan", "jordan.lee@example.com", "skipped"),
        attempt("gateway", "Shared Team Gateway", "ok"),
      ],
      "ok",
    );
    const ids = (value: RoutingTraceEntry) =>
      stageEndpoints(target, value).map((endpoint) => endpoint.id);
    expect(ids(direct)).toEqual(["anthropic", "gateway", "jordan"]);
    expect(ids(rerouted)).toEqual(["anthropic", "gateway", "jordan"]);
  });

  it("uses the current service without failover and appends unknown tries", () => {
    const single = { ...target, autoFailoverEnabled: false };
    const value = entry(
      [
        attempt("anthropic", "Anthropic", "failed", "timeout"),
        attempt("backup", "Backup API", "ok"),
      ],
      "ok",
    );
    expect(stageEndpoints(single, value).map((row) => row.id)).toEqual([
      "anthropic",
      "backup",
    ]);
    expect(stageEndpoints(undefined, entry([], "pending"))).toEqual([]);
  });

  it("marks only the tries the dot has reached, and resting services", () => {
    const value = entry(
      [
        attempt("anthropic", "Anthropic", "failed", "rateLimited"),
        attempt("gateway", "Shared Team Gateway", "pending"),
      ],
      "pending",
    );
    const endpoints = stageEndpoints(target, value);
    expect(Object.fromEntries(endpointMarks(endpoints, value, 1))).toEqual({
      anthropic: { kind: "failed", error: "rateLimited" },
      jordan: { kind: "resting" },
    });
    expect(endpointMarks(endpoints, value, 2).get("gateway")).toEqual({
      kind: "pending",
    });
  });

  it("lists routed tools in target order, plus tools still in the list", () => {
    const overview = {
      running: true,
      address: "127.0.0.1",
      port: 1,
      activeConnections: 0,
      totalRequests: 0,
      successRequests: 0,
      failedRequests: 0,
      failoverCount: 0,
      targets: (
        ["claude-code", "codex", "gemini-cli", "grok-build"] as const
      ).map((tool) => ({
        ...target,
        tool,
        takeoverEnabled: tool === "gemini-cli" || tool === "claude-code",
      })),
    };
    expect(stageTools(overview, [])).toEqual(["claude-code", "gemini-cli"]);
    expect(
      stageTools(overview, [entry([], "ok", { tool: "grok-build" })]),
    ).toEqual(["claude-code", "gemini-cli", "grok-build"]);
  });
});

describe("liveCaption", () => {
  beforeAll(() => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { routing: en.routing },
      true,
      true,
    );
    i18n.addResourceBundle(
      "zh",
      "translation",
      { routing: zh.routing },
      true,
      true,
    );
  });

  function caption(
    value: RoutingTraceEntry,
    done: number,
    language = "en",
    hideEmails = true,
  ) {
    return liveCaption(value, planFlight(value), done, {
      t: i18n.getFixedT(language),
      language,
      hideEmails,
    });
  }

  const rerouted = entry(
    [
      attempt("anthropic", "Anthropic", "failed", "rateLimited"),
      attempt("gateway", "Shared Team Gateway", "ok", null, 1_840),
    ],
    "ok",
  );

  it("narrates each step of a rerouted request in plain words", () => {
    expect(caption(rerouted, 0, "zh")).toBe(
      "Claude Code 的请求发给了 Anthropic…",
    );
    expect(caption(rerouted, 2, "zh")).toBe("Anthropic 限流");
    expect(caption(rerouted, 3, "zh")).toBe(
      "Anthropic 限流，改走 Shared Team Gateway",
    );
    expect(caption(rerouted, 5, "zh")).toBe("Shared Team Gateway 用 2.3秒答完");
    expect(caption(rerouted, 99, "en")).toBe(
      "Shared Team Gateway answered in 2.3s",
    );
  });

  it("says when a request waits for a service and when the tool gets an error", () => {
    expect(caption(entry([], "pending"), 1)).toBe(
      "Claude Code sent a request; choosing a service…",
    );
    const failed = entry(
      [attempt("jordan", "jordan.lee@example.com", "failed", "timeout")],
      "failed",
      { error: "timeout" },
    );
    expect(caption(failed, 2)).toBe("j***@example.com: Timed out");
    expect(caption(failed, 2, "en", false)).toBe(
      "jordan.lee@example.com: Timed out",
    );
    expect(caption(failed, 99)).toBe("Claude Code got an error: Timed out");
  });

  it("names a paused service that was passed over", () => {
    const skipped = entry(
      [
        attempt("jordan", "jordan.lee@example.com", "skipped"),
        attempt("gateway", "Shared Team Gateway", "pending"),
      ],
      "pending",
    );
    expect(caption(skipped, 0)).toBe(
      "Claude Code sent a request; choosing a service…",
    );
    expect(caption(skipped, 2)).toBe(
      "j***@example.com is paused. Trying Shared Team Gateway instead",
    );
  });
});
