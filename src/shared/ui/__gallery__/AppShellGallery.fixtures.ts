import type { QueryClient } from "@tanstack/react-query";
import { backupKeys } from "@/entities/backup";
import { desktopAppKeys } from "@/entities/desktop-app";
import { extensionKeys } from "@/entities/extension";
import { healthKeys, type ProviderConnectivity } from "@/entities/health";
import { importKeys } from "@/entities/import";
import { networkProxyKeys } from "@/entities/network-proxy";
import { operationKeys } from "@/entities/operation";
import { providerKeys } from "@/entities/provider";
import { routingKeys } from "@/entities/routing";
import { sessionKeys } from "@/entities/session";
import { settingsKeys } from "@/entities/settings";
import { toolKeys } from "@/entities/tool";
import { updateKeys } from "@/entities/update";
import { usageKeys } from "@/entities/usage";
import type {
  BackupList,
  DesktopApp,
  Extension,
  HealthSnapshot,
  ImportPreview,
  LocalExtensionInventory,
  NetworkProxySettings,
  ProductSettings,
  Provider,
  ProviderConnectionProfile,
  ProviderRuntimeContext,
  RoutingOverview,
  RoutingProvider,
  SessionList,
  Tool,
  ToolCapabilities,
  UpdateStatus,
  UsageMetrics,
  UsageOverview,
} from "@/native";

const CAPABILITIES: ToolCapabilities = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: false,
  canLaunch: true,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: true,
};

export const GALLERY_TOOLS: Tool[] = [
  {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "tool.claude-code.description",
    status: "installed",
    version: "2.1.3",
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: "npm",
  },
  {
    id: "codex",
    name: "Codex",
    descriptionKey: "tool.codex.description",
    status: "installed",
    version: "0.42.0",
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: "npm",
  },
  {
    id: "gemini-cli",
    name: "Gemini CLI",
    descriptionKey: "tool.gemini-cli.description",
    status: "updateAvailable",
    version: "0.8.1",
    latestVersion: "0.9.0",
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: "npm",
  },
];

const SETTINGS: ProductSettings = {
  advancedMode: true,
  importPromptSeen: true,
  toolScope: null,
  extensionScope: null,
  extensionKind: null,
  downloadStrategy: "automatic",
  automaticProviderFailover: false,
  terminalApp: null,
};

const HEALTH: HealthSnapshot = {
  providers: GALLERY_TOOLS.map((tool) => ({
    tool: tool.id,
    configured: true,
    configuredCount: tool.id === "claude-code" ? 2 : 1,
    checkTargets: [],
  })),
  configs: GALLERY_TOOLS.map((tool) => ({
    tool: tool.id,
    status: "readable" as const,
  })),
  mcp: { total: 4, enabled: 3 },
};

const DESKTOP_APPS: DesktopApp[] = [
  {
    id: "codex-app",
    name: "ChatGPT / Codex",
    status: "installed",
    version: "1.12.4",
    latestVersion: null,
    relatedTool: "codex",
    configurationRelationship: "sharedConfiguration",
    canLaunch: true,
    environment: "macos",
    installerHandoff: "directOfficialPackage",
    uninstallHandoff: "revealApplication",
    updatesManagedByVendor: true,
    canRollback: false,
    canManageMcp: false,
  },
  {
    id: "claude-desktop",
    name: "Claude Desktop",
    status: "installed",
    version: "0.12.2",
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
  },
];

const PROVIDERS: Provider[] = [
  {
    id: "anthropic-official",
    tool: "claude-code",
    name: "Anthropic",
    kind: "official",
    active: true,
    baseUrl: "https://api.anthropic.com",
    apiKey: "sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-3K9F",
    websiteUrl: "https://www.anthropic.com/claude-code",
    testable: true,
    canRemove: false,
  },
  {
    id: "team-gateway",
    tool: "claude-code",
    name: "Shared Team Gateway",
    kind: "custom",
    active: false,
    baseUrl: "https://api.example.test/claude",
    apiKey: "sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-8P2M",
    websiteUrl: null,
    testable: true,
    canRemove: true,
  },
];

const CONNECTION_PROFILE: ProviderConnectionProfile = {
  defaultPresetId: "official",
  modelRequired: false,
  baseUrlTakesNoVersion: false,
  presets: [
    {
      id: "official",
      serviceName: "Anthropic API",
      defaultName: "Anthropic",
      defaultModel: "claude-sonnet-5",
      baseUrl: "https://api.anthropic.com",
      websiteUrl: "https://www.anthropic.com",
      apiKeyUrl: "https://console.anthropic.com",
      official: true,
    },
  ],
};

