import { isTerminal, type Operation } from "@/entities/operation";
import type { ToolId } from "@/entities/tool";
import type { ToolCardAction } from "@/shared/ui/ToolCard";

function isToolMutation(operation: Operation): boolean {
  return operation.kind !== "scan" && operation.kind !== "testProviders";
}

/**
 * Each tool card shows at most one real active operation. Normally the
 * backend's per-tool lock guarantees at most one; if a stale cache
 * abnormally contains more than one at once, pick the one with the newer
 * startedAt, but the busy set still locks this tool regardless.
 */
export function activeToolOperations(
  operations: readonly Operation[],
): ReadonlyMap<ToolId, Operation> {
  const active = new Map<ToolId, Operation>();
  for (const operation of operations) {
    if (
      operation.tool === null ||
      isTerminal(operation.status) ||
      !isToolMutation(operation)
    )
      continue;

    const current = active.get(operation.tool);
    if (
      !current ||
      (operation.startedAt ?? Number.NEGATIVE_INFINITY) >
        (current.startedAt ?? Number.NEGATIVE_INFINITY)
    ) {
      active.set(operation.tool, operation);
    }
  }
  return active;
}

/**
 * A failed task stays on its tool card until the user dismisses it. This keeps
 * an already-open process log mounted across the running → failed transition.
 * Active work always wins; dismissing the newest failure never reveals an
 * older failure underneath it.
 */
export function toolCardOperations(
  operations: readonly Operation[],
  dismissedFailures: ReadonlySet<string>,
  awaitingInventory: ReadonlySet<ToolId>,
): ReadonlyMap<ToolId, Operation> {
  const visible = new Map(activeToolOperations(operations));
  const latestToolTerminal = new Map<ToolId, Operation>();
  const latestExtensionTerminal = new Map<ToolId, Operation>();

  for (const operation of operations) {
    if (
      operation.tool === null ||
      !isToolMutation(operation) ||
      (operation.status !== "failed" && operation.status !== "success")
    ) {
      continue;
    }
    const terminalByScope = operation.extension
      ? latestExtensionTerminal
      : latestToolTerminal;
    const current = terminalByScope.get(operation.tool);
    const timestamp = operation.finishedAt ?? operation.startedAt ?? -1;
    const currentTimestamp = current?.finishedAt ?? current?.startedAt ?? -1;
    if (!current || timestamp > currentTimestamp) {
      terminalByScope.set(operation.tool, operation);
    }
  }

  const terminalFeedback = new Map<ToolId, Operation>();
  function retainIfVisible(tool: ToolId, operation: Operation): void {
    const failedAndVisible =
      operation.status === "failed" && !dismissedFailures.has(operation.id);
    const successAwaitingInventory =
      operation.extension === null &&
      operation.status === "success" &&
      awaitingInventory.has(tool);
    if (!failedAndVisible && !successAwaitingInventory) return;

    const current = terminalFeedback.get(tool);
    const timestamp = operation.finishedAt ?? operation.startedAt ?? -1;
    const currentTimestamp = current?.finishedAt ?? current?.startedAt ?? -1;
    if (!current || timestamp > currentTimestamp) {
      terminalFeedback.set(tool, operation);
    }
  }
  for (const [tool, operation] of latestToolTerminal) {
    retainIfVisible(tool, operation);
  }
  for (const [tool, operation] of latestExtensionTerminal) {
    retainIfVisible(tool, operation);
  }
  for (const [tool, operation] of terminalFeedback) {
    if (!visible.has(tool)) visible.set(tool, operation);
  }

  return visible;
}

/** Only the tool whose terminal result is newer than inventory needs a lock. */
export function toolsAwaitingInventory(
  operations: readonly Operation[],
  inventoryUpdatedAt: number,
): ReadonlySet<ToolId> {
  const tools = new Set<ToolId>();
  for (const operation of operations) {
    if (
      operation.tool !== null &&
      isToolMutation(operation) &&
      operation.extension === null &&
      isTerminal(operation.status) &&
      operation.finishedAt !== null &&
      operation.finishedAt > inventoryUpdatedAt
    ) {
      tools.add(operation.tool);
    }
  }
  return tools;
}

/** Retry reopens the same safe product flow; it never replays a raw command. */
export function retryActionForOperation(
  operation: Operation,
): ToolCardAction | null {
  if (operation.extension !== null) return null;
  switch (operation.kind) {
    case "install":
      return "install";
    case "update":
      return "update";
    case "uninstall":
      return "remove";
    case "repair":
      return "fix";
    case "changeVersion":
      return "version";
    case "testProviders":
    case "scan":
      return null;
  }
}

/**
 * Spec §41: write operations on the same Tool are mutually exclusive. The
 * backend's OperationManager already enforces the lock — this layer acts
 * preemptively, disabling the button while a task is already running,
 * instead of letting the user click and then eat an OPERATION_CONFLICT
 * dialog. Different Tools don't affect each other and can run concurrently.
 */
export function busyTools(
  operations: readonly Operation[],
): ReadonlySet<ToolId> {
  return new Set(activeToolOperations(operations).keys());
}
