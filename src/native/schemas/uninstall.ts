import { z } from "zod";

/**
 * Spec §32: uninstall has three parts, and by default only the app itself is
 * removed. This is **input** validation -- both flags must be explicitly true
 * before any user data is deleted.
 */
export const uninstallOptionsSchema = z.object({
  removeSettings: z.boolean(),
  removeCache: z.boolean(),
});

export type UninstallOptions = z.infer<typeof uninstallOptionsSchema>;

export const UNINSTALL_APP_ONLY: UninstallOptions = {
  removeSettings: false,
  removeCache: false,
};
