import { z } from "zod";

/** ADR-0049: only the switch crosses the wire, never a value or placeholder. */
export const privacyProtectionSchema = z
  .object({
    enabled: z.boolean(),
  })
  .strict();

export type PrivacyProtection = z.infer<typeof privacyProtectionSchema>;
