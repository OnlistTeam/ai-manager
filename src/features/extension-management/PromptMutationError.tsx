import { AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toErrorCopy } from "@/shared/lib/nativeError";

interface PromptMutationErrorProps {
  error: Error;
  titleKey: string;
}

export function PromptMutationError({
  error,
  titleKey,
}: PromptMutationErrorProps) {
  const { t } = useTranslation();
  const copy = toErrorCopy(error);
  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
    >
      <AlertCircle
        className="mt-0.5 h-4 w-4 shrink-0 text-danger"
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-caption font-medium text-content">{t(titleKey)}</p>
        <p className="mt-0.5 text-caption leading-5 text-content-muted">
          {t(copy.messageKey)}
        </p>
        {copy.remediationKey ? (
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {t(copy.remediationKey)}
          </p>
        ) : null}
      </div>
    </div>
  );
}