const RUNTIME_CONTEXT: ProviderRuntimeContext = {
  tool: "claude-code",
  liveConfigPaths: ["~/.claude/settings.json"],
  resources: [
    {
      id: "global-instructions",
      kind: "instructions",
      scope: "global",
      path: "~/.claude/CLAUDE.md",
      exists: true,
      action: "edit",
      sizeBytes: 2_048,
      measurementLimited: false,
    },
    {
      id: "session-data-0",
      kind: "sessionData",
      scope: "project",
      path: "~/.claude/projects",
      exists: true,
      action: "browse",
      sizeBytes: 18_874_368,
      measurementLimited: false,
    },
  ],
  storage: {
    totalBytes: 18_876_416,
    sessionBytes: 18_874_368,
    sessionCount: 24,
    measurementLimited: false,
  },
  effectiveConnection: {
    endpoint: "https://relay.example.test/",
    endpointSource: { kind: "liveConfig", path: "~/.claude/settings.json" },
    credential: "configured",
    credentialSource: { kind: "liveConfig", path: "~/.claude/settings.json" },
    providerId: "relay",
    shellInspected: true,
  },
};

function routingProvider(
  id: string,
  name: string,
  priority: number | null,
  current = false,
): RoutingProvider {
  return {
    id,
    name,
    priority,
    current,
    healthy: true,
    consecutiveFailures: 0,
  };
}

const ROUTING: RoutingOverview = {
  running: true,
  address: "127.0.0.1",
  port: 15_721,
  activeConnections: 2,
  totalRequests: 1_284,
  successRequests: 1_276,
  failedRequests: 8,
  failoverCount: 2,
  targets: [
    {
      tool: "claude-code",
      takeoverEnabled: true,
      autoFailoverEnabled: true,
      currentProvider: routingProvider(
        "anthropic-official",
        "Anthropic",
        1,
        true,
      ),
      queue: [
        routingProvider("anthropic-official", "Anthropic", 1, true),
        routingProvider("team-gateway", "Shared Team Gateway", 2),
      ],
      available: [routingProvider("backup-api", "Backup API", null)],
    },
    ...(["codex", "gemini-cli", "grok-build"] as const).map((tool) => ({
      tool,
      takeoverEnabled: false,
      autoFailoverEnabled: false,
      currentProvider: null,
      queue: [],
      available: [],
    })),
  ],
};

const MANAGED_SKILLS: Extension[] = [
  {
    kind: "skill",
    id: "release-helper",
    scope: { kind: "tool", id: "claude-code" },
    name: "Release helper",
    description: "Build, sign and verify desktop releases.",
    management: "managed",
    enabled: true,
    canDisable: true,
  },
  {
    kind: "skill",
    id: "frontend-review",
    scope: { kind: "tool", id: "claude-code" },
    name: "Frontend review",
    description: "Review interface quality and accessibility.",
    management: "managed",
    enabled: false,
    canDisable: true,
  },
];

const LOCAL_EXTENSIONS: LocalExtensionInventory = {
  items: [
    {
      kind: "skill",
      id: "local-design-audit",
      scope: { kind: "tool", id: "claude-code" },
      name: "Local design audit",
      description: "Found in a local Claude Code skill directory.",
      management: "detected",
      enabled: true,
      canDisable: true,
    },
  ],
  scopes: [
    { tool: "claude-code", kind: "skill", status: "ready" },
    { tool: "claude-code", kind: "mcp", status: "ready" },
    { tool: "codex", kind: "skill", status: "ready" },
    { tool: "codex", kind: "mcp", status: "ready" },
  ],
  truncated: false,
};

function usageMetrics(
  requests: number,
  tokens: number,
  cost: string,
): UsageMetrics {
  return {
    requests,
    estimatedCostUsd: cost,
    tokens,
    successRatePercent: 98.4,
    cacheHitRatePercent: 46.2,
  };
}

