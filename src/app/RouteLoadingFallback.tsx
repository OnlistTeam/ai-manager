import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { SpatialPageHeader } from "@/shared/ui/SpatialPageHeader";
import { NAV_ITEMS, type AppRoute } from "./routes";

/**
 * Which shape each destination starts with. Only the home page starts with a
 * spatial hero; every other page starts with a SectionHeader. If the
 * placeholder always drew the big model, the moment loading finished the page
 * would suddenly lose an object it never actually had. New pages must declare
 * their shape here (enforced by the type), rather than writing
 * `route === "home"` in render code (AI_RULES rule 8).
 */
const ROUTE_LOADING_SHAPE = {
  home: "hero",
  tools: "section",
  services: "section",
  extensions: "section",
  mcp: "section",
  prompts: "section",
  data: "section",
  settings: "section",
} as const satisfies Record<AppRoute, "hero" | "section">;

export interface RouteLoadingFallbackProps {
  route?: AppRoute;
}

/**
 * Keeps the destination's visual identity in place while its local route
 * chunk is decoded. The route table is the source of both label and icon, so
 * loading never introduces a second navigation mapping.
 */
export function RouteLoadingFallback({
  route = "home",
}: RouteLoadingFallbackProps) {
  const { t } = useTranslation();
  const item = NAV_ITEMS.find((candidate) => candidate.id === route);

  // AppRoute and NAV_ITEMS are kept in lockstep by routes.test.ts. Returning a
  // semantic generic state here still fails softly if that invariant regresses
  // in a locally modified build.
  if (!item) {
    return (
      <section
        role="status"
        aria-live="polite"
        aria-label={t("nav.loadingPage")}
        className="min-h-[196px] rounded-[30px] border border-hairline bg-layer-1"
      />
    );
  }

  if (ROUTE_LOADING_SHAPE[route] === "section") {
    return (
      <section
        role="status"
        aria-live="polite"
        aria-label={t("nav.loadingPage")}
        className="flex flex-col gap-6"
      >
        <SectionHeader
          as="h1"
          title={t(item.labelKey)}
          action={
            <LoaderCircle
              aria-hidden="true"
              className="h-4 w-4 text-content-muted motion-safe:animate-spin"
            />
          }
        />
        <div
          aria-hidden="true"
          className="min-h-[196px] rounded-[30px] border border-hairline bg-layer-1"
        />
      </section>
    );
  }

  return (
    <section role="status" aria-live="polite" aria-label={t("nav.loadingPage")}>
      <SpatialPageHeader
        title={t(item.labelKey)}
        icon={item.icon}
        model="environment"
        tone="brand"
        loading
        eyebrow={
          <span className="inline-flex items-center gap-2">
            <LoaderCircle
              aria-hidden="true"
              className="h-4 w-4 motion-safe:animate-spin"
            />
            {t("nav.loadingPage")}
          </span>
        }
      />
    </section>
  );
}
