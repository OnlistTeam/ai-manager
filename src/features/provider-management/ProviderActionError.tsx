import { AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toErrorCopy } from "@/shared/lib/nativeError";

interface ProviderActionErrorProps {
  title: string;
  error: Error;
  retryHint: string;
}

/** Durable, redacted feedback for an action the same provider card can retry. */
export function ProviderActionError({
  title,
  error,
  retryHint,
}: ProviderActionErrorProps) {
  const { t } = useTranslation();
  const copy = toErrorCopy(error);

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
      <p className="mt-1.5 text-content-muted">{retryHint}</p>
    </div>
  );
}
