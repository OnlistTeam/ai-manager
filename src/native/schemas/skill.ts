import { z } from "zod";
import { operationIdSchema } from "./operation";

export const skillSourceSchema = z.object({
  owner: z.string().min(1),
  repository: z.string().min(1),
  branch: z.string().min(1),
  directory: z.string().min(1),
});

/** Product catalog row: friendly metadata plus provenance, never Skill body. */
export const skillCatalogItemSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  description: z.string().nullable(),
  source: skillSourceSchema,
  installed: z.boolean(),
  mirrorUsed: z.boolean(),
});

export const skillCatalogSchema = z.array(skillCatalogItemSchema);

/** Known update projection: no hashes, repository path, or Skill content. */
export const skillUpdateSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
});

export const skillUpdateListSchema = z.array(skillUpdateSchema);

export const skillRepositorySchema = z.object({
  id: z.string().regex(/^[0-9a-f]{64}$/),
  owner: z.string().min(1).max(39),
  repository: z.string().min(1).max(100),
  branch: z.string().max(255),
  enabled: z.boolean(),
});

export const skillRepositoryListSchema = z.array(skillRepositorySchema);

export const skillRepositoryDraftSchema = z
  .object({
    owner: z.string().min(1).max(39),
    repository: z.string().min(1).max(100),
    branch: z.string().max(255),
    enabled: z.boolean(),
  })
  .strict();

export const skillBackupSchema = z
  .object({
    id: z.string().regex(/^[0-9a-f]{64}$/),
    name: z.string().min(1).max(120),
    description: z.string().max(500).nullable(),
    createdAt: z.number().int().positive().max(253_402_300_799),
    conflicts: z.boolean(),
  })
  .strict();

export const skillBackupListSchema = z.array(skillBackupSchema);

/** Native picker result. The selected path deliberately has no wire field. */
export const skillZipInstallOutcomeSchema = z.discriminatedUnion("status", [
  z.object({ status: z.literal("cancelled") }).strict(),
  z
    .object({
      status: z.literal("started"),
      operation: operationIdSchema,
    })
    .strict(),
]);

export type SkillSource = z.infer<typeof skillSourceSchema>;
export type SkillCatalogItem = z.infer<typeof skillCatalogItemSchema>;
export type SkillUpdate = z.infer<typeof skillUpdateSchema>;
export type SkillRepository = z.infer<typeof skillRepositorySchema>;
export type SkillRepositoryDraft = z.infer<typeof skillRepositoryDraftSchema>;
export type SkillBackup = z.infer<typeof skillBackupSchema>;
export type SkillZipInstallOutcome = z.infer<
  typeof skillZipInstallOutcomeSchema
>;
