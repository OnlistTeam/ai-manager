import { z } from "zod";
import { nativeErrorPayloadSchema } from "./error";
import { routingOverviewSchema } from "./routing";
import { toolIdSchema } from "./tool";

/** Mirrors `MAX_ROUTING_TRACE_ENTRIES` in `domain/routing_trace.rs`. */
export const MAX_ROUTING_TRACE_ENTRIES = 60;
const MAX_ROUTING_TRACE_ATTEMPTS = 16;

const countSchema = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);

export const routingErrorCategorySchema = z.enum([
  "rateLimited",
  "authFailed",
  "serverError",
  "timeout",
  "network",
  "rejected",
  "unavailable",
  "cancelled",
  "other",
]);

export const routingTraceAttemptSchema = z
  .object({
    providerId: z.string().min(1).max(256),
    providerName: z.string().min(1).max(256),
    outcome: z.enum(["ok", "failed", "skipped"]),
    httpStatus: z.number().int().min(100).max(599).nullable(),
    error: routingErrorCategorySchema.nullable(),
    ms: countSchema,
  })
  .strict();

export const routingTraceEntrySchema = z
  .object({
    seq: countSchema,
    revision: countSchema,
    startedAt: z.number().int(),
    tool: toolIdSchema,
    // 128 characters on the Rust side; UTF-16 may need twice the units.
    model: z.string().min(1).max(256).nullable(),
    attempts: z
      .array(routingTraceAttemptSchema)
      .max(MAX_ROUTING_TRACE_ATTEMPTS),
    status: z.enum(["pending", "ok", "failed"]),
    error: routingErrorCategorySchema.nullable(),
    totalMs: countSchema.nullable(),
    failedOver: z.boolean(),
  })
  .strict();

export const routingTraceCountsSchema = z
  .object({
    requests: countSchema,
    rerouted: countSchema,
    failed: countSchema,
  })
  .strict();

export const routingTraceSnapshotSchema = z
  .object({
    revision: countSchema,
    counts: routingTraceCountsSchema,
    entries: z.array(routingTraceEntrySchema).max(MAX_ROUTING_TRACE_ENTRIES),
  })
  .strict();

export const routingTraceUpdateSchema = z
  .object({
    revision: countSchema,
    counts: routingTraceCountsSchema,
    entry: routingTraceEntrySchema,
  })
  .strict();

export const routingLiveModeOutcomeSchema = z
  .object({
    overview: routingOverviewSchema,
    failures: z
      .array(
        z
          .object({ tool: toolIdSchema, error: nativeErrorPayloadSchema })
          .strict(),
      )
      .max(4),
  })
  .strict();

export type RoutingErrorCategory = z.infer<typeof routingErrorCategorySchema>;
export type RoutingTraceAttempt = z.infer<typeof routingTraceAttemptSchema>;
export type RoutingTraceEntry = z.infer<typeof routingTraceEntrySchema>;
export type RoutingTraceCounts = z.infer<typeof routingTraceCountsSchema>;
export type RoutingTraceSnapshot = z.infer<typeof routingTraceSnapshotSchema>;
export type RoutingTraceUpdate = z.infer<typeof routingTraceUpdateSchema>;
export type RoutingLiveModeOutcome = z.infer<
  typeof routingLiveModeOutcomeSchema
>;
