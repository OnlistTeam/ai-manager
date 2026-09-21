import { AlertTriangle, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useSkillUpdates } from "@/entities/skill-update";
import { Button } from "@/shared/ui/Button";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";

interface SkillUpdatesNoticeProps {
  updates: ReturnType<typeof useSkillUpdates>;
}

/**
 * The upstream checker skips repositories it cannot reach. We therefore show
 * known updates and failures, but never turn an empty result into an "all
 * current" claim.
 */
export function SkillUpdatesNotice({ updates }: SkillUpdatesNoticeProps) {
  const { t } = useTranslation();
  const initiallyChecking = updates.isPending && !updates.isFetched;

  if (initiallyChecking) {
    return (
      <DetectionStatus
        label={t("extensions.skill.update.checking")}
        className="min-h-20"
      />
    );
  }

  if (updates.isError) {
    return (
      <aside
        role="alert"
        aria-label={t("extensions.skill.update.checkErrorTitle")}
        aria-busy={updates.isFetching || undefined}
        className="flex min-w-0 flex-col gap-3 rounded-xl border border-warning/30 bg-warning/10 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
      >
        <div className="flex min-w-0 items-start gap-3">
          <AlertTriangle
            className="mt-0.5 h-4 w-4 shrink-0 text-warning"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="text-body font-medium text-content">
              {t("extensions.skill.update.checkErrorTitle")}
            </p>
            <p className="mt-0.5 text-caption leading-5 text-content-muted">
              {t("extensions.skill.update.checkErrorDescription")}
            </p>
          </div>
        </div>
        <Button
          size="sm"
          variant="secondary"
          className="self-start sm:self-auto"
          loading={updates.isFetching}
          onClick={() => void updates.refetch()}
        >
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          {t("extensions.skill.update.retry")}
        </Button>
      </aside>
    );
  }

  const count = updates.data?.length ?? 0;
  if (count === 0) return null;

  return (
    <p
      role="status"
      className="flex items-center gap-2 rounded-xl border border-warning/25 bg-warning/8 px-4 py-3 text-caption font-medium text-content"
    >
      <RefreshCw className="h-4 w-4 text-warning" aria-hidden="true" />
      {t("extensions.skill.update.knownCount", { count })}
    </p>
  );
}
