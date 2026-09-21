import { useState } from "react";
import type { ToolId } from "@/entities/tool";

/** Inner areas of the AI Services page, in tab order (ADR-0034). */
export const SERVICES_TABS = ["services", "routing", "usage"] as const;

export type ServicesTab = (typeof SERVICES_TABS)[number];

export const DEFAULT_SERVICES_TAB: ServicesTab = "services";

/**
 * Which inner area is showing, plus the one-shot tool intent the shell handed
 * over. The intent survives tab switches until the endpoints tab has actually
 * been shown once; after the user leaves that tab the remembered scope rules,
 * so an old Home intent can never override a choice the user made here.
 */
export function useServicesTab(
  preferredTab: ServicesTab | null,
  preferredToolId: ToolId | null,
) {
  const [tab, setTab] = useState<ServicesTab>(
    preferredTab ?? DEFAULT_SERVICES_TAB,
  );
  const [toolIntent, setToolIntent] = useState<ToolId | null>(preferredToolId);

  const select = (next: ServicesTab) => {
    if (next === tab) return;
    if (tab === "services") setToolIntent(null);
    setTab(next);
  };

  return { tab, toolIntent, select };
}
