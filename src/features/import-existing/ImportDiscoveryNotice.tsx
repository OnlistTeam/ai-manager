import type { Ref } from "react";
import { AlertTriangle, Download, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { EmptyState } from "@/shared/ui/EmptyState";

export interface ImportDiscoveryRecoveryProps {
  refreshing: boolean;
  disabled?: boolean;
  retryButtonRef?: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Keeps the first discovery error and its only retry stable during refetch. */
export function ImportDiscoveryUnavailable({
  refreshing,
  retryButtonRef,
  onRetry,
}: ImportDiscoveryRecoveryProps) {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      aria-label={t("preferences.import.error.title")}
      aria-busy={refreshing || undefined}
    >
      <EmptyState
        icon={Download}
        title={t("preferences.import.error.title")}
        description={t("preferences.import.error.description")}
        action={
          <Button ref={retryButtonRef} loading={refreshing} onClick={onRetry}>
            <RefreshCw className="h-4 w-4" aria-hidden="true" />
            {t("preferences.import.retry")}
          </Button>
        }
        className="rounded-lg border border-hairline bg-layer-1 py-6 shadow-sm"
      />
    </div>
  );
}

/** Keeps the last trusted discovery result visible while importing is paused. */
export function ImportRefreshNotice({
  refreshing,
  disabled = false,
  retryButtonRef,
  onRetry,
}: ImportDiscoveryRecoveryProps) {
  const { t } = useTranslation();

  return (
    <aside
      role="alert"
      aria-label={t("preferences.import.refreshError.title")}
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
            {t("preferences.import.refreshError.title")}
          </p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {t("preferences.import.refreshError.description")}
          </p>
        </div>
      </div>
      <Button
        ref={retryButtonRef}
        variant="secondary"
        size="sm"
        className="self-start sm:self-auto"
        loading={refreshing}
        disabled={disabled}
        onClick={onRetry}
      >
        <RefreshCw className="h-4 w-4" aria-hidden="true" />
        {t("preferences.import.retry")}
      </Button>
    </aside>
  );
}
