import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { native, NativeError } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

describe("native.networkProxy", () => {
  it("reads only the safe product projection", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_network_proxy_get`,
        async ({ request }) => {
          expect(await request.json()).toEqual({});
          return HttpResponse.json({
            configured: true,
            url: "http://127.0.0.1:7890",
            protected: false,
          });
        },
      ),
    );

    await expect(native.networkProxy.get()).resolves.toEqual({
      configured: true,
      url: "http://127.0.0.1:7890",
      protected: false,
    });
  });

  it.each([
    ["http://127.0.0.1:7890", "http://127.0.0.1:7890"],
    [null, null],
  ])("sends the explicit proxy choice %s", async (url, expected) => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_network_proxy_save`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({
            configured: expected !== null,
            url: expected,
            protected: false,
          });
        },
      ),
    );

    await native.networkProxy.save(url);
    expect(body).toEqual({ url: expected });
  });

  it.each([
    {
      configured: true,
      url: "http://user:secret@127.0.0.1:7890",
      protected: false,
    },
    { configured: true, url: null, protected: false },
    { configured: false, url: "http://127.0.0.1:7890", protected: false },
    { configured: true, url: null, protected: true, password: "private" },
  ])("rejects an inconsistent or expanded response", async (response) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, () =>
        HttpResponse.json(response),
      ),
    );
    await expect(native.networkProxy.get()).rejects.toBeInstanceOf(NativeError);
  });
});
