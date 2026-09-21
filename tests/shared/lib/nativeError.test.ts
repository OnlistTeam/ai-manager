import { describe, expect, it } from "vitest";
import { NativeError } from "@/native";
import { payloadToErrorCopy, toErrorCopy } from "@/shared/lib/nativeError";

describe("toErrorCopy", () => {
  it("keeps every field of a structured backend error", () => {
    const error = new NativeError({
      code: "UNINSTALL_FAILED",
      messageKey: "error.tool.removePathRefused",
      technicalMessage: "/opt/claude is outside the home directory",
      remediation: "error.remediation.uninstallManually",
      contextId: "op-7",
    });
    expect(toErrorCopy(error)).toEqual({
      code: "UNINSTALL_FAILED",
      messageKey: "error.tool.removePathRefused",
      technicalMessage: "/opt/claude is outside the home directory",
      remediationKey: "error.remediation.uninstallManually",
      contextId: "op-7",
    });
  });

  it("falls back to a translatable key for anything else", () => {
    const copy = toErrorCopy(new Error("boom"));
    expect(copy.code).toBe("INTERNAL");
    expect(copy.messageKey).toBe("error.native.unrecognized");
    expect(copy.remediationKey).toBeNull();
    // The technical detail can be kept, but it is never rendered as user-facing copy.
    expect(copy.technicalMessage).toBe("boom");
  });

  it("never turns a raw value into user-facing copy", () => {
    expect(toErrorCopy("exit code 127").messageKey).toBe(
      "error.native.unrecognized",
    );
    expect(toErrorCopy(undefined).technicalMessage).toBeNull();
  });

  it("converts the error carried on an operation", () => {
    expect(
      payloadToErrorCopy({
        code: "INSTALL_FAILED",
        messageKey: "error.tool.installFailed",
        technicalMessage: null,
        remediation: "error.remediation.retryOrViewDetails",
        contextId: null,
      }),
    ).toMatchObject({
      code: "INSTALL_FAILED",
      messageKey: "error.tool.installFailed",
      remediationKey: "error.remediation.retryOrViewDetails",
    });
  });
});
