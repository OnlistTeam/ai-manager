import { AlertTriangle, RefreshCw, Waypoints } from "lucide-react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { EmptyState } from "@/shared/ui/EmptyState";

interface ServicesToolsUnavailableProps {
  refreshing: boolean;
  retryButtonRef: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Keeps an initial tool read failure and its keyboard context mounted. */
export function ServicesToolsUnavailable({
  refreshing,
  retryButtonRef,
  onRetry,
}: ServicesToolsUnavailableProps) {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      aria-label={t("services.toolsError.title")}
      aria-busy={refreshing || undefined}
    >
      <EmptyState
        icon={Waypoints}
        title={t("services.toolsError.title")}
        description={t("services.toolsError.description")}
        action={
          <Button ref={retryButtonRef} loading={refreshing} onClick={onRetry}>
            {t("services.refresh")}
          </Button>
        }
      />
    </div>
  );
}

interface ServicesToolRefreshNoticeProps {
  refreshing: boolean;
  retryButtonRef: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Explains why retained service content is temporarily read-only. */
export function ServicesToolRefreshNotice({
  refreshing,
  retryButtonRef,
  onRetry,
}: ServicesToolRefreshNoticeProps) {
  const { t } = useTranslation();

  return (
    <aside
      role="alert"
      aria-label={t("services.toolsRefreshError.title")}
      aria-busy={refreshing || undefined}
      className="flex min-w-0 flex-col gap-3 rounded-lg border border-warning/30 bg-warning/10 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
    >
      <div className="flex min-w-0 items-start gap-3">
        <AlertTriangle
          className="mt-0.5 h-4 w-4 shrink-0 text-warning"
          aria-hidden="true"
        />
        <div className="min-w-0">
          <p className="text-body font-medium text-content">
            {t("services.toolsRefreshError.title")}
          </p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {t("services.toolsRefreshError.description")}
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
        {t("services.toolsRefreshError.action")}
      </Button>
    </aside>
  );
}
