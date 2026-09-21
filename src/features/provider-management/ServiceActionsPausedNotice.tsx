import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/ui/cn";

export interface ServiceActionsPausedNoticeProps {
  className?: string;
}

/** Local fail-closed explanation for a service action that was already open. */
export function ServiceActionsPausedNotice({
  className,
}: ServiceActionsPausedNoticeProps) {
  const { t } = useTranslation();

  return (
    <aside
      role="alert"
      aria-label={t("services.actionsPaused.title")}
      className={cn(
        "mb-4 flex min-w-0 items-start gap-2 rounded-lg border border-warning/30 bg-warning/10 p-3",
        className,
      )}
    >
      <AlertTriangle
        className="mt-0.5 h-4 w-4 shrink-0 text-warning"
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-caption font-medium text-content">
          {t("services.actionsPaused.title")}
        </p>
        <p className="mt-0.5 text-caption leading-5 text-content-muted">
          {t("services.actionsPaused.description")}
        </p>
      </div>
    </aside>
  );
}
