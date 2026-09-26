import type { RoutingOverview } from "@/entities/routing";

const EMAIL =
  /([\p{L}\p{N}._%+-])[\p{L}\p{N}._%+-]*@([\p{L}\p{N}-]+(?:\.[\p{L}\p{N}-]+)+)/gu;

/**
 * Services are often named after the account they sign in with. The live
 * panel always shows such names as `a***@example.com`.
 */
export function maskEmails(text: string): string {
  return text.replace(EMAIL, "$1***@$2");
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

/** Current services of the tools that go through AI Manager, masked and unique. */
export function routedServiceNames(overview: RoutingOverview): string[] {
  const names = overview.targets
    .filter((target) => target.takeoverEnabled && target.currentProvider)
    .map((target) => maskEmails(target.currentProvider?.name ?? ""));
  return [...new Set(names)].filter((name) => name.length > 0);
}

/** Tools live routing would take over: the ones with a current service. */
export function liveRoutingCandidates(overview: RoutingOverview) {
  return overview.targets
    .filter((target) => target.currentProvider !== null)
    .map((target) => target.tool);
}
