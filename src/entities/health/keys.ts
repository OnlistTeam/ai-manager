import type { ToolId } from "@/native/schemas/tool";

/** Query keys depend on the installed set, never on detection completion order. */
export function canonicalHealthTools(tools: readonly ToolId[]): ToolId[] {
  return [...new Set(tools)].sort();
}

export const healthKeys = {
  all: ["health"] as const,
  snapshots: ["health", "snapshot"] as const,
  snapshot: (tools: readonly ToolId[]) =>
    ["health", "snapshot", ...canonicalHealthTools(tools)] as const,
  connectivity: () => ["health", "connectivity"] as const,
};
