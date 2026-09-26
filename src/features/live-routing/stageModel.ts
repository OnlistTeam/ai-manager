import type {
  RoutingErrorCategory,
  RoutingOverview,
  RoutingTarget,
  RoutingTraceEntry,
  ToolId,
} from "@/entities/routing";

/** One service row on the right of the stage. */
export interface StageEndpoint {
  id: string;
  name: string;
  /** Paused after repeated failures; routing passes over it for now. */
  resting: boolean;
}

export type EndpointMark =
  | { kind: "pending" }
  | { kind: "ok"; ms: number }
  | { kind: "failed"; error: RoutingErrorCategory }
  | { kind: "skipped" }
  | { kind: "resting" };

/**
 * Each tool's dots and marks share one natural hue, so a request can be
 * followed by colour as well as by its wire. Success and failure use the
 * status tokens instead.
 */
const TOOL_HUES: Partial<Record<ToolId, string>> = {
  "claude-code": "18 60% 57%",
  codex: "44 55% 50%",
  "gemini-cli": "100 30% 47%",
  "grok-build": "28 16% 52%",
};
const FALLBACK_HUE = "30 8% 55%";

export function toolHue(tool: ToolId): string {
  return TOOL_HUES[tool] ?? FALLBACK_HUE;
}

/**
 * Tools shown on the left: every tool that goes through AI Manager now, plus
 * any tool that still has a request in the recent list, in the routing
 * targets' order.
 */
export function stageTools(
  overview: RoutingOverview | undefined,
  entries: readonly RoutingTraceEntry[],
): ToolId[] {
  const seen = new Set(entries.map((entry) => entry.tool));
  const ordered = (overview?.targets ?? [])
    .filter((target) => target.takeoverEnabled || seen.has(target.tool))
    .map((target) => target.tool);
  const extra = [...seen].filter((tool) => !ordered.includes(tool));
  return [...ordered, ...extra];
}

/**
 * The services a tool's requests are routed to, in the order routing tries
 * them: its failover queue when failover is on, else its current service.
 * A service a request actually tried that is not in that list is added at
 * the end, so the rows keep their places from one request to the next.
 */
export function stageEndpoints(
  target: RoutingTarget | undefined,
  entry: RoutingTraceEntry | undefined,
): StageEndpoint[] {
  const configured =
    target && target.autoFailoverEnabled && target.queue.length > 0
      ? target.queue
      : target?.currentProvider
        ? [target.currentProvider]
        : [];
  const rows: StageEndpoint[] = configured.map((provider) => ({
    id: provider.id,
    name: provider.name,
    resting: !provider.healthy,
  }));
  for (const attempt of entry?.attempts ?? []) {
    if (!rows.some((row) => row.id === attempt.providerId)) {
      rows.push({
        id: attempt.providerId,
        name: attempt.providerName,
        resting: false,
      });
    }
  }
  return rows;
}

/**
 * The mark after each service row: what happened on the latest request's
 * tries the stage has already reached, else whether the service is resting.
 */
export function endpointMarks(
  endpoints: readonly StageEndpoint[],
  entry: RoutingTraceEntry | undefined,
  revealed: number,
): Map<string, EndpointMark> {
  const marks = new Map<string, EndpointMark>();
  for (const endpoint of endpoints) {
    if (endpoint.resting) marks.set(endpoint.id, { kind: "resting" });
  }
  for (const attempt of entry?.attempts.slice(0, revealed) ?? []) {
    marks.set(
      attempt.providerId,
      attempt.outcome === "ok"
        ? { kind: "ok", ms: attempt.ms }
        : attempt.outcome === "failed"
          ? { kind: "failed", error: attempt.error ?? "other" }
          : { kind: attempt.outcome },
    );
  }
  return marks;
}
