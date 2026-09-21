import type { Tool } from "@/entities/tool";

/** A tool is launchable only when detection found a usable installation. */
export function isToolLaunchable(tool: Tool): boolean {
  return (
    tool.capabilities.canLaunch &&
    (tool.status === "installed" || tool.status === "updateAvailable")
  );
}

/** Preserve the backend registry order and make the selected name visible in UI. */
export function firstLaunchableTool(tools: readonly Tool[]): Tool | null {
  return tools.find(isToolLaunchable) ?? null;
}
