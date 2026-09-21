import { RotateCcw, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { SkillBackup } from "@/entities/skill-backup";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";

interface SkillBackupRowProps {
  backup: SkillBackup;
  createdAt: string;
  actionsBlocked: boolean;
  restoring: boolean;
  deleting: boolean;
  onRestore: (backup: SkillBackup) => void;
  onDelete: (backup: SkillBackup) => void;
}

export function SkillBackupRow({
  backup,
  createdAt,
  actionsBlocked,
  restoring,
  deleting,
  onRestore,
  onDelete,
}: SkillBackupRowProps) {
  const { t } = useTranslation();
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const mutationPending = restoring || deleting;

  return (
    <li className="rounded-xl border border-hairline bg-layer-1 px-4 py-3 shadow-sm">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="break-words text-body font-medium text-content">
              {backup.name}
            </p>
            {backup.conflicts ? (
              <Badge tone="warning">
                {t("extensions.skill.backups.alreadyInstalled")}
              </Badge>
            ) : null}
          </div>
          {backup.description ? (
            <p className="mt-1 break-words text-caption leading-5 text-content-muted">
              {backup.description}
            </p>
          ) : null}
          <p className="mt-2 text-caption text-content-muted">
            {t("extensions.skill.backups.createdAt", { date: createdAt })}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button
            size="sm"
            variant="secondary"
            loading={restoring}
            disabled={actionsBlocked || backup.conflicts}
            aria-label={t("extensions.skill.backups.restoreNamed", {
              name: backup.name,
            })}
            onClick={() => onRestore(backup)}
          >
            <RotateCcw className="h-4 w-4" aria-hidden="true" />
            {t("extensions.skill.backups.restore")}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="text-danger hover:bg-danger/10 hover:text-danger"
            disabled={actionsBlocked}
            aria-label={t("extensions.skill.backups.deleteNamed", {
              name: backup.name,
            })}
            onClick={() => setConfirmingDelete(true)}
          >
            <Trash2 className="h-4 w-4" aria-hidden="true" />
            {t("extensions.skill.backups.delete")}
          </Button>
        </div>
      </div>

      {confirmingDelete ? (
        <div
          role="alertdialog"
          aria-label={t("extensions.skill.backups.deleteTitle", {
            name: backup.name,
          })}
          className="mt-3 flex flex-col gap-3 border-t border-hairline pt-3 sm:flex-row sm:items-center sm:justify-between"
        >
          <p className="text-caption leading-5 text-content-muted">
            {t("extensions.skill.backups.deleteDescription")}
          </p>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              size="sm"
              variant="ghost"
              disabled={mutationPending}
              onClick={() => setConfirmingDelete(false)}
            >
              {t("ds.action.cancel")}
            </Button>
            <Button
              size="sm"
              variant="danger"
              loading={deleting}
              disabled={actionsBlocked && !deleting}
              onClick={() => onDelete(backup)}
            >
              {t("extensions.skill.backups.deleteConfirm")}
            </Button>
          </div>
        </div>
      ) : null}
    </li>
  );
}
