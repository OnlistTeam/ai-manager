import { useEffect, useLayoutEffect, useState, type RefObject } from "react";
import type { RoutingTraceEntry } from "@/entities/routing";
import { createFlightEngine, type TrackState } from "./flightEngine";

/**
 * Connects the flight engine to React: new trace revisions go in, and the
 * legs flown per request come out as state, so marks and the caption follow
 * the dots. Requests already listed when the stage opens are not replayed.
 * `layout` changes identity whenever the wires move, so waiting dots follow.
 */
export function useFlightPlayer(
  entries: readonly RoutingTraceEntry[],
  rootRef: RefObject<Element | null>,
  reducedMotion: boolean,
  layout: unknown,
): ReadonlyMap<number, TrackState> {
  const [seededUpTo] = useState(() =>
    entries.reduce((highest, entry) => Math.max(highest, entry.seq), 0),
  );
  const [tracks, setTracks] = useState<ReadonlyMap<number, TrackState>>(
    () => new Map(),
  );
  const [engine] = useState(() =>
    createFlightEngine({ root: () => rootRef.current, onChange: setTracks }),
  );

  useEffect(() => () => engine.dispose(), [engine]);

  useEffect(() => {
    const update = () =>
      engine.setStill(reducedMotion || document.visibilityState === "hidden");
    update();
    document.addEventListener("visibilitychange", update);
    return () => document.removeEventListener("visibilitychange", update);
  }, [engine, reducedMotion]);

  useEffect(() => {
    engine.sync(entries, seededUpTo);
  }, [engine, entries, seededUpTo]);

  useLayoutEffect(() => {
    engine.relayout();
  }, [engine, tracks, layout]);

  return tracks;
}
