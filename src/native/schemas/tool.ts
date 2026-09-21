import { z } from "zod";

import { desktopAppIdSchema, toolIdSchema } from "./ids";

export { toolIdSchema };

export const toolStatusSchema = z.enum([
  "notInstalled",
  "installed",
  "updateAvailable",
  "broken",
  "unknown",
]);

export const toolCapabilitiesSchema = z.object({
  canInstall: z.boolean(),
  canUpdate: z.boolean(),
  canUninstall: z.boolean(),
  canRepair: z.boolean(),
  canLaunch: z.boolean(),
  canManageProvider: z.boolean(),
  canManageMcp: z.boolean(),
  canManageSkills: z.boolean(),
  canManagePrompts: z.boolean(),
  canManageVersion: z.boolean(),
});

export const toolAccessRequirementSchema = z.enum([
  "vendorOrProvider",
  "provider",
]);

export const toolUseCaseSchema = z.enum([
  "officialCoding",
  "modelChoice",
  "personalAutomation",
]);

export const toolDiscoverySchema = z
  .object({
    publisher: z.string().trim().min(1).max(80),
    access: toolAccessRequirementSchema,
    useCases: z.array(toolUseCaseSchema).min(1).max(3),
  })
  .strict();

export const toolSchema = z.object({
  id: toolIdSchema,
  name: z.string(),
  descriptionKey: z.string(),
  // New native builds always supply this. Optional parsing keeps a rolling
  // frontend/native upgrade from hiding the whole tool inventory.
  discovery: toolDiscoverySchema.optional(),
  status: toolStatusSchema,
  version: z.string().nullable(),
  latestVersion: z.string().nullable(),
  capabilities: toolCapabilitiesSchema,
  sessionsInsideSettings: z.boolean(),
  environment: z.string().nullable(),
  // The desktop application that owns this tool's configuration directory, if
  // one is installed. A tool can have a live configuration without its command
  // line: the Codex application uses the same directory as the Codex CLI.
  // Optional for the same rolling-upgrade reason as `discovery`.
  configurationSharedWith: desktopAppIdSchema.nullish(),
});

export const toolListSchema = z.array(toolSchema);

export const toolLaunchOutcomeSchema = z.enum(["launched", "cancelled"]);
export const toolLaunchDirectoryModeSchema = z.enum(["default", "choose"]);

export const toolInstallSourceSchema = z.enum([
  "notInstalled",
  "npm",
  "pnpm",
  "bun",
  "volta",
  "uv",
  "pipx",
  "brew",
  "nativeInstaller",
  "unmanaged",
]);

export const toolUpdateMethodSchema = z.enum([
  "nativeSelfUpdate",
  "toolSelfUpdate",
  "officialInstaller",
  "homebrew",
  "npm",
  "pnpm",
  "bun",
  "volta",
  "uv",
  "pipx",
]);

export const toolUpdateBlockReasonSchema = z.enum([
  "notInstalled",
  "ambiguousInstallation",
  "unsupportedInstallation",
  "inspectionFailed",
]);

export const updatePreviewFingerprintSchema = z
  .string()
  .regex(/^[0-9a-f]{64}$/);

export const toolUpdateInstallationSchema = z
  .object({
    source: toolInstallSourceSchema,
    version: z.string().min(1).max(64).nullable(),
    runnable: z.boolean(),
    isDefault: z.boolean(),
    location: z.string().min(1).max(4096),
  })
  .strict();

export const toolUpdateAttemptPreviewSchema = z
  .object({
    method: toolUpdateMethodSchema,
    commands: z.array(z.string().min(1).max(4096)).min(1).max(8),
  })
  .strict();

export const toolUpdateReadyPreviewSchema = z
  .object({
    tool: toolIdSchema,
    previewFingerprint: updatePreviewFingerprintSchema,
    targetVersion: z.string().min(1).max(64),
    source: toolInstallSourceSchema.exclude(["notInstalled"]),
    installations: z.array(toolUpdateInstallationSchema).min(1).max(16),
    attempts: z.array(toolUpdateAttemptPreviewSchema).min(1).max(8),
    multipleInstallations: z.boolean(),
  })
  .strict();

