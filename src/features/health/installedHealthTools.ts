import type { ToolId } from "@/entities/tool";

/** Installed scopes are the only ones with live configuration worth checking. */
export function installedHealthTools(
  tools: readonly { id: ToolId; status: string }[],
): ToolId[] {
  return tools
    .filter(
      (tool) =>
        tool.status === "installed" || tool.status === "updateAvailable",
    )
    .map((tool) => tool.id);
}
