/** Inputs behind the Update All hint, all derived from the fresh inventory. */
export interface UpdateAllHintInputs {
  scheduling: boolean;
  sourcesUnavailable: boolean;
  sourcesPending: boolean;
  retrying: boolean;
  readyCount: number;
  /** Some tool that could update already has a task running. */
  updateableBusy: boolean;
  outdatedCount: number;
  checkingVersions: boolean;
  /** Installed tools whose latest version is still unknown. */
  unverified: number;
}

/**
 * "Everything is already up to date" is a positive assertion, and it's only
 * earned once every installed tool's latest version has actually been
 * checked. Still checking → confirming; can't check (offline/rate-limited) →
 * temporarily unable to confirm. Not knowing means we don't get to conclude
 * on its behalf.
 * Returns undefined when the action needs no hint because it is simply ready.
 */
export function updateAllHintKey(
  input: UpdateAllHintInputs,
): string | undefined {
  if (input.scheduling) return "home.updateAll.hints.running";
  if (input.sourcesUnavailable) return "home.updateAll.hints.unavailable";
  if (input.sourcesPending || input.retrying) {
    return "home.updateAll.hints.checking";
  }
  if (input.readyCount > 0) return undefined;
  if (input.updateableBusy) return "home.updateAll.hints.running";
  if (input.outdatedCount > 0) return "home.updateAll.hints.review";
  if (input.checkingVersions) return "home.updateAll.hints.checking";
  if (input.unverified > 0) return "home.updateAll.hints.unavailable";
  return "home.updateAll.hints.current";
}
