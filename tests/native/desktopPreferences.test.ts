import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { native, NativeError } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const preferences = {
  launchOnStartup: false,
  silentStartup: false,
  showInTray: true,
  minimizeToTrayOnClose: true,
};

describe("native.desktopPreferences", () => {
  it("reads the strict desktop preference projection", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_preferences_get`, () =>
        HttpResponse.json(preferences),
      ),
    );
    await expect(native.desktopPreferences.get()).resolves.toEqual(preferences);
  });

  it("sends a complete, safe preference choice", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_preferences_save`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json(preferences);
        },
      ),
    );
    await native.desktopPreferences.save(preferences);
    expect(body).toEqual({ settings: preferences });
  });

  it.each([
    { ...preferences, privatePath: "/Users/alice" },
    { ...preferences, showInTray: false },
    { ...preferences, launchOnStartup: false, silentStartup: true },
  ])("rejects expanded or unsafe native data", async (response) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_preferences_get`, () =>
        HttpResponse.json(response),
      ),
    );
    await expect(native.desktopPreferences.get()).rejects.toBeInstanceOf(
      NativeError,
    );
  });
});
