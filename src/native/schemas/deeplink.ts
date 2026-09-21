import { z } from "zod";
import { operationIdSchema } from "./operation";
import { toolIdSchema } from "./tool";

export const deepLinkOriginSchema = z.enum(["argv", "paste"]);
export const deepLinkResourceSchema = z.enum([
  "provider",
  "mcp",
  "prompt",
  "skill",
]);
/** Which field of the link holds a credential. The value itself never crosses IPC. */
export const deepLinkCredentialFieldSchema = z.enum(["apiKey", "config"]);
export const deepLinkBlockReasonSchema = z.enum([
  "noSupportedTool",
  "credentialRequired",
]);

/** `tool` is null when the link names an application this product does not manage. */
export const deepLinkTargetSchema = z
  .object({
    tool: toolIdSchema.nullable(),
    name: z.string().nullable(),
    supported: z.boolean(),
  })
  .strict();

export const deepLinkPreviewSchema = z
  .object({
    id: z.string().min(1),
    origin: deepLinkOriginSchema,
    resource: deepLinkResourceSchema,
    name: z.string().nullable(),
    endpoint: z.string().nullable(),
    items: z.array(z.string()),
    targets: z.array(deepLinkTargetSchema),
    credentialFields: z.array(deepLinkCredentialFieldSchema),
    blocked: deepLinkBlockReasonSchema.nullable(),
    expiresAt: z.number(),
  })
  .strict();

export const deepLinkPreviewListSchema = z.array(deepLinkPreviewSchema);

export const deepLinkImportOutcomeSchema = z
  .object({
    resource: deepLinkResourceSchema,
    tools: z.array(toolIdSchema),
    operations: z.array(operationIdSchema),
    applied: z.number(),
  })
  .strict();

/** The native queue announces only that it changed; the previews are fetched. */
export const deepLinkPendingEventSchema = z
  .object({ pending: z.number() })
  .strict();

export type DeepLinkOrigin = z.infer<typeof deepLinkOriginSchema>;
export type DeepLinkResource = z.infer<typeof deepLinkResourceSchema>;
export type DeepLinkCredentialField = z.infer<
  typeof deepLinkCredentialFieldSchema
>;
export type DeepLinkBlockReason = z.infer<typeof deepLinkBlockReasonSchema>;
export type DeepLinkTarget = z.infer<typeof deepLinkTargetSchema>;
export type DeepLinkPreview = z.infer<typeof deepLinkPreviewSchema>;
export type DeepLinkImportOutcome = z.infer<typeof deepLinkImportOutcomeSchema>;
export type DeepLinkPendingEvent = z.infer<typeof deepLinkPendingEventSchema>;
