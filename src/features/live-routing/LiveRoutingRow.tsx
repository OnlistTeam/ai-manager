import { AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { RoutingTraceEntry } from "@/entities/routing";
import { cn } from "@/shared/ui/cn";
import { ToolGlyph } from "@/shared/ui/ToolArtwork";
import { LiveRoutingAttempts } from "./LiveRoutingAttempts";
import { formatClock, formatDuration } from "./liveRoutingFormat";

/** Shared column template so the header-less list lines up. */
export const LIVE_ROUTING_ROW_GRID =
  "grid grid-cols-[4.5rem_minmax(0,8rem)_minmax(0,1fr)_minmax(0,1.6fr)_5.5rem] items-center gap-x-3";

function Outcome({ entry }: { entry: RoutingTraceEntry }) {
  const { t, i18n } = useTranslation();

  if (entry.status === "pending") {
    return (
      <span className="flex items-center justify-end gap-1.5 text-content-muted">
        <span
          className="h-1.5 w-1.5 rounded-full bg-brand motion-safe:animate-pulse"
          aria-hidden="true"
        />
        {t("routing.live.pending")}
      </span>
    );
  }

  const duration =
    entry.totalMs === null ? "" : formatDuration(entry.totalMs, i18n.language);
  if (entry.status === "failed") {
    return (
      <span
        title={duration}
        className="flex min-w-0 items-center justify-end gap-1 text-content"
      >
        <AlertCircle
          className="h-3 w-3 shrink-0 text-danger"
          aria-hidden="true"
        />
        <span className="truncate">
          {t(`routing.live.errorCategory.${entry.error ?? "other"}`)}
        </span>
      </span>
    );
  }
  return (
    <span className="text-right tabular-nums text-content-muted">
      {duration}
    </span>
  );
}

/** One request on one line: time · tool · model · services tried · outcome. */
export function LiveRoutingRow({
  entry,
  fresh,
}: {
  entry: RoutingTraceEntry;
  fresh: boolean;
}) {
  const { t, i18n } = useTranslation();

  return (
    <li
      data-trace-seq={entry.seq}
      data-trace-status={entry.status}
      className={cn(
        LIVE_ROUTING_ROW_GRID,
        "min-h-8 border-b border-hairline px-3 py-1.5 text-caption last:border-b-0",
        fresh && "motion-safe:animate-ds-overlay-in",
      )}
    >
      <time
        dateTime={new Date(entry.startedAt).toISOString()}
        className="tabular-nums text-content-muted"
      >
        {formatClock(entry.startedAt, i18n.language)}
      </time>
      <span className="flex min-w-0 items-center gap-1.5 text-content">
        <ToolGlyph toolId={entry.tool} className="h-3.5 w-3.5 shrink-0" />
        <span className="truncate">{t(`routing.tool.${entry.tool}`)}</span>
      </span>
      <span
        className={cn(
          "truncate text-content-muted",
          entry.model !== null && "font-mono",
        )}
        title={entry.model ?? undefined}
      >
        {entry.model ?? t("routing.live.unknownModel")}
      </span>
      {entry.status === "pending" && entry.attempts.length === 0 ? (
        <span aria-hidden="true" />
      ) : (
        <LiveRoutingAttempts attempts={entry.attempts} />
      )}
      <Outcome entry={entry} />
    </li>
  );
}
