import { z } from "zod";

export const MAX_OPENCLAW_WORKSPACE_CONTENT_BYTES = 1024 * 1024;
export const MAX_OPENCLAW_WORKSPACE_SEARCH_CHARS = 200;

export const openClawWorkspaceFileIdSchema = z.enum([
  "agents",
  "soul",
  "user",
  "identity",
  "tools",
  "memory",
  "heartbeat",
  "bootstrap",
  "boot",
]);

export type OpenClawWorkspaceFileId = z.infer<
  typeof openClawWorkspaceFileIdSchema
>;

export const openClawWorkspaceFileStatusSchema = z.enum([
  "missing",
  "ready",
  "unavailable",
]);

export const openClawWorkspaceFileSummarySchema = z
  .object({
    id: openClawWorkspaceFileIdSchema,
    filename: z.string().min(1).max(32),
    status: openClawWorkspaceFileStatusSchema,
    sizeBytes: z.number().int().nonnegative(),
    modifiedAt: z.number().int().nonnegative().nullable(),
  })
  .strict();

export const openClawWorkspaceOverviewSchema = z
  .object({
    files: z.array(openClawWorkspaceFileSummarySchema).max(9),
    existingFiles: z.number().int().nonnegative().max(9),
    dailyMemoryCount: z.number().int().nonnegative(),
    dailyMemoryBytes: z.number().int().nonnegative(),
    totalBytes: z.number().int().nonnegative(),
    limited: z.boolean(),
  })
  .strict();

export const openClawWorkspaceDocumentSchema = z
  .object({
    id: openClawWorkspaceFileIdSchema,
    filename: z.string().min(1).max(32),
    exists: z.boolean(),
    content: z.string(),
    sizeBytes: z.number().int().nonnegative(),
    modifiedAt: z.number().int().nonnegative().nullable(),
  })
  .strict();

export const openClawDailyMemorySummarySchema = z
  .object({
    date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/u),
    sizeBytes: z.number().int().nonnegative(),
    modifiedAt: z.number().int().nonnegative().nullable(),
    preview: z.string().max(220),
    matchCount: z.number().int().nonnegative(),
  })
  .strict();

export const openClawDailyMemoryListSchema = z
  .object({
    items: z.array(openClawDailyMemorySummarySchema).max(200),
    totalCount: z.number().int().nonnegative(),
    totalBytes: z.number().int().nonnegative(),
    limited: z.boolean(),
  })
  .strict();

export const openClawDailyMemoryDocumentSchema = z
  .object({
    date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/u),
    exists: z.boolean(),
    content: z.string(),
    sizeBytes: z.number().int().nonnegative(),
    modifiedAt: z.number().int().nonnegative().nullable(),
  })
  .strict();

export const openClawWorkspaceWriteOutcomeSchema = z
  .object({ backupCreated: z.boolean() })
  .strict();

export const openClawWorkspaceDirectorySchema = z.enum([
  "workspace",
  "daily-memory",
]);

export type OpenClawWorkspaceFileStatus = z.infer<
  typeof openClawWorkspaceFileStatusSchema
>;
export type OpenClawWorkspaceFileSummary = z.infer<
  typeof openClawWorkspaceFileSummarySchema
>;
export type OpenClawWorkspaceOverview = z.infer<
  typeof openClawWorkspaceOverviewSchema
>;
export type OpenClawWorkspaceDocument = z.infer<
  typeof openClawWorkspaceDocumentSchema
>;
export type OpenClawDailyMemorySummary = z.infer<
  typeof openClawDailyMemorySummarySchema
>;
export type OpenClawDailyMemoryList = z.infer<
  typeof openClawDailyMemoryListSchema
>;
export type OpenClawDailyMemoryDocument = z.infer<
  typeof openClawDailyMemoryDocumentSchema
>;
export type OpenClawWorkspaceWriteOutcome = z.infer<
  typeof openClawWorkspaceWriteOutcomeSchema
>;
export type OpenClawWorkspaceDirectory = z.infer<
  typeof openClawWorkspaceDirectorySchema
>;
