import type {
  EffectiveConnection,
  Provider,
  ProviderEditProfile,
} from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import type { QuickCheckItem } from "@/features/health";

/** An address in force that matches no saved endpoint, e.g. a shell variable. */
export type ExternalConnection = EffectiveConnection & { endpoint: string };

/**
 * What a tool is connected to (ADR-0051, ADR-0053).
 *
 * The saved inventory answers first. The effective connection (ADR-0035),
 * read in the background because its login-shell probe is slow, replaces
 * that answer once it arrives and differs: a variable exported in a shell
 * profile or a hand-edited config file can point the tool somewhere else.
 */
export type ToolConnection =
  | { kind: "loading" }
  | { kind: "unavailable" }
  | { kind: "service"; provider: Provider }
  | { kind: "official"; provider: Provider | null }
  | { kind: "external"; connection: ExternalConnection }
  | { kind: "added"; count: number }
  | { kind: "notConnected" };

/** Installed tools that can use an API endpoint, in inventory order. */
export function connectableTools(tools: readonly Tool[]): Tool[] {
  return tools.filter(
    (tool) =>
      (tool.status === "installed" || tool.status === "updateAvailable") &&
      tool.capabilities.canManageProvider,
  );
}

/**
 * What the effective connection says is in force, when it says anything
 * definite: the saved endpoint it matched, or an address none of them has.
 * Unknown or model-history evidence is no answer, and neither is a tool left
 * on its own default, which the saved list already describes.
 */
function inForce(
  providers: readonly Provider[],
  effective: EffectiveConnection | null,
): Extract<ToolConnection, { kind: "service" | "external" }> | null {
  const definite =
    effective && (effective.selection ?? "configuration") === "configuration";
  if (!definite) return null;
  if (effective.providerId !== null) {
    const provider = providers.find(
      (entry) => entry.id === effective.providerId,
    );
    return provider ? { kind: "service", provider } : null;
  }
  const { endpoint } = effective;
  return endpoint === null
    ? null
    : { kind: "external", connection: { ...effective, endpoint } };
}

export function toolConnection(
  tool: Tool,
  providers: readonly Provider[] | undefined,
  unavailable: boolean,
  effective: EffectiveConnection | null = null,
): ToolConnection {
  if (providers === undefined) {
    return unavailable ? { kind: "unavailable" } : { kind: "loading" };
  }
  // Additive tools keep every saved endpoint side by side and pick the model
  // inside the tool, so no single entry is "the" connection.
  const additive = providers.filter((provider) => provider.additive);
  if (additive.length > 0) return { kind: "added", count: additive.length };

  const evidence = inForce(providers, effective);
  if (evidence?.kind === "external") return evidence;
  const active =
    evidence?.provider ?? providers.find((provider) => provider.active);
  if (active) {
    return active.kind === "official"
      ? { kind: "official", provider: active }
      : { kind: "service", provider: active };
  }
  // With nothing selected here the tool falls back to its own sign-in when it
  // has one; a tool that only works through an endpoint is simply unconnected.
  return tool.discovery?.access === "vendorOrProvider"
    ? { kind: "official", provider: null }
    : { kind: "notConnected" };
}

/** The saved entry the row names as in use, if the row names one. */
export function inUseProviderId(connection: ToolConnection): string | null {
  return connection.kind === "service" || connection.kind === "official"
    ? (connection.provider?.id ?? null)
    : null;
}

/**
 * What the row's picker lists: every non-additive entry, the one in use first
 * and the rest in the user's own order from the API Endpoints page.
 */
export function pickerProviders(
  providers: readonly Provider[] | undefined,
  inUseId: string | null,
): Provider[] {
  const choosable = (providers ?? []).filter((provider) => !provider.additive);
  return [
    ...choosable.filter((provider) => provider.id === inUseId),
    ...choosable.filter((provider) => provider.id !== inUseId),
  ];
}

/** A model name is shown only when the saved endpoint pins exactly one. */
export function pinnedModel(
  profile: ProviderEditProfile | undefined,
): string | null {
  return profile?.models.length === 1 ? (profile.models[0] ?? null) : null;
}

/**
 * Findings Home already states elsewhere. A missing endpoint reads as the
 * row's own picker, and an available update is counted in the status line,
 * which links to the Software page where updates are reviewed; listing either
 * again would say the same thing twice and put an update action on Home.
 */
export function shownElsewhereOnHome(
  item: QuickCheckItem,
  rowToolIds: ReadonlySet<string>,
): boolean {
  if (item.toolId === null || !rowToolIds.has(item.toolId)) return false;
  return (
    (item.kind === "tool" && item.resolution === "updateTool") ||
    (item.kind === "provider" && item.resolution === "connectService")
  );
}
