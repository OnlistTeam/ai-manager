import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  type CSSProperties,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import { Sidebar, type SidebarItem } from "@/shared/ui/Sidebar";
import {
  DRAG_REGION_ATTR,
  DRAG_REGION_STYLE,
  isMac,
  isWindows,
} from "@/lib/platform";
import { NAV_ITEMS, isAppRoute, type AppRoute } from "./routes";
import { useAppRoute } from "./useAppRoute";
import { useRouteIntents } from "./useRouteIntents";
import { useRouteMotion } from "./useRouteMotion";
import { RouteLoadingFallback } from "./RouteLoadingFallback";
import { RouteBackdrop } from "./RouteBackdrop";
import { RouteContentTransition } from "./RouteContentTransition";
import { WindowControls } from "@/features/window-chrome";
import { AppStatusBar } from "./AppStatusBar";

import brandMark from "@/assets/spatial/models/v5/mark.png";

export { RouteLoadingFallback } from "./RouteLoadingFallback";
/**
 * macOS windows use `titleBarStyle: "Overlay"` (see src-tauri/tauri.conf.json),
 * so the system traffic lights are drawn directly in the top-left corner of
 * the client area. Without a draggable placeholder bar, the traffic lights
 * would overlap the sidebar logo row, and the window wouldn't be draggable
 * under fullSizeContentView either. Other platforms use the native title bar
 * and need no placeholder (height 0, not rendered), matching the semantics of
 * legacy `src/App.tsx`'s DEFAULT_DRAG_BAR_HEIGHT.
 *
 * Windows draws its own title bar instead (see WindowControls, ADR-0044), so
 * it needs no placeholder either. Linux is the one platform still using the
 * native bar: drag regions are disabled there for a Wayland defect, and an
 * undecorated window nobody can drag would be a worse trade than a seam.
 */
const MAC_DRAG_BAR_HEIGHT = 28; // px

const HomePage = lazy(async () => ({
  default: (await import("@/pages/home/HomePage")).HomePage,
}));
const ToolsPage = lazy(async () => ({
  default: (await import("@/pages/tools/ToolsPage")).ToolsPage,
}));
const ServicesPage = lazy(async () => ({
  default: (await import("@/pages/services/ServicesPage")).ServicesPage,
}));
const ExtensionsPage = lazy(async () => ({
  default: (await import("@/pages/extensions/ExtensionsPage")).ExtensionsPage,
}));
const DataPage = lazy(async () => ({
  default: (await import("@/pages/data/DataPage")).DataPage,
}));
const SettingsPage = lazy(async () => ({
  default: (await import("@/pages/settings/SettingsPage")).SettingsPage,
}));

