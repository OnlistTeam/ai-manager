import { z } from "zod";
import { toolIdSchema } from "./ids";

/**
 * Model and thinking-effort wire types (ADR-0054). Mirrors Rust
 * `domain::ToolModelChoice`; a `null` model or effort means this product's
 * key is absent and the tool decides.
 */

/** Mirrors `MAX_MODEL_NAME_CHARS` in the Rust domain. */
export const MAX_MODEL_NAME_CHARS = 256;

const settingValueSchema = z.string().min(1).max(MAX_MODEL_NAME_CHARS);

export const effortOverrideSchema = z
  .object({
    model: settingValueSchema,
    effort: settingValueSchema,
  })
  .strict();

export const toolModelChoiceSchema = z
  .object({
    tool: toolIdSchema,
    model: settingValueSchema.nullable(),
    effort: settingValueSchema.nullable(),
    effortLevels: z.array(settingValueSchema).max(16),
    officialModels: z.array(settingValueSchema).max(32),
    effortOverrides: z.array(effortOverrideSchema).max(64),
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

export type EffortOverride = z.infer<typeof effortOverrideSchema>;
export type ToolModelChoice = z.infer<typeof toolModelChoiceSchema>;
