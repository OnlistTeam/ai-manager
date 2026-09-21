import {
  AlertCircle,
  CheckCircle2,
  ChevronDown,
  ShieldCheck,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { RoutingOverview } from "@/entities/routing";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";

export function RoutingSummary({
  overview,
  busy,
  onStop,
}: {
  overview: RoutingOverview;
  busy: boolean;
  onStop: () => void;
}) {
  const { t } = useTranslation();
  const takeoverCount = overview.targets.filter(
    (target) => target.takeoverEnabled,
  ).length;
  const needsRestore = !overview.running && takeoverCount > 0;
  const Icon = needsRestore
    ? AlertCircle
    : overview.running
      ? CheckCircle2
      : ShieldCheck;
  const metrics = [
    { key: "requests", value: overview.totalRequests },
    { key: "success", value: overview.successRequests },
    { key: "failures", value: overview.failedRequests },
    { key: "failovers", value: overview.failoverCount },
  ];
  const showStats = overview.running || metrics.some(({ value }) => value > 0);
  const address = overview.address?.includes(":")
    ? `[${overview.address}]`
    : overview.address;

  return (
    <div className="flex flex-wrap items-start gap-x-4 gap-y-2 text-caption text-content-muted">
      <p role="status" className="flex min-h-9 min-w-0 items-center gap-2">
        <Icon
          className={cn(
            "h-4 w-4 shrink-0",
            needsRestore
              ? "text-warning"
              : overview.running
                ? "text-success"
                : "text-content-muted",
          )}
          aria-hidden="true"
        />
        <span>
          {needsRestore
            ? t("routing.summary.needsRestore")
            : overview.running
              ? `${t("routing.hero.running")} · ${t("routing.hero.takeovers", { count: takeoverCount })}`
              : t("routing.summary.inactive")}
        </span>
      </p>
      {overview.running && address && overview.port ? (
        <span className="flex min-h-9 items-center break-all font-mono">
          {address}:{overview.port}
        </span>
      ) : null}
      {showStats ? (
        <details className="group">
          <summary
            tabIndex={0}
            className={cn(
              "flex min-h-9 cursor-pointer list-none items-center gap-1 rounded-md hover:text-content [&::-webkit-details-marker]:hidden",
              FOCUS_RING,
            )}
          >
            {t("routing.summary.stats")}
            <ChevronDown
              className="h-3.5 w-3.5 group-open:rotate-180"
              aria-hidden="true"
            />
          </summary>
          <dl className="flex flex-wrap gap-x-4 gap-y-2 pb-1">
            {metrics.map(({ key, value }) => (
              <div key={key} className="flex gap-1.5">
                <dt>{t(`routing.hero.metric.${key}`)}</dt>
                <dd className="tabular-nums text-content">
                  {value.toLocaleString()}
                </dd>
              </div>
            ))}
          </dl>
        </details>
      ) : null}
      {overview.running || takeoverCount > 0 ? (
        <Button
          size="sm"
          variant="secondary"
          className="ml-auto"
          disabled={busy}
          onClick={onStop}
        >
          {t("routing.stop.action")}
        </Button>
      ) : null}
    </div>
  );
}