const USAGE: UsageOverview = {
  periodDays: 30,
  startDate: "2026-07-27",
  endDate: "2026-08-25",
  summary: usageMetrics(1_284, 8_642_000, "42.370000"),
  byTool: [
    { tool: "claude-code", metrics: usageMetrics(612, 4_120_000, "21.420000") },
    { tool: "codex", metrics: usageMetrics(438, 3_110_000, "13.650000") },
    { tool: "gemini-cli", metrics: usageMetrics(234, 1_412_000, "7.300000") },
  ],
  trend: [
    ["2026-08-19", 112, "3.810000", 742_000],
    ["2026-08-20", 148, "4.620000", 936_000],
    ["2026-08-21", 176, "5.420000", 1_104_000],
    ["2026-08-22", 194, "6.180000", 1_238_000],
    ["2026-08-23", 218, "7.050000", 1_441_000],
    ["2026-08-24", 205, "7.260000", 1_398_000],
    ["2026-08-25", 231, "8.030000", 1_783_000],
  ].map(([date, requests, estimatedCostUsd, tokens]) => ({
    date: String(date),
    requests: Number(requests),
    estimatedCostUsd: String(estimatedCostUsd),
    tokens: Number(tokens),
  })),
};

const SESSIONS: SessionList = {
  items: [
    {
      reference: "c".repeat(64),
      tool: "codex",
      title: "Prepare the signed desktop release",
      preview: "Verify notarization, packaging and updater signatures.",
      projectName: "AI Manager",
      createdAt: 1_777_000_000,
      lastActiveAt: 1_777_000_900,
      resumable: true,
    },
    {
      reference: "a".repeat(64),
      tool: "claude-code",
      title: "Polish the settings experience",
      preview: "Audit keyboard flow, dark mode and narrow layouts.",
      projectName: "AI Manager",
      createdAt: 1_776_900_000,
      lastActiveAt: 1_776_901_200,
      resumable: true,
    },
    {
      reference: "b".repeat(64),
      tool: "gemini-cli",
      title: "Review the provider configuration",
      preview: "Compare local configuration precedence and overrides.",
      projectName: "Shared Platform",
      createdAt: 1_776_800_000,
      lastActiveAt: 1_776_804_000,
      resumable: false,
    },
  ],
  totalCount: 3,
  limited: false,
};

const NETWORK_PROXY: NetworkProxySettings = {
  configured: true,
  url: "http://127.0.0.1:7890",
  protected: false,
};

const BACKUPS: BackupList = {
  files: [
    {
      name: "ai-manager-2026-08-25.db",
      createdAt: "2026-08-25T02:00:00Z",
      sizeBytes: 2_408_448,
    },
  ],
};

const IMPORT_PREVIEW: ImportPreview = {
  available: false,
  summary: { services: 0, mcpServers: 0, skills: 0 },
};

const UPDATE_STATUS: UpdateStatus = {
  currentVersion: "0.1.0",
  availableVersion: null,
  channelReady: false,
  phase: "unconfigured",
  downloadedBytes: 0,
  totalBytes: null,
  attempt: 0,
  maxAttempts: 0,
};

export function seedAppShellGallery(client: QueryClient): void {
  client.setQueryData(settingsKeys.current(), SETTINGS);
  client.setQueryData(toolKeys.list(), GALLERY_TOOLS);
  client.setQueryData(operationKeys.list(), []);
  client.setQueryData(desktopAppKeys.list(), DESKTOP_APPS);
  client.setQueryData(
    healthKeys.snapshot(GALLERY_TOOLS.map((tool) => tool.id)),
    HEALTH,
  );
  client.setQueryData<ProviderConnectivity>(healthKeys.connectivity(), {
    "claude-code": {
      "anthropic-official": {
        providerId: "anthropic-official",
        reachability: "operational",
        responseTimeMs: 184,
        httpStatus: 200,
      },
    },
  });
  client.setQueryData(providerKeys.list("claude-code"), PROVIDERS);
  client.setQueryData(
    providerKeys.connectionProfile("claude-code"),
    CONNECTION_PROFILE,
  );
  client.setQueryData(
    providerKeys.runtimeContext("claude-code"),
    RUNTIME_CONTEXT,
  );
  client.setQueryData(routingKeys.overview(), ROUTING);
  client.setQueryData(extensionKeys.localInventory(), LOCAL_EXTENSIONS);
  client.setQueryData(
    extensionKeys.list("claude-code", "skill"),
    MANAGED_SKILLS,
  );
  client.setQueryData(usageKeys.overview(), USAGE);
  client.setQueryData(sessionKeys.list("", null), SESSIONS);
  client.setQueryData(networkProxyKeys.current(), NETWORK_PROXY);
  client.setQueryData(backupKeys.list(), BACKUPS);
  client.setQueryData(importKeys.preview(), IMPORT_PREVIEW);
  client.setQueryData(updateKeys.status(), UPDATE_STATUS);
}
