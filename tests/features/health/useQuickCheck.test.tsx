import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { healthKeys, writeProviderConnectivity } from "@/entities/health";
import { toolKeys } from "@/entities/tool";
import { useQuickCheck } from "@/features/health";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const TOOL = {
  id: "claude-code",
  name: "Claude Code",
  descriptionKey: "tool.claude-code.description",
  status: "installed",
  version: "1.0.0",
  latestVersion: null,
  capabilities: {
    canInstall: true,
    canUpdate: true,
    canUninstall: true,
    canRepair: false,
    canManageProvider: true,
    canManageMcp: true,
    canManageSkills: true,
    canManagePrompts: true,
    canManageVersion: true,
    canLaunch: true,
  },
  sessionsInsideSettings: false,
  environment: "macos",
};

describe("useQuickCheck", () => {
  it("combines the tool query, read-only snapshot and prior manual cache", async () => {
    let healthRequest: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([
          TOOL,
          { ...TOOL, id: "codex", name: "Codex", status: "notInstalled" },
        ]),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_health_snapshot`,
        async ({ request }) => {
          healthRequest = await request.json();
          return HttpResponse.json({
            providers: [
              {
                tool: "claude-code",
                configured: true,
                configuredCount: 1,
                checkTargets: [{ providerId: "relay", name: "Relay" }],
              },
            ],
            configs: [{ tool: "claude-code", status: "readable" }],
            mcp: { total: 1, enabled: 1 },
          });
        },
      ),
    );
    const client = createTestQueryClient();
    writeProviderConnectivity(client, "claude-code", "relay", {
      providerId: "relay",
      reachability: "failed",
      responseTimeMs: null,
      httpStatus: null,
    });
    const { result } = renderHook(() => useQuickCheck(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(result.current.data).toBeDefined());

    expect(healthRequest).toEqual({ tools: ["claude-code"] });
    expect(result.current.data?.attentionCount).toBe(1);
    expect(result.current.checkedAt).toBeGreaterThan(0);
    expect(result.current.isError).toBe(false);
    expect(result.current.refreshError).toBe(false);
    expect(client.getQueryData(healthKeys.connectivity())).toBeDefined();
  });

  it("reports a snapshot failure instead of manufacturing Ready", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([TOOL]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        HttpResponse.text("boom", { status: 500 }),
      ),
    );
    const { result } = renderHook(() => useQuickCheck(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(result.current.data).toBeUndefined();
    expect(result.current.refreshError).toBe(false);
    expect(result.current.checkedAt).toBeNull();
  });

  it("keeps the last successful result when a manual refresh fails", async () => {
    let snapshotFails = false;
    let toolsDisappear = false;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json(toolsDisappear ? [] : [TOOL]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        snapshotFails
          ? HttpResponse.text("boom", { status: 500 })
          : HttpResponse.json({
              providers: [
                {
                  tool: "claude-code",
                  configured: true,
                  configuredCount: 1,
                  checkTargets: [],
                },
              ],
              configs: [{ tool: "claude-code", status: "readable" }],
              mcp: { total: 0, enabled: 0 },
            }),
      ),
    );
    const { result } = renderHook(() => useQuickCheck(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.data).toBeDefined());
    const successfulData = result.current.data;
    const successfulAt = result.current.checkedAt;

    toolsDisappear = true;
    snapshotFails = true;
    await act(async () => {
      await result.current.refetch();
    });
    await waitFor(() => expect(result.current.refreshError).toBe(true));

    expect(result.current.isError).toBe(false);
    expect(result.current.data).toEqual(successfulData);
    expect(result.current.data?.installedCount).toBe(1);
    expect(result.current.checkedAt).toBe(successfulAt);
  });

  it("rechecks the snapshot when a retained tool refresh recovers", async () => {
    let toolReads = 0;
    let snapshotReads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () => {
        toolReads += 1;
        return toolReads === 2
          ? HttpResponse.text("private tool path", { status: 500 })
          : HttpResponse.json([TOOL]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () => {
        snapshotReads += 1;
        return HttpResponse.json({
          providers: [
            {
              tool: "claude-code",
              configured: true,
              configuredCount: 1,
              checkTargets: [],
            },
          ],
          configs: [{ tool: "claude-code", status: "readable" }],
          mcp: { total: 0, enabled: 0 },
        });
      }),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useQuickCheck(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(snapshotReads).toBe(1);

    await act(async () => {
      await client.invalidateQueries({ queryKey: toolKeys.all });
    });
    await waitFor(() => expect(result.current.refreshError).toBe(true));

    await act(async () => {
      await result.current.refetch();
    });
    await waitFor(() => expect(result.current.refreshError).toBe(false));
    expect(toolReads).toBe(3);
    expect(snapshotReads).toBe(2);
  });
});

/**
 * Opening the app is not a request to go online. The local list is files on
 * this machine and is read as before; the latest-version lookup is the one
 * part that leaves the computer, and it waits to be asked (ADR-0043).
 */
describe("what opening the home page is allowed to do on its own", () => {
  it("reads the local list but does not check versions over the network", async () => {
    let versionChecks = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([TOOL]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_tools_check_versions`, () => {
        versionChecks += 1;
        return HttpResponse.json([TOOL]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        HttpResponse.json({
          providers: [],
          configs: [{ tool: "claude-code", status: "readable" }],
          mcp: { total: 0, enabled: 0 },
        }),
      ),
    );

    const { result } = renderHook(() => useQuickCheck(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(versionChecks).toBe(0);

    // Asking explicitly still runs it: the check is deferred, not removed.
    await result.current.recheck();
    await waitFor(() => expect(versionChecks).toBe(1));
  });
});
