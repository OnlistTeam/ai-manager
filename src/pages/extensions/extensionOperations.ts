import type { ExtensionKind, ExtensionScope } from "@/entities/extension";
import { isTerminal, type Operation } from "@/entities/operation";

/** Background extension tasks of one kind, whichever app they run for. */
export function runningKindOperations(
  operations: readonly Operation[],
  kind: ExtensionKind,
): Operation[] {
  return operations.filter(
    (operation) =>
      operation.extension?.kind === kind && !isTerminal(operation.status),
  );
}

/** The background extension task running for one app, if any. */
export function runningScopeOperation(
  operations: readonly Operation[],
  scope: ExtensionScope,
): Operation | undefined {
  return operations.find(
    (operation) =>
      operation.extension !== null &&
      !isTerminal(operation.status) &&
      (scope.kind === "tool"
        ? operation.tool === scope.id && operation.desktopApp === null
        : operation.desktopApp === scope.id && operation.tool === null),
  );
}
