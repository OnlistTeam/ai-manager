import { z } from "zod";

/** Corresponds 1:1 with Rust `domain::ErrorCode`; any change here is a breaking change. */
export const errorCodeSchema = z.enum([
  "TOOL_NOT_FOUND",
  "INSTALL_FAILED",
  "UPDATE_FAILED",
  "UPDATE_PREVIEW_STALE",
  "UNINSTALL_FAILED",
  "LAUNCH_FAILED",
  "CONFIG_PARSE_FAILED",
  "CONFIG_WRITE_FAILED",
  "PROVIDER_UNREACHABLE",
  "PROVIDER_NOT_FOUND",
  "SESSION_NOT_FOUND",
  "EXTENSION_NOT_FOUND",
  "BACKUP_NOT_FOUND",
  "MCP_UNAVAILABLE",
  "PERMISSION_DENIED",
  "NETWORK_ERROR",
  "OPERATION_CONFLICT",
  "UPSTREAM_ERROR",
  "INTERNAL",
]);

export type ErrorCode = z.infer<typeof errorCodeSchema>;

export const nativeErrorPayloadSchema = z.object({
  code: errorCodeSchema,
  messageKey: z.string(),
  technicalMessage: z.string().nullable(),
  remediation: z.string().nullable(),
  contextId: z.string().nullable(),
});

export type NativeErrorPayload = z.infer<typeof nativeErrorPayloadSchema>;

/** The frontend only branches on `code`; matching error strings is forbidden (AI_RULES types & errors section). */
export class NativeError extends Error {
  readonly code: ErrorCode;
  readonly messageKey: string;
  readonly technicalMessage: string | null;
  readonly remediation: string | null;
  readonly contextId: string | null;

  constructor(payload: NativeErrorPayload) {
    super(`[${payload.code}] ${payload.messageKey}`);
    this.name = "NativeError";
    this.code = payload.code;
    this.messageKey = payload.messageKey;
    this.technicalMessage = payload.technicalMessage;
    this.remediation = payload.remediation;
    this.contextId = payload.contextId;
  }
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

function describe(raw: unknown): string {
  if (raw instanceof Error) return raw.message;
  if (typeof raw === "string") return raw;
  try {
    return JSON.stringify(raw);
  } catch {
    return String(raw);
  }
}

/**
 * Tauri rejects with an object at real runtime, while test mocks reject with an
 * Error whose message is JSON text -- both paths must converge on the same NativeError.
 */
export function toNativeError(raw: unknown): NativeError {
  const candidate =
    typeof raw === "string"
      ? parseJson(raw)
      : raw instanceof Error
        ? parseJson(raw.message)
        : raw;

  const parsed = nativeErrorPayloadSchema.safeParse(candidate);
  if (parsed.success) {
    return new NativeError(parsed.data);
  }

  return new NativeError({
    code: "INTERNAL",
    messageKey: "error.native.unrecognized",
    technicalMessage: describe(raw),
    remediation: null,
    contextId: null,
  });
}
