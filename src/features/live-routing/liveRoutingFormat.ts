import type { RoutingOverview } from "@/entities/routing";

const EMAIL =
  /([\p{L}\p{N}._%+-])[\p{L}\p{N}._%+-]*@([\p{L}\p{N}-]+(?:\.[\p{L}\p{N}-]+)+)/gu;

/**
 * Services are often named after the account they sign in with. The live
 * panel shows such names as `a***@example.com` unless the user turns the
 * masking off.
 */
export function maskEmails(text: string): string {
  return text.replace(EMAIL, "$1***@$2");
}

export function serviceLabel(name: string, hideEmails: boolean): string {
  return hideEmails ? maskEmails(name) : name;
}

export function formatDuration(ms: number, language: string): string {
  if (ms < 1000) {
    return new Intl.NumberFormat(language, {
      style: "unit",
      unit: "millisecond",
      unitDisplay: "narrow",
    }).format(ms);
  }
  return new Intl.NumberFormat(language, {
    style: "unit",
    unit: "second",
    unitDisplay: "narrow",
    maximumFractionDigits: ms < 10_000 ? 1 : 0,
  }).format(ms / 1000);
}

export function formatClock(epochMs: number, language: string): string {
  return new Intl.DateTimeFormat(language, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(epochMs);
}

/** Tools live routing would take over: the ones with a current service. */
export function liveRoutingCandidates(overview: RoutingOverview) {
  return overview.targets
    .filter((target) => target.currentProvider !== null)
    .map((target) => target.tool);
}
