import { QueryClient } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { warmSessionEnvironment } from "@/app/warmSessionEnvironment";
import { backupKeys } from "@/entities/backup";
import { desktopAppKeys } from "@/entities/desktop-app";
import { extensionKeys } from "@/entities/extension";
import { healthKeys } from "@/entities/health";
import { networkProxyKeys } from "@/entities/network-proxy";
import { providerKeys } from "@/entities/provider";
import { sessionKeys } from "@/entities/session";
import { settingsKeys } from "@/entities/settings";
import { skillRepositoryKeys } from "@/entities/skill-repository";
import { skillBackupKeys } from "@/entities/skill-backup";
import { skillUpdateKeys } from "@/entities/skill-update";
import { toolKeys } from "@/entities/tool";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const tools = [
  {
    id: "codex",
    name: "Codex CLI",
    descriptionKey: "tools.description.codex",
    status: "installed",
    version: "1.2.3",
    latestVersion: null,
    capabilities: {
      canInstall: true,
      canUpdate: true,
      canUninstall: true,
      canRepair: true,
      canManageProvider: true,
      canManageMcp: true,
      canManageSkills: true,
      canManagePrompts: true,
      canManageVersion: true,
      canLaunch: true,
    },
    sessionsInsideSettings: false,
    environment: null,
  },
  {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "tools.description.claudeCode",
    status: "installed",
    version: "2.3.4",
    latestVersion: null,
    capabilities: {
      canInstall: true,
      canUpdate: true,
      canUninstall: true,
      canRepair: true,
      canManageProvider: true,
      canManageMcp: true,
      canManageSkills: true,
      canManagePrompts: true,
      canManageVersion: true,
      canLaunch: true,
    },
    sessionsInsideSettings: false,
    environment: null,
  },
] as const;

