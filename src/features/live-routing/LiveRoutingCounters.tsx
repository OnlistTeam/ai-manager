import { useTranslation } from "react-i18next";
import type { RoutingTraceCounts } from "@/entities/routing";

const COUNTERS = ["requests", "rerouted", "failed"] as const;

/** Requests, rerouted, errors the tools saw: totals since AI Manager started. */
export function LiveRoutingCounters({
  counts,
}: {
  counts: RoutingTraceCounts;
}) {
  const { t, i18n } = useTranslation();
  const format = new Intl.NumberFormat(i18n.language);

  return (
    <dl className="flex shrink-0 items-baseline gap-x-4 text-caption">
      {COUNTERS.map((key) => (
        <div
          key={key}
          data-counter={key}
          className="flex items-baseline gap-1.5"
        >
          <dt className="text-content-muted">
            {t(`routing.live.counts.${key}`)}
          </dt>
          <dd className="font-medium tabular-nums text-content">
            {format.format(counts[key])}
          </dd>
        </div>
      ))}
    </dl>
  );
}
