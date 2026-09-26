import { z } from "zod";

/**
 * One environment variable (local) or request header (remote). A list rather
 * than a record so a repeated name reaches native validation instead of being
 * collapsed. The value is often an API key.
 */
const mcpVariableSchema = z
  .object({
    name: z.string().min(1).max(128),
    value: z.string().max(8_192),
  })
  .strict();

const variablesSchema = z.array(mcpVariableSchema).max(64);

const localConnectionSchema = z
  .object({
    transport: z.literal("stdio"),
    command: z.string().min(1).max(512),
    arguments: z.array(z.string().max(2_048)).max(64),
    env: variablesSchema,
  })
  .strict();

const remoteConnectionSchema = z.discriminatedUnion("transport", [
  z
    .object({
      transport: z.literal("http"),
      url: z.string().max(2_048),
      headers: variablesSchema,
    })
    .strict(),
  z
    .object({
      transport: z.literal("sse"),
      url: z.string().max(2_048),
      headers: variablesSchema,
    })
    .strict(),
]);

export const mcpConnectionDraftSchema = z.union([
  localConnectionSchema,
  remoteConnectionSchema,
]);

/**
 * Inbound-only product draft (ADR-0047). It carries typed fields only: no
 * free-form JSON, no stable id. Pasted configuration is parsed in the renderer
 * into these fields; the JSON text itself never crosses IPC.
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