export function AppShell() {
  const { t } = useTranslation();
  const { route, navigate } = useAppRoute();
  const activeRoute = route;
  const { direction, navigate: navigateWithMotion } = useRouteMotion(
    activeRoute,
    navigate,
  );
  const mainRef = useRef<HTMLElement>(null);
  const mac = isMac();
  const windows = isWindows();

  /*
   * Dialogs and menus render into portals under `document.body`, so they are
   * not descendants of the canvas below and cannot read a route variable
   * declared on it. Mirroring the route onto `<html>` puts the whole palette in
   * scope for every node in the document, portals included.
   */
  useEffect(() => {
    document.documentElement.dataset.route = activeRoute;
  }, [activeRoute]);

  // Every navigation starts the next page at the top of the shared viewport.
  const navigateFromTop = useCallback(
    (next: AppRoute) => {
      if (mainRef.current) mainRef.current.scrollTop = 0;
      navigateWithMotion(next);
    },
    [navigateWithMotion],
  );
  const { intents, openRoute, openServices, openExtensions } =
    useRouteIntents(navigateFromTop);

  const toSidebarItem = (item: (typeof NAV_ITEMS)[number]): SidebarItem => ({
    id: item.id,
    label: t(item.labelKey),
    icon: item.icon,
  });
  const railItems = NAV_ITEMS.filter((item) => item.placement !== "footer").map(
    toSidebarItem,
  );
  const footerItems = NAV_ITEMS.filter(
    (item) => item.placement === "footer",
  ).map(toSidebarItem);

  // The single route → page mapping table. Adding a page means adding a row
  // here; rendering code must never contain an `if (route === "home")` chain
  // (same spirit as AI_RULES rule 8).
  //
  // The table holds render functions, not components: the element type is
  // always one of the module-level lazy components above. If each entry were
  // written as an inline component, the table would be rebuilt whenever
  // intents change, and React would unmount and remount the whole subtree
  // because the component's identity changed — clicking the sidebar again on
  // the current page, or any open* call that lands on the current page, would
  // clear the search term, selection, and open dialogs.
  const pages = useMemo<Record<AppRoute, () => ReactNode>>(
    () => ({
      home: () => (
        <HomePage
          onOpenTools={() => openRoute("tools")}
          onOpenServices={openServices}
          onOpenExtensions={() => openExtensions()}
        />
      ),
      tools: () => (
        <ToolsPage
          preferredToolId={intents.toolDetails}
          onOpenServices={openServices}
          onOpenExtensions={openExtensions}
        />
      ),
      services: () => (
        <ServicesPage
          preferredToolId={intents.serviceTool}
          preferredTab={intents.servicesTab}
        />
      ),
      extensions: () => (
        <ExtensionsPage
          key="skill"
          fixedKind="skill"
          preferredTab={intents.extensionsTab}
          preferredKind={intents.extensionsKind}
        />
      ),
      mcp: () => (
        <ExtensionsPage
          key="mcp"
          fixedKind="mcp"
          preferredTab={intents.extensionsTab}
        />
      ),
      prompts: () => (
        <ExtensionsPage
          key="prompt"
          fixedKind="prompt"
          preferredTab={intents.extensionsTab}
        />
      ),
      data: () => <DataPage />,
      settings: () => <SettingsPage onOpenServices={openServices} />,
    }),
    [intents, openRoute, openExtensions, openServices],
  );

  const renderRoute = useCallback(
    (pageRoute: AppRoute): ReactNode => (
      <Suspense fallback={<RouteLoadingFallback route={pageRoute} />}>
        {pages[pageRoute]()}
      </Suspense>
    ),
    [pages],
  );

  return (
    <div
      data-app-window=""
      data-route={activeRoute}
      className="app-window-canvas relative flex h-full max-h-full w-full flex-row overflow-hidden text-content"
    >
      <RouteBackdrop route={activeRoute} />
      {windows ? <WindowControls /> : null}
      {mac ? (
        <div
          data-window-drag-region=""
          aria-hidden="true"
          className="absolute inset-x-0 top-0 z-20 w-full"
          style={
            {
              height: MAC_DRAG_BAR_HEIGHT,
              ...DRAG_REGION_STYLE,
            } as CSSProperties
          }
          {...DRAG_REGION_ATTR}
        />
      ) : null}
      <Sidebar
        items={railItems}
        footerItems={footerItems}
        activeId={activeRoute}
        onSelect={(id) => {
          if (isAppRoute(id)) openRoute(id);
        }}
        className={
          mac ? "app-sidebar relative z-10 pt-10" : "app-sidebar relative z-10"
        }
        logo={
          <span className="flex items-center gap-2.5">
            <img
              src={brandMark}
              alt=""
              draggable={false}
              className="app-logo-mark h-9 w-9 shrink-0"
            />
            <span className="truncate text-heading font-medium text-content">
              {t("nav.appName")}
            </span>
          </span>
        }
      />
      <div className="relative z-10 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-transparent">
        {/* The top material layer belongs to the content area, not the
         * window: its purpose is to fade scrolling content at the top and
         * host the top-right controls. If it were mounted on the window, its
         * dark gradient would spill across the sidebar and bury the brand
         * mark in the top-left corner under a layer of shadow. The sidebar
         * doesn't scroll and has no controls. */}
        <AppStatusBar mac={mac} windows={windows} />
        <main
          ref={mainRef}
          data-app-scroll-viewport=""
          data-scrollbar-mode="hidden"
          className="app-scroll-viewport scrollbar-hidden min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto bg-transparent"
        >
          {/* Spec §51: don't stretch indefinitely on ultra-wide screens, cap the content area at 1400px.
           * px-5/lg:px-7 (not px-6/lg:px-8) — `.app-route-transition` inside
           * gives every route's leading/trailing control room for its
           * keyboard focus ring via its own 4px padding (src/index.css),
           * so 4px of this padding moved there; the total inset from the
           * window edge is unchanged. */}
          <div
            className={`app-scroll-content mx-auto min-w-0 w-full max-w-[1400px] overflow-x-hidden px-5 pb-10 lg:px-7 lg:pb-12 ${
              mac ? "pt-[88px] lg:pt-[92px]" : "pt-[72px] lg:pt-[76px]"
            }`}
          >
            <RouteContentTransition
              route={activeRoute}
              direction={direction}
              renderRoute={renderRoute}
            />
          </div>
        </main>
      </div>
    </div>
  );
}
