import { AlertCircle, Database, ShieldCheck } from "lucide-react";
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
import { UsageSyncIndicator, UsageSyncStatus } from "./UsageSyncStatus";
import { useUsageAutoSync } from "./useUsageAutoSync";

export function UsagePage() {
  const { t } = useTranslation();
  const pageRef = useRef<HTMLDivElement>(null);
  const usage = useUsageOverview();
  const refresh = useRefreshUsage();
  const dataAvailable = usage.data !== undefined;
  const initiallyLoading = usage.isPending && !usage.isFetched;
  const unavailable = usage.isFetched && !dataAvailable;
  const stale = usage.isError && dataAvailable;
  const hasRecords = dataAvailable && usage.data.summary.requests > 0;
  const syncError = refresh.error ? toErrorCopy(refresh.error) : null;
  const sourceIssues = refresh.data?.sync.sourceIssues ?? 0;
  useUsageAutoSync(refresh.syncIfDue);

  return (
    <div
      ref={pageRef}
      role="region"
      aria-label={t("usage.title")}
      tabIndex={-1}
      className="flex min-w-0 flex-col gap-4 outline-none"
    >
      <SectionHeader
        as="h2"
        title={t("usage.title")}
        description={t("usage.description")}
        action={refresh.isPending && hasRecords ? <UsageSyncIndicator /> : null}
      />

      {initiallyLoading ? <DetectionStatus label={t("usage.loading")} /> : null}
      {refresh.isPending && !hasRecords && !initiallyLoading ? (
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
          <div className="min-w-0 flex-1">
            <p className="text-body font-medium text-content">
              {t("usage.sync.errorTitle")}
            </p>
            <p className="mt-0.5 text-caption text-content-muted">
              {t(syncError.messageKey)} {t("usage.sync.errorHint")}
            </p>
          </div>
          <Button
            size="sm"
            variant="secondary"
            loading={refresh.isPending}
            onClick={refresh.mutate}
          >
            {t("usage.error.retry")}
          </Button>
        </Card>
      ) : null}

      {refresh.isSuccess && refresh.data && sourceIssues > 0 ? (
        <Card
          role="status"
          className="flex items-start gap-3 border-warning/25 bg-warning/5"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-warning"
            aria-hidden="true"
          />
          <p className="text-caption leading-5 text-content-muted">
            {t("usage.sync.partial", refresh.data.sync)}
          </p>
        </Card>
      ) : null}

      {usage.data ? (
        usage.data.summary.requests === 0 ? (
          refresh.isPending ? null : (
            <EmptyState
              icon={Database}
              title={t("usage.empty.title")}
              description={t("usage.empty.description")}
            />
          )
        ) : (
          <>
            <UsageHero overview={usage.data} />
            <div className="grid gap-4 xl:grid-cols-[minmax(0,1.25fr)_minmax(320px,0.75fr)] xl:items-start">
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
