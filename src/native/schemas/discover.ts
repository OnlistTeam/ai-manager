import { z } from "zod";
import { nativeErrorPayloadSchema } from "./error";
import { extensionScopeSchema } from "./extension";

/**
 * Twins of Rust `domain::discover` (ADR-0063). A server carries no command,
 * URL, header or package: the renderer names it by id and native builds the
 * connection.
 */
export const discoverTransportSchema = z.enum(["stdio", "http", "sse"]);
export const discoverRunnerSchema = z.enum(["npx", "uvx", "docker"]);
export const discoverInputTargetSchema = z.enum(["env", "header", "argument"]);
export const discoverInputKindSchema = z.enum([
  "apiKey",
  "accessToken",
  "folder",
]);
export const discoverLinkSchema = z.enum(["homepage", "page", "repository"]);

export const discoverInputSchema = z.object({
  key: z.string().min(1),
  target: discoverInputTargetSchema,
  label: z.string(),
  kind: discoverInputKindSchema.nullable(),
  description: z.string().nullable(),
  site: z.string().nullable(),
  placeholder: z.string().nullable(),
  secret: z.boolean(),
  required: z.boolean(),
});

export const discoverMcpServerSchema = z.object({
  id: z.string().min(1),
  name: z.string(),
  title: z.string(),
  description: z.string().nullable(),
  publisher: z.string().nullable(),
  icon: z.string().nullable(),
  homepage: z.boolean(),
  transport: discoverTransportSchema,
  runs: discoverRunnerSchema.nullable(),
  signIn: z.boolean(),
  featured: z.boolean(),
  inputs: z.array(discoverInputSchema),
  added: z.string().nullable(),
});

export const discoverMcpReachSchema = z.object({
  scope: extensionScopeSchema,
  transports: z.array(discoverTransportSchema),
});

export const discoverMcpListSchema = z.object({
  items: z.array(discoverMcpServerSchema),
  reach: z.array(discoverMcpReachSchema),
  sourceError: nativeErrorPayloadSchema.nullable(),
});

export const discoverSkillSchema = z.object({
  id: z.string().min(1),
  source: z.string(),
  skillId: z.string(),
  name: z.string(),
  installs: z.number().int().nonnegative(),
  official: z.boolean(),
  icon: z.string(),
  description: z.string().nullable(),
  added: z.string().nullable(),
});

export const discoverSkillListSchema = z.object({
  items: z.array(discoverSkillSchema),
  sourceError: nativeErrorPayloadSchema.nullable(),
});

export const discoverDescriptionsSchema = z.record(z.string(), z.string());

/** A value typed into a server's dialog; usually a secret. */
export const discoverInputValueSchema = z
  .object({
    key: z.string().min(1).max(128),
    value: z.string().max(8_192),
  })
  .strict();

export type DiscoverTransport = z.infer<typeof discoverTransportSchema>;
export type DiscoverRunner = z.infer<typeof discoverRunnerSchema>;
export type DiscoverInput = z.infer<typeof discoverInputSchema>;
export type DiscoverInputKind = z.infer<typeof discoverInputKindSchema>;
export type DiscoverLink = z.infer<typeof discoverLinkSchema>;
export type DiscoverMcpServer = z.infer<typeof discoverMcpServerSchema>;
export type DiscoverMcpReach = z.infer<typeof discoverMcpReachSchema>;
export type DiscoverMcpList = z.infer<typeof discoverMcpListSchema>;
export type DiscoverSkill = z.infer<typeof discoverSkillSchema>;
export type DiscoverSkillList = z.infer<typeof discoverSkillListSchema>;
export type DiscoverInputValue = z.infer<typeof discoverInputValueSchema>;
