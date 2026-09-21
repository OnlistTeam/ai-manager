import type { Ref } from "react";
import { AlertTriangle, LoaderCircle, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";

export type SkillCatalogInventoryNoticeMode =
  | "refreshing"
  | "unavailable"
  | "retained";

export interface SkillCatalogInventoryNoticeProps {
  mode: SkillCatalogInventoryNoticeMode;
  refreshing: boolean;
  retryButtonRef?: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Explains whether the visible catalog is current without exposing transport errors. */
export function SkillCatalogInventoryNotice({
  mode,
  refreshing,
  retryButtonRef,
  onRetry,
}: SkillCatalogInventoryNoticeProps) {
  const { t } = useTranslation();
  const isProgress = mode === "refreshing";
  const isUnavailable = mode === "unavailable";
  const title = t(
    isProgress
      ? "extensions.skill.catalog.refreshingTitle"
      : isUnavailable
        ? "extensions.skill.catalog.errorTitle"
        : "extensions.skill.catalog.refreshErrorTitle",
  );
  const description = t(
    isProgress
      ? "extensions.skill.catalog.refreshingDescription"
      : isUnavailable
        ? "extensions.skill.catalog.errorDescription"
        : "extensions.skill.catalog.refreshErrorDescription",
  );

  return (
    <aside
      role={isProgress ? "status" : "alert"}
      aria-label={title}
      aria-busy={refreshing || undefined}
      className={cn(
        "mt-4 flex flex-col gap-3 rounded-lg border px-4 py-3 sm:flex-row sm:items-center sm:justify-between",
        isProgress && "border-brand/20 bg-brand/5",
        isUnavailable && "border-danger/25 bg-danger/5",
        mode === "retained" && "border-warning/30 bg-warning/10",
      )}
    >
      <div className="flex min-w-0 items-start gap-3">
        {isProgress ? (
          <LoaderCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-brand motion-safe:animate-spin"
            aria-hidden="true"
          />
        ) : (
          <AlertTriangle
            className={cn(
              "mt-0.5 h-4 w-4 shrink-0",
              isUnavailable ? "text-danger" : "text-warning",
            )}
            aria-hidden="true"
          />
        )}
        <div className="min-w-0">
          <p className="text-body font-medium text-content">{title}</p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {description}
          </p>
        </div>
      </div>
      {!isProgress ? (
        <Button
          ref={retryButtonRef}
          variant="secondary"
          size="sm"
          className="self-start sm:self-auto"
          loading={refreshing}
          onClick={onRetry}
        >
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          {t("extensions.skill.catalog.retry")}
        </Button>
      ) : null}
    </aside>
  );
}
