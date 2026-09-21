import { describe, expect, it } from "vitest";
import { isDiagnosticWarning } from "@/entities/operation/OperationLogDetails";

describe("operation log presentation", () => {
  it("recognises package-manager warnings written to stderr", () => {
    expect(
      isDiagnosticWarning('npm warn Unknown env config "_jsr-registry"'),
    ).toBe(true);
    expect(isDiagnosticWarning("pnpm WARN deprecated dependency")).toBe(true);
    expect(isDiagnosticWarning("warning: cache is stale")).toBe(true);
  });

  it("does not turn actual stderr failures into warnings", () => {
    expect(isDiagnosticWarning("npm ERR! registry request failed")).toBe(false);
    expect(isDiagnosticWarning("permission denied")).toBe(false);
  });
});
