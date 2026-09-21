import { z } from "zod";

const localConnectionSchema = z
  .object({
    transport: z.literal("stdio"),
    command: z.string().min(1).max(512),
    arguments: z.array(z.string().max(2_048)).max(64),
  })
  .strict();

const remoteConnectionSchema = z.discriminatedUnion("transport", [
  z
    .object({ transport: z.literal("http"), url: z.string().max(2_048) })
    .strict(),
  z
    .object({ transport: z.literal("sse"), url: z.string().max(2_048) })
    .strict(),
]);

export const mcpConnectionDraftSchema = z.union([
  localConnectionSchema,
  remoteConnectionSchema,
]);

/**
 * Inbound-only product draft. It intentionally has no JSON, environment,
 * header, token, or stable-id field; those are rejected before native IPC.
 */
export const mcpInstallDraftSchema = z
  .object({
    name: z.string().min(1).max(80),
    description: z.string().max(240).nullable(),
    connection: mcpConnectionDraftSchema,
  })
  .strict();

export type McpConnectionDraft = z.infer<typeof mcpConnectionDraftSchema>;
export type McpInstallDraft = z.infer<typeof mcpInstallDraftSchema>;
