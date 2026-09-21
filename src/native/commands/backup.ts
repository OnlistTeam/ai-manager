import { invokeNative } from "../client";
import {
  backupExportOutcomeSchema,
  backupImportOutcomeSchema,
  backupListSchema,
  backupScheduleSchema,
  restoreOutcomeSchema,
  type BackupExportOutcome,
  type BackupImportOutcome,
  type BackupList,
  type BackupSchedule,
  type RestoreOutcome,
} from "../schemas/backup";

/**
 * All four write operations return the refreshed full list: creating a backup lets the
 * retention policy quietly prune the oldest ones, and the frontend has no way to guess
 * which ones survive (the same trick as Phase 5's enable/disable returning the full
 * extension list).
 */
export const backup = {
  list(): Promise<BackupList> {
    return invokeNative("app_backups_list", backupListSchema);
  },

  create(): Promise<BackupList> {
    return invokeNative("app_backup_create", backupListSchema);
  },

  restore(name: string): Promise<RestoreOutcome> {
    return invokeNative("app_backup_restore", restoreOutcomeSchema, {
      backup: name,
    });
  },

  /** Named `remove`, not `delete`: the latter is a reserved word, easy to misread at call sites. */
  remove(name: string): Promise<BackupList> {
    return invokeNative("app_backup_delete", backupListSchema, {
      backup: name,
    });
  },

  rename(source: string, name: string): Promise<BackupList> {
    return invokeNative("app_backup_rename", backupListSchema, {
      backup: source,
      name,
    });
  },

  exportArchive(): Promise<BackupExportOutcome> {
    return invokeNative("app_backup_export", backupExportOutcomeSchema);
  },

  importArchive(): Promise<BackupImportOutcome> {
    return invokeNative("app_backup_import", backupImportOutcomeSchema);
  },
};

export const backupSchedule = {
  get(): Promise<BackupSchedule> {
    return invokeNative("app_backup_schedule_get", backupScheduleSchema);
  },

  /** Returns the read-back policy, not an echo of the request. */
  save(schedule: BackupSchedule): Promise<BackupSchedule> {
    return invokeNative("app_backup_schedule_save", backupScheduleSchema, {
      schedule,
    });
  },
};
