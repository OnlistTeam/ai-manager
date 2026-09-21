import type { Ref } from "react";
import { AlertTriangle, Archive, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";

export function BackupInventorySkeleton() {
  const { t } = useTranslation();

  return <DetectionStatus label={t("preferences.backup.loading")} />;
}

export interface BackupInventoryRecoveryProps {
  refreshing: boolean;
  retryButtonRef?: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Keeps the first-read error and its only retry control mounted during recovery. */
export function BackupInventoryUnavailable({
  refreshing,
  retryButtonRef,
  onRetry,
}: BackupInventoryRecoveryProps) {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      aria-label={t("preferences.backup.error.title")}
      aria-busy={refreshing || undefined}
    >
      <EmptyState
        icon={Archive}
        title={t("preferences.backup.error.title")}
        description={t("preferences.backup.error.description")}
        action={
          <Button ref={retryButtonRef} loading={refreshing} onClick={onRetry}>
            {t("preferences.backup.retry")}
          </Button>
        }
      />
    </div>
  );
}

/** Retains the last trusted list while a fresh authoritative read is unavailable. */
export function BackupInventoryNotice({
  refreshing,
  retryButtonRef,
  onRetry,
}: BackupInventoryRecoveryProps) {
  const { t } = useTranslation();

  return (
    <aside
      role="alert"
      aria-label={t("preferences.backup.refreshError.title")}
      aria-busy={refreshing || undefined}
      className="flex flex-col gap-3 rounded-lg border border-warning/30 bg-warning/10 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
    >
      <div className="flex min-w-0 items-start gap-3">
        <AlertTriangle
          className="mt-0.5 h-4 w-4 shrink-0 text-warning"
          aria-hidden="true"
        />
        <div className="min-w-0">
          <p className="text-body font-medium text-content">
            {t("preferences.backup.refreshError.title")}
          </p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {t("preferences.backup.refreshError.description")}
          </p>
        </div>
      </div>
      <Button
        ref={retryButtonRef}
        variant="secondary"
        size="sm"
        className="self-start sm:self-auto"
        loading={refreshing}
        onClick={onRetry}
      >
        <RefreshCw className="h-4 w-4" aria-hidden="true" />
        {t("preferences.backup.retry")}
      </Button>
    </aside>
  );
}
