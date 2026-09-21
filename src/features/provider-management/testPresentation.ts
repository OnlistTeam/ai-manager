import type { ProviderTestResult } from "@/entities/provider";

export interface ReachabilityCopy {
  tone: "success" | "warning" | "danger";
  labelKey: string;
  responseTimeMs: number | null;
}

/**
 * Three states → presentation. The backend deliberately doesn't send a
 * display string (its `message` is a raw, untranslated string — currently
 * in Chinese — and may include a raw address); branching is based on the
 * stable enum value, not on the error text (§43).
 */
export function describeReachability(
  result: ProviderTestResult,
): ReachabilityCopy {
  const tone =
    result.reachability === "operational"
      ? "success"
      : result.reachability === "degraded"
        ? "warning"
        : "danger";
  return {
    tone,
    labelKey: `services.test.${result.reachability}`,
    responseTimeMs: result.responseTimeMs,
  };
}
