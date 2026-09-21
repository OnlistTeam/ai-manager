import { z } from "zod";

/**
 * Model-probe wire types (ADR-0041).
 *
 * This is the one provider path whose results carry text the user reads: a
 * model's reply, or the upstream service's own refusal. Everything else about
 * the probe — the address, the key — stays in the backend.
 */

/** Mirrors `MAX_PROBE_PROMPT_CHARS` in the Rust domain. */
export const MAX_PROBE_PROMPT_CHARS = 200;
/** Mirrors `MAX_PROBE_MODELS` in the Rust domain. */
export const MAX_PROBE_MODELS = 1_000;

export const providerWireProtocolSchema = z.enum([
  "openAi",
  "anthropic",
  "gemini",
]);

export const probeModelKindSchema = z.enum(["text", "image"]);

export const probeModelSchema = z
  .object({
    id: z.string(),
    kind: probeModelKindSchema,
  })
  .strict();

export const modelCatalogRejectionSchema = z
  .object({
    status: z.number().int(),
    detail: z.string(),
  })
  .strict();

export const modelCatalogSchema = z
  .object({
    protocol: providerWireProtocolSchema,
    models: z.array(probeModelSchema).max(MAX_PROBE_MODELS),
    truncated: z.boolean(),
    rejection: modelCatalogRejectionSchema.nullable(),
  })
  .strict()
  .refine(
    (catalog) => catalog.rejection === null || catalog.models.length === 0,
    { message: "A refused catalogue cannot also list models" },
  );

/**
 * A reply is a discriminated union so the renderer branches on `kind` rather
 * than on the presence of a field.
 */
export const modelProbeReplySchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("text"), text: z.string() }).strict(),
  z
    .object({
      kind: z.literal("image"),
      mime: z.string(),
      base64: z.string(),
    })
    .strict(),
  z.object({ kind: z.literal("empty") }).strict(),
  z.object({ kind: z.literal("rejected"), detail: z.string() }).strict(),
  z.object({ kind: z.literal("imageLinkOnly") }).strict(),
]);

export const modelProbeOutcomeSchema = z
  .object({
    model: z.string(),
    latencyMs: z.number().int(),
    httpStatus: z.number().int().nullable(),
    reply: modelProbeReplySchema,
  })
  .strict();

/** Input only. Mirrors the backend's own validation so a bad request never leaves the renderer. */
export const modelProbeRequestSchema = z
  .object({
    model: z.string().trim().min(1).max(256),
    kind: probeModelKindSchema,
    prompt: z.string().trim().min(1).max(MAX_PROBE_PROMPT_CHARS),
  })
  .strict();

export type ProviderWireProtocol = z.infer<typeof providerWireProtocolSchema>;
export type ProbeModelKind = z.infer<typeof probeModelKindSchema>;
export type ProbeModel = z.infer<typeof probeModelSchema>;
export type ModelCatalogRejection = z.infer<typeof modelCatalogRejectionSchema>;
export type ModelCatalog = z.infer<typeof modelCatalogSchema>;
export type ModelProbeReply = z.infer<typeof modelProbeReplySchema>;
export type ModelProbeOutcome = z.infer<typeof modelProbeOutcomeSchema>;
export type ModelProbeRequest = z.infer<typeof modelProbeRequestSchema>;
