import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import {
  DEFAULT_EXTENSION_TAB,
  EXTENSION_TABS,
  resolveExtensionScope,
  supportedExtensionScopes,
} from "@/features/extension-management";
import type { Tool, ToolCapabilities, ToolId } from "@/entities/tool";
import type { DesktopApp } from "@/entities/desktop-app";

function capabilities(overrides: Partial<ToolCapabilities>): ToolCapabilities {
  return {
    canInstall: false,
    canUpdate: false,
    canUninstall: false,
    canRepair: false,
    canManageProvider: false,
    canManageMcp: false,
    canManageSkills: false,
    canManagePrompts: false,
    canManageVersion: false,
    canLaunch: true,
    ...overrides,
  };
}

function tool(
  id: ToolId,
  status: Tool["status"],
  overrides: Partial<ToolCapabilities>,
): Tool {
  return {
    id,
    name: id,
    descriptionKey: `tools.description.${id}`,
    status,
    version: status === "notInstalled" ? null : "1.0.0",
    latestVersion: null,
    capabilities: capabilities(overrides),
    sessionsInsideSettings: false,
    environment: null,
  };
}

const CLAUDE_DESKTOP: DesktopApp = {
  id: "claude-desktop",
  name: "Claude Desktop",
  status: "installed",
  version: "1.0.0",
  latestVersion: null,
  relatedTool: "claude-code",
  configurationRelationship: "separateConfiguration",
  canLaunch: true,
  environment: "macos",
  installerHandoff: "directOfficialPackage",
  uninstallHandoff: "revealApplication",
  updatesManagedByVendor: true,
  canRollback: false,
  canManageMcp: true,
};

describe("EXTENSION_TABS", () => {
  it("lists the three inner pages in the order spec 36 gives them", () => {
    expect(EXTENSION_TABS.map((tab) => tab.kind)).toEqual([
      "skill",
      "mcp",
      "prompt",
    ]);
    expect(DEFAULT_EXTENSION_TAB).toBe(EXTENSION_TABS[0]);
  });

  it("gives each tab its own capability field and no other", () => {
    const cases = [
      ["skill", capabilities({ canManageSkills: true })],
      ["mcp", capabilities({ canManageMcp: true })],
      ["prompt", capabilities({ canManagePrompts: true })],
    ] as const;

    for (const [allowed, caps] of cases) {
      for (const tab of EXTENSION_TABS) {
        expect(tab.supports(caps)).toBe(tab.kind === allowed);
      }
    }
  });

  it("offers nothing when a tool can manage nothing", () => {
    const none = capabilities({});
    for (const tab of EXTENSION_TABS) {
      expect(tab.supports(none)).toBe(false);
    }
  });

  it("resolves the exact remembered capability scope for startup warming", () => {
    const claude = tool("claude-code", "installed", {
      canManageSkills: true,
      canManagePrompts: true,
    });
    const codex = tool("codex", "installed", {
      canManageSkills: true,
      canManageMcp: true,
    });

    const scope = resolveExtensionScope([claude, codex], [], "skill", {
      kind: "tool",
      id: "codex",
    });

    expect(scope.activeTab.kind).toBe("skill");
    expect(scope.scopedTargets.map((item) => item.key)).toEqual([
      "tool:claude-code",
      "tool:codex",
    ]);
    expect(scope.activeScope?.scope).toEqual({ kind: "tool", id: "codex" });
  });

  it("falls back by capability and never warms an uninstalled tool", () => {
    const unavailable = tool("claude-code", "notInstalled", {
      canManageSkills: true,
    });
    const installed = tool("codex", "installed", { canManageMcp: true });

    const scope = resolveExtensionScope([unavailable, installed], [], null, {
      kind: "tool",
      id: "claude-code",
    });

    expect(scope.activeTab.kind).toBe("mcp");
    expect(scope.scopedTargets.map((item) => item.key)).toEqual(["tool:codex"]);
    expect(scope.activeScope?.scope).toEqual({ kind: "tool", id: "codex" });
  });

  it("enumerates every installed capability scope without hard-coded tools", () => {
    const claude = tool("claude-code", "installed", {
      canManageSkills: true,
      canManagePrompts: true,
    });
    const codex = tool("codex", "installed", {
      canManageSkills: true,
      canManageMcp: true,
    });
    const unavailable = tool("gemini-cli", "notInstalled", {
      canManageSkills: true,
      canManageMcp: true,
    });

    expect(supportedExtensionScopes([claude, codex, unavailable])).toEqual([
      { kind: "skill", scope: { kind: "tool", id: "claude-code" } },
      { kind: "skill", scope: { kind: "tool", id: "codex" } },
      { kind: "mcp", scope: { kind: "tool", id: "codex" } },
      { kind: "prompt", scope: { kind: "tool", id: "claude-code" } },
    ]);
  });

  it("offers Claude Desktop as its own MCP scope without inventing a ToolId", () => {
    const claude = tool("claude-code", "installed", {
      canManageMcp: true,
    });
    const resolved = resolveExtensionScope([claude], [CLAUDE_DESKTOP], "mcp", {
      kind: "desktopApp",
      id: "claude-desktop",
    });

    expect(resolved.scopedTargets.map((target) => target.scope)).toEqual([
      { kind: "tool", id: "claude-code" },
      { kind: "desktopApp", id: "claude-desktop" },
    ]);
    expect(resolved.activeScope?.desktopApp?.id).toBe("claude-desktop");
    expect(resolved.activeScope?.tool).toBeNull();
  });

  it("points every copy key at real English text", () => {
    const copy: Record<string, unknown> = { extensions: en.extensions };
    const resolve = (key: string) =>
      key.split(".").reduce<unknown>((node, part) => {
        if (node !== null && typeof node === "object" && part in node) {
          return (node as Record<string, unknown>)[part];
        }
        return undefined;
      }, copy);

    for (const tab of EXTENSION_TABS) {
      for (const key of [
        tab.titleKey,
        tab.explainerKey,
        tab.emptyTitleKey,
        tab.emptyDescriptionKey,
      ]) {
        expect(typeof resolve(key)).toBe("string");
      }
      if (tab.noteKey !== null) {
        expect(typeof resolve(tab.noteKey)).toBe("string");
      }
    }
  });

  it("carries spec 36's own MCP wording", () => {
    expect(en.extensions.mcp.explainer).toBe(
      "Connect AI tools to files, browsers and other services.",
    );
  });
});
