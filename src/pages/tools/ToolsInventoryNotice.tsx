import { PackageSearch, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { EmptyState } from "@/shared/ui/EmptyState";
import { InventoryRefreshNotice } from "@/shared/ui/InventoryRefreshNotice";

export interface ToolsRefreshButtonProps {
  refreshing: boolean;
  onRefresh: () => void;
}

export function ToolsRefreshButton({
  refreshing,
  onRefresh,
}: ToolsRefreshButtonProps) {
  const { t } = useTranslation();

  return (
    <Button
      variant="secondary"
      size="sm"
      loading={refreshing}
      onClick={onRefresh}
    >
      <RefreshCw className="h-4 w-4" aria-hidden="true" />
      {t("tools.refresh")}
    </Button>
  );
}

export interface ToolsUnavailableStateProps {
  refreshing: boolean;
  onRetry: () => void;
}

/** Keeps the initial read failure and its retry control stable while refetching. */
export function ToolsUnavailableState({
  refreshing,
  onRetry,
}: ToolsUnavailableStateProps) {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      aria-label={t("tools.error.title")}
      aria-busy={refreshing || undefined}
    >
      <EmptyState
        icon={PackageSearch}
        title={t("tools.error.title")}
        description={t("tools.error.description")}
        action={
          <Button loading={refreshing} onClick={onRetry}>
            {t("tools.refresh")}
          </Button>
        }
      />
    </div>
  );
}

export interface ToolsInventoryNoticeProps {
  refreshing: boolean;
  onRetry: () => void;
}

/** Keeps the last detector result visible while a fresh authoritative read is unavailable. */
export function ToolsInventoryNotice({
  refreshing,
  onRetry,
}: ToolsInventoryNoticeProps) {
  const { t } = useTranslation();

  return (
    <InventoryRefreshNotice
      title={t("tools.refreshError.title")}
      description={t("tools.refreshError.description")}
      actionLabel={t("tools.refresh")}
      refreshing={refreshing}
      onRetry={onRetry}
    />
  );
}