const desktopApps = [
  {
    id: "codex-app",
    name: "ChatGPT / Codex",
    status: "installed",
    version: "26.527.60818",
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
] as const;

const health = {
  providers: [],
  configs: [],
  mcp: { total: 0, enabled: 0 },
};

const backups = { files: [] };

const networkProxy = { configured: false, url: null, protected: false };

describe("warmSessionEnvironment", () => {
  it("warms each expensive local inventory once and reuses it for the session", async () => {
    const calls = {
      tools: 0,
      settings: 0,
      backups: 0,
      apps: 0,
      health: 0,
      extensions: 0,
      scopedExtensions: 0,
      importPreview: 0,
      providers: 0,
      connection: 0,
      runtime: 0,
      networkProxy: 0,
      skillUpdates: 0,
      skillBackups: 0,
      skillRepositories: 0,
      sessions: 0,
    };
    const providerScopes: string[] = [];
    const connectionScopes: string[] = [];
    const runtimeScopes: string[] = [];
    const extensionScopes: string[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () => {
        calls.tools += 1;
        return HttpResponse.json(tools);
      }),
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () => {
        calls.apps += 1;
        return HttpResponse.json(desktopApps);
      }),
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () => {
        calls.backups += 1;
        return HttpResponse.json(backups);
      }),
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, () => {
        calls.sessions += 1;
        return HttpResponse.json({ items: [], totalCount: 0, limited: false });
      }),
      http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, () => {
        calls.networkProxy += 1;
        return HttpResponse.json(networkProxy);
      }),
      http.post(`${TAURI_ENDPOINT}/app_skill_updates_check`, () => {
        calls.skillUpdates += 1;
        return HttpResponse.json([
          { id: "anthropics/skills:review", name: "Review" },
        ]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_skill_backups_list`, () => {
        calls.skillBackups += 1;
        return HttpResponse.json([
          {
            id: "b".repeat(64),
            name: "Review",
            description: null,
            createdAt: 1_787_689_200,
            conflicts: false,
          },
        ]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_skill_repositories_list`, () => {
        calls.skillRepositories += 1;
        return HttpResponse.json([
          {
            id: "a".repeat(64),
            owner: "anthropics",
            repository: "skills",
            branch: "main",
            enabled: true,
          },
        ]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () => {
        calls.settings += 1;
        return HttpResponse.json({
          advancedMode: false,
          importPromptSeen: false,
          toolScope: "codex",
          extensionKind: null,
          // A previous build persisted this wire value. Startup warming must
          // normalize it before it reaches the shared session cache.
          downloadStrategy: "chinaResilient",
        });
      }),
      http.post(`${TAURI_ENDPOINT}/app_extensions_local_inventory`, () => {
        calls.extensions += 1;
        return HttpResponse.json({ items: [], scopes: [], truncated: false });
      }),
      http.post(
        `${TAURI_ENDPOINT}/app_extensions_list`,
        async ({ request }) => {
          calls.scopedExtensions += 1;
          const body = (await request.json()) as {
            scope: { kind: string; id: string };
            kind: string;
          };
          extensionScopes.push(
            `${body.kind}:${body.scope.kind}:${body.scope.id}`,
          );
          return HttpResponse.json([]);
        },
      ),
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () => {
        calls.importPreview += 1;
        return HttpResponse.json({
          available: false,
          summary: { services: 0, mcpServers: 0, skills: 0 },
        });
      }),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () => {
        calls.health += 1;
        return HttpResponse.json(health);
      }),
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, async ({ request }) => {
        calls.providers += 1;
        const body = (await request.json()) as { tool: string };
        providerScopes.push(body.tool);
        return HttpResponse.json([]);
      }),
      http.post(
        `${TAURI_ENDPOINT}/app_provider_connection_profile`,
        async ({ request }) => {
          calls.connection += 1;
          const body = (await request.json()) as { tool: string };
          connectionScopes.push(body.tool);
          return HttpResponse.json({
            defaultPresetId: "official",
            modelRequired: false,
            baseUrlTakesNoVersion: false,
            presets: [
              {
                id: "official",
                serviceName: "OpenAI API",
                defaultName: "OpenAI",
                defaultModel: "gpt-5",
                baseUrl: "https://api.example.test",
                websiteUrl: "https://openai.com",
                apiKeyUrl: "https://platform.openai.com",
                official: true,
              },
            ],
          });
        },
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_provider_runtime_context`,
        async ({ request }) => {
          calls.runtime += 1;
          const body = (await request.json()) as { tool: string };
          runtimeScopes.push(body.tool);
          return HttpResponse.json({
            tool: body.tool,
            liveConfigPaths: [],
            resources: [],
            storage: {
              totalBytes: 0,
              sessionBytes: 0,
              sessionCount: 0,
              measurementLimited: false,
            },
            effectiveConnection: null,
          });
        },
      ),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });

    await warmSessionEnvironment(client);
    await warmSessionEnvironment(client);

    expect(calls).toEqual({
      tools: 1,
      settings: 1,
      backups: 1,
      apps: 1,
      health: 1,
      extensions: 1,
      scopedExtensions: 6,
      importPreview: 1,
      providers: 2,
      connection: 2,
      runtime: 2,
      networkProxy: 1,
      skillUpdates: 1,
      skillBackups: 1,
      skillRepositories: 1,
      sessions: 1,
    });
    expect(providerScopes.sort()).toEqual(["claude-code", "codex"]);
    expect(connectionScopes.sort()).toEqual(["claude-code", "codex"]);
    expect(runtimeScopes.sort()).toEqual(["claude-code", "codex"]);
    expect(extensionScopes.sort()).toEqual([
      "mcp:tool:claude-code",
      "mcp:tool:codex",
      "prompt:tool:claude-code",
      "prompt:tool:codex",
      "skill:tool:claude-code",
      "skill:tool:codex",
    ]);
    expect(client.getQueryData(toolKeys.list())).toEqual(tools);
    expect(client.getQueryData(settingsKeys.current())).toMatchObject({
      toolScope: "codex",
      downloadStrategy: "automatic",
    });
    expect(client.getQueryData(desktopAppKeys.list())).toEqual(desktopApps);
    expect(client.getQueryData(backupKeys.list())).toEqual(backups);
    expect(client.getQueryData(sessionKeys.list("", null))).toEqual({
      items: [],
      totalCount: 0,
      limited: false,
    });
    expect(client.getQueryData(networkProxyKeys.current())).toEqual(
      networkProxy,
    );
    expect(client.getQueryData(extensionKeys.localInventory())).toEqual({
      items: [],
      scopes: [],
      truncated: false,
    });
    expect(
      client.getQueryData(extensionKeys.list("tool:codex", "skill")),
    ).toEqual([]);
    expect(
      client.getQueryData(extensionKeys.list("tool:claude-code", "prompt")),
    ).toEqual([]);
    expect(
      client.getQueryData(healthKeys.snapshot(["claude-code", "codex"])),
    ).toEqual(health);
    expect(client.getQueryData(providerKeys.list("codex"))).toEqual([]);
    expect(client.getQueryData(providerKeys.list("claude-code"))).toEqual([]);
    expect(
      client.getQueryData(providerKeys.runtimeContext("codex")),
    ).toMatchObject({ tool: "codex" });
    expect(
      client.getQueryData(providerKeys.runtimeContext("claude-code")),
    ).toMatchObject({ tool: "claude-code" });
    expect(client.getQueryData(skillUpdateKeys.list())).toEqual([
      { id: "anthropics/skills:review", name: "Review" },
    ]);
    expect(client.getQueryData(skillBackupKeys.list())).toEqual([
      {
        id: "b".repeat(64),
        name: "Review",
        description: null,
        createdAt: 1_787_689_200,
        conflicts: false,
      },
    ]);
    expect(client.getQueryData(skillRepositoryKeys.list())).toEqual([
      {
        id: "a".repeat(64),
        owner: "anthropics",
        repository: "skills",
        branch: "main",
        enabled: true,
      },
    ]);
  });

  it("does not inspect an old import source after the prompt was handled", async () => {
    let importCalls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({
          advancedMode: false,
          importPromptSeen: true,
          toolScope: null,
          extensionKind: null,
          downloadStrategy: "automatic",
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.json(backups),
      ),
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, () =>
        HttpResponse.json({ items: [], totalCount: 0, limited: false }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, () =>
        HttpResponse.json(networkProxy),
      ),
      http.post(`${TAURI_ENDPOINT}/app_skill_updates_check`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_extensions_local_inventory`, () =>
        HttpResponse.json({ items: [], scopes: [], truncated: false }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        HttpResponse.json(health),
      ),
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () => {
        importCalls += 1;
        return HttpResponse.json({
          available: false,
          summary: { services: 0, mcpServers: 0, skills: 0 },
        });
      }),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });

    await warmSessionEnvironment(client);
    await warmSessionEnvironment(client);

    expect(importCalls).toBe(0);
  });
});
