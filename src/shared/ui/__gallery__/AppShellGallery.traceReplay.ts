import type { QueryClient } from "@tanstack/react-query";
import {
  EMPTY_ROUTING_TRACE,
  mergeRoutingTraceUpdate,
  routingKeys,
} from "@/entities/routing";
import type {
  RoutingTraceAttempt,
  RoutingTraceEntry,
  RoutingTraceSnapshot,
  ToolId,
} from "@/native";
import { traceAttempt as attempt } from "./AppShellGallery.fixtures";

/**
 * Gallery only: replays a few scripted requests into the routing trace cache
 * the way the native `routing://trace` event would, so the live stage can be
 * seen and captured mid-flight. Enabled with `?gallery=shell&replay=trace`.
 */

type Frame = [
  atMs: number,
  attempts: RoutingTraceAttempt[],
  status: RoutingTraceEntry["status"],
  totalMs?: number,
  error?: RoutingTraceEntry["error"],
];

interface Scenario {
  tool: ToolId;
  model: string;
  frames: Frame[];
}

const RATE_LIMITED = attempt(
  "Anthropic API",
  "failed",
  420,
  "rateLimited",
  429,
);
const ANTHROPIC_OK = attempt("Anthropic API", "ok", 640);
const GATEWAY_OK = attempt("Shared Team Gateway", "ok", 1_840);
const RELAY_OK = attempt("Team Relay", "ok", 1_180);
const RESTING = attempt("Anthropic API", "skipped");
const TIMED_OUT = attempt("jordan.lee@example.com", "failed", 1_600, "timeout");
const SERVER_ERROR = attempt(
  "Shared Team Gateway",
  "failed",
  380,
  "serverError",
  503,
);

const SCENARIOS: Scenario[] = [
  {
    tool: "claude-code",
    model: "claude-sonnet-5",
    frames: [
      [0, [], "pending"],
      [250, [attempt("Anthropic API", "pending")], "pending"],
      [
        2_000,
        [RATE_LIMITED, attempt("Shared Team Gateway", "pending")],
        "pending",
      ],
      [3_400, [RATE_LIMITED, GATEWAY_OK], "pending"],
      [4_300, [RATE_LIMITED, GATEWAY_OK], "ok", 2_310],
    ],
  },
  {
    tool: "codex",
    model: "gpt-5.5",
    frames: [
      [0, [], "pending"],
      [200, [attempt("Team Relay", "pending")], "pending"],
      [1_900, [RELAY_OK], "pending"],
      [2_500, [RELAY_OK], "ok", 1_650],
    ],
  },
  {
    tool: "claude-code",
    model: "claude-haiku-5",
    frames: [
      [0, [], "pending"],
      [200, [attempt("Anthropic API", "pending")], "pending"],
      [1_700, [ANTHROPIC_OK], "ok", 690],
    ],
  },
  {
    tool: "claude-code",
    model: "claude-sonnet-5",
    frames: [
      [0, [], "pending"],
      [250, [RESTING, attempt("jordan.lee@example.com", "pending")], "pending"],
      [
        2_100,
        [RESTING, TIMED_OUT, attempt("Shared Team Gateway", "pending")],
        "pending",
      ],
      [
        3_600,
        [RESTING, TIMED_OUT, SERVER_ERROR],
        "failed",
        2_050,
        "serverError",
      ],
    ],
  },
];

/** A new request starts this often, so two may be in the air at once. */
const INTERVAL_MS = 3_200;

export function startRoutingTraceReplay(client: QueryClient): () => void {
  const timers = new Set<ReturnType<typeof setTimeout>>();
  const read = () =>
    client.getQueryData<RoutingTraceSnapshot>(routingKeys.trace()) ??
    EMPTY_ROUTING_TRACE;
  let revision = read().revision;
  let seq = read().entries.reduce((highest, e) => Math.max(highest, e.seq), 0);
  let next = 0;

  const later = (ms: number, run: () => void) => {
    const timer = setTimeout(() => {
      timers.delete(timer);
      run();
    }, ms);
    timers.add(timer);
  };

  const play = () => {
    const scenario = SCENARIOS[next % SCENARIOS.length]!;
    next += 1;
    seq += 1;
    const entrySeq = seq;
    const startedAt = Date.now();
    scenario.frames.forEach(([at, attempts, status, totalMs, error], index) => {
      later(at, () => {
        const current = read();
        revision += 1;
        const finished = status !== "pending";
        const counts = {
          requests: current.counts.requests + (index === 0 ? 1 : 0),
          rerouted:
            current.counts.rerouted + (finished && attempts.length > 1 ? 1 : 0),
          failed: current.counts.failed + (status === "failed" ? 1 : 0),
        };
        const entry: RoutingTraceEntry = {
          seq: entrySeq,
          revision,
          startedAt,
          tool: scenario.tool,
          model: scenario.model,
          attempts,
          status,
          error: error ?? null,
          totalMs: totalMs ?? null,
          failedOver: attempts.length > 1,
        };
        client.setQueryData(
          routingKeys.trace(),
          mergeRoutingTraceUpdate(current, { revision, counts, entry }),
        );
      });
    });
    later(INTERVAL_MS, play);
  };

  play();
  return () => {
    for (const timer of timers) clearTimeout(timer);
    timers.clear();
  };
}
