import { ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { BackupFile } from "@/entities/backup";
import { Badge } from "@/shared/ui/Badge";
import { Card } from "@/shared/ui/Card";
import { backupLabel } from "./backupLabel";
import { BackupRow } from "./BackupRow";

export interface BackupListCardProps {
  files: BackupFile[];
  busy: boolean;
  onRename: (file: BackupFile) => void;
  onRestore: (file: BackupFile) => void;
  onDelete: (file: BackupFile) => void;
}

function latestBackup(files: BackupFile[]): BackupFile {
  return files.reduce((latest, candidate) => {
    const latestAt = Date.parse(latest.createdAt);
    const candidateAt = Date.parse(candidate.createdAt);
    if (Number.isNaN(candidateAt)) return latest;
    if (Number.isNaN(latestAt) || candidateAt > latestAt) return candidate;
    return latest;
  });
}

export function BackupListCard({
  files,
  busy,
  onRename,
  onRestore,
  onDelete,
}: BackupListCardProps) {
  const { t, i18n } = useTranslation();
  const latest = latestBackup(files);

  return (
    <Card padding="none" className="overflow-hidden rounded-xl">
      <div className="flex flex-wrap items-center gap-3 border-b border-hairline bg-layer-1 p-4">
        <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-success/10 text-success">
          <ShieldCheck className="h-5 w-5" aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-body font-medium text-content">
            {t("preferences.backup.summary.title")}
          </p>
          <p className="mt-0.5 break-words text-caption text-content-muted">
            {t("preferences.backup.summary.latest", {
              name: backupLabel(latest, i18n.language),
            })}
          </p>
        </div>
        <Badge tone="success">
          {t("preferences.backup.summary.count", { count: files.length })}
        </Badge>
      </div>
      <ul className="px-4">
        {files.map((file) => (
          <BackupRow
            key={file.name}
            file={file}
            latest={file.name === latest.name}
            busy={busy}
            onRename={() => onRename(file)}
            onRestore={() => onRestore(file)}
            onDelete={() => onDelete(file)}
          />
        ))}
      </ul>
    </Card>
  );
}
