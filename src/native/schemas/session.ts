import { z } from "zod";
import { toolIdSchema } from "./tool";

export const sessionReferenceSchema = z.string().regex(/^[a-f0-9]{64}$/);
const timestampSchema = z.number().int().safe().nullable();

export const sessionSummarySchema = z
  .object({
    reference: sessionReferenceSchema,
    tool: toolIdSchema,
    title: z.string().max(160).nullable(),
    preview: z.string().max(280).nullable(),
    projectName: z.string().max(120).nullable(),
    createdAt: timestampSchema,
    lastActiveAt: timestampSchema,
    resumable: z.boolean(),
  })
  .strict();

export const sessionListSchema = z
  .object({
    items: z.array(sessionSummarySchema).max(500),
    totalCount: z.number().int().nonnegative().max(4_294_967_295),
    limited: z.boolean(),
  })
  .strict();

export const sessionMessageRoleSchema = z.enum([
  "user",
  "assistant",
  "system",
  "tool",
  "other",
]);

export const sessionMessageSchema = z
  .object({
    role: sessionMessageRoleSchema,
    content: z.string().max(65_536),
    timestamp: timestampSchema,
    truncated: z.boolean(),
  })
  .strict();

/** Only available after opening a session: the two facts used to self-diagnose when something goes wrong. */
export const sessionThreadSchema = z
  .object({
    reference: sessionReferenceSchema,
    messages: z.array(sessionMessageSchema).max(500),
    totalCount: z.number().int().nonnegative().max(4_294_967_295),
    limited: z.boolean(),
    workingDirectory: z.string().max(4096).nullable(),
    resumeCommand: z.string().max(4096).nullable(),
  })
  .strict();

export type SessionSummary = z.infer<typeof sessionSummarySchema>;
export type SessionList = z.infer<typeof sessionListSchema>;
export type SessionMessageRole = z.infer<typeof sessionMessageRoleSchema>;
export type SessionMessage = z.infer<typeof sessionMessageSchema>;
export type SessionThread = z.infer<typeof sessionThreadSchema>;
