import { Loader2, RefreshCw, TriangleAlert } from "lucide-react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";

interface TaskToolInventoryNoticeProps {
  retained: boolean;
  refreshing: boolean;
  retryButtonRef: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Compact recovery surface for the tool authority used by completion actions. */
export function TaskToolInventoryNotice({
  retained,
  refreshing,
  retryButtonRef,
  onRetry,
}: TaskToolInventoryNoticeProps) {
  const { t } = useTranslation();
  const titleKey = retained
    ? "taskCenter.toolAuthority.refreshTitle"
    : "taskCenter.toolAuthority.unavailableTitle";
  const descriptionKey = retained
    ? "taskCenter.toolAuthority.refreshDescription"
    : "taskCenter.toolAuthority.unavailableDescription";

  return (
    <aside
      role="alert"
      aria-label={t(titleKey)}
      aria-busy={refreshing || undefined}
      className="mb-3 rounded-xl border border-warning/30 bg-warning/10 p-3"
    >
      <div className="flex items-start gap-3">
        <TriangleAlert
          className="mt-0.5 h-4 w-4 shrink-0 text-warning"
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <p className="text-body font-medium text-content">{t(titleKey)}</p>
          <p className="mt-1 text-caption leading-5 text-content-muted">
            {t(descriptionKey)}
          </p>
          <Button
            ref={retryButtonRef}
            variant="secondary"
            size="sm"
            className="mt-3"
            loading={refreshing}
            onClick={onRetry}
          >
            <RefreshCw className="h-4 w-4" aria-hidden="true" />
            {t("taskCenter.toolAuthority.retry")}
          </Button>
        </div>
      </div>
    </aside>
  );
}

export type TaskToolPauseReason = "checking" | "failed" | "changed";

interface TaskToolActionsPausedNoticeProps {
  reason: TaskToolPauseReason;
}

/** Keeps an already-open launch confirmation truthful while tools are rechecked. */
export function TaskToolActionsPausedNotice({
  reason,
}: TaskToolActionsPausedNoticeProps) {
  const { t } = useTranslation();
  const checking = reason === "checking";
  const changed = reason === "changed";
  const Icon = checking ? Loader2 : TriangleAlert;
  const titleKey = changed
    ? "taskCenter.toolAuthority.changedTitle"
    : "taskCenter.toolAuthority.pausedTitle";
  const descriptionKey = changed
    ? "taskCenter.toolAuthority.changedDescription"
    : "taskCenter.toolAuthority.pausedDescription";

  return (
    <div
      role={checking ? "status" : "alert"}
      aria-label={t(titleKey)}
      aria-busy={checking || undefined}
      className="mt-3 flex items-start gap-3 rounded-lg border border-warning/30 bg-warning/10 p-3"
    >
      <Icon
        className={
          checking
            ? "mt-0.5 h-4 w-4 shrink-0 text-warning motion-safe:animate-spin"
            : "mt-0.5 h-4 w-4 shrink-0 text-warning"
        }
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-body font-medium text-content">{t(titleKey)}</p>
        <p className="mt-0.5 text-caption leading-5 text-content-muted">
          {t(descriptionKey)}
        </p>
      </div>
    </div>
  );
}
