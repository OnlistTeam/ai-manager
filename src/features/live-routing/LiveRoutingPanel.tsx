import { useTranslation } from "react-i18next";
import { useRoutingOverview, useRoutingTrace } from "@/entities/routing";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { LiveRoutingHeader } from "./LiveRoutingHeader";
import { LiveRoutingLive } from "./LiveRoutingLive";
import { useHideEmails } from "./useHideEmails";

export interface LiveRoutingPanelProps {
  className?: string;
}

/**
 * The live view of live routing (ADR-0050): a stage that plays each request
 * from the tool through AI Manager to the services it tried, one sentence
 * about the latest step, the totals, and the recent requests one per line.
 * Reads its own data, so a page only decides where to place it.
 */
export function LiveRoutingPanel({ className }: LiveRoutingPanelProps) {
  const { t } = useTranslation();
  const overview = useRoutingOverview();
  const trace = useRoutingTrace();
  const [hideEmails, setHideEmails] = useHideEmails();

  return (
    <section
      aria-label={t("routing.live.panelLabel")}
      data-live-routing-panel=""
      className={cn(
        "overflow-hidden rounded-xl border border-hairline bg-layer-1 shadow-sm",
        className,
      )}
    >
      <LiveRoutingHeader
        hideEmails={hideEmails}
        onHideEmailsChange={setHideEmails}
      />
      {trace.data ? (
        <LiveRoutingLive
          overview={overview.data}
          snapshot={trace.data}
          hideEmails={hideEmails}
        />
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
