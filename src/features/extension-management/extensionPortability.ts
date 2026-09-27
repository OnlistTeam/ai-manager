import { extensionScopeKey, type ExtensionScope } from "@/entities/extension";
import type { UnifiedExtensionRow } from "./unifiedExtensionRows";

export interface PortabilityTarget {
  key: string;
  name: string;
  scope: ExtensionScope;
}

/**
 * Whether the row's portability hint applies in this app: the item carries a
 * hint and the app is not one it is known to work in (ADR-0062).
 */
export function mayNotWorkIn(
  row: UnifiedExtensionRow,
  target: PortabilityTarget,
): boolean {
  if (row.portability === null) return false;
  const key = target.key;
  return !row.portability.worksIn.some(
    (scope) => extensionScopeKey(scope) === key,
  );
}

/** The apps where the item is on and may not work as it is set up. */
export function portabilityAffected<Target extends PortabilityTarget>(
  row: UnifiedExtensionRow,
  targets: readonly Target[],
): Target[] {
  return targets.filter(
    (target) =>
      (row.entries.get(target.key)?.enabled ?? false) &&
      mayNotWorkIn(row, target),
  );
}
