import { useCallback, useState } from "react";
import { extensionScopeKey, type ExtensionKind } from "@/entities/extension";
import type { ToolId } from "@/entities/tool";
import type { ExtensionsTab } from "@/pages/extensions/useExtensionsWorkspaceTab";
import type { ServicesTab } from "@/pages/services/useServicesTab";
import type { AppRoute } from "./routes";

/**
 * One-shot navigation intents. A page consumes them on mount; every ordinary
 * navigation clears them so a stale intent never resurfaces on a later visit.
 * They live in shell state only: nothing here is persisted.
 */
export interface RouteIntents {
  serviceTool: ToolId | null;
  servicesTab: ServicesTab | null;
  extensionsTab: ExtensionsTab | null;
  /** Which kind tab to land on. Without it the page falls back to the remembered kind. */
  extensionsKind: ExtensionKind | null;
  toolDetails: ToolId | null;
}

const NO_INTENTS: RouteIntents = {
  serviceTool: null,
  servicesTab: null,
  extensionsTab: null,
  extensionsKind: null,
  toolDetails: null,
};

function extensionsTabKey(tab: ExtensionsTab | null): string | null {
  return tab === null || typeof tab === "string" ? tab : extensionScopeKey(tab);
}

function sameIntents(a: RouteIntents, b: RouteIntents): boolean {
  return (
    a.serviceTool === b.serviceTool &&
    a.servicesTab === b.servicesTab &&
    extensionsTabKey(a.extensionsTab) === extensionsTabKey(b.extensionsTab) &&
    a.extensionsKind === b.extensionsKind &&
    a.toolDetails === b.toolDetails
  );
}

export function useRouteIntents(navigate: (route: AppRoute) => void) {
  const [intents, setIntents] = useState<RouteIntents>(NO_INTENTS);

  const open = useCallback(
    (route: AppRoute, next: Partial<RouteIntents> = {}) => {
      // If the content hasn't changed, hand back the same object so React
      // skips this update: clicking the current page's sidebar item again
      // shouldn't re-render the entire shell.
      setIntents((current) => {
        const merged = { ...NO_INTENTS, ...next };
        return sameIntents(current, merged) ? current : merged;
      });
      navigate(route);
    },
    [navigate],
  );

  const openRoute = useCallback((route: AppRoute) => open(route), [open]);
  const openServices = useCallback(
    (toolId?: ToolId, tab?: ServicesTab) =>
      open("services", {
        serviceTool: toolId ?? null,
        servicesTab: tab ?? null,
      }),
    [open],
  );
  const openExtensions = useCallback(
    (tab?: ExtensionsTab, kind?: ExtensionKind) => {
      if (tab === "workspace") {
        open("tools", { toolDetails: "openclaw" });
        return;
      }
      const route =
        kind === "prompt"
          ? "prompts"
          : kind === "mcp" || tab?.kind === "desktopApp"
            ? "mcp"
            : "extensions";
      open(route, {
        extensionsTab: tab ?? null,
        extensionsKind: kind ?? null,
      });
    },
    [open],
  );

  return {
    intents,
    openRoute,
    openServices,
    openExtensions,
  };
}
