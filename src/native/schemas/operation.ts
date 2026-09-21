import { z } from "zod";
import { nativeErrorPayloadSchema } from "./error";
import { desktopAppIdSchema } from "./desktopApp";
import { extensionKindSchema } from "./extension";
import { providerTestResultSchema } from "./provider";
import { toolIdSchema, toolSchema } from "./tool";

export const operationKindSchema = z.enum([
  "install",
  "update",
  "uninstall",
  "repair",
  "changeVersion",
  "testProviders",
  "scan",
]);

export const operationStatusSchema = z.enum([
  "queued",
  "running",
  "success",
  "failed",
  "cancelled",
]);

export const operationExtensionSchema = z.object({
  kind: extensionKindSchema,
  id: z.string().min(1),
  name: z.string().min(1),
});

export const operationLogKindSchema = z.enum([
  "phase",
  "command",
  "stdout",
  "stderr",
  "system",
]);

export const operationLogEntrySchema = z
  .object({
    timestamp: z.number().int(),
    kind: operationLogKindSchema,
    messageKey: z.string().min(1).nullable(),
    detail: z.string().max(4_096).nullable(),
  })
  .refine(
    (entry) => (entry.messageKey === null) !== (entry.detail === null),
    "operation log entries must contain exactly one messageKey or detail",
  );

export const toolUpdateRecoverySchema = z.discriminatedUnion("kind", [
  z
    .object({
      kind: z.literal("available"),
      targetVersion: z
        .string()
        .min(1)
        .max(64)
        .regex(/^[0-9][A-Za-z0-9.+-]*$/),
    })
    .strict(),
  z.object({ kind: z.literal("historyUnavailable") }).strict(),
  z.object({ kind: z.literal("ownershipChanged") }).strict(),
  z.object({ kind: z.literal("ownerUnsupported") }).strict(),
  z.object({ kind: z.literal("inspectionFailed") }).strict(),
]);

export const operationOutputSchema = z.discriminatedUnion("kind", [
  z
    .object({
      kind: z.literal("providerTests"),
      results: z.array(providerTestResultSchema).max(512),
    })
    .strict(),
  // A finished lifecycle action publishes the detection it already verified,
  // so a card can show the real new state without waiting for a full re-scan
  // of every tool.
  z
    .object({
      kind: z.literal("toolInventory"),
      tool: toolSchema,
    })
    .strict(),
]);

export const operationSchema = z
  .object({
    id: z.string(),
    kind: operationKindSchema,
    tool: toolIdSchema.nullable(),
    desktopApp: desktopAppIdSchema.nullable().default(null),
    // `.default(null)` keeps older persisted/test snapshots readable while the
    // Rust wire format always emits a literal null for ordinary tool work.
    extension: operationExtensionSchema.nullable().default(null),
    status: operationStatusSchema,
    progress: z.number().int().min(0).max(100),
    messageKey: z.string().nullable(),
    // Older backends never advertised cancellation, so they fail closed.
    canCancel: z.boolean().default(false),
    // Operation logs are in-memory only. The default keeps older test snapshots
    // and an already-running older backend readable during a development reload.
    logs: z.array(operationLogEntrySchema).max(120).default([]),
    // Older in-memory snapshots from a development reload predate R2.4.
    updateRecovery: toolUpdateRecoverySchema.nullable().default(null),
    // Output first appeared with R2.8. Older development snapshots normalize to null.
    output: operationOutputSchema.nullable().default(null),
    error: nativeErrorPayloadSchema.nullable(),
    startedAt: z.number().int().nullable(),
    finishedAt: z.number().int().nullable(),
  })
  .superRefine((operation, context) => {
    const providerTask = operation.kind === "testProviders";
    const providerOutput = operation.output?.kind === "providerTests";
    if (
      providerTask !== providerOutput ||
      (providerTask &&
        (operation.tool === null ||
          operation.desktopApp !== null ||
          operation.extension !== null ||
          operation.canCancel))
    ) {
      context.addIssue({
        code: "custom",
        path: ["output"],
        message: "Provider test operation kind, target and output must agree",
      });
    }
    const targetCount =
      Number(operation.tool !== null) + Number(operation.desktopApp !== null);
    if (
      targetCount > 1 ||
      (operation.desktopApp !== null && operation.extension === null) ||
      (operation.extension !== null && targetCount !== 1)
    ) {
      context.addIssue({
        code: "custom",
        path: ["desktopApp"],
        message: "operation target and extension metadata must agree",
      });
    }
    if (
      operation.output?.kind === "toolInventory" &&
      (operation.status !== "success" ||
        operation.extension !== null ||
        operation.output.tool.id !== operation.tool)
    ) {
      context.addIssue({
        code: "custom",
        path: ["output", "tool"],
        message:
          "A verified detection belongs only to its own succeeded tool action",
      });
    }
    if (operation.output?.kind === "providerTests") {
      const ids = operation.output.results.map((result) => result.providerId);
      if (new Set(ids).size !== ids.length) {
        context.addIssue({
          code: "custom",
          path: ["output", "results"],
          message: "Provider test results must have unique provider ids",
        });
      }
    }
  });

export const operationListSchema = z.array(operationSchema);

/** On the Rust side, OperationId is a bare string via `#[serde(transparent)]`. */
export const operationIdSchema = z.string().min(1);

export type OperationKind = z.infer<typeof operationKindSchema>;
export type OperationStatus = z.infer<typeof operationStatusSchema>;
export type OperationExtension = z.infer<typeof operationExtensionSchema>;
export type OperationLogKind = z.infer<typeof operationLogKindSchema>;
export type OperationLogEntry = z.infer<typeof operationLogEntrySchema>;
export type ToolUpdateRecovery = z.infer<typeof toolUpdateRecoverySchema>;
export type OperationOutput = z.infer<typeof operationOutputSchema>;
export type Operation = z.infer<typeof operationSchema>;
