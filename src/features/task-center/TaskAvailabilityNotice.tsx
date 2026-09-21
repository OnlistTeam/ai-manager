import { Loader2, RefreshCw, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useOperations } from "@/entities/operation";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";

export type TaskAvailability = "checking" | "unavailable" | null;

export interface TaskAvailabilityNoticeProps {
  state: TaskAvailability;
  refreshing?: boolean;
  onRetry: () => void;
}

export function useTaskAvailability() {
  const operations = useOperations();
  const state: TaskAvailability = operations.data
    ? null
    : operations.isError
      ? "unavailable"
      : "checking";
  const actionsBlocked =
    state !== null || operations.isFetching || operations.isError;

  return {
    operations,
    state,
    actionsBlocked,
  };
}

/** One explanation owns the temporary page-wide lifecycle lock. */
export function TaskAvailabilityNotice({
  state,
  refreshing = false,
  onRetry,
}: TaskAvailabilityNoticeProps) {
  const { t } = useTranslation();
  if (state === null) return null;

  const unavailable = state === "unavailable";
  const Icon = unavailable ? TriangleAlert : Loader2;

  return (
    <Card
      padding="sm"
      role={unavailable ? "alert" : "status"}
      aria-busy={refreshing || undefined}
      className={
        unavailable
          ? "border-warning/30 bg-warning/5"
          : "border-brand/20 bg-brand/5"
      }
    >
      <div className="flex items-center gap-3">
        <span
          aria-hidden="true"
          className={
            unavailable
              ? "flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-warning/10 text-warning"
              : "flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-brand/10 text-brand"
          }
        >
          <Icon
            className={
              unavailable ? "h-4 w-4" : "h-4 w-4 motion-safe:animate-spin"
            }
          />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-body font-medium text-content">
            {t(
              unavailable
                ? "taskCenter.unavailable.title"
                : "taskCenter.loading.summary",
            )}
          </p>
          <p className="mt-0.5 text-caption text-content-muted">
            {t(
              unavailable
                ? "taskCenter.unavailable.description"
                : "taskCenter.loading.label",
            )}
          </p>
        </div>
        {unavailable ? (
          <Button
            variant="secondary"
            size="sm"
            loading={refreshing}
            onClick={onRetry}
          >
            {refreshing ? null : (
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
            )}
            {t("taskCenter.unavailable.retry")}
          </Button>
        ) : null}
      </div>
    </Card>
  );
}
