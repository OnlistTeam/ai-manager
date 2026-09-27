import type {
  EffectiveConnection,
  Provider,
  ProviderEffectiveState,
} from "@/entities/provider";
import { sourceLabel } from "@/features/provider-management";

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
