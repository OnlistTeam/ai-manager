import { extensionKeys } from "@/entities/extension";

/**
 * Discover lists live under the extensions prefix: a finished Skill or MCP
 * task invalidates `extensionKeys.all`, so the cards re-read what is already
 * added and flip to Added without a second rule. Native keeps the network
 * answers, so that refetch only re-reads the local inventory.
 *
 * Descriptions and pictures do not change with the inventory and sit under
 * their own prefix.
 */
export const discoverKeys = {
  all: [...extensionKeys.all, "discover"] as const,
  list: (kind: "skill" | "mcp", query: string) =>
    [...extensionKeys.all, "discover", kind, query] as const,
  descriptions: (ids: readonly string[]) =>
    ["discover", "descriptions", ids.join("\n")] as const,
  icon: (url: string) => ["discover", "icon", url] as const,
};
