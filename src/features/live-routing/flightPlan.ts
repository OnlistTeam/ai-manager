import type { RoutingTraceEntry, ToolId } from "@/entities/routing";

/**
 * How one request travels across the live stage. A trace entry becomes a list
 * of legs along the wires; the list only grows as the entry changes, so the
 * player can keep a count of legs already flown and fly the rest.
 */
export type FlightTone = "request" | "failure" | "success";

export type FlightLegKind =
  /** Tool → AI Manager. */
  | "send"
  /** AI Manager → the service being tried. */
  | "try"
  /** A service passed over while it rests: a mark, no travel. */
  | "skip"
  /** The failed service → back to AI Manager. */
  | "bounce"
  /** The answering service → AI Manager. */
  | "answer"
  /** AI Manager → the tool, with the answer. */
  | "deliver"
  /** AI Manager → the tool, with the error it gets. */
  | "giveUp";

export interface FlightLeg {
  kind: FlightLegKind;
  /** The wire the dot travels, or null for a step that only sets a mark. */
  wire: string | null;
  /** Travel from the wire's end back to its start. */
  reverse: boolean;
  tone: FlightTone;
  /** Index into the entry's attempts, for steps about one try. */
  attempt: number | null;
}

export interface FlightPlan {
  legs: FlightLeg[];
  /** The request has finished: no further legs will be added. */
  complete: boolean;
}

export const LEG_MS: Record<FlightLegKind, number> = {
  send: 900,
  try: 1000,
  skip: 0,
  bounce: 700,
  answer: 800,
  deliver: 900,
  giveUp: 1100,
};

export function toolWire(tool: ToolId): string {
  return `tool:${tool}`;
}

export function endpointWire(providerId: string): string {
  return `endpoint:${providerId}`;
}

export function planFlight(entry: RoutingTraceEntry): FlightPlan {
  const tool = toolWire(entry.tool);
  const legs: FlightLeg[] = [
    {
      kind: "send",
      wire: tool,
      reverse: false,
      tone: "request",
      attempt: null,
    },
  ];
  const finished = entry.status !== "pending";
  let atEndpoint: string | null = null;

  entry.attempts.forEach((attempt, index) => {
    const wire = endpointWire(attempt.providerId);
    if (attempt.outcome === "skipped") {
      legs.push({
        kind: "skip",
        wire: null,
        reverse: false,
        tone: "request",
        attempt: index,
      });
      return;
    }
    legs.push({
      kind: "try",
      wire,
      reverse: false,
      tone: "request",
      attempt: index,
    });
    if (attempt.outcome === "failed") {
      legs.push({
        kind: "bounce",
        wire,
        reverse: true,
        tone: "failure",
        attempt: index,
      });
      atEndpoint = null;
    } else {
      atEndpoint = wire;
    }
  });

  if (entry.status === "ok" && atEndpoint !== null) {
    const answered = entry.attempts.length - 1;
    legs.push(
      {
        kind: "answer",
        wire: atEndpoint,
        reverse: true,
        tone: "success",
        attempt: answered,
      },
      {
        kind: "deliver",
        wire: tool,
        reverse: true,
        tone: "success",
        attempt: answered,
      },
    );
  } else if (finished) {
    if (atEndpoint !== null) {
      legs.push({
        kind: "bounce",
        wire: atEndpoint,
        reverse: true,
        tone: "failure",
        attempt: entry.attempts.length - 1,
      });
    }
    legs.push({
      kind: "giveUp",
      wire: tool,
      reverse: true,
      tone: "failure",
      attempt: null,
    });
  }

  return { legs, complete: finished };
}

/**
 * How many of the entry's tries the stage may mark after `done` legs: a try
 * is shown once the dot has reached its service (or passed it over).
 */
export function revealedAttempts(plan: FlightPlan, done: number): number {
  let revealed = 0;
  plan.legs.slice(0, done).forEach((leg) => {
    if ((leg.kind === "try" || leg.kind === "skip") && leg.attempt !== null) {
      revealed = Math.max(revealed, leg.attempt + 1);
    }
  });
  return revealed;
}

/** The wire a request waits on after its last flown leg, while it is open. */
export function heldWire(plan: FlightPlan, done: number): string | null {
  if (plan.complete || done < plan.legs.length) return null;
  const last = [...plan.legs].reverse().find((leg) => leg.wire !== null);
  return last?.wire ?? null;
}
