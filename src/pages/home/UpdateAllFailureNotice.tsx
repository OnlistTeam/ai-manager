import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/shared/ui/Button";

export interface UpdateAllFailureNoticeProps {
  failedCount: number;
  startedCount: number;
  skippedCount: number;
  onReviewSkipped?: () => void;
}

/** Keeps a bulk begin failure beside the exact retry that remains available. */
export function UpdateAllFailureNotice({
  failedCount,
  startedCount,
  skippedCount,
  onReviewSkipped,
}: UpdateAllFailureNoticeProps) {
  const { t } = useTranslation();
  const partial = startedCount > 0;
  const title = t(
    partial
      ? "home.updateAll.failure.partial.title"
      : "home.updateAll.failure.all.title",
  );
  const description = t(
    partial
      ? "home.updateAll.failure.partial.description"
      : "home.updateAll.failure.all.description",
    { count: partial ? startedCount : failedCount },
  );

  return (
    <div
      role="alert"
      aria-label={title}
      className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-3"
    >
      <AlertTriangle
        className="mt-0.5 h-4 w-4 shrink-0 text-warning"
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-caption font-medium text-content">{title}</p>
        {failedCount > 0 ? (
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {description}
          </p>
        ) : null}
        {skippedCount > 0 ? (
          <p className="mt-1 text-caption leading-5 text-content-muted">
            {t("home.updateAll.skippedSummary", { count: skippedCount })}
          </p>
        ) : null}
        {skippedCount > 0 && onReviewSkipped ? (
          <Button
            variant="ghost"
            size="sm"
            className="mt-2 -ml-3"
            onClick={onReviewSkipped}
          >
            {t("home.updateAll.hints.review")}
          </Button>
        ) : null}
      </div>
    </div>
  );
}
