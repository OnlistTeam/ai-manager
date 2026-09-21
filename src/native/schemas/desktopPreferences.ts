import { z } from "zod";

export const desktopPreferencesSchema = z
  .object({
    launchOnStartup: z.boolean(),
    silentStartup: z.boolean(),
    showInTray: z.boolean(),
    minimizeToTrayOnClose: z.boolean(),
  })
  .strict()
  .superRefine((value, context) => {
    if (value.silentStartup && (!value.launchOnStartup || !value.showInTray)) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message: "silent startup requires auto-launch and a visible tray",
      });
    }
    if (value.minimizeToTrayOnClose && !value.showInTray) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message: "minimize-on-close requires a visible tray",
      });
    }
  });

export type DesktopPreferences = z.infer<typeof desktopPreferencesSchema>;
