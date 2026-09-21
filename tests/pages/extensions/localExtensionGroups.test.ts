import { describe, expect, it } from "vitest";
import type { Extension } from "@/entities/extension";
import {
  groupLocalExtensionItems,
  skillOwnersById,
} from "@/pages/extensions/localExtensionGroups";

function detected(overrides: Partial<Extension> = {}): Extension {
  return {
    kind: "skill",
    id: "shared-item",
    scope: { kind: "tool", id: "claude-code" },
    name: "Shared item",
    description: null,
    management: "detected",
    enabled: true,
    canDisable: false,
    ...overrides,
  };
}

describe("groupLocalExtensionItems", () => {
  it("merges a proven shared Skill across tool scopes", () => {
    const groups = groupLocalExtensionItems([
      detected(),
      detected({ scope: { kind: "tool", id: "codex" } }),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.tools).toEqual(["claude-code", "codex"]);
  });

  it("does not conflate same-id MCP entries whose private configs cannot be compared", () => {
    const groups = groupLocalExtensionItems([
      detected({ kind: "mcp" }),
      detected({ kind: "mcp", scope: { kind: "tool", id: "codex" } }),
    ]);
    expect(groups).toHaveLength(2);
    expect(groups.map((group) => group.tools)).toEqual([
      ["claude-code"],
      ["codex"],
    ]);
  });
});

describe("skillOwnersById", () => {
  it("names every tool that already carries the Skill", () => {
    const owners = skillOwnersById(
      [detected(), detected({ scope: { kind: "tool", id: "codex" } })],
      new Map([
        ["claude-code", "Claude Code"],
        ["codex", "Codex CLI"],
      ] as const),
    );
    expect(owners.get("shared-item")).toEqual([
      { tool: "claude-code", name: "Claude Code" },
      { tool: "codex", name: "Codex CLI" },
    ]);
  });

  it("keeps an unnamed tool visible instead of dropping a real copy", () => {
    const owners = skillOwnersById([detected()], new Map());
    expect(owners.get("shared-item")).toEqual([
      { tool: "claude-code", name: "claude-code" },
    ]);
  });

  it("leaves MCP entries out; only Skills share a directory identity", () => {
    const owners = skillOwnersById(
      [detected({ kind: "mcp" })],
      new Map([["claude-code", "Claude Code"]] as const),
    );
    expect(owners.size).toBe(0);
  });
});
