import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { NAV_ITEMS, type AppRoute } from "./routes";

export interface RouteLoadingFallbackProps {
  route?: AppRoute;
}

/**
 * Keeps the destination's name in place while its local route chunk is
 * decoded. Every page, Home included, opens with a compact header rather than
 * a spatial hero, so the placeholder never draws a model the page does not
 * have. The route table is the source of the label, so loading never
 * introduces a second navigation mapping.
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

  return (
    <section
      role="status"
      aria-live="polite"
      aria-label={t("nav.loadingPage")}
      className="flex flex-col gap-4"
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
