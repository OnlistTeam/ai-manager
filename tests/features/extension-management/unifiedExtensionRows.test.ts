import { describe, expect, it } from "vitest";
import type { Extension } from "@/entities/extension";
import {
  representativeEntry,
  unifiedExtensionRows,
} from "@/features/extension-management";

function entry(overrides: Partial<Extension> = {}): Extension {
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

describe("unifiedExtensionRows", () => {
  it("merges one found Skill across every app that lists it", () => {
    const rows = unifiedExtensionRows([
      [entry()],
      [entry({ scope: { kind: "tool", id: "codex" } })],
    ]);
    expect(rows).toHaveLength(1);
    expect([...(rows[0]?.entries.keys() ?? [])]).toEqual([
      "tool:claude-code",
      "tool:codex",
    ]);
  });

  it("keeps each app's own switch on a managed row", () => {
    const managed = { kind: "mcp", management: "managed" } as const;
    const rows = unifiedExtensionRows([
      [entry({ ...managed, enabled: true })],
      [
        entry({
          ...managed,
          enabled: false,
          scope: { kind: "tool", id: "codex" },
        }),
      ],
      [
        entry({
          ...managed,
          enabled: true,
          scope: { kind: "desktopApp", id: "claude-desktop" },
        }),
      ],
    ]);
    expect(rows).toHaveLength(1);
    expect(
      [...(rows[0]?.entries.values() ?? [])].map((item) => item.enabled),
    ).toEqual([true, false, true]);
  });

  it("never merges a managed item with a found one that shares its id", () => {
    const rows = unifiedExtensionRows([
      [entry({ management: "managed", enabled: false, canDisable: true })],
      [entry({ scope: { kind: "tool", id: "codex" } })],
    ]);
    expect(rows.map((row) => row.key)).toEqual([
      "managed:shared-item",
      "detected:shared-item",
    ]);
  });

  it("lists managed items in native order, then found ones by name", () => {
    const rows = unifiedExtensionRows([
      [
        entry({ id: "zeta", name: "Zeta", management: "managed" }),
        entry({ id: "alpha", name: "Alpha", management: "managed" }),
        entry({ id: "found-b", name: "Beta" }),
        entry({ id: "found-a", name: "Able" }),
      ],
    ]);
    expect(rows.map((row) => row.id)).toEqual([
      "zeta",
      "alpha",
      "found-a",
      "found-b",
    ]);
  });

  it("hands whole-item flows one of the row's real entries", () => {
    const [row] = unifiedExtensionRows([
      [entry({ scope: { kind: "tool", id: "codex" } })],
    ]);
    expect(row && representativeEntry(row).scope).toEqual({
      kind: "tool",
      id: "codex",
    });
  });
});
