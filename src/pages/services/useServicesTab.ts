import { useState } from "react";
import type { ToolId } from "@/entities/tool";

/** Inner areas of the AI Services page, in tab order (ADR-0034). */
export const SERVICES_TABS = ["services", "routing", "usage"] as const;

export type ServicesTab = (typeof SERVICES_TABS)[number];

export const DEFAULT_SERVICES_TAB: ServicesTab = "services";

/**
 * The endpoints tab's own two pages: the saved list, and the page behind "Add
 * endpoint" (ADR-0057). Page state, not a route: it lives only while the tab
 * is open, and leaving the tab or switching tool lands back on the list.
 */
export type ServicesEndpointsView = "endpoints" | "add";

/**
 * Which inner area is showing, plus the one-shot tool intent the shell handed
 * over. The intent survives tab switches until the endpoints tab has actually
 * been shown once; after the user leaves that tab the remembered scope rules,
 * so an old Home intent can never override a choice the user made here.
 */
export function useServicesTab(
  preferredTab: ServicesTab | null,
  preferredToolId: ToolId | null,
  reselected = 0,
) {
  const [tab, setTab] = useState<ServicesTab>(
    preferredTab ?? DEFAULT_SERVICES_TAB,
  );
  const [toolIntent, setToolIntent] = useState<ToolId | null>(preferredToolId);
  const [view, setView] = useState<ServicesEndpointsView>("endpoints");
  // Clicking this page in the sidebar again leaves the add page for the list,
  // the way the breadcrumb does.
  const [seenReselect, setSeenReselect] = useState(reselected);
  if (reselected !== seenReselect) {
    setSeenReselect(reselected);
    setView("endpoints");
  }

  const select = (next: ServicesTab) => {
    if (next === tab) return;
    if (tab === "services") setToolIntent(null);
    setView("endpoints");
    setTab(next);
  };

  return { tab, toolIntent, view, setView, select };
}
