import { useState } from "react";
import type { ExtensionScope } from "@/entities/extension";
import type { Tool } from "@/entities/tool";

export const WORKSPACE_TAB = "workspace" as const;

/** What the shell may ask the page to open on: an inventory scope or the workspace. */
export type ExtensionsTab = ExtensionScope | typeof WORKSPACE_TAB;

/** The workspace tab exists only while the OpenClaw CLI is installed. */
export function openClawWorkspaceAvailable(tools: readonly Tool[]): boolean {
  return tools.some(
    (tool) => tool.id === "openclaw" && tool.status !== "notInstalled",
  );
}

/**
 * Whether the OpenClaw workspace tab is showing. Unlike the extension kinds
 * it is page-local UI state that never touches ProductSettings, and it yields
 * to the kind tabs by itself whenever the tab is no longer available.
 */
export function useExtensionsWorkspaceTab(
  available: boolean,
  preferred: boolean,
) {
  const [selected, setSelected] = useState(preferred);
  return {
    active: available && selected,
    open: () => setSelected(true),
    close: () => setSelected(false),
  };
}
