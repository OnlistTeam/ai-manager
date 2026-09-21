import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";

// This file only cares about AppRoot's own wiring; the shell is replaced with
// a stub so the whole page tree doesn't get dragged in.
vi.mock("@/app/AppShell", () => ({ AppShell: () => <div>shell</div> }));

// Imported directly from the module to make sure this wiring test only
// evaluates AppRoot; AppShell is still loaded through the dynamic module
// stub above, keeping the page tree out of this test.
import { AppRoot } from "@/app/AppRoot";
import { updateKeys } from "@/entities/update";
import { server } from "../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const SETTINGS = {
  advancedMode: false,
  importPromptSeen: false,
  toolScope: null,
  extensionScope: null,
  extensionKind: null,
  downloadStrategy: "automatic",
  automaticProviderFailover: false,
  terminalApp: null,
};

describe("AppRoot", () => {
  let updateStarts = 0;

  beforeEach(async () => {
    window.localStorage.clear();
    updateStarts = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(SETTINGS),
      ),
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({
          available: false,
          summary: { services: 0, mcpServers: 0, skills: 0 },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () => {
        updateStarts += 1;
        return HttpResponse.json({
          currentVersion: "1.0.0",
          availableVersion: null,
          channelReady: false,
          phase: "unconfigured",
          downloadedBytes: 0,
          totalBytes: null,
          attempt: 0,
          maxAttempts: 0,
        });
      }),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, nav: en.nav, preferences: en.preferences },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("loads the fail-closed channel status when the product shell starts", async () => {
    const client = createTestQueryClient();
    render(<AppRoot />, { wrapper: withQueryClient(client) });
    await waitFor(() => expect(updateStarts).toBe(1));
    expect(client.getQueryData(updateKeys.status())).toEqual({
      currentVersion: "1.0.0",
      availableVersion: null,
      channelReady: false,
      phase: "unconfigured",
      downloadedBytes: 0,
      totalBytes: null,
      attempt: 0,
      maxAttempts: 0,
    });
  });

  it("never makes the first screen wait for update status", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_update_start`,
        () => new Promise<Response>(() => undefined),
      ),
    );
    render(<AppRoot />, { wrapper: withQueryClient(createTestQueryClient()) });
    expect(await screen.findByText("shell")).toBeInTheDocument();
  });

  it("holds the startup screen until the settings baseline has arrived", () => {
    // Every page below assumes the settings cache is populated, so the shell
    // may not mount before the real value comes back.
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_settings_get`,
        () => new Promise<Response>(() => undefined),
      ),
    );
    render(<AppRoot />, { wrapper: withQueryClient(createTestQueryClient()) });
    const startup = screen.getByRole("status", {
      name: "Starting AI Manager",
    });
    expect(
      within(startup).getByRole("heading", { name: en.nav.appName }),
    ).toBeInTheDocument();
    expect(within(startup).getByText(en.nav.localFirst)).toBeVisible();
    expect(startup).toHaveAttribute("data-route", "home");
    expect(startup).toHaveClass("app-window-canvas");
    const scene = startup.querySelector(".spatial-scene");
    expect(scene).toHaveAttribute("data-model", "environment");
    expect(scene).toHaveAttribute("data-loading", "true");
    expect(scene?.querySelector(".spatial-scene__glyph img")).not.toBeNull();
    expect(screen.queryByText("shell")).toBeNull();
  });

  it("opens the shell on the page the user was last on", async () => {
    window.localStorage.setItem("aimanager.route", "services");
    render(<AppRoot />, { wrapper: withQueryClient(createTestQueryClient()) });
    expect(await screen.findByText("shell")).toBeInTheDocument();
    expect(window.localStorage.getItem("aimanager.route")).toBe("services");
  });

  it("asks an existing user once when a compatible setup is found", async () => {
    let saved: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({
          available: true,
          summary: { services: 12, mcpServers: 8, skills: 15 },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        saved = await request.json();
        return HttpResponse.json({ ...SETTINGS, importPromptSeen: true });
      }),
    );
    render(<AppRoot />, { wrapper: withQueryClient(createTestQueryClient()) });
    expect(await screen.findByText("shell")).toBeInTheDocument();
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getAllByRole("button")).toHaveLength(2);
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.preferences.import.skip,
      }),
    );
    await waitFor(() =>
      expect(saved).toEqual({
        settings: { ...SETTINGS, importPromptSeen: true },
      }),
    );
  });

  it("does not mark the prompt seen when no source exists", async () => {
    let checked = false;
    let saved = false;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () => {
        checked = true;
        return HttpResponse.json({
          available: false,
          summary: { services: 0, mcpServers: 0, skills: 0 },
        });
      }),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, () => {
        saved = true;
        return HttpResponse.json(SETTINGS);
      }),
    );
    render(<AppRoot />, { wrapper: withQueryClient(createTestQueryClient()) });
    await waitFor(() => expect(checked).toBe(true));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(saved).toBe(false);
  });

  it("still opens the shell when the settings cannot be read", async () => {
    let saved = false;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.text("boom", { status: 500 }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({
          available: true,
          summary: { services: 12, mcpServers: 8, skills: 15 },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, () => {
        saved = true;
        return HttpResponse.json(SETTINGS);
      }),
    );
    render(<AppRoot />, { wrapper: withQueryClient(createTestQueryClient()) });
    expect(await screen.findByText("shell")).toBeInTheDocument();
    // There is no full-record snapshot to save the answer onto, so the startup
    // question is not asked at all rather than asked and then lost.
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(saved).toBe(false);
  });
});
