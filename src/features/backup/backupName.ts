import type { BackupFile } from "@/entities/backup";

const AUTOMATIC_BACKUP_NAME = /^db_backup_\d{8}_\d{6}(?:_\d+)?\.db$/;

export type BackupNameIssue =
  | "required"
  | "tooLong"
  | "invalid"
  | "exists"
  | "unchanged";

/** The label portion accepted by upstream `Database::rename_backup`. */
export function normalizeBackupName(input: string): string {
  const trimmed = input.trim();
  return trimmed.endsWith(".db") ? trimmed.slice(0, -3) : trimmed;
}

export function backupFilenameForName(input: string): string {
  return `${normalizeBackupName(input)}.db`;
}

/** Automatic restore points keep their friendly timestamp; renamed ones use the chosen label. */
export function customBackupName(file: BackupFile): string | null {
  if (AUTOMATIC_BACKUP_NAME.test(file.name)) return null;
  const name = normalizeBackupName(file.name);
  return name.length > 0 ? name : null;
}

export function validateBackupName(
  input: string,
  source: BackupFile,
  files: BackupFile[],
): BackupNameIssue | null {
  const name = normalizeBackupName(input);
  if (name.length === 0) return "required";
  if (new TextEncoder().encode(name).byteLength > 100) return "tooLong";
  if (
    name.includes("..") ||
    name.includes("/") ||
    name.includes("\\") ||
    name.includes("\0")
  ) {
    return "invalid";
  }

  const target = `${name}.db`;
  if (target === source.name) return "unchanged";
  if (files.some((file) => file.name === target && file.name !== source.name)) {
    return "exists";
  }
  return null;
}
