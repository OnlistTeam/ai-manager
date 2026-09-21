import { NativeError, type ErrorCode, type NativeErrorPayload } from "@/native";

/**
 * Spec §42: the frontend **only** gets renderable error fields from here.
 * `messageKey` / `remediationKey` go through i18n; `technicalMessage` is for
 * View Details only — never render `technicalMessage` as the primary copy anywhere.
 */
export interface ErrorCopy {
  code: ErrorCode;
  messageKey: string;
  remediationKey: string | null;
  technicalMessage: string | null;
  contextId: string | null;
}

export function payloadToErrorCopy(payload: NativeErrorPayload): ErrorCopy {
  return {
    code: payload.code,
    messageKey: payload.messageKey,
    remediationKey: payload.remediation,
    technicalMessage: payload.technicalMessage,
    contextId: payload.contextId,
  };
}

/**
 * Normalize anything that gets thrown into the same shape. A non-NativeError
 * always gets a generic key — never use `Error.message` (which might be
 * "exit code 127") as user-facing copy.
 */
export function toErrorCopy(error: unknown): ErrorCopy {
  if (error instanceof NativeError) {
    return payloadToErrorCopy({
      code: error.code,
      messageKey: error.messageKey,
      technicalMessage: error.technicalMessage,
      remediation: error.remediation,
      contextId: error.contextId,
    });
  }
  return {
    code: "INTERNAL",
    messageKey: "error.native.unrecognized",
    remediationKey: null,
    technicalMessage: error instanceof Error ? error.message : null,
    contextId: null,
  };
}
