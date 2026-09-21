import type { Tool } from "@/entities/tool";
import type { EnvironmentStatus } from "@/shared/ui/StatusBadge";

export interface EnvironmentSummary {
  status: EnvironmentStatus;
  total: number;
  /** Number of installed tools (including ones with an update available). */
  installed: number;
  updatable: number;
  /**
   * Number of tools that are installed but whose latest version isn't known
   * yet (the version check hasn't returned, or didn't find one).
   *
   * This gates the phrase "up to date": `updatable === 0` only means we
   * **didn't find** an update, not that everything is up to date. To assert
   * everything is up to date, `unverified === 0` must also hold.
   * It isn't something the user needs to act on, so it doesn't count toward
   * `attention` and doesn't change `status`.
   */
  unverified: number;
  /** Number of items needing user action: broken + updatable + undetected. */
  attention: number;
}

/**
 * Spec §27: status has only three levels — no pseudo-precise scoring like "83".
 * Spec §38: the MVP only uses data we already have — "installed or not /
 * version / update available or not". No PATH conflict scanning, no
 * multi-version Node management, or other P1-level capabilities.
 */
export function summarizeEnvironment(
  tools: readonly Tool[],
): EnvironmentSummary {
  let installed = 0;
  let updatable = 0;
  let unverified = 0;
  let broken = 0;
  let unknown = 0;

  for (const tool of tools) {
    if (tool.status === "installed" || tool.status === "updateAvailable") {
      installed += 1;
      if (tool.latestVersion === null) unverified += 1;
    }
    if (tool.status === "updateAvailable") updatable += 1;
    if (tool.status === "broken") broken += 1;
    if (tool.status === "unknown") unknown += 1;
  }

  const attention = broken + updatable + unknown;

  // Anything broken -> the user must act. Nothing installed, or something
  // updatable/undetected -> worth a look.
  // Everything else (at least one installed, nothing broken, no known
  // pending update) -> all clear.
  // Note `ready` only says "no known problems" — it doesn't mean versions
  // have been verified (see `unverified`); "haven't found the latest version
  // yet" is not something that should alarm the user, so it's not promoted
  // to `attention` here.
  const status: EnvironmentStatus =
    broken > 0
      ? "action"
      : installed === 0 || attention > 0
        ? "attention"
        : "ready";

  return {
    status,
    total: tools.length,
    installed,
    updatable,
    unverified,
    attention,
  };
}
