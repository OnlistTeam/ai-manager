import {
  MAX_ROUTING_TRACE_ENTRIES,
  type RoutingTraceEntry,
  type RoutingTraceSnapshot,
  type RoutingTraceUpdate,
} from "@/native";

export const EMPTY_ROUTING_TRACE: RoutingTraceSnapshot = {
  revision: 0,
  counts: { requests: 0, rerouted: 0, failed: 0 },
  entries: [],
};

/**
 * Keeps the newer copy of every entry (by its revision), newest request
 * first, capped like the native ring. Snapshots and pushed updates may arrive
 * in any order, so merging never lets an older copy win.
 */
function mergeEntries(
  current: readonly RoutingTraceEntry[],
  incoming: readonly RoutingTraceEntry[],
): RoutingTraceEntry[] {
  const bySeq = new Map<number, RoutingTraceEntry>();
  for (const entry of [...current, ...incoming]) {
    const kept = bySeq.get(entry.seq);
    if (!kept || entry.revision > kept.revision) bySeq.set(entry.seq, entry);
  }
  return [...bySeq.values()]
    .sort((left, right) => right.seq - left.seq)
    .slice(0, MAX_ROUTING_TRACE_ENTRIES);
}

function merge(
  current: RoutingTraceSnapshot | undefined,
  revision: number,
  counts: RoutingTraceSnapshot["counts"],
  entries: readonly RoutingTraceEntry[],
): RoutingTraceSnapshot {
  const base = current ?? EMPTY_ROUTING_TRACE;
  const newer = revision > base.revision;
  return {
    revision: Math.max(base.revision, revision),
    counts: newer ? counts : base.counts,
    entries: mergeEntries(base.entries, entries),
  };
}

export function mergeRoutingTraceSnapshot(
  current: RoutingTraceSnapshot | undefined,
  snapshot: RoutingTraceSnapshot,
): RoutingTraceSnapshot {
  return merge(current, snapshot.revision, snapshot.counts, snapshot.entries);
}

export function mergeRoutingTraceUpdate(
  current: RoutingTraceSnapshot | undefined,
  update: RoutingTraceUpdate,
): RoutingTraceSnapshot {
  return merge(current, update.revision, update.counts, [update.entry]);
}
