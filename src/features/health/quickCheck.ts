import type { ProviderConnectivity } from "@/entities/health";
import { connectivityFor } from "@/entities/health";
import type { Tool } from "@/entities/tool";
import type {
  ConfigReadStatus,
  HealthSnapshot,
  LocalExtensionInventory,
  ToolId,
} from "@/native";
import type { EnvironmentStatus } from "@/shared/ui/StatusBadge";

export type QuickCheckItemStatus = EnvironmentStatus | "info";
export type QuickCheckItemKind =
  | "tool"
  | "provider"
  | "config"
  | "connectivity"
  | "mcp";

export type QuickCheckResolution =
  | "updateTool"
  | "repairTool"
  | "reviewTool"
  | "connectService"
  | "reviewService"
  | "reviewExtensions";

export interface QuickCheckItem {
  id: string;
  /** Stable navigation context for tool-owned signals; null for global inventory. */
  toolId: ToolId | null;
  kind: QuickCheckItemKind;
  status: QuickCheckItemStatus;
  titleKey: string;
  descriptionKey: string;
  values: Record<string, string | number>;
  /** Safe next step. Missing means the row is informational only. */
  resolution?: QuickCheckResolution;
}

export interface QuickCheckSummary {
  status: EnvironmentStatus;
  attentionCount: number;
  installedCount: number;
  updatableCount: number;
  configuredServiceCount: number;
  mcpTotalCount: number;
  mcpEnabledCount: number;
  items: QuickCheckItem[];
}

/** A file that does not exist yet is "not configured", never a fault. */
const CONFIG_ITEM: Record<
  ConfigReadStatus,
  {
    status: QuickCheckItemStatus;
    copy: string;
    resolution?: QuickCheckResolution;
  }
> = {
  readable: { status: "ready", copy: "configReadable" },
  missing: { status: "info", copy: "configMissing" },
  unreadable: {
    status: "action",
    copy: "configUnreadable",
    resolution: "reviewService",
  },
};

const RANK: Record<QuickCheckItemStatus, number> = {
  info: 0,
  ready: 0,
  attention: 1,
  action: 2,
};

function itemId(...parts: string[]): string {
  return JSON.stringify(parts);
}

function displayName(tool: ToolId, names: ReadonlyMap<ToolId, string>): string {
  return names.get(tool) ?? tool;
}

/**
 * Pure aggregation of the §38 allow-list. Duplicate source rows collapse by semantic id; if two
 * contradictory rows arrive, the more urgent one wins so the Home count cannot be inflated or
 * accidentally softened.
 */
