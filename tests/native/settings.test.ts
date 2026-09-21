import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  DEFAULT_PRODUCT_SETTINGS,
  native,
  type ProductSettings,
} from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const STORED: ProductSettings = {
  advancedMode: true,
  importPromptSeen: true,
  toolScope: "codex",
  extensionScope: { kind: "tool", id: "codex" },
  extensionKind: "mcp",
  downloadStrategy: "automatic",
  automaticProviderFailover: true,
  terminalApp: null,
};

describe("native.settings", () => {
  it("reads the stored preferences", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(STORED),
      ),
    );
    await expect(native.settings.get()).resolves.toEqual(STORED);
  });

  it.each(["officialOnly", "chinaResilient"])(
    "normalizes the legacy %s strategy at the native boundary",
    async (legacyStrategy) => {
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
          HttpResponse.json({
            ...STORED,
            downloadStrategy: legacyStrategy,
          }),
        ),
      );
      await expect(native.settings.get()).resolves.toEqual(STORED);
    },
  );

  it("migrates a pre-failover settings record to the conservative off value", async () => {
    const { automaticProviderFailover: _oldMissingField, ...olderRecord } =
      STORED;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(olderRecord),
      ),
    );

    await expect(native.settings.get()).resolves.toEqual({
      ...olderRecord,
      automaticProviderFailover: false,
    });
  });

  it("sends the whole record and hands back what the backend read again", async () => {
    let received: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        received = await request.json();
        return HttpResponse.json(STORED);
      }),
    );
    await expect(native.settings.save(STORED)).resolves.toEqual(STORED);
    // The parameter name is the contract: the Rust side's argument is literally named `settings`.
    expect(received).toEqual({ settings: STORED });
  });

  it("refuses a response that is missing a field", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({ advancedMode: true }),
      ),
    );
    await expect(native.settings.get()).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });
  });

  it("refuses a scope this build does not know", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({ ...STORED, toolScope: "claude" }),
      ),
    );
    await expect(native.settings.get()).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });
  });

  it("mirrors the Rust defaults field by field", () => {
    expect(DEFAULT_PRODUCT_SETTINGS).toEqual({
      advancedMode: false,
      importPromptSeen: false,
      toolScope: null,
      extensionScope: null,
      extensionKind: null,
      downloadStrategy: "automatic",
      automaticProviderFailover: false,
      terminalApp: null,
    });
  });
});
