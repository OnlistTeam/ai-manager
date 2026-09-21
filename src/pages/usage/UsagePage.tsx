import {
  AlertCircle,
  CheckCircle2,
  Database,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { useRefreshUsage, useUsageOverview } from "@/entities/usage";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { Card } from "@/shared/ui/Card";
import { EmptyState } from "@/shared/ui/EmptyState";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { UsageHero } from "./UsageHero";
import { UsageToolBreakdown } from "./UsageToolBreakdown";
import { UsageTrendChart } from "./UsageTrendChart";
import { UsageSyncStatus } from "./UsageSyncStatus";

export function UsagePage() {
  const { t } = useTranslation();
  const pageRef = useRef<HTMLDivElement>(null);
  const usage = useUsageOverview();
  const refresh = useRefreshUsage();
  const dataAvailable = usage.data !== undefined;
  const initiallyLoading = usage.isPending && !usage.isFetched;
  const unavailable = usage.isFetched && !dataAvailable;
  const stale = usage.isError && dataAvailable;
  const syncError = refresh.error ? toErrorCopy(refresh.error) : null;

  function syncLocal(): void {
    refresh.mutate();
  }

  return (
    <div
      ref={pageRef}
      role="region"
      aria-label={t("usage.title")}
      tabIndex={-1}
      className="flex min-w-0 flex-col gap-6 outline-none"
    >
      <SectionHeader
        as="h2"
        title={t("usage.title")}
        description={t("usage.description")}
        action={
          dataAvailable && usage.data.summary.requests > 0 ? (
            <Button loading={refresh.isPending} onClick={syncLocal}>
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
              {t("usage.sync.action")}
            </Button>
          ) : null
        }
      />

      {initiallyLoading ? <DetectionStatus label={t("usage.loading")} /> : null}
      {refresh.isPending ? (
        <UsageSyncStatus startedAt={refresh.submittedAt} />
      ) : null}

      {unavailable ? (
        <div role="alert" aria-label={t("usage.error.title")}>
          <EmptyState
            icon={AlertCircle}
            title={t("usage.error.title")}
            description={t("usage.error.description")}
            action={
              <Button
                loading={usage.isFetching}
                onClick={() => void usage.refetch()}
              >
                {t("usage.error.retry")}
              </Button>
            }
          />
        </div>
      ) : null}

      {stale ? (
        <Card
          role="alert"
          className="flex items-start gap-3 border-warning/25 bg-warning/5"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-warning"
            aria-hidden="true"
          />
          <div className="min-w-0 flex-1">
            <p className="text-body font-medium text-content">
              {t("usage.stale.title")}
            </p>
            <p className="mt-0.5 text-caption leading-5 text-content-muted">
              {t("usage.stale.description")}
            </p>
          </div>
          <Button
            size="sm"
            variant="secondary"
            loading={usage.isFetching}
            onClick={() => void usage.refetch()}
          >
            {t("usage.error.retry")}
          </Button>
        </Card>
      ) : null}

      {syncError ? (
        <Card
          role="alert"
          className="flex items-start gap-3 border-danger/25 bg-danger/5"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="text-body font-medium text-content">
              {t("usage.sync.errorTitle")}
            </p>
            <p className="mt-0.5 text-caption text-content-muted">
              {t(syncError.messageKey)} {t("usage.sync.errorHint")}
            </p>
          </div>
        </Card>
      ) : null}

      {refresh.isSuccess && refresh.data ? (
        <Card
          role="status"
          className="flex items-start gap-3 border-success/20 bg-success/5"
        >
          {refresh.data.sync.sourceIssues > 0 ? (
            <AlertCircle
              className="mt-0.5 h-4 w-4 shrink-0 text-warning"
              aria-hidden="true"
            />
          ) : (
            <CheckCircle2
              className="mt-0.5 h-4 w-4 shrink-0 text-success"
              aria-hidden="true"
            />
          )}
          <p className="text-caption leading-5 text-content-muted">
            {t(
              refresh.data.sync.sourceIssues > 0
                ? "usage.sync.partial"
                : "usage.sync.success",
              refresh.data.sync,
            )}
          </p>
        </Card>
      ) : null}

      {usage.data ? (
        usage.data.summary.requests === 0 ? (
          <EmptyState
            icon={Database}
            title={t("usage.empty.title")}
            description={t("usage.empty.description")}
            action={
              <Button loading={refresh.isPending} onClick={syncLocal}>
                <RefreshCw className="h-4 w-4" aria-hidden="true" />
                {t("usage.sync.action")}
              </Button>
            }
          />
        ) : (
          <>
            <UsageHero overview={usage.data} />
            <div className="grid gap-6 xl:grid-cols-[minmax(0,1.25fr)_minmax(320px,0.75fr)] xl:items-start">
              <UsageTrendChart days={usage.data.trend} />
              <Card padding="lg" className="rounded-xl">
                <ShieldCheck
                  className="h-5 w-5 text-success"
                  aria-hidden="true"
                />
                <h2 className="mt-3 text-heading text-content">
                  {t("usage.privacy.title")}
                </h2>
                <p className="mt-1 text-caption leading-5 text-content-muted">
                  {t("usage.privacy.description")}
                </p>
              </Card>
            </div>
            {usage.data.byTool.length > 0 ? (
              <UsageToolBreakdown items={usage.data.byTool} />
            ) : null}
          </>
        )
      ) : null}
    </div>
  );
}
