import { z } from "zod";
import { invokeNative } from "./client";

export const appUpdatePhaseSchema = z.enum([
  "unconfigured",
  "idle",
  "checking",
  "downloading",
  "ready",
  "upToDate",
  "failed",
]);

export const updateStatusSchema = z
  .object({
    currentVersion: z.string().min(1).max(128),
    availableVersion: z.string().min(1).max(128).nullable(),
    channelReady: z.boolean(),
    phase: appUpdatePhaseSchema,
    downloadedBytes: z.number().int().nonnegative(),
    totalBytes: z.number().int().nonnegative().nullable(),
    attempt: z.number().int().min(0).max(3),
    maxAttempts: z.number().int().min(0).max(3),
  })
  .strict()
  .superRefine((status, context) => {
    if (status.attempt > status.maxAttempts) {
      context.addIssue({
        code: "custom",
        path: ["attempt"],
        message: "attempt cannot exceed maxAttempts",
      });
    }
    if (status.channelReady && status.maxAttempts === 0) {
      context.addIssue({
        code: "custom",
        path: ["maxAttempts"],
        message: "a ready channel needs a retry budget",
      });
    }
    if (
      !status.channelReady &&
      (status.attempt !== 0 || status.maxAttempts !== 0)
    ) {
      context.addIssue({
        code: "custom",
        path: ["attempt"],
        message: "an unconfigured channel cannot report update attempts",
      });
    }
  });

export type AppUpdatePhase = z.infer<typeof appUpdatePhaseSchema>;
export type UpdateStatus = z.infer<typeof updateStatusSchema>;

/** Starts the signed background check/download or returns the shared state. */
export function startAppUpdate(force = false): Promise<UpdateStatus> {
  return invokeNative("app_update_start", updateStatusSchema, { force });
}

export function getAppUpdateStatus(): Promise<UpdateStatus> {
  return invokeNative("app_update_status", updateStatusSchema);
}

export function installAppUpdateAndRestart(): Promise<boolean> {
  return invokeNative("app_update_install_and_restart", z.boolean());
}

export function openAppDownloadPage(): Promise<boolean> {
  return invokeNative("app_update_open_download_page", z.boolean());
}

/** Backward-compatible product entry point used by the shared update query. */
export function checkForUpdate(): Promise<UpdateStatus> {
  return startAppUpdate(false);
}
