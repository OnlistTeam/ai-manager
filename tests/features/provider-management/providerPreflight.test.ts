import { describe, expect, it } from "vitest";
import { supportsLightweightProviderFailover } from "@/features/provider-management";
import type { ToolId } from "@/native";

describe("lightweight provider failover scope", () => {
  it("matches the four tools with a single switchable provider data plane", () => {
    const expected = new Set<ToolId>([
      "claude-code",
      "codex",
      "gemini-cli",
      "grok-build",
    ]);

    for (const tool of [
      "claude-code",
      "codex",
      "opencode",
      "gemini-cli",
      "grok-build",
      "openclaw",
      "hermes",
      "pi",
    ] satisfies ToolId[]) {
      expect(supportsLightweightProviderFailover(tool)).toBe(
        expected.has(tool),
      );
    }
  });
});
