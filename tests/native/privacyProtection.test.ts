import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { native, NativeError } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

describe("native.privacyProtection", () => {
  it("reads the switch without sending anything", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_privacy_protection_get`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({ enabled: true });
        },
      ),
    );

    await expect(native.privacyProtection.get()).resolves.toEqual({
      enabled: true,
    });
    expect(body).toEqual({});
  });

  it.each([true, false])(
    "sends the explicit choice %s and returns the read-back",
    async (enabled) => {
      let body: unknown;
      server.use(
        http.post(
          `${TAURI_ENDPOINT}/app_privacy_protection_set`,
          async ({ request }) => {
            body = await request.json();
            return HttpResponse.json({ enabled });
          },
        ),
      );

      await expect(native.privacyProtection.set(enabled)).resolves.toEqual({
        enabled,
      });
      expect(body).toEqual({ enabled });
    },
  );

  it.each([
    {},
    { enabled: "yes" },
    { enabled: true, maskedValues: ["sk-secret"] },
    { enabled: true, placeholder: "{{API_KEY_abcdefgh}}" },
  ])("rejects a reply outside the switch-only contract %#", async (reply) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_privacy_protection_get`, () =>
        HttpResponse.json(reply),
      ),
    );

    const error = await native.privacyProtection.get().catch((e) => e);
    expect(error).toBeInstanceOf(NativeError);
    expect(error.messageKey).toBe("error.native.responseSchemaMismatch");
  });
});
