import type { TFunction } from "i18next";
import type { ToolLoginStatus } from "@/entities/provider";

/** Claude spells its plans in lower case (`max`, `pro`). */
function planName(plan: string): string {
  return plan.charAt(0).toUpperCase() + plan.slice(1);
}

/**
 * One line for the tool's own sign-in (ADR-0060): the state, then the account
 * and plan when the tool names them. `null` while unread or when the tool
 * could not say, so nothing is shown rather than a guess.
 */
export function toolLoginSummary(
  status: ToolLoginStatus | undefined,
  t: TFunction,
): string | null {
  if (!status || status.state === "unknown") return null;
  const parts = [t(`services.login.state.${status.state}`)];
  if (status.state === "signedIn") {
    if (status.account) parts.push(status.account);
    if (status.plan) parts.push(planName(status.plan));
  }
  return parts.join(" · ");
}
