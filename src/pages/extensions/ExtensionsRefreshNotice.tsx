import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";

export interface ExtensionsRefreshNoticeProps {
  refreshing: boolean;
  onRetry: () => void;
}

/** Keeps the last scoped inventory visible while its authoritative refresh is unavailable. */
export function ExtensionsRefreshNotice({
  refreshing,
  onRetry,
}: ExtensionsRefreshNoticeProps) {
  const { t } = useTranslation();

  return (
    <aside
      role="alert"
      aria-label={t("extensions.refreshError.title")}
      aria-busy={refreshing || undefined}
      className="flex items-start gap-3 rounded-lg border border-warning/30 bg-warning/10 px-4 py-3"
    >
      <AlertTriangle
        className="mt-0.5 h-4 w-4 shrink-0 text-warning"
        aria-hidden="true"
      />
      <div className="min-w-0 flex-1">
        <p className="text-body font-medium text-content">
          {t("extensions.refreshError.title")}
        </p>
        <p className="mt-0.5 text-caption leading-5 text-content-muted">
          {t("extensions.refreshError.description")}
        </p>
      </div>
      <Button
        size="sm"
        variant="secondary"
        loading={refreshing}
        onClick={onRetry}
      >
        {t("extensions.refresh")}
      </Button>
    </aside>
  );
}
