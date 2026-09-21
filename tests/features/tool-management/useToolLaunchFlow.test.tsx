import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import type { Tool } from "@/entities/tool";
import { useToolLaunchFlow } from "@/features/tool-management";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const toastMocks = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn() }));
vi.mock("sonner", () => ({ toast: toastMocks }));

const TOOL: Tool = {
  id: "codex",
  name: "Codex CLI",
  descriptionKey: "tool.codex.description",
  status: "installed",
  version: "1.0.0",
  latestVersion: null,
  capabilities: {
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
  },
  sessionsInsideSettings: false,
  environment: "macos",
};

const origin = {
  id: "origin",
  tool: "codex",
  name: "Current Relay",
  kind: "custom",
  active: true,
  baseUrl: "https://origin.example.test",
  apiKey: "sk-ant-api03-abcdefghijkl1234",
  websiteUrl: null,
  testable: true,
  canRemove: false,
};

const backup = {
  ...origin,
  id: "backup",
  name: "Backup Relay",
  active: false,
  canRemove: true,
};

describe("useToolLaunchFlow provider preflight", () => {
  it("does not probe before confirmation and recovers within the second action", async () => {
    const prepared: unknown[] = [];
    const recovered: unknown[] = [];
    const launched: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_launch_prepare`,
        async ({ request }) => {
          prepared.push(await request.json());
          return HttpResponse.json({
            status: "unreachable",
            originProviderId: "origin",
            activeProviderId: "origin",
            providers: [origin, backup],
            checks: [
              {
                providerId: "origin",
                reachability: "failed",
                responseTimeMs: null,
                httpStatus: null,
              },
            ],
          });
        },
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_provider_next_healthy`,
        async ({ request }) => {
          recovered.push(await request.json());
          return HttpResponse.json({
            status: "failedOver",
            originProviderId: "origin",
            activeProviderId: "backup",
            providers: [
              { ...origin, active: false, canRemove: true },
              { ...backup, active: true, canRemove: false },
            ],
            checks: [
              {
                providerId: "backup",
                reachability: "operational",
                responseTimeMs: 58,
                httpStatus: 200,
              },
            ],
          });
        },
      ),
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
        launched.push(await request.json());
        return HttpResponse.json("launched");
      }),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useToolLaunchFlow(), {
      wrapper: withQueryClient(client),
    });

    act(() => result.current.openTool(TOOL));
    expect(prepared).toEqual([]);
    expect(recovered).toEqual([]);
    expect(launched).toEqual([]);

    act(() => result.current.confirm("choose"));

    await waitFor(() =>
      expect(result.current.providerRecovery?.providerName).toBe(
        "Current Relay",
      ),
    );
    expect(prepared).toEqual([{ tool: "codex" }]);
    expect(launched).toEqual([]);
    expect(result.current.providerRecovery?.canTryNext).toBe(true);

    act(() => result.current.providerRecovery?.onTryNext());

    await waitFor(() =>
      expect(launched).toEqual([{ tool: "codex", directoryMode: "choose" }]),
    );
    expect(recovered).toEqual([{ tool: "codex", failedProvider: "origin" }]);
    expect(result.current.tool).toBeNull();
    expect(toastMocks.success).toHaveBeenCalledTimes(2);
  });
});
