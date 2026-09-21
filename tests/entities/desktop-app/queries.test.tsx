import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  desktopAppKeys,
  useDesktopApps,
  useLaunchDesktopApp,
  useOpenDesktopAppOfficialDownload,
  useOpenDesktopAppUninstall,
} from "@/entities/desktop-app";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const inventory = [
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
];

describe("desktop app queries", () => {
  it("loads and caches the native inventory", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json(inventory),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useDesktopApps(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(desktopAppKeys.list())).toEqual(inventory);
  });

  it("passes the selected desktop app id to the launch mutation", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_app_launch`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json("launched");
        },
      ),
    );
    const { result } = renderHook(() => useLaunchDesktopApp(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate("codex-app");
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(body).toEqual({ app: "codex-app" });
  });

  it("passes only the selected id to the official-download mutation", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_app_open_official_download`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({ handoff: "directOfficialPackage" });
        },
      ),
    );
    const { result } = renderHook(() => useOpenDesktopAppOfficialDownload(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate("claude-desktop");
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(body).toEqual({ app: "claude-desktop" });
  });

  it("passes only the selected id to the uninstall-handoff mutation", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_app_open_uninstall`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json("opened");
        },
      ),
    );
    const { result } = renderHook(() => useOpenDesktopAppUninstall(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate("codex-app");
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(body).toEqual({ app: "codex-app" });
  });
});
