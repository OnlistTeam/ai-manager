import { AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toErrorCopy } from "@/shared/lib/nativeError";

interface ProviderConnectFailureProps {
  error: Error;
}

/** Safe, durable recovery copy for the beginner connection form. */
export function ProviderConnectFailure({ error }: ProviderConnectFailureProps) {
  const { t } = useTranslation();
  const copy = toErrorCopy(error);
  const title = t("services.connect.errorTitle");

  return (
    <div
      role="alert"
      aria-label={title}
      className="rounded-lg border border-danger/25 bg-danger/10 p-3 text-caption text-content"
    >
      <p className="flex items-start gap-2 font-medium">
        <AlertCircle
          className="mt-0.5 h-4 w-4 shrink-0 text-danger"
          aria-hidden="true"
        />
        <span>{title}</span>
      </p>
      <p className="mt-1.5">{t(copy.messageKey)}</p>
      {copy.remediationKey ? (
        <p className="mt-1.5 text-content-muted">{t(copy.remediationKey)}</p>
      ) : null}
      <p className="mt-1.5 text-content-muted">
        {t("services.connect.retryHint")}
      </p>
    </div>
  );
}
