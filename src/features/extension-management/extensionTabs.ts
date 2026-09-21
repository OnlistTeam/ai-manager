import type { ExtensionKind } from "@/entities/extension";
import type { DesktopApp } from "@/entities/desktop-app";
import { hasManageableConfiguration } from "@/entities/tool";
import type { Tool, ToolCapabilities } from "@/entities/tool";
import {
  extensionScopeKey,
  toolExtensionScope,
  type ExtensionScope,
} from "@/native";

export interface ExtensionTab {
  kind: ExtensionKind;
  titleKey: string;
  /** Spec §36: Beginner Mode must explain what this kind of extension does before anything else. */
  explainerKey: string;
  /** One reminder specific to this kind; `null` if there isn't one. */
  noteKey: string | null;
  emptyTitleKey: string;
  emptyDescriptionKey: string;
  /** Which field of `ToolCapabilities` to read. **No branching on tool name** (AI_RULES rule 8). */
  supports: (capabilities: ToolCapabilities) => boolean;
}

const SKILLS: ExtensionTab = {
  kind: "skill",
  titleKey: "extensions.skill.title",
  explainerKey: "extensions.skill.explainer",
  noteKey: null,
  emptyTitleKey: "extensions.skill.empty.title",
  emptyDescriptionKey: "extensions.skill.empty.description",
  supports: (capabilities) => capabilities.canManageSkills,
};

const MCP: ExtensionTab = {
  kind: "mcp",
  titleKey: "extensions.mcp.title",
  explainerKey: "extensions.mcp.explainer",
  noteKey: null,
  emptyTitleKey: "extensions.mcp.empty.title",
  emptyDescriptionKey: "extensions.mcp.empty.description",
  supports: (capabilities) => capabilities.canManageMcp,
};

const PROMPTS: ExtensionTab = {
  kind: "prompt",
  titleKey: "extensions.prompt.title",
  explainerKey: "extensions.prompt.explainer",
  // This kind is single-select: picking another one swaps it in, there's no
  // "turn all off". The user needs to know that up front.
  noteKey: "extensions.prompt.exclusive",
  emptyTitleKey: "extensions.prompt.empty.title",
  emptyDescriptionKey: "extensions.prompt.empty.description",
  supports: (capabilities) => capabilities.canManagePrompts,
};

/** Order matches the on-page display order, taken verbatim from the in-page list in Spec §36. */
export const EXTENSION_TABS: readonly ExtensionTab[] = [SKILLS, MCP, PROMPTS];

export const DEFAULT_EXTENSION_TAB: ExtensionTab = SKILLS;

export interface ResolvedExtensionScope {
  activeTab: ExtensionTab;
  scopedTargets: ExtensionScopeOption[];
  activeScope: ExtensionScopeOption | null;
}

export interface SupportedExtensionScope {
  kind: ExtensionKind;
  scope: ExtensionScope;
}

export interface ExtensionScopeOption {
  key: string;
  scope: ExtensionScope;
  name: string;
  /** The scope remains visible even when this extension kind is unsupported. */
  supported: boolean;
  tool: Tool | null;
  desktopApp: DesktopApp | null;
}

function installedToolsForTab(tools: readonly Tool[]): Tool[] {
  return tools.filter(hasManageableConfiguration);
}

function targetsForTab(
  tools: readonly Tool[],
  desktopApps: readonly DesktopApp[],
  tab: ExtensionTab,
): ExtensionScopeOption[] {
  const toolTargets = installedToolsForTab(tools).map((tool) => {
    const scope = toolExtensionScope(tool.id);
    return {
      key: extensionScopeKey(scope),
      scope,
      name: tool.name,
      supported: tab.supports(tool.capabilities),
      tool,
      desktopApp: null,
    };
  });
  const desktopTargets =
    tab.kind === "mcp"
      ? desktopApps
          .filter((app) => app.status !== "notInstalled")
          .map((desktopApp) => {
            const scope: ExtensionScope = {
              kind: "desktopApp",
              id: desktopApp.id,
            };
            return {
              key: extensionScopeKey(scope),
              scope,
              name: desktopApp.name,
              supported: desktopApp.canManageMcp,
              tool: null,
              desktopApp,
            };
          })
      : [];
  return [...toolTargets, ...desktopTargets];
}

/**
 * Returns every real page scope the current installation can expose. Startup
 * warming consumes this same capability mapping, so it cannot drift into
 * hard-coded tool/kind combinations as the upstream registry grows.
 */
export function supportedExtensionScopes(
  tools: readonly Tool[],
  desktopApps: readonly DesktopApp[] = [],
): SupportedExtensionScope[] {
  return EXTENSION_TABS.flatMap((tab) =>
    targetsForTab(tools, desktopApps, tab)
      .filter((target) => target.supported)
      .map((target) => ({
        kind: tab.kind,
        scope: target.scope,
      })),
  );
}

/**
 * The page resolver keeps installed targets visible while preferring a usable
 * kind and scope when there is no explicit remembered choice. Startup warming
 * uses `supportedExtensionScopes`, so unsupported targets never trigger a
 * speculative backend read.
 */
export function resolveExtensionScope(
  tools: readonly Tool[],
  desktopApps: readonly DesktopApp[],
  rememberedKind: ExtensionKind | null,
  rememberedScope: ExtensionScope | null,
): ResolvedExtensionScope {
  const eligible = (tab: ExtensionTab): ExtensionScopeOption[] =>
    targetsForTab(tools, desktopApps, tab);
  const rememberedTab = EXTENSION_TABS.find(
    (tab) => tab.kind === rememberedKind,
  );
  const activeTab =
    rememberedTab ??
    EXTENSION_TABS.find((tab) =>
      eligible(tab).some((target) => target.supported),
    ) ??
    DEFAULT_EXTENSION_TAB;
  const scopedTargets = eligible(activeTab);
  const rememberedKey =
    rememberedScope === null ? null : extensionScopeKey(rememberedScope);
  const activeScope =
    scopedTargets.find((target) => target.key === rememberedKey) ??
    scopedTargets.find((target) => target.supported) ??
    scopedTargets[0] ??
    null;

  return { activeTab, scopedTargets, activeScope };
}
