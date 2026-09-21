import { z } from "zod";
import { toolIdSchema } from "./tool";

const safeCountSchema = z
  .number()
  .int()
  .nonnegative()
  .max(Number.MAX_SAFE_INTEGER);

export const routingProviderSchema = z
  .object({
    id: z.string().min(1).max(256),
    name: z.string().min(1).max(256),
    priority: z.number().int().positive().max(512).nullable(),
    current: z.boolean(),
    healthy: z.boolean(),
    consecutiveFailures: z.number().int().nonnegative().max(4_294_967_295),
  })
  .strict();

export const routingTargetSchema = z
  .object({
    tool: toolIdSchema,
    takeoverEnabled: z.boolean(),
    autoFailoverEnabled: z.boolean(),
    currentProvider: routingProviderSchema.nullable(),
    queue: z.array(routingProviderSchema).max(512),
    available: z.array(routingProviderSchema).max(512),
  })
  .strict();

export const routingOverviewSchema = z
  .object({
    running: z.boolean(),
    address: z.string().min(1).max(255).nullable(),
    port: z.number().int().positive().max(65_535).nullable(),
    activeConnections: safeCountSchema,
    totalRequests: safeCountSchema,
    successRequests: safeCountSchema,
    failedRequests: safeCountSchema,
    failoverCount: safeCountSchema,
    targets: z.array(routingTargetSchema).length(4),
  })
  .strict()
  .superRefine((overview, context) => {
    if (
      overview.running !== (overview.address !== null && overview.port !== null)
    ) {
      context.addIssue({
        code: "custom",
        message: "running state and endpoint must agree",
      });
    }
    if (new Set(overview.targets.map((target) => target.tool)).size !== 4) {
      context.addIssue({
        code: "custom",
        message: "routing targets must be unique",
      });
    }
  });

export type RoutingProvider = z.infer<typeof routingProviderSchema>;
export type RoutingTarget = z.infer<typeof routingTargetSchema>;
export type RoutingOverview = z.infer<typeof routingOverviewSchema>;
