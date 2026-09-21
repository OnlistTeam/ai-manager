import type { UninstallOptions } from "@/native";

export interface UninstallImpact {
  /** Things that will be deleted, listed in terms the user can understand (i18n keys). */
  removedKeys: string[];
  /** Side effects that must be spelled out up front (i18n keys). */
  warningKeys: string[];
  destructive: boolean;
}

/**
 * Facts relevant to the uninstall copy, sent by the backend. **Must come
 * only from `Tool`** — branching by tool name on the frontend is explicitly
 * forbidden by AI_RULES.
 */
export interface UninstallFacts {
  /** This tool's chat history lives inside the settings directory, so deleting settings inevitably deletes it too. */
  sessionsInsideSettings: boolean;
}

/**
 * Spec §32's three options. The side effects of `removeSettings` are
 * explained based on **this tool's actual layout**: the true/false value is
 * computed by the backend's `tool_paths::sessions_inside_settings` from the
 * two path lists the uninstall executor actually deletes, so this copy can
 * never drift from real behavior. The order follows the backend's deletion
 * order (cache first, then settings), so the list doesn't read out of step
 * with what actually happens.
 */
export function describeUninstall(
  options: UninstallOptions,
  facts: UninstallFacts,
): UninstallImpact {
  const removedKeys = ["tools.uninstall.impact.app"];
  const warningKeys: string[] = [];

  if (options.removeCache) {
    removedKeys.push("tools.uninstall.impact.cache");
  }
  if (options.removeSettings) {
    removedKeys.push("tools.uninstall.impact.settings");
    if (facts.sessionsInsideSettings) {
      warningKeys.push("tools.uninstall.warning.sessionsInsideSettings");
    }
  }

  const destructive = options.removeSettings || options.removeCache;
  if (destructive) {
    // Decision 5 (Phase 3a): the backend rejects the whole request and
    // deletes nothing if a path falls outside $HOME; unchecking the box and
    // retrying lets the user complete the app uninstall — this must be
    // said up front, not explained after the fact once it fails.
    warningKeys.push("tools.uninstall.warning.outsideHome");
  }

  return { removedKeys, warningKeys, destructive };
}
