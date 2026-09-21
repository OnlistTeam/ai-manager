import { z } from "zod";
import { toolIdSchema } from "./tool";

export const healthProviderTargetSchema = z
  .object({
    providerId: z.string(),
    name: z.string(),
  })
  .strict();

export const providerHealthSchema = z
  .object({
    tool: toolIdSchema,
    configured: z.boolean(),
    configuredCount: z.number().int().nonnegative(),
    checkTargets: z.array(healthProviderTargetSchema),
  })
  .strict()
  .refine((health) => health.configured === health.configuredCount > 0, {
    message: "configured must agree with configuredCount",
  });

export const configReadStatusSchema = z.enum([
  "readable",
  "missing",
  "unreadable",
]);

export const configHealthSchema = z
  .object({
    tool: toolIdSchema,
    status: configReadStatusSchema,
  })
  .strict();

export const mcpHealthSchema = z
  .object({
    total: z.number().int().nonnegative(),
    enabled: z.number().int().nonnegative(),
  })
  .strict()
  .refine((health) => health.enabled <= health.total, {
    message: "enabled MCP count cannot exceed total",
  });

export const healthSnapshotSchema = z
  .object({
    providers: z.array(providerHealthSchema),
    configs: z.array(configHealthSchema),
    mcp: mcpHealthSchema,
  })
  .strict();

export type HealthProviderTarget = z.infer<typeof healthProviderTargetSchema>;
export type ProviderHealth = z.infer<typeof providerHealthSchema>;
export type ConfigReadStatus = z.infer<typeof configReadStatusSchema>;
export type ConfigHealth = z.infer<typeof configHealthSchema>;
export type McpHealth = z.infer<typeof mcpHealthSchema>;
export type HealthSnapshot = z.infer<typeof healthSnapshotSchema>;
