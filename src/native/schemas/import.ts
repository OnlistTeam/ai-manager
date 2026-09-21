import { z } from "zod";

/** Mirrors Rust `domain::ImportSummary` field for field. */
export const importSummarySchema = z
  .object({
    services: z.number().int().nonnegative(),
    mcpServers: z.number().int().nonnegative(),
    skills: z.number().int().nonnegative(),
  })
  .strict();

export const importPreviewSchema = z
  .object({
    available: z.boolean(),
    summary: importSummarySchema,
  })
  .strict();

export const importOutcomeSchema = z
  .object({
    imported: importSummarySchema,
  })
  .strict();

export type ImportSummary = z.infer<typeof importSummarySchema>;
export type ImportPreview = z.infer<typeof importPreviewSchema>;
export type ImportOutcome = z.infer<typeof importOutcomeSchema>;
