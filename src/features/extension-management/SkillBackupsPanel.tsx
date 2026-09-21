import { AlertCircle, ArchiveRestore, RefreshCw } from "lucide-react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useSkillBackups, type SkillBackup } from "@/entities/skill-backup";
import type { ToolId } from "@/entities/tool";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { SkillBackupRow } from "./SkillBackupRow";
import {
  useDeleteSkillBackup,
  useRestoreSkillBackup,
} from "./useSkillBackupMutations";

interface SkillBackupsPanelProps {
  tool: ToolId;
  blocked: boolean;
  onRestoreStarted: () => void;
  onBusyChange: (busy: boolean) => void;
}

export function formatSkillBackupDate(
  unixSeconds: number,
  locale: string,
): string {
  const date = new Date(unixSeconds * 1000);
  if (!Number.isFinite(unixSeconds) || Number.isNaN(date.getTime())) return "—";
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

export function SkillBackupsPanel({
  tool,
  blocked,
  onRestoreStarted,
  onBusyChange,
}: SkillBackupsPanelProps) {
  const { t, i18n } = useTranslation();
  const refreshRef = useRef<HTMLButtonElement>(null);
  const backups = useSkillBackups();
  const restore = useRestoreSkillBackup();
  const remove = useDeleteSkillBackup();
  const data = backups.data ?? [];
  const dataAvailable = backups.data !== undefined;
  const mutationError = restore.error ?? remove.error;
  const errorCopy = mutationError ? toErrorCopy(mutationError) : null;
  const actionsBlocked =
    blocked || backups.isFetching || restore.isPending || remove.isPending;

  useEffect(() => {
    if (backups.isFetching) return;

    const frame = window.requestAnimationFrame(() => {
      if (!refreshRef.current?.disabled) refreshRef.current?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [backups.isFetching]);

  useEffect(() => {
    onBusyChange(restore.isPending || remove.isPending);
    return () => onBusyChange(false);
  }, [onBusyChange, remove.isPending, restore.isPending]);

  const restoreBackup = (backup: SkillBackup) => {
    remove.reset();
    restore.mutate(
      { tool, backup },
      {
        onSuccess: () => onRestoreStarted(),
      },
    );
  };

  const deleteBackup = (backup: SkillBackup) => {
    restore.reset();
    remove.mutate(backup);
  };

  return (
    <div className="flex flex-col gap-4">
      <section className="flex flex-col gap-3 rounded-xl border border-hairline bg-layer-1 p-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex min-w-0 items-start gap-3">
          <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand/10 text-brand">
            <ArchiveRestore className="h-5 w-5" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <h3 className="font-medium text-content">
              {t("extensions.skill.backups.localTitle")}
            </h3>
            <p className="mt-1 text-caption leading-5 text-content-muted">
              {t("extensions.skill.backups.localDescription")}
            </p>
          </div>
        </div>
        <Button
          ref={refreshRef}
          size="sm"
          variant="secondary"
          loading={backups.isFetching}
          disabled={restore.isPending || remove.isPending}
          onClick={() => void backups.refetch()}
        >
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          {t("extensions.skill.backups.refresh")}
        </Button>
      </section>

      {backups.isPending && !dataAvailable ? (
        <DetectionStatus
          label={t("extensions.skill.backups.loading")}
          className="min-h-24"
        />
      ) : null}

      {backups.isError ? (
        <div
          role="alert"
          aria-label={t("extensions.skill.backups.errorTitle")}
          className="flex items-start gap-2 rounded-xl border border-warning/30 bg-warning/10 px-4 py-3"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-warning"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="text-caption font-medium text-content">
              {t("extensions.skill.backups.errorTitle")}
            </p>
            <p className="mt-0.5 text-caption text-content-muted">
              {t("extensions.skill.backups.errorDescription")}
            </p>
          </div>
        </div>
      ) : null}

      {errorCopy ? (
        <div
          role="alert"
          aria-label={t(errorCopy.messageKey)}
          className="flex items-start gap-2 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="text-caption font-medium text-content">
              {t(errorCopy.messageKey)}
            </p>
            <p className="mt-0.5 text-caption text-content-muted">
              {t("extensions.skill.backups.mutationErrorHint")}
            </p>
          </div>
        </div>
      ) : null}

      {dataAvailable && data.length === 0 ? (
        <p className="rounded-xl border border-dashed border-hairline px-4 py-8 text-center text-body text-content-muted">
          {t("extensions.skill.backups.empty")}
        </p>
      ) : null}

      {data.length > 0 ? (
        <ul className="scrollbar-subtle flex max-h-[42vh] flex-col gap-2 overflow-y-auto pr-1">
          {data.map((backup) => (
            <SkillBackupRow
              key={backup.id}
              backup={backup}
              createdAt={formatSkillBackupDate(
                backup.createdAt,
                i18n.resolvedLanguage ?? i18n.language,
              )}
              actionsBlocked={actionsBlocked}
              restoring={
                restore.isPending && restore.variables?.backup.id === backup.id
              }
              deleting={remove.isPending && remove.variables?.id === backup.id}
              onRestore={restoreBackup}
              onDelete={deleteBackup}
            />
          ))}
        </ul>
      ) : null}
    </div>
  );
}
