import { z } from "zod";
import { desktopAppIdSchema, toolIdSchema } from "./ids";

export { desktopAppIdSchema };

export const desktopAppStatusSchema = z.enum([
  "installed",
  "updateAvailable",
  "notInstalled",
  "unsupported",
  "unknown",
]);

export const desktopAppConfigurationRelationshipSchema = z.enum([
  "sharedConfiguration",
  "separateConfiguration",
  "standaloneApplication",
]);

export const desktopAppInstallerHandoffSchema = z.enum([
  "directOfficialPackage",
  "officialDownloadPage",
  "unsupported",
]);

export const desktopAppUninstallHandoffSchema = z.enum([
  "revealApplication",
  "systemSettings",
  "unsupported",
]);

const desktopVersionSchema = z
  .string()
  .min(1)
  .max(128)
  // eslint-disable-next-line no-control-regex -- control characters are the validation target
  .refine((value) => !/[\u0000-\u001f<>&]/.test(value));

export const desktopAppSchema = z
  .object({
    id: desktopAppIdSchema,
    name: z.string().min(1).max(64),
    status: desktopAppStatusSchema,
    version: desktopVersionSchema.nullable(),
    latestVersion: desktopVersionSchema.nullable().default(null),
    relatedTool: toolIdSchema.nullable(),
    configurationRelationship: desktopAppConfigurationRelationshipSchema,
    canLaunch: z.boolean(),
    environment: z.enum(["macos", "windows", "linux", "unknown"]),
    installerHandoff: desktopAppInstallerHandoffSchema,
    uninstallHandoff: desktopAppUninstallHandoffSchema,
    updatesManagedByVendor: z.boolean(),
    canRollback: z.boolean(),
    canManageMcp: z.boolean(),
  })
  .strict()
  .superRefine((app, context) => {
    const installed =
      app.status === "installed" || app.status === "updateAvailable";
    if (app.canLaunch && !installed) {
      context.addIssue({
        code: "custom",
        message: "only an installed desktop app can be launchable",
      });
    }
    if (
      app.canManageMcp &&
      (app.id !== "claude-desktop" ||
        !installed ||
        (app.environment !== "macos" && app.environment !== "windows"))
    ) {
      context.addIssue({
        code: "custom",
        path: ["canManageMcp"],
        message:
          "only installed Claude Desktop on macOS or Windows can manage MCP",
      });
    }
    const standalone =
      app.configurationRelationship === "standaloneApplication";
    if (standalone !== (app.relatedTool === null)) {
      context.addIssue({
        code: "custom",
        path: ["relatedTool"],
        message: "standalone apps must not claim a related CLI tool",
      });
    }
  });

export const desktopAppListSchema = z
  .array(desktopAppSchema)
  .max(desktopAppIdSchema.options.length)
  .refine(
    (apps) => new Set(apps.map((app) => app.id)).size === apps.length,
    "desktop app ids must be unique",
  );

export const desktopAppLaunchOutcomeSchema = z.literal("launched");
export const desktopAppOfficialDownloadOutcomeSchema = z
  .object({ handoff: desktopAppInstallerHandoffSchema })
  .strict();
export const desktopAppUninstallOutcomeSchema = z.literal("opened");

export type DesktopAppId = z.infer<typeof desktopAppIdSchema>;
export type DesktopAppStatus = z.infer<typeof desktopAppStatusSchema>;
export type DesktopAppConfigurationRelationship = z.infer<
  typeof desktopAppConfigurationRelationshipSchema
>;
export type DesktopAppInstallerHandoff = z.infer<
  typeof desktopAppInstallerHandoffSchema
>;
export type DesktopAppUninstallHandoff = z.infer<
  typeof desktopAppUninstallHandoffSchema
>;
export type DesktopApp = z.infer<typeof desktopAppSchema>;
export type DesktopAppLaunchOutcome = z.infer<
  typeof desktopAppLaunchOutcomeSchema
>;
export type DesktopAppOfficialDownloadOutcome = z.infer<
  typeof desktopAppOfficialDownloadOutcomeSchema
>;
export type DesktopAppUninstallOutcome = z.infer<
  typeof desktopAppUninstallOutcomeSchema
>;
