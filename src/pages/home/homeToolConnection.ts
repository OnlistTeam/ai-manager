import type { Provider, ProviderEditProfile } from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import type { QuickCheckItem } from "@/features/health";

/**
 * What the saved inventory says a tool is connected to (ADR-0051, ADR-0053).
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

/**
 * What the row's picker lists: every non-additive entry, the one in use first
 * and the rest in the user's own order from the API Endpoints page.
 */
export function pickerProviders(
  providers: readonly Provider[] | undefined,
): Provider[] {
  const choosable = (providers ?? []).filter((provider) => !provider.additive);
  return [
    ...choosable.filter((provider) => provider.active),
    ...choosable.filter((provider) => !provider.active),
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
