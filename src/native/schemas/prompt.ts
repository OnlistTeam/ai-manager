import { z } from "zod";
import { toolIdSchema } from "./tool";

export const MAX_PROMPT_NAME_CHARS = 80;
export const MAX_PROMPT_DESCRIPTION_CHARS = 500;
export const MAX_PROMPT_CONTENT_BYTES = 1024 * 1024;

const utf8Length = (value: string): number =>
  new TextEncoder().encode(value).byteLength;

export const promptDraftSchema = z
  .object({
    name: z.string().trim().min(1).max(MAX_PROMPT_NAME_CHARS),
    description: z.string().trim().max(MAX_PROMPT_DESCRIPTION_CHARS).nullable(),
    content: z
      .string()
      .refine((value) => value.trim().length > 0)
      .refine((value) => !value.includes("\0"))
      .refine((value) => utf8Length(value) <= MAX_PROMPT_CONTENT_BYTES),
  })
  .strict();

/** Content is returned only by the explicit edit-detail command. */
export const promptDetailSchema = z
  .object({
    id: z.string().min(1),
    tool: toolIdSchema,
    name: z.string(),
    description: z.string().nullable(),
    content: z.string(),
    enabled: z.boolean(),
  })
  .strict();

export type PromptDraft = z.infer<typeof promptDraftSchema>;
export type PromptDetail = z.infer<typeof promptDetailSchema>;
