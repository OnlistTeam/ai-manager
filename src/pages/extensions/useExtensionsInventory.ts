import { extensionScopeKey, useExtensions } from "@/entities/extension";
import type { ExtensionScope } from "@/entities/extension";
import { useSkillUpdates } from "@/entities/skill-update";
import { useUnifiedExtensions } from "@/features/extension-management";
import { useTaskAvailability } from "@/features/task-center";
import {
  runningKindOperations,
  runningScopeOperation,
} from "./extensionOperations";
import type { useExtensionScope } from "./useExtensionScope";

/**
 * What the page lists and whether it may change it. Skills and MCP read
 * every supported app at once (ADR-0048); prompts read the one app selected.
 */
export function useExtensionsInventory(
  scope: ReturnType<typeof useExtensionScope>,
  preferredScope: ExtensionScope | null,
  toolActionsBlocked: boolean,
) {
  const { activeKind, activeTab, activeScope } = scope;
  const task = useTaskAvailability();
  const operations = task.operations.data ?? [];
  const unifiedLayout = activeTab.layout === "unified";
  const unifiedTargets =
    unifiedLayout && activeKind !== null
      ? scope.scopedTargets.filter((target) => target.supported)
      : [];
  // The add flows still start in one app: the one the shell routed to, or
  // the first. Every other app is one switch away on the new row.
  const preferredKey =
    preferredScope === null ? null : extensionScopeKey(preferredScope);
  const target = unifiedLayout
    ? (unifiedTargets.find((candidate) => candidate.key === preferredKey) ??
      unifiedTargets[0] ??
      null)
    : activeScope?.supported
      ? activeScope
      : null;
  const unified = useUnifiedExtensions(activeTab.kind, unifiedTargets);
  const extensions = useExtensions(
    activeScope?.scope ?? null,
    activeTab.kind,
    !unifiedLayout && (activeScope?.supported ?? false),
  );
  const kindOperations = unifiedLayout
    ? runningKindOperations(operations, activeTab.kind)
    : [];
  const activeOperation =
    !unifiedLayout && activeScope
      ? runningScopeOperation(operations, activeScope.scope)
      : undefined;
  const skillUpdates = useSkillUpdates(activeTab.kind === "skill");
  const listed = unifiedLayout ? unified : extensions;

  return {
    task,
    unifiedLayout,
    target,
    unified,
    extensions,
    kindOperations,
    activeOperation,
    skillUpdates,
    mutationsBlocked:
      toolActionsBlocked ||
      listed.isFetching ||
      listed.isError ||
      task.actionsBlocked ||
      activeOperation !== undefined ||
      kindOperations.length > 0,
  };
}
