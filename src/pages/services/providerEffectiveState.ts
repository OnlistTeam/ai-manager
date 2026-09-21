import type {
  EffectiveConnection,
  Provider,
  ProviderEffectiveState,
} from "@/entities/provider";
import { sourceLabel } from "./effectiveConnectionCopy";

const ORDER: Record<ProviderEffectiveState, number> = {
  inUse: 0,
  defaultModel: 0,
  recentModel: 0,
  unknown: 2,
  overridden: 1,
  saved: 2,
};

/**
 * Missing evidence must never be replaced with the database's last selection.
 */
export function providerEffectiveState(
  provider: Provider,
  effective: EffectiveConnection | null | undefined,
): ProviderEffectiveState {
  if (!effective || effective.selection === "unknown") return "unknown";
  if (effective.providerId === provider.id) {
    return effective.selection === "recentModel" ||
      effective.selection === "defaultModel"
      ? effective.selection
      : "inUse";
  }
  if (provider.active) return "overridden";
  return "saved";
}

export function overrideSourceFor(
  effective: EffectiveConnection,
): string | null {
  return (
    sourceLabel(effective.endpointSource) ||
    sourceLabel(effective.credentialSource) ||
    null
  );
}

export function sortByEffectiveState(
  providers: readonly Provider[],
  effective: EffectiveConnection | null | undefined,
): Provider[] {
  return providers
    .map((provider, index) => ({
      provider,
      index,
      rank: ORDER[providerEffectiveState(provider, effective)],
    }))
    .sort((left, right) => left.rank - right.rank || left.index - right.index)
    .map((item) => item.provider);
}
