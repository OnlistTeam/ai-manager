import { invoke } from "@tauri-apps/api/core";
import type { ZodType } from "zod";
import { NativeError, toNativeError } from "./schemas/error";

/**
 * The single project-wide invoke exit point (Spec §21). Every return value must pass
 * Zod validation, and every rejection must be normalized to a NativeError.
 */
export async function invokeNative<T>(
  command: string,
  schema: ZodType<T>,
  payload: Record<string, unknown> = {},
): Promise<T> {
  let raw: unknown;
  try {
    raw = await invoke(command, payload);
  } catch (error) {
    throw toNativeError(error);
  }

  const parsed = schema.safeParse(raw);
  if (!parsed.success) {
    throw new NativeError({
      code: "INTERNAL",
      messageKey: "error.native.responseSchemaMismatch",
      technicalMessage: `${command}: ${parsed.error.message}`,
      remediation: null,
      contextId: null,
    });
  }
  return parsed.data;
}
