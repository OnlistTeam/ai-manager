import { z } from "zod";
import { toolIdSchema } from "./tool";

const safeCountSchema = z
  .number()
  .int()
  .nonnegative()
  .max(Number.MAX_SAFE_INTEGER);
const percentSchema = z.number().finite().min(0).max(100);
const usdSchema = z.string().regex(/^\d+\.\d{6}$/);
const dateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);

export const usageMetricsSchema = z
  .object({
    requests: safeCountSchema,
    estimatedCostUsd: usdSchema,
    tokens: safeCountSchema,
    successRatePercent: percentSchema,
    cacheHitRatePercent: percentSchema,
  })
  .strict();

export const usageToolBreakdownSchema = z
  .object({
    tool: toolIdSchema,
    metrics: usageMetricsSchema,
  })
  .strict();

export const usageDaySchema = z
  .object({
    date: dateSchema,
    requests: safeCountSchema,
    estimatedCostUsd: usdSchema,
    tokens: safeCountSchema,
  })
  .strict();

export const usageOverviewSchema = z
  .object({
    periodDays: z.literal(30),
    startDate: dateSchema,
    endDate: dateSchema,
    summary: usageMetricsSchema,
    byTool: z.array(usageToolBreakdownSchema).max(8),
    trend: z.array(usageDaySchema).max(31),
  })
  .strict();

export const usageSyncSummarySchema = z
  .object({
    filesScanned: z.number().int().nonnegative().max(4_294_967_295),
    recordsImported: z.number().int().nonnegative().max(4_294_967_295),
    recordsSkipped: z.number().int().nonnegative().max(4_294_967_295),
    sourceIssues: z.number().int().nonnegative().max(4_294_967_295),
  })
  .strict();

export const usageRefreshResultSchema = z
  .object({
    overview: usageOverviewSchema,
    sync: usageSyncSummarySchema,
  })
  .strict();

export type UsageMetrics = z.infer<typeof usageMetricsSchema>;
export type UsageToolBreakdown = z.infer<typeof usageToolBreakdownSchema>;
export type UsageDay = z.infer<typeof usageDaySchema>;
export type UsageOverview = z.infer<typeof usageOverviewSchema>;
export type UsageSyncSummary = z.infer<typeof usageSyncSummarySchema>;
export type UsageRefreshResult = z.infer<typeof usageRefreshResultSchema>;