export function aggregateQuickCheck(
  tools: readonly Tool[],
  snapshot: HealthSnapshot,
  connectivity: ProviderConnectivity,
  localExtensions?: LocalExtensionInventory | null,
): QuickCheckSummary {
  const items = new Map<string, QuickCheckItem>();
  const names = new Map<ToolId, string>();
  const installed = new Set<ToolId>();
  const updatable = new Set<ToolId>();
  const configuredServices = new Map<ToolId, number>();

  const add = (item: QuickCheckItem) => {
    const previous = items.get(item.id);
    if (!previous || RANK[item.status] > RANK[previous.status]) {
      items.set(item.id, item);
    }
  };

  for (const tool of tools) {
    names.set(tool.id, tool.name);
    if (tool.status === "installed" || tool.status === "updateAvailable") {
      installed.add(tool.id);
    }
    if (tool.status === "updateAvailable") updatable.add(tool.id);

    const common = {
      id: itemId("tool", tool.id),
      toolId: tool.id,
      kind: "tool" as const,
      values: {
        name: tool.name,
        version: tool.version ?? "—",
        latestVersion: tool.latestVersion ?? "—",
      },
    };
    if (tool.status === "broken") {
      add({
        ...common,
        status: "action",
        resolution: tool.capabilities.canRepair ? "repairTool" : "reviewTool",
        titleKey: "preferences.check.item.toolBroken.title",
        descriptionKey: "preferences.check.item.toolBroken.description",
      });
    } else if (tool.status === "updateAvailable") {
      add({
        ...common,
        status: "attention",
        resolution: "updateTool",
        titleKey: "preferences.check.item.toolUpdate.title",
        descriptionKey: "preferences.check.item.toolUpdate.description",
      });
    } else if (tool.status === "unknown") {
      add({
        ...common,
        status: "attention",
        resolution: "reviewTool",
        titleKey: "preferences.check.item.toolUnknown.title",
        descriptionKey: "preferences.check.item.toolUnknown.description",
      });
    } else if (tool.status === "installed") {
      add({
        ...common,
        status: "ready",
        titleKey: "preferences.check.item.toolReady.title",
        descriptionKey: "preferences.check.item.toolReady.description",
      });
    } else {
      add({
        ...common,
        status: "info",
        titleKey: "preferences.check.item.toolNotInstalled.title",
        descriptionKey: "preferences.check.item.toolNotInstalled.description",
      });
    }
  }

  for (const provider of snapshot.providers) {
    if (!installed.has(provider.tool)) continue;
    configuredServices.set(
      provider.tool,
      Math.max(
        configuredServices.get(provider.tool) ?? 0,
        provider.configuredCount,
      ),
    );
    const name = displayName(provider.tool, names);
    add({
      id: itemId("provider", provider.tool),
      toolId: provider.tool,
      kind: "provider",
      status: provider.configured ? "ready" : "attention",
      titleKey: provider.configured
        ? "preferences.check.item.providerReady.title"
        : "preferences.check.item.providerMissing.title",
      descriptionKey: provider.configured
        ? "preferences.check.item.providerReady.description"
        : "preferences.check.item.providerMissing.description",
      values: { name, count: provider.configuredCount },
      resolution: provider.configured ? undefined : "connectService",
    });

    for (const target of provider.checkTargets) {
      const result = connectivityFor(
        connectivity,
        provider.tool,
        target.providerId,
      );
      const status: QuickCheckItemStatus =
        result?.reachability === "operational"
          ? "ready"
          : result === undefined
            ? "info"
            : "attention";
      const state = result?.reachability ?? "notChecked";
      add({
        id: itemId("connectivity", provider.tool, target.providerId),
        toolId: provider.tool,
        kind: "connectivity",
        status,
        titleKey: `preferences.check.item.connectivity.${state}.title`,
        descriptionKey: `preferences.check.item.connectivity.${state}.description`,
        values: {
          name,
          provider: target.name,
          ms: result?.responseTimeMs ?? "—",
        },
        resolution:
          result !== undefined && result.reachability !== "operational"
            ? "reviewService"
            : undefined,
      });
    }
  }

  for (const config of snapshot.configs) {
    if (!installed.has(config.tool)) continue;
    const name = displayName(config.tool, names);
    const item = CONFIG_ITEM[config.status];
    add({
      id: itemId("config", config.tool),
      toolId: config.tool,
      kind: "config",
      status: item.status,
      titleKey: `preferences.check.item.${item.copy}.title`,
      descriptionKey: `preferences.check.item.${item.copy}.description`,
      values: { name },
      resolution: item.resolution,
    });
  }

  if (localExtensions === null) {
    add({
      id: itemId("mcp", "inventory-unavailable"),
      toolId: null,
      kind: "mcp",
      status: "attention",
      titleKey: "preferences.check.item.mcpInventoryUnavailable.title",
      descriptionKey:
        "preferences.check.item.mcpInventoryUnavailable.description",
      values: {},
      resolution: "reviewExtensions",
    });
  } else if (localExtensions !== undefined) {
    const unavailableMcpScopes = localExtensions.scopes.filter(
      (scope) =>
        scope.kind === "mcp" &&
        installed.has(scope.tool) &&
        scope.status === "unavailable",
    ).length;
    if (unavailableMcpScopes > 0 || localExtensions.truncated) {
      add({
        id: itemId("mcp", "inventory-partial"),
        toolId: null,
        kind: "mcp",
        status: "attention",
        titleKey: "preferences.check.item.mcpInventoryPartial.title",
        descriptionKey:
          "preferences.check.item.mcpInventoryPartial.description",
        values: { count: unavailableMcpScopes },
        resolution: "reviewExtensions",
      });
    }
  }

  add({
    id: itemId("mcp"),
    toolId: null,
    kind: "mcp",
    status: "info",
    titleKey: "preferences.check.item.mcp.title",
    descriptionKey: "preferences.check.item.mcp.description",
    values: { enabled: snapshot.mcp.enabled, total: snapshot.mcp.total },
  });

  const result = [...items.values()].sort(
    (left, right) => RANK[right.status] - RANK[left.status],
  );
  const attentionCount = result.filter(
    (item) => item.status === "attention" || item.status === "action",
  ).length;
  const status: EnvironmentStatus = result.some(
    (item) => item.status === "action",
  )
    ? "action"
    : attentionCount > 0 || installed.size === 0
      ? "attention"
      : "ready";

  return {
    status,
    attentionCount,
    installedCount: installed.size,
    updatableCount: updatable.size,
    configuredServiceCount: [...configuredServices.values()].reduce(
      (total, count) => total + count,
      0,
    ),
    mcpTotalCount: snapshot.mcp.total,
    mcpEnabledCount: snapshot.mcp.enabled,
    items: result,
  };
}
