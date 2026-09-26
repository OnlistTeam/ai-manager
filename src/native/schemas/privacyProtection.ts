import { z } from "zod";

/**
 * ADR-0049: only the user's choices cross the wire, never a masked value or
 * a placeholder. `words` is the normalized list the backend stored.
 */
export const privacyProtectionSchema = z
  .object({
    maskSecrets: z.boolean(),
    maskPersonal: z.boolean(),
    words: z.array(z.string()),
  })
  .strict();

export type PrivacyProtection = z.infer<typeof privacyProtectionSchema>;

/** A partial change; the backend keeps every field left out. */
export type PrivacyProtectionPatch = Partial<PrivacyProtection>;
