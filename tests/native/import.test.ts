import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { native, type ImportPreview } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const PREVIEW: ImportPreview = {
  available: true,
  summary: { services: 12, mcpServers: 8, skills: 15 },
};

describe("native.importExisting", () => {
  it("previews only counts and availability", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json(PREVIEW),
      ),
    );
    await expect(native.importExisting.preview()).resolves.toEqual(PREVIEW);
  });

  it("runs without sending source data and parses the imported counts", async () => {
    let received: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_run`, async ({ request }) => {
        received = await request.json();
        return HttpResponse.json({ imported: PREVIEW.summary });
      }),
    );
    await expect(native.importExisting.run()).resolves.toEqual({
      imported: PREVIEW.summary,
    });
    expect(received).toEqual({});
  });

  it("rejects a response that tries to expose source configuration", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({
          ...PREVIEW,
          settingsConfig: { apiKey: "secret" },
        }),
      ),
    );
    await expect(native.importExisting.preview()).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });
  });

  it("rejects fractional or negative counts", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_run`, () =>
        HttpResponse.json({
          imported: { services: -1, mcpServers: 1.5, skills: 0 },
        }),
      ),
    );
    await expect(native.importExisting.run()).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });
  });
});
