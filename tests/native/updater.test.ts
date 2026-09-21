import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  checkForUpdate,
  installAppUpdateAndRestart,
  openAppDownloadPage,
  startAppUpdate,
} from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

describe("native application updater", () => {
  it("starts the shared backend state machine without exposing updater resources", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, async ({ request }) => {
        seen.push(await request.json());
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

    await expect(checkForUpdate()).resolves.toMatchObject({
      currentVersion: "1.0.0",
      channelReady: false,
      phase: "unconfigured",
    });
    await expect(startAppUpdate(true)).resolves.toMatchObject({
      phase: "unconfigured",
    });
    expect(seen).toEqual([{ force: false }, { force: true }]);
  });

  it("asks the backend to install only the already verified package", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_update_install_and_restart`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json(true);
        },
      ),
    );

    await expect(installAppUpdateAndRestart()).resolves.toBe(true);
    expect(seen).toEqual([{}]);
  });

  it("opens only the backend-owned official download page", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_update_open_download_page`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json(true);
        },
      ),
    );

    await expect(openAppDownloadPage()).resolves.toBe(true);
    expect(seen).toEqual([{}]);
  });

  it("rejects malformed progress instead of trusting an unchecked payload", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () =>
        HttpResponse.json({
          currentVersion: "1.0.0",
          availableVersion: "1.1.0",
          channelReady: true,
          phase: "ready",
          downloadedBytes: -1,
          totalBytes: 10,
          attempt: 1,
          maxAttempts: 3,
        }),
      ),
    );

    await expect(checkForUpdate()).rejects.toMatchObject({
      code: "INTERNAL",
    });
  });
});
