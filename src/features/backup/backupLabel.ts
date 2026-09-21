import type { BackupFile } from "@/entities/backup";
import { customBackupName } from "./backupName";

/**
 * Human phrasing like "Aug 19, 2026, 10:15 AM". Spec §99: users don't need to
 * recognize `db_backup_20260819_101500.db`; that name has no place in the
 * primary information hierarchy.
 *
 * The backend's `createdAt` comes from the file timestamp and is an empty
 * string when unavailable, so if parsing fails we fall back to the file name
 * — at least that's still an identifier that lines up with other rows in the UI.
 */
export function backupLabel(file: BackupFile, locale: string): string {
  const customName = customBackupName(file);
  if (customName) return customName;
  const at = new Date(file.createdAt);
  if (Number.isNaN(at.getTime())) {
    return file.name;
  }
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(at);
}

/**
 * "~2.1" — the unit and the "~" are the copy's responsibility. Byte-level
 * precision is meaningless to a non-technical user, so exact values never
 * reach the first screen of the backup list. Anything under 0.1 always
 * shows as 0.1: "0.0 MB" would look like the backup failed.
 */
export function backupSizeMb(file: BackupFile): string {
  const mb = file.sizeBytes / (1024 * 1024);
  return Math.max(mb, 0.1).toFixed(1);
}
