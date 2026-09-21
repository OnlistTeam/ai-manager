import { AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { cn } from "@/shared/ui/cn";

export interface ToolActionErrorProps {
  error: Error;
  className?: string;
}

/** Keeps a failed task start beside the action that can retry it. */
export function ToolActionError({ error, className }: ToolActionErrorProps) {
  const { t } = useTranslation();
  const copy = toErrorCopy(error);
  const title = t(copy.messageKey);

  return (
    <div
      role="alert"
      aria-label={title}
      className={cn(
        "flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3",
        className,
      )}
    >
      <AlertCircle
        className="mt-0.5 h-4 w-4 shrink-0 text-danger"
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-caption font-medium text-content">{title}</p>
        {copy.remediationKey ? (
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {t(copy.remediationKey)}
          </p>
        ) : null}
      </div>
    </div>
  );
}
