import { AlertCircle, CheckCircle2, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { RoutingOverview } from "@/entities/routing";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";

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
  // While live routing is on, its switch is the way to stop; the button only
  // covers a route running with no tool, or tools left pointing at a stopped one.
  const showStop = overview.running !== takeoverCount > 0;
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
      {showStop ? (
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
