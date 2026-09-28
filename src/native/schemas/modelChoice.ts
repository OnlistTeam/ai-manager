import { z } from "zod";
import { toolIdSchema } from "./ids";
import { effectiveConnectionSourceSchema } from "./provider";

/**
 * Model and thinking-effort wire types (ADR-0055). Mirrors Rust
 * `domain::ToolModelChoice`; a `null` model means this product's key is
 * absent and the tool decides.
 */

/** Mirrors `MAX_MODEL_NAME_CHARS` in the Rust domain. */
export const MAX_MODEL_NAME_CHARS = 256;

const settingValueSchema = z.string().min(1).max(MAX_MODEL_NAME_CHARS);

/** The effort a new session runs at, as the tool itself resolves it. */
export const effortInForceSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("toolDefault") }).strict(),
  z.object({ kind: z.literal("level"), level: settingValueSchema }).strict(),
  z.object({ kind: z.literal("fixed"), level: settingValueSchema }).strict(),
  z
    .object({
      kind: z.literal("terminal"),
      level: settingValueSchema,
      source: effectiveConnectionSourceSchema,
    })
    .strict(),
]);

/**
 * The suffix the tool reads on a model name as its context window, such as
 * Claude Code's `[1m]`, and the smallest window it stands for.
 */
export const contextMarkerSchema = z
  .object({
    suffix: z.string().min(1).max(16),
    minTokens: z.number().int().positive(),
  })
  .strict();

export const toolModelChoiceSchema = z
  .object({
    tool: toolIdSchema,
    model: settingValueSchema.nullable(),
    effort: effortInForceSchema,
    effortLevels: z.array(settingValueSchema).max(16),
    officialModels: z.array(settingValueSchema).max(32),
    contextMarker: contextMarkerSchema.nullable(),
  })
  .strict();

/**
 * A typed model name, checked the way the backend checks it so a name it
 * would refuse never leaves the renderer.
 */
export const modelNameSchema = z
  .string()
  .trim()
  .min(1)
  .max(MAX_MODEL_NAME_CHARS)
  .refine(
    // eslint-disable-next-line no-control-regex
    (value) => !/[\u0000-\u001f\u007f"\\]/.test(value),
    "Model name contains a control character or a quote",
  );

export type ContextMarker = z.infer<typeof contextMarkerSchema>;
export type EffortInForce = z.infer<typeof effortInForceSchema>;
export type ToolModelChoice = z.infer<typeof toolModelChoiceSchema>;