export const toolUpdatePreviewSchema = z.discriminatedUnion("state", [
  z
    .object({
      state: z.literal("ready"),
      preview: toolUpdateReadyPreviewSchema,
    })
    .strict(),
  z
    .object({
      state: z.literal("blocked"),
      tool: toolIdSchema,
      reason: toolUpdateBlockReasonSchema,
    })
    .strict(),
]);

export const toolUpdatePreviewListSchema = z
  .array(toolUpdatePreviewSchema)
  .min(1)
  .max(toolIdSchema.options.length);

export const toolVersionRestrictionSchema = z.enum([
  "nativeInstaller",
  "brew",
  "unmanaged",
]);

export const toolVersionTagSchema = z
  .object({
    tag: z
      .string()
      .min(1)
      .max(32)
      .regex(/^[A-Za-z0-9._-]+$/),
    version: z.string().min(1).max(64),
  })
  .strict();

export const toolVersionCatalogSchema = z
  .object({
    tool: toolIdSchema,
    source: toolInstallSourceSchema,
    canChangeVersion: z.boolean(),
    restriction: toolVersionRestrictionSchema.nullable(),
    latestVersion: z.string().max(64).nullable(),
    distTags: z.array(toolVersionTagSchema).max(32).default([]),
    versions: z.array(z.string().min(1).max(64)).max(200),
    mirrorUsed: z.boolean(),
  })
  .strict();

export const toolUninstallTargetKindSchema = z.enum([
  "command",
  "directory",
  "file",
  "symbolicLink",
  "missingPath",
]);

export const toolUninstallTargetSchema = z
  .object({
    kind: toolUninstallTargetKindSchema,
    value: z.string().min(1).max(4096),
    canRemoveAutomatically: z.boolean(),
  })
  .strict();

export const toolUninstallPreviewSchema = z
  .object({
    tool: toolIdSchema,
    app: z.array(toolUninstallTargetSchema).min(1).max(8),
    settings: z.array(toolUninstallTargetSchema).max(8),
    cache: z.array(toolUninstallTargetSchema).max(8),
  })
  .strict();

export type ToolId = z.infer<typeof toolIdSchema>;
export type ToolStatus = z.infer<typeof toolStatusSchema>;
export type ToolCapabilities = z.infer<typeof toolCapabilitiesSchema>;
export type ToolAccessRequirement = z.infer<typeof toolAccessRequirementSchema>;
export type ToolUseCase = z.infer<typeof toolUseCaseSchema>;
export type ToolDiscovery = z.infer<typeof toolDiscoverySchema>;
export type Tool = z.infer<typeof toolSchema>;
export type ToolLaunchOutcome = z.infer<typeof toolLaunchOutcomeSchema>;
export type ToolLaunchDirectoryMode = z.infer<
  typeof toolLaunchDirectoryModeSchema
>;
export type ToolInstallSource = z.infer<typeof toolInstallSourceSchema>;
export type ToolUpdateMethod = z.infer<typeof toolUpdateMethodSchema>;
export type ToolUpdateBlockReason = z.infer<typeof toolUpdateBlockReasonSchema>;
export type ToolUpdateInstallation = z.infer<
  typeof toolUpdateInstallationSchema
>;
export type ToolUpdateAttemptPreview = z.infer<
  typeof toolUpdateAttemptPreviewSchema
>;
export type ToolUpdateReadyPreview = z.infer<
  typeof toolUpdateReadyPreviewSchema
>;
export type ToolUpdatePreview = z.infer<typeof toolUpdatePreviewSchema>;
export type ToolVersionRestriction = z.infer<
  typeof toolVersionRestrictionSchema
>;
export type ToolVersionTag = z.infer<typeof toolVersionTagSchema>;
export type ToolVersionCatalog = z.infer<typeof toolVersionCatalogSchema>;
export type ToolUninstallTargetKind = z.infer<
  typeof toolUninstallTargetKindSchema
>;
export type ToolUninstallTarget = z.infer<typeof toolUninstallTargetSchema>;
export type ToolUninstallPreview = z.infer<typeof toolUninstallPreviewSchema>;
