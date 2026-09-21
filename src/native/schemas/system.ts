import { z } from "zod";

/**
 * Mirrors `InitErrorPayload` in src-tauri/src/init_status.rs: what the backend
 * recorded when it could not open the product database or configuration.
 */
export const initErrorPayloadSchema = z
  .object({
    path: z.string(),
    error: z.string(),
    /** `db_version_too_new` asks for the in-app upgrade screen instead of exiting. */
    kind: z.string().optional(),
    db_version: z.number().int().optional(),
    supported_version: z.number().int().optional(),
  })
  .strict();

export type InitErrorPayload = z.infer<typeof initErrorPayloadSchema>;
