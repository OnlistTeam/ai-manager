import { z } from "zod";

/**
 * Shell start-up variable wire types (ADR-0042).
 *
 * The value travels because the edit dialog has to prefill it — asking someone
 * to retype an address they cannot see is not an edit, it is a guess — and
 * because the write compares it to refuse a line that changed in the meantime.
 * The path travels already shortened to `~/…`, and is display only: a write
 * names the variable and the line, and the backend re-locates the file itself.
 */

export const shellVariableLocationSchema = z
  .object({
    variable: z.string(),
    path: z.string(),
    line: z.number().int().positive(),
    value: z.string(),
    /** False for a command substitution or interpolation, where replacing the
     *  text with a literal would change what the line does. */
    editable: z.boolean(),
  })
  .strict();

export const shellVariableUpdateSchema = z
  .object({
    variable: z.string().min(1),
    line: z.number().int().positive(),
    expectedValue: z.string(),
    /** A line break would split one assignment into two statements. */
    newValue: z
      .string()
      .max(4096)
      .regex(/^[^\n\r]*$/),
  })
  .strict();

export const shellVariableWrittenSchema = z
  .object({
    path: z.string(),
    line: z.number().int().positive(),
  })
  .strict();

export const shellVariableLocationListSchema = z.array(
  shellVariableLocationSchema,
);

export type ShellVariableLocation = z.infer<typeof shellVariableLocationSchema>;
export type ShellVariableUpdate = z.infer<typeof shellVariableUpdateSchema>;
export type ShellVariableWritten = z.infer<typeof shellVariableWrittenSchema>;
