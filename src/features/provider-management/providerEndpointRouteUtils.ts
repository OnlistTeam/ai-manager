import type {
  ProviderAdvancedDraft,
  ProviderEndpointTestResult,
} from "@/entities/provider";
import { MAX_PROVIDER_ENDPOINT_CANDIDATES } from "@/native";

export interface ProviderEndpointRoute {
  id: string;
  url: string;
  stored: boolean;
}

export type ProviderEndpointValidation =
  | "invalid"
  | "duplicate"
  | "limit"
  | "empty";

export function normalizeProviderEndpoint(raw: string): string | null {
  try {
    const parsed = new URL(raw.trim());
    const host = parsed.hostname.replace(/^\[|\]$/gu, "").toLowerCase();
    const loopback =
      host === "localhost" ||
      host.endsWith(".localhost") ||
      host === "::1" ||
      /^127(?:\.\d{1,3}){3}$/u.test(host);
    if (
      (parsed.protocol !== "https:" &&
        !(parsed.protocol === "http:" && loopback)) ||
      parsed.username !== "" ||
      parsed.password !== "" ||
      parsed.search !== "" ||
      parsed.hash !== ""
    ) {
      return null;
    }
    let normalized = parsed.toString();
    while (normalized.endsWith("/")) normalized = normalized.slice(0, -1);
    return normalized || null;
  } catch {
    return null;
  }
}

/**
 * The trailing version segment of an address (`v1`, `v1beta`, ...), or `null`.
 * Mirrors `is_version_segment` in the model probe's endpoint arithmetic.
 */
export function trailingVersionSegment(raw: string): string | null {
  try {
    const last = new URL(raw.trim()).pathname.split("/").filter(Boolean).at(-1);
    return last !== undefined && /^v\d+[a-z]*$/u.test(last) ? last : null;
  } catch {
    return null;
  }
}

export function buildProviderEndpointPatch(input: {
  baseUrl: string;
  routes: ProviderEndpointRoute[];
  baseUrlDirty: boolean;
  routesDirty: boolean;
  autoSelectDirty: boolean;
  autoSelect: boolean;
  includeRoutes: boolean;
}):
  | { patch: Omit<ProviderAdvancedDraft, "headers">; validation: null }
  | { patch: null; validation: ProviderEndpointValidation } {
  const selected =
    input.baseUrl.trim() === ""
      ? null
      : normalizeProviderEndpoint(input.baseUrl);
  if (input.baseUrl.trim() !== "" && selected === null) {
    return { patch: null, validation: "invalid" };
  }
  const candidates = Array.from(
    new Set([
      ...input.routes.map((route) => route.url),
      ...(selected ? [selected] : []),
    ]),
  );
  if (candidates.length > MAX_PROVIDER_ENDPOINT_CANDIDATES) {
    return { patch: null, validation: "limit" };
  }
  return {
    validation: null,
    patch: {
      baseUrlChanged: input.baseUrlDirty,
      baseUrl: input.baseUrlDirty ? selected : null,
      endpointCandidates:
        input.includeRoutes && (input.routesDirty || input.baseUrlDirty)
          ? candidates
          : null,
      endpointAutoSelect:
        input.includeRoutes && input.autoSelectDirty ? input.autoSelect : null,
    },
  };
}

export function fastestProviderEndpoint(
  results: ProviderEndpointTestResult[],
  routes: ProviderEndpointRoute[],
): ProviderEndpointRoute | undefined {
  const best = results
    .filter((result) => result.failure === null && result.latencyMs !== null)
    .sort((left, right) => left.latencyMs! - right.latencyMs!)[0];
  return routes.find((route) => route.id === best?.candidateId);
}
