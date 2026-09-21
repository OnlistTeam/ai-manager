import { AlertCircle, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";

export interface BackupActionErrorProps {
  error: ErrorCopy;
  disabled?: boolean;
  onRetry: () => void;
}

export function BackupActionError({
  error,
  disabled = false,
  onRetry,
}: BackupActionErrorProps) {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      className="flex flex-col gap-3 rounded-lg border border-danger/30 bg-danger/5 p-4 sm:flex-row sm:items-center sm:justify-between"
    >
      <div className="flex min-w-0 items-start gap-3">
        <AlertCircle
          className="mt-0.5 h-5 w-5 shrink-0 text-danger"
          aria-hidden="true"
        />
        <div>
          <p className="text-body font-medium text-content">
            {t(error.messageKey)}
          </p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {t("preferences.backup.error.actionRetry")}
          </p>
        </div>
      </div>
      <Button
        variant="secondary"
        size="sm"
        className="self-start sm:self-auto"
        disabled={disabled}
        onClick={onRetry}
      >
        <RefreshCw className="h-4 w-4" aria-hidden="true" />
        {t("preferences.backup.retry")}
      </Button>
    </div>
  );
}
