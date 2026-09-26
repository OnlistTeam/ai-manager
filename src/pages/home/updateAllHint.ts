/** Inputs behind the Update All hint, all derived from the fresh inventory. */
export interface UpdateAllHintInputs {
  scheduling: boolean;
  sourcesUnavailable: boolean;
  sourcesPending: boolean;
  retrying: boolean;
  readyCount: number;
  /** Some tool that could update already has a task running. */
  updateableBusy: boolean;
}

/**
 * Update All is only on screen while some installed tool has an update, so
 * the hint never has to say "everything is current"; it only explains why the
 * button cannot start right now. Returns undefined when it simply can.
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
  return "home.updateAll.hints.review";
}
