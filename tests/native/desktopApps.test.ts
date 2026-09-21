import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { native, NativeError } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const codexApp = {
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
};

describe("native.desktopApps", () => {
  it("reads only the safe desktop application projection", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_apps_list`,
        async ({ request }) => {
          expect(await request.json()).toEqual({});
          return HttpResponse.json([codexApp]);
        },
      ),
    );
    await expect(native.desktopApps.list()).resolves.toEqual([codexApp]);
  });

  it("launches with only the stable app id", async () => {
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
    await expect(native.desktopApps.launch("codex-app")).resolves.toBe(
      "launched",
    );
    expect(body).toEqual({ app: "codex-app" });
  });

  it("opens an official vendor page without accepting a renderer URL", async () => {
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
    await expect(
      native.desktopApps.openOfficialDownload("codex-app"),
    ).resolves.toEqual({ handoff: "directOfficialPackage" });
    expect(body).toEqual({ app: "codex-app" });
  });

  it("opens the OS uninstall handoff using only the stable app id", async () => {
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
    await expect(native.desktopApps.openUninstall("codex-app")).resolves.toBe(
      "opened",
    );
    expect(body).toEqual({ app: "codex-app" });
  });

  it.each([
    {
      label: "native paths",
      response: [{ ...codexApp, bundlePath: "/Applications/Codex.app" }],
    },
    {
      label: "inconsistent launch state",
      response: [{ ...codexApp, canLaunch: true, status: "notInstalled" }],
    },
    {
      label: "unsafe versions",
      response: [{ ...codexApp, version: "1.0<script>" }],
    },
    {
      label: "unsafe latest versions",
      response: [{ ...codexApp, latestVersion: "1.0<script>" }],
    },
    { label: "duplicate identities", response: [codexApp, codexApp] },
  ])("rejects $label in inventory", async ({ response }) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json(response),
      ),
    );
    await expect(native.desktopApps.list()).rejects.toBeInstanceOf(NativeError);
  });
});
