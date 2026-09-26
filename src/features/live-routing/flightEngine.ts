import type { RoutingTraceEntry, ToolId } from "@/entities/routing";
import {
  findDot,
  findWire,
  glowTool,
  placeDot,
  toneFill,
  wireLength,
} from "./flightDom";
import { LEG_MS, planFlight, type FlightPlan } from "./flightPlan";
import { easeInOut } from "./stageGeometry";

/** Requests animated at once; older ones jump to their current step. */
export const MAX_FLIGHTS = 6;

export interface TrackState {
  tool: ToolId;
  plan: FlightPlan;
  /** Legs already flown. */
  done: number;
}

interface ActiveLeg {
  index: number;
  startedAt: number;
  path: SVGPathElement;
  length: number;
  dot: SVGCircleElement;
}

interface Track extends TrackState {
  seq: number;
  leg: ActiveLeg | null;
}

export interface FlightEngineOptions {
  root: () => Element | null;
  onChange: (tracks: ReadonlyMap<number, TrackState>) => void;
}

/**
 * Plays trace entries as dots along the stage's wires, one leg after the
 * other per request and several requests at once, on a single
 * requestAnimationFrame loop that stops when nothing moves. While `still`
 * (reduced motion, hidden window) every request jumps to its current step.
 */
export function createFlightEngine({ root, onChange }: FlightEngineOptions) {
  const tracks = new Map<number, Track>();
  const retired = new Set<number>();
  let frame: number | null = null;
  let still = false;

  const pending = (track: Track) =>
    track.leg !== null || track.done < track.plan.legs.length;

  function publish(): void {
    for (const [seq, track] of tracks) {
      if (track.plan.complete && !pending(track)) {
        tracks.delete(seq);
        retired.add(seq);
      }
    }
    onChange(
      new Map(
        [...tracks].map(([seq, { tool, plan, done }]) => [
          seq,
          { tool, plan, done },
        ]),
      ),
    );
  }

  function finishAll(): void {
    for (const track of tracks.values()) {
      track.done = track.plan.legs.length;
      track.leg = null;
    }
  }

  function startLeg(track: Track, now: number, element: Element): boolean {
    const leg = track.plan.legs[track.done];
    if (!leg) return false;
    const path = leg.wire ? findWire(element, leg.wire) : null;
    const length = path ? wireLength(path) : 0;
    if (!path || length === 0 || LEG_MS[leg.kind] === 0) {
      // Nothing to travel along (a mark-only step, or a wire not on stage).
      track.done += 1;
      return true;
    }
    const dot = findDot(element, track.seq);
    if (!dot) return false; // Rendered on the next commit; try next frame.
    dot.style.fill = toneFill(leg.tone, track.tool);
    if (leg.kind === "send") glowTool(element, track.tool);
    track.leg = { index: track.done, startedAt: now, path, length, dot };
    return false;
  }

  function step(now: number): void {
    frame = null;
    const element = root();
    let changed = false;
    for (const track of tracks.values()) {
      while (element && track.leg === null && startLeg(track, now, element)) {
        changed = true;
      }
      const active = track.leg;
      if (!active) continue;
      const leg = track.plan.legs[active.index]!;
      let progress = (now - active.startedAt) / LEG_MS[leg.kind];
      if (active.path.isConnected) {
        placeDot(
          active.dot,
          active.path,
          active.length,
          leg.reverse,
          easeInOut(progress),
        );
      } else {
        // The wire left the stage (another tool's services took the right
        // column): the leg ends out of sight instead of jumping.
        active.dot.setAttribute("visibility", "hidden");
        progress = 1;
      }
      if (progress >= 1) {
        track.done = active.index + 1;
        track.leg = null;
        changed = true;
      }
    }
    if (changed) publish();
    schedule();
  }

  function schedule(): void {
    if (still || frame !== null || ![...tracks.values()].some(pending)) return;
    frame = requestAnimationFrame(step);
  }

  function stop(): void {
    if (frame !== null) cancelAnimationFrame(frame);
    frame = null;
  }

  return {
    /** Folds the latest entries (newest first) into the tracks. */
    sync(entries: readonly RoutingTraceEntry[], seededUpTo: number): void {
      const present = new Set(entries.map((entry) => entry.seq));
      for (const seq of tracks.keys()) {
        if (!present.has(seq)) tracks.delete(seq);
      }
      for (const seq of retired) if (!present.has(seq)) retired.delete(seq);
      for (const entry of entries) {
        if (retired.has(entry.seq)) continue;
        const plan = planFlight(entry);
        const track = tracks.get(entry.seq);
        if (track) {
          track.plan = plan;
          continue;
        }
        // Requests already on screen when the stage opened do not replay.
        const seeded = entry.seq <= seededUpTo;
        tracks.set(entry.seq, {
          seq: entry.seq,
          tool: entry.tool,
          plan,
          done: seeded ? plan.legs.length : 0,
          leg: null,
        });
      }
      const newestFirst = [...tracks.values()].sort((a, b) => b.seq - a.seq);
      for (const track of newestFirst.slice(MAX_FLIGHTS)) {
        track.done = track.plan.legs.length;
        track.leg = null;
      }
      if (still) finishAll();
      publish();
      schedule();
    },

    setStill(next: boolean): void {
      still = next;
      if (!still) {
        schedule();
        return;
      }
      stop();
      finishAll();
      publish();
    },

    /** Re-reads wire lengths and puts waiting dots back on their wires. */
    relayout(): void {
      const element = root();
      if (!element) return;
      for (const track of tracks.values()) {
        if (track.leg) {
          track.leg.length = wireLength(track.leg.path);
          continue;
        }
        const last = [...track.plan.legs.slice(0, track.done)]
          .reverse()
          .find((leg) => leg.wire !== null);
        const path = last?.wire ? findWire(element, last.wire) : null;
        const dot = findDot(element, track.seq);
        const length = path ? wireLength(path) : 0;
        if (!dot) continue;
        if (!last || !path || length === 0) {
          // Its wire is not on stage now; the dot waits out of sight.
          dot.setAttribute("visibility", "hidden");
          continue;
        }
        dot.style.fill = toneFill(last.tone, track.tool);
        placeDot(dot, path, length, last.reverse, 1);
      }
    },

    dispose: stop,
  };
}

export type FlightEngine = ReturnType<typeof createFlightEngine>;
