import type { Operation, OperationStatus } from "@/native";

const TERMINAL_STATUSES: readonly OperationStatus[] = [
  "success",
  "failed",
  "cancelled",
];

/** In the state machine of Spec §39, these three are terminal states that never change again. */
export function isTerminal(status: OperationStatus): boolean {
  return TERMINAL_STATUSES.includes(status);
}

/** §40 "keep recent history after a task ends" — how many entries to keep is defined here, not scattered across components. */
export const OPERATION_HISTORY_LIMIT = 20;

/**
 * When over the limit, only terminal entries are trimmed; non-terminal
 * entries (running/queued) are never trimmed, even if that means the list
 * temporarily exceeds OPERATION_HISTORY_LIMIT. Otherwise a progress bar
 * that's still running could be indiscriminately trimmed out of the cache
 * and vanish from the UI, only reappearing as a "new op" on its next push.
 * The number of non-terminal entries is itself bounded by the per-tool
 * mutex (at most "number of tools" can be running at once), so it can't
 * grow unbounded. Trim from the tail forward, preserving the existing
 * "most recent first" ordering, and prefer trimming the oldest terminal
 * entries first.
 */
function trimTerminalOverflow(operations: readonly Operation[]): Operation[] {
  const overflow = operations.length - OPERATION_HISTORY_LIMIT;
  if (overflow <= 0) return operations.slice();

  const trimmed = operations.slice();
  let remaining = overflow;
  for (
    let index = trimmed.length - 1;
    index >= 0 && remaining > 0;
    index -= 1
  ) {
    if (isTerminal(trimmed[index].status)) {
      trimmed.splice(index, 1);
      remaining -= 1;
    }
  }
  return trimmed;
}

/**
 * Merge a single push into the cached list. Same id replaces in place
 * (progress update); a new id is prepended and history is trimmed.
 * Pure function: does not mutate its arguments, so it can be fed directly
 * into a `setQueryData` updater.
 */
export function mergeOperation(
  current: readonly Operation[],
  incoming: Operation,
): Operation[] {
  const index = current.findIndex((item) => item.id === incoming.id);
  if (index >= 0) {
    // Native emits outside its mutex. A cancellation-request snapshot and the
    // immediately following terminal event can therefore arrive out of order;
    // terminal evidence must never be regressed to Running.
    if (isTerminal(current[index].status) && !isTerminal(incoming.status)) {
      return current.slice();
    }
    const next = current.slice();
    next[index] = incoming;
    return next;
  }
  return trimTerminalOverflow([incoming, ...current]);
}

/** Most recent first; entries without a start time sort last (in theory only for the instant right after queuing). */
export function sortByRecency(operations: readonly Operation[]): Operation[] {
  return operations
    .slice()
    .sort((left, right) => (right.startedAt ?? -1) - (left.startedAt ?? -1));
}

/**
 * `useOperations`'s baseline fetch and the event bridge (`useOperationEvents`)
 * are two independent async sources: if a terminal event lands before the
 * post-restart fetch resolves, it may already have written to the cache. If
 * the fetch's snapshot were then used to overwrite the cache wholesale, an
 * op that already finished would get knocked back to an older running
 * snapshot, and `useOperations`'s `staleTime: Infinity` means it would
 * almost never self-heal from that. So we reconcile entry by entry instead
 * of overwriting wholesale:
 *
 * - For an id present in fetch: if the cached version is already terminal
 *   while the fetch version isn't, the fetch snapshot is older — keep the
 *   cached (event) version; otherwise trust fetch (it has the freshest,
 *   most complete fields for this query).
 * - Cache-only (not in fetch) and non-terminal: keep it. It may be a new
 *   op that `begin`s after fetch took its snapshot and hasn't made it into
 *   that list query yet — we must not drop a progress bar that's still
 *   running.
 * - Cache-only and already terminal: also keep it rather than dropping it.
 *   Such an entry represents "the op finished before fetch's list query
 *   landed" — it's newer than the fetch snapshot, and dropping it would
 *   reintroduce a variant of the "stale snapshot knocks it back" problem
 *   this function exists to fix (just "knocked back to running" becomes
 *   "knocked back to nonexistent"). The real cap on history length is
 *   converged by `mergeOperation`'s `trimTerminalOverflow` as events keep
 *   streaming in; we don't re-trim here.
 *
 * Pure function: does not mutate its arguments.
 */
export function reconcileOperations(
  fetched: readonly Operation[],
  cached: readonly Operation[] | undefined,
): Operation[] {
  if (!cached || cached.length === 0) return fetched.slice();

  const cacheById = new Map(
    cached.map((operation) => [operation.id, operation]),
  );
  const fetchedIds = new Set(fetched.map((operation) => operation.id));

  const reconciled = fetched.map((fetchedOperation) => {
    const cachedOperation = cacheById.get(fetchedOperation.id);
    if (
      cachedOperation &&
      isTerminal(cachedOperation.status) &&
      !isTerminal(fetchedOperation.status)
    ) {
      return cachedOperation;
    }
    return fetchedOperation;
  });

  const cacheOnly = cached.filter((operation) => !fetchedIds.has(operation.id));

  return [...cacheOnly, ...reconciled];
}
