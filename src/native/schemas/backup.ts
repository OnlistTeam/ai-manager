import { z } from "zod";

/** Mirrors Rust `domain::BackupFile` field for field. */
export const backupFileSchema = z.object({
  name: z.string(),
  /** RFC 3339. The backend takes it from the file's timestamp; when that's unavailable it's an empty string -- consumers must have a fallback. */
  createdAt: z.string(),
  sizeBytes: z.number(),
});

/** Deliberately no backup folder path here: paths never cross IPC, the UI just says "saved on this computer". */
export const backupListSchema = z.object({
  files: z.array(backupFileSchema),
});

export const restoreOutcomeSchema = z.object({
  backups: backupListSchema,
  toolsOutOfSync: z.boolean(),
});

export const backupExportOutcomeSchema = z.discriminatedUnion("status", [
  z.object({ status: z.literal("cancelled") }).strict(),
  z.object({ status: z.literal("exported") }).strict(),
]);

export const backupImportOutcomeSchema = z.discriminatedUnion("status", [
  z.object({ status: z.literal("cancelled") }).strict(),
  z
    .object({
      status: z.literal("imported"),
      backups: backupListSchema,
      toolsOutOfSync: z.boolean(),
    })
    .strict(),
]);

/** Daily on/off plus how many restore points stay; upstream hours never cross IPC. */
export const backupScheduleSchema = z
  .object({
    automatic: z.boolean(),
    retainCount: z.number().int().min(1),
  })
  .strict();

export type BackupFile = z.infer<typeof backupFileSchema>;
export type BackupSchedule = z.infer<typeof backupScheduleSchema>;
export type BackupList = z.infer<typeof backupListSchema>;
export type RestoreOutcome = z.infer<typeof restoreOutcomeSchema>;
export type BackupExportOutcome = z.infer<typeof backupExportOutcomeSchema>;
export type BackupImportOutcome = z.infer<typeof backupImportOutcomeSchema>;
