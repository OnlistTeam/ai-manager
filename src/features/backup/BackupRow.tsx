import { ArchiveRestore } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { BackupFile } from "@/entities/backup";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { backupLabel, backupSizeMb } from "./backupLabel";

export interface BackupRowProps {
  file: BackupFile;
  latest: boolean;
  /** Legacy caller compatibility; raw file identity is no longer prominent. */
  advancedMode?: boolean;
  busy: boolean;
  onRename: () => void;
  onRestore: () => void;
  onDelete: () => void;
}

export function BackupRow({
  file,
  latest,
  busy,
  onRename,
  onRestore,
  onDelete,
}: BackupRowProps) {
  const { t, i18n } = useTranslation();
  const label = backupLabel(file, i18n.language);

  return (
    <li className="flex flex-col gap-3 border-b border-hairline py-4 last:border-b-0 sm:flex-row sm:items-center sm:justify-between">
      <div className="flex min-w-0 items-start gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-brand/10 text-brand">
          <ArchiveRestore className="h-4 w-4" aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-start gap-2">
            <p className="min-w-0 flex-[1_1_12rem] break-words text-body font-medium text-content">
              {label}
            </p>
            {latest ? (
              <Badge tone="success">
                {t("preferences.backup.summary.latestBadge")}
              </Badge>
            ) : null}
          </div>
          <p className="mt-0.5 text-caption text-content-muted">
            {t("preferences.backup.size", { size: backupSizeMb(file) })}
          </p>
        </div>
      </div>
      <div className="flex shrink-0 gap-2 self-end sm:self-auto">
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          aria-label={t("preferences.backup.renameNamed", { name: label })}
          onClick={onRename}
        >
          {t("preferences.backup.rename")}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          disabled={busy}
          aria-label={t("preferences.backup.restoreNamed", { name: label })}
          onClick={onRestore}
        >
          {t("preferences.backup.restore")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          aria-label={t("preferences.backup.deleteNamed", { name: label })}
          onClick={onDelete}
        >
          {t("preferences.backup.delete")}
        </Button>
      </div>
    </li>
  );
}
