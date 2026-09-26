import type { Provider, ProviderEditProfile } from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import type { QuickCheckItem } from "@/features/health";

/**
 * What the saved inventory says a tool is connected to (ADR-0051).
 *
 * This reads only the saved list, never the terminal environment: a variable
 * exported in a shell profile can still override it, and the API Endpoints
 * page, which does inspect the terminal, carries that precise answer.
 */
export type ToolConnection =
  | { kind: "loading" }
  | { kind: "unavailable" }
  | { kind: "service"; provider: Provider }
  | { kind: "official"; provider: Provider | null }
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

export function toolConnection(
  tool: Tool,
  providers: readonly Provider[] | undefined,
  unavailable: boolean,
): ToolConnection {
  if (providers === undefined) {
    return unavailable ? { kind: "unavailable" } : { kind: "loading" };
  }
  // Additive tools keep every saved endpoint side by side and pick the model
  // inside the tool, so no single entry is "the" connection.
  const additive = providers.filter((provider) => provider.additive);
  if (additive.length > 0) return { kind: "added", count: additive.length };

  const active = providers.find((provider) => provider.active);
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

/** Saved endpoints the row can switch to: every non-additive entry not in use. */
export function switchChoices(
  providers: readonly Provider[] | undefined,
): Provider[] {
  return (providers ?? []).filter(
    (provider) => !provider.additive && !provider.active,
  );
}

/** A model name is shown only when the saved endpoint pins exactly one. */
export function pinnedModel(
  profile: ProviderEditProfile | undefined,
): string | null {
  return profile?.models.length === 1 ? (profile.models[0] ?? null) : null;
}

/**
 * Findings a tool row already states. An available update gets its own
 * button on the row, and a missing endpoint reads as the row's connection, so
 * repeating either in the findings list would say the same thing twice.
 */
export function coveredByToolRow(
  item: QuickCheckItem,
  rowToolIds: ReadonlySet<string>,
): boolean {
  if (item.toolId === null || !rowToolIds.has(item.toolId)) return false;
  return (
    (item.kind === "tool" && item.resolution === "updateTool") ||
    (item.kind === "provider" && item.resolution === "connectService")
  );
}
