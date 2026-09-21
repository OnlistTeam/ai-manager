import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ErrorCopy } from "@/shared/lib/nativeError";

export interface DeepLinkErrorNoticeProps {
  error: ErrorCopy;
}

/**
 * Import failures happen inside a dialog with no details affordance, so the
 * recovery sentence is spelled out here instead of pointing at a View Details
 * action that is not on screen.
 */
export function DeepLinkErrorNotice({ error }: DeepLinkErrorNoticeProps) {
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
          {t("deeplink.error.retry")}
        </p>
      </div>
    </div>
  );
}
