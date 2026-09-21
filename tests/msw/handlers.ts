import { http, HttpResponse, type JsonBodyType } from "msw";
const TAURI_ENDPOINT = "http://tauri.local";

const withJson = async <T>(request: Request): Promise<T> => {
  try {
    const body = await request.text();
    if (!body) return {} as T;
    return JSON.parse(body) as T;
  } catch {
    return {} as T;
  }
};

const success = <T extends JsonBodyType>(payload: T) =>
  HttpResponse.json(payload);

export const handlers = [
  http.post(`${TAURI_ENDPOINT}/app_operations_list`, () => success([])),
  // Checking the latest version is what every page with a mounted tool list
  // fires in the background. Default answer is "nothing found," so tests see
  // whatever each mocks in app_tools_list — the merge itself is tested
  // separately in tests/entities/tool/queries.test.tsx.
  http.post(`${TAURI_ENDPOINT}/app_tools_check_versions`, () => success([])),
  http.post(`${TAURI_ENDPOINT}/app_desktop_preferences_get`, () =>
    success({
      launchOnStartup: false,
      silentStartup: false,
      showInTray: true,
      minimizeToTrayOnClose: true,
    }),
  ),
  http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, () =>
    success({ configured: false, url: null, protected: false }),
  ),
  http.post(
    `${TAURI_ENDPOINT}/app_desktop_preferences_save`,
    async ({ request }) => {
      const { settings } = await withJson<{ settings: JsonBodyType }>(request);
      return success(settings);
    },
  ),
  http.post(`${TAURI_ENDPOINT}/app_update_start`, () =>
    success({
      currentVersion: "0.1.0",
      availableVersion: null,
      channelReady: false,
      phase: "unconfigured",
      downloadedBytes: 0,
      totalBytes: null,
      attempt: 0,
      maxAttempts: 0,
    }),
  ),
  http.post(`${TAURI_ENDPOINT}/app_update_status`, () =>
    success({
      currentVersion: "0.1.0",
      availableVersion: null,
      channelReady: false,
      phase: "unconfigured",
      downloadedBytes: 0,
      totalBytes: null,
      attempt: 0,
      maxAttempts: 0,
    }),
  ),
  http.post(`${TAURI_ENDPOINT}/app_update_install_and_restart`, () =>
    success(true),
  ),
  http.post(`${TAURI_ENDPOINT}/app_update_open_download_page`, () =>
    success(true),
  ),
  http.post(`${TAURI_ENDPOINT}/app_backup_schedule_get`, () =>
    success({ automatic: true, retainCount: 10 }),
  ),
  http.post(
    `${TAURI_ENDPOINT}/app_backup_schedule_save`,
    async ({ request }) => {
      const { schedule } = await withJson<{ schedule: JsonBodyType }>(request);
      return success(schedule);
    },
  ),
  http.post(`${TAURI_ENDPOINT}/app_backup_export`, () =>
    success({ status: "cancelled" }),
  ),
  http.post(`${TAURI_ENDPOINT}/app_backup_import`, () =>
    success({ status: "cancelled" }),
  ),
  http.post(
    `${TAURI_ENDPOINT}/app_tool_version_history`,
    async ({ request }) => {
      const { tool } = await withJson<{ tool: string }>(request);
      return success({ tool, previousVersion: null, events: [] });
    },
  ),
  http.post(
    `${TAURI_ENDPOINT}/app_provider_runtime_context`,
    async ({ request }) => {
      const { tool } = await withJson<{ tool: string }>(request);
      return success({
        tool,
        liveConfigPaths: ["~/.config/ai-manager/tool-config"],
        resources: [
          {
            id: "global-instructions",
            kind: "instructions",
            scope: "global",
            path: "~/.config/ai-manager/AGENTS.md",
            exists: true,
            action: "edit",
            sizeBytes: 1_024,
            measurementLimited: false,
          },
        ],
        storage: {
          totalBytes: 1_024,
          sessionBytes: 0,
          sessionCount: 0,
          measurementLimited: false,
        },
        effectiveConnection: null,
      });
    },
  ),
  http.post(`${TAURI_ENDPOINT}/app_provider_runtime_resource_open`, () =>
    success("editorOpened"),
  ),
  http.post(
    `${TAURI_ENDPOINT}/app_provider_activation_prepare`,
    async ({ request }) => {
      const { tool, provider } = await withJson<{
        tool: string;
        provider: string;
      }>(request);
      const selected = {
        id: provider,
        tool,
        name: provider,
        kind: "custom",
        active: true,
        baseUrl: null,
        apiKey: null,
        websiteUrl: null,
        testable: false,
        canRemove: false,
      };
      return success({
        status: "notChecked",
        originProviderId: provider,
        activeProviderId: provider,
        providers: [selected],
        checks: [],
      });
    },
  ),
  http.post(`${TAURI_ENDPOINT}/app_provider_launch_prepare`, () =>
    success({
      status: "notChecked",
      originProviderId: null,
      activeProviderId: null,
      providers: [],
      checks: [],
    }),
  ),
  http.post(
    `${TAURI_ENDPOINT}/app_provider_next_healthy`,
    async ({ request }) => {
      const { tool, failedProvider } = await withJson<{
        tool: string;
        failedProvider: string;
      }>(request);
      return success({
        status: "unreachable",
        originProviderId: failedProvider,
        activeProviderId: null,
        providers: [
          {
            id: failedProvider,
            tool,
            name: failedProvider,
            kind: "custom",
            active: false,
            baseUrl: null,
            apiKey: null,
            websiteUrl: null,
            testable: true,
            canRemove: true,
          },
        ],
        checks: [],
      });
    },
  ),
  http.post(`${TAURI_ENDPOINT}/app_extensions_local_inventory`, () =>
    success({ items: [], scopes: [], truncated: false }),
  ),
  http.post(`${TAURI_ENDPOINT}/app_skill_updates_check`, () => success([])),
  http.post(`${TAURI_ENDPOINT}/app_skill_backups_list`, () => success([])),
  http.post(`${TAURI_ENDPOINT}/app_skill_repositories_list`, () => success([])),
  http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_overview`, () =>
    success({
      files: [],
      existingFiles: 0,
      dailyMemoryCount: 0,
      dailyMemoryBytes: 0,
      totalBytes: 0,
      limited: false,
    }),
  ),
  http.post(`${TAURI_ENDPOINT}/app_openclaw_daily_memories`, () =>
    success({ items: [], totalCount: 0, totalBytes: 0, limited: false }),
  ),
  http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_open_directory`, () =>
    success(null),
  ),
];
