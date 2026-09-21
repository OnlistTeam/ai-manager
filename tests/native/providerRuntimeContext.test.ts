import { describe, expect, it } from "vitest";
import { providerRuntimeContextSchema } from "@/native";

/** The native side reports paths as unbounded strings; a deep path must not fail the whole read. */
describe("providerRuntimeContextSchema", () => {
  it("accepts paths far longer than any realistic default location", () => {
    const deep = `/Users/${"a".repeat(1200)}/CLAUDE.md`;
    const parsed = providerRuntimeContextSchema.safeParse({
      tool: "claude-code",
      liveConfigPaths: [deep],
      resources: [
        {
          id: "global-instructions",
          kind: "instructions",
          scope: "global",
          path: deep,
          exists: true,
          action: "edit",
          sizeBytes: 128,
          measurementLimited: false,
        },
      ],
      storage: {
        totalBytes: 0,
        sessionBytes: 0,
        sessionCount: 0,
        measurementLimited: false,
      },
      effectiveConnection: null,
    });
    expect(parsed.success).toBe(true);
  });
});
