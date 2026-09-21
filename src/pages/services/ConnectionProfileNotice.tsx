import { AlertTriangle, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";

export interface RecoverableNoticeProps {
  title: string;
  description: string;
  /** When omitted, this is a plain notice with no retry control. */
  retryLabel?: string;
  refreshing?: boolean;
  onRetry?: () => void;
}

/** Shared skeleton for every "load failed partway through, can retry" banner at the top of a page. */
export function RecoverableNotice({
  title,
  description,
  retryLabel,
  refreshing = false,
  onRetry,
}: RecoverableNoticeProps) {
  return (
    <aside
      role="alert"
      aria-label={title}
      aria-busy={refreshing || undefined}
      className="flex flex-col gap-3 rounded-lg border border-warning/30 bg-warning/10 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
    >
      <div className="flex min-w-0 items-start gap-3">
        <AlertTriangle
          className="mt-0.5 h-4 w-4 shrink-0 text-warning"
          aria-hidden="true"
        />
        <div className="min-w-0">
          <p className="text-body font-medium text-content">{title}</p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {description}
          </p>
        </div>
      </div>
      {onRetry && retryLabel ? (
        <Button
          variant="secondary"
          size="sm"
          className="self-start sm:self-auto"
          loading={refreshing}
          onClick={onRetry}
        >
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          {retryLabel}
        </Button>
      ) : null}
    </aside>
  );
}

export interface ConnectionProfileNoticeProps {
  refreshing: boolean;
  onRetry: () => void;
}

/** Keeps a transient template read failure recoverable without exposing it. */
export function ConnectionProfileNotice({
  refreshing,
  onRetry,
}: ConnectionProfileNoticeProps) {
  const { t } = useTranslation();

  return (
    <RecoverableNotice
      title={t("services.connect.profileErrorTitle")}
      description={t("services.connect.profileError")}
      retryLabel={t("services.connect.profileRetry")}
      refreshing={refreshing}
      onRetry={onRetry}
    />
  );
}
