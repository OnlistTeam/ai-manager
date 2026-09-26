import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  EMPTY_ROUTING_TRACE,
  useRoutingOverview,
  useRoutingTrace,
  type ToolId,
} from "@/entities/routing";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { LiveRoutingCounters } from "./LiveRoutingCounters";
import { LiveRoutingFlow } from "./LiveRoutingFlow";
import { LiveRoutingList } from "./LiveRoutingList";

export interface LiveRoutingPanelProps {
  className?: string;
}

/**
 * The live view of live routing (ADR-0050): where requests come from, where
 * they went, and the recent requests one per line. Reads its own data, so a
 * page only decides where to place it.
 */
export function LiveRoutingPanel({ className }: LiveRoutingPanelProps) {
  const { t } = useTranslation();
  const overview = useRoutingOverview();
  const trace = useRoutingTrace();
  const snapshot = trace.data ?? EMPTY_ROUTING_TRACE;
  const busyTools = useMemo(
    () =>
      new Set<ToolId>(
        snapshot.entries
          .filter((entry) => entry.status === "pending")
          .map((entry) => entry.tool),
      ),
    [snapshot.entries],
  );

  return (
    <section
      aria-label={t("routing.live.panelLabel")}
      data-live-routing-panel=""
      className={cn(
        "overflow-hidden rounded-xl border border-hairline bg-layer-1 shadow-sm",
        className,
      )}
    >
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b border-hairline px-3 py-2.5">
        {overview.data ? (
          <LiveRoutingFlow overview={overview.data} busyTools={busyTools} />
        ) : null}
        <LiveRoutingCounters counts={snapshot.counts} />
      </div>
      {trace.data ? (
        <LiveRoutingList entries={trace.data.entries} />
      ) : trace.isError ? (
        <div
          role="alert"
          className="flex items-center justify-between gap-3 px-3 py-2 text-caption text-content-muted"
        >
          <span>{t("routing.live.traceError")}</span>
          <Button
            size="sm"
            variant="secondary"
            loading={trace.isFetching}
            onClick={() => void trace.refetch()}
          >
            {t("routing.error.retry")}
          </Button>
        </div>
      ) : (
        <p className="px-3 py-3 text-caption text-content-muted">
          {t("routing.live.loading")}
        </p>
      )}
    </section>
  );
}
