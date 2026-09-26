import { ArrowRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { RoutingOverview, ToolId } from "@/entities/routing";
import { cn } from "@/shared/ui/cn";
import { ToolGlyph } from "@/shared/ui/ToolArtwork";
import { routedServiceNames } from "./liveRoutingFormat";

/** Your tools → AI Manager → services, on one line. */
export function LiveRoutingFlow({
  overview,
  busyTools,
}: {
  overview: RoutingOverview;
  busyTools: ReadonlySet<ToolId>;
}) {
  const { t } = useTranslation();
  const services = routedServiceNames(overview);

  return (
    <p className="flex min-w-0 items-center gap-2 text-caption">
      <span className="flex shrink-0 items-center gap-1">
        {overview.targets.map(({ tool, takeoverEnabled }) => {
          const name = t(`routing.tool.${tool}`);
          const label = t(
            takeoverEnabled
              ? "routing.live.flow.routed"
              : "routing.live.flow.direct",
            { name },
          );
          return (
            <span
              key={tool}
              title={label}
              data-flow-tool={tool}
              data-active={takeoverEnabled}
              className={cn(
                "relative flex h-6 w-6 items-center justify-center rounded-md border",
                takeoverEnabled
                  ? "border-brand/40 bg-brand/10"
                  : "border-hairline opacity-40 grayscale",
              )}
            >
              <ToolGlyph toolId={tool} className="h-3.5 w-3.5" />
              {busyTools.has(tool) ? (
                <span
                  className="absolute -right-0.5 -top-0.5 h-1.5 w-1.5 rounded-full bg-brand motion-safe:animate-pulse"
                  aria-hidden="true"
                />
              ) : null}
              <span className="sr-only">{label}</span>
            </span>
          );
        })}
      </span>
      <ArrowRight
        className="h-3.5 w-3.5 shrink-0 text-content-muted"
        aria-hidden="true"
      />
      <span className="shrink-0 rounded-md border border-brand/30 bg-brand/10 px-2 py-0.5 font-medium text-content">
        {t("routing.live.flow.manager")}
      </span>
      <ArrowRight
        className="h-3.5 w-3.5 shrink-0 text-content-muted"
        aria-hidden="true"
      />
      <span className="min-w-0 truncate text-content">
        {services.length > 0
          ? services.join(t("routing.live.listSeparator"))
          : t("routing.live.flow.noServices")}
      </span>
    </p>
  );
}
