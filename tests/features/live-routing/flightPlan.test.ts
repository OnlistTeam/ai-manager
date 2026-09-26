import { describe, expect, it } from "vitest";
import type {
  RoutingTraceAttempt,
  RoutingTraceEntry,
} from "@/entities/routing";
import {
  heldWire,
  planFlight,
  revealedAttempts,
} from "@/features/live-routing/flightPlan";

function attempt(
  id: string,
  outcome: RoutingTraceAttempt["outcome"],
  error: RoutingTraceAttempt["error"] = null,
): RoutingTraceAttempt {
  return {
    providerId: id,
    providerName: id.toUpperCase(),
    outcome,
    httpStatus: null,
    error,
    ms: 120,
  };
}

function entry(
  attempts: RoutingTraceAttempt[],
  status: RoutingTraceEntry["status"],
  error: RoutingTraceEntry["error"] = null,
): RoutingTraceEntry {
  return {
    seq: 1,
    revision: 1,
    startedAt: 0,
    tool: "claude-code",
    model: null,
    attempts,
    status,
    error,
    totalMs: status === "pending" ? null : 900,
    failedOver: attempts.length > 1,
  };
}

const route = (plan: ReturnType<typeof planFlight>) =>
  plan.legs.map((leg) =>
    leg.wire === null
      ? `${leg.kind}(${leg.attempt})`
      : `${leg.kind}:${leg.wire}${leg.reverse ? "<" : ">"}:${leg.tone}`,
  );

describe("planFlight", () => {
  it("holds a new request at AI Manager until a service is tried", () => {
    const plan = planFlight(entry([], "pending"));
    expect(route(plan)).toEqual(["send:tool:claude-code>:request"]);
    expect(plan.complete).toBe(false);
    expect(heldWire(plan, 1)).toBe("tool:claude-code");
    expect(heldWire(plan, 0)).toBeNull();
  });

  it("flies to a pending try and waits on its wire", () => {
    const plan = planFlight(entry([attempt("a", "pending")], "pending"));
    expect(route(plan)).toEqual([
      "send:tool:claude-code>:request",
      "try:endpoint:a>:request",
    ]);
    expect(heldWire(plan, 2)).toBe("endpoint:a");
  });

  it("brings a direct answer back to the tool", () => {
    const plan = planFlight(entry([attempt("a", "ok")], "ok"));
    expect(route(plan)).toEqual([
      "send:tool:claude-code>:request",
      "try:endpoint:a>:request",
      "answer:endpoint:a<:success",
      "deliver:tool:claude-code<:success",
    ]);
    expect(plan.complete).toBe(true);
    expect(heldWire(plan, plan.legs.length)).toBeNull();
  });

  it("returns a failed try to AI Manager and goes on to the next service", () => {
    const plan = planFlight(
      entry(
        [
          attempt("a", "failed", "rateLimited"),
          attempt("b", "skipped"),
          attempt("c", "ok"),
        ],
        "ok",
      ),
    );
    expect(route(plan)).toEqual([
      "send:tool:claude-code>:request",
      "try:endpoint:a>:request",
      "bounce:endpoint:a<:failure",
      "skip(1)",
      "try:endpoint:c>:request",
      "answer:endpoint:c<:success",
      "deliver:tool:claude-code<:success",
    ]);
  });

  it("hands the error to the tool when every service failed", () => {
    const plan = planFlight(
      entry(
        [attempt("a", "failed", "timeout"), attempt("b", "failed", "network")],
        "failed",
        "network",
      ),
    );
    expect(route(plan)).toEqual([
      "send:tool:claude-code>:request",
      "try:endpoint:a>:request",
      "bounce:endpoint:a<:failure",
      "try:endpoint:b>:request",
      "bounce:endpoint:b<:failure",
      "giveUp:tool:claude-code<:failure",
    ]);
    expect(planFlight(entry([], "failed", "unavailable")).legs).toHaveLength(2);
  });

  it("only ever appends legs as the same request progresses", () => {
    const steps = [
      entry([], "pending"),
      entry([attempt("a", "pending")], "pending"),
      entry([attempt("a", "failed"), attempt("b", "pending")], "pending"),
      entry([attempt("a", "failed"), attempt("b", "ok")], "pending"),
      entry([attempt("a", "failed"), attempt("b", "ok")], "ok"),
    ].map((step) => route(planFlight(step)));
    steps.slice(1).forEach((legs, index) => {
      expect(legs.slice(0, steps[index]!.length)).toEqual(steps[index]);
    });
  });

  it("reveals a try's mark once the dot has reached its service", () => {
    const plan = planFlight(
      entry([attempt("a", "failed"), attempt("b", "ok")], "ok"),
    );
    expect(revealedAttempts(plan, 0)).toBe(0);
    expect(revealedAttempts(plan, 1)).toBe(0);
    expect(revealedAttempts(plan, 2)).toBe(1);
    expect(revealedAttempts(plan, 4)).toBe(2);
    expect(revealedAttempts(plan, plan.legs.length)).toBe(2);
  });
});
