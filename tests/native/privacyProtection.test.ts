import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { native, NativeError } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const STORED = {
  maskSecrets: true,
  maskPersonal: false,
  words: ["Project Kite", "张三"],
};

describe("native.privacyProtection", () => {
  it("reads the choices without sending anything", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_privacy_protection_get`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json(STORED);
        },
      ),
    );

    await expect(native.privacyProtection.get()).resolves.toEqual(STORED);
    expect(body).toEqual({});
  });

  it.each([
    { maskSecrets: false },
    { maskPersonal: true },
    { words: ["acme", "kite"] },
    { words: [] },
  ])(
    "sends only the changed field %o and returns the read-back",
    async (patch) => {
      let body: unknown;
      const saved = { ...STORED, ...patch };
      server.use(
        http.post(
          `${TAURI_ENDPOINT}/app_privacy_protection_set`,
          async ({ request }) => {
            body = await request.json();
            return HttpResponse.json(saved);
          },
        ),
      );

      await expect(native.privacyProtection.set(patch)).resolves.toEqual(saved);
      expect(body).toEqual({ patch });
    },
  );

  it.each([
    {},
    { enabled: true },
    { maskSecrets: "yes", maskPersonal: false, words: [] },
    { maskSecrets: true, maskPersonal: false },
    { maskSecrets: true, maskPersonal: false, words: "acme" },
    { maskSecrets: true, maskPersonal: false, words: [1] },
    { ...STORED, maskedValues: ["sk-secret"] },
    { ...STORED, placeholder: "{{API_KEY_abcdefgh}}" },
  ])("rejects a reply outside the choices-only contract %#", async (reply) => {
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
