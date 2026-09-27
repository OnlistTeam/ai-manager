import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { native, NativeError } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const SYSTEM = {
  mode: "auto",
  url: null,
  protected: false,
  inUse: "http://127.0.0.1:7890",
  source: "system",
};
const CUSTOM = {
  mode: "custom",
  url: "socks5://127.0.0.1:1080",
  protected: false,
  inUse: "socks5://127.0.0.1:1080",
  source: "custom",
};
const OFF = {
  mode: "off",
  url: null,
  protected: false,
  inUse: null,
  source: "off",
};

describe("native.networkProxy", () => {
  it("reads only the safe product projection", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_network_proxy_get`,
        async ({ request }) => {
          expect(await request.json()).toEqual({});
          return HttpResponse.json(SYSTEM);
        },
      ),
    );

    await expect(native.networkProxy.get()).resolves.toEqual(SYSTEM);
  });

  it.each([
    ["custom", "socks5://127.0.0.1:1080", CUSTOM],
    ["off", null, OFF],
  ] as const)("sends the %s choice", async (mode, url, response) => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_network_proxy_save`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json(response);
        },
      ),
    );

    await native.networkProxy.save(mode, url);
    expect(body).toEqual({ mode, url });
  });

  it.each([
    { ...CUSTOM, url: "http://user:secret@127.0.0.1:7890" },
    { ...CUSTOM, url: null },
    { ...SYSTEM, url: "http://127.0.0.1:7890" },
    { ...SYSTEM, inUse: "http://user:secret@proxy.example.com:8080" },
    { ...SYSTEM, source: "none" },
    { ...OFF, inUse: "http://127.0.0.1:7890" },
    { ...CUSTOM, url: null, inUse: null, protected: true, password: "x" },
  ])("rejects an inconsistent or expanded response", async (response) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, () =>
        HttpResponse.json(response),
      ),
    );
    await expect(native.networkProxy.get()).rejects.toBeInstanceOf(NativeError);
  });
});
