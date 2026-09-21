import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ErrorCopy } from "@/shared/lib/nativeError";

export interface ImportErrorNoticeProps {
  error: ErrorCopy;
}

/**
 * Import has no durable error-details surface. Use import-specific recovery copy instead of the
 * generic remediation that tells the user to open a View Details action which is not present.
 */
export function ImportErrorNotice({ error }: ImportErrorNoticeProps) {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-lg bg-danger/10 p-3"
    >
      <AlertTriangle
        className="mt-0.5 h-4 w-4 shrink-0 text-danger"
        aria-hidden="true"
      />
      <div>
        <p className="text-caption font-medium text-danger">
          {t(error.messageKey)}
        </p>
        <p className="mt-1 text-caption leading-5 text-content-muted">
          {t("preferences.import.error.retry")}
        </p>
      </div>
    </div>
  );
}
