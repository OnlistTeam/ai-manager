import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  networkProxyKeys,
  useNetworkProxy,
  useSaveNetworkProxy,
} from "@/entities/network-proxy";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const direct = { configured: false, url: null, protected: false };
const proxied = {
  configured: true,
  url: "http://127.0.0.1:7890",
  protected: false,
};

describe("network proxy queries", () => {
  it("loads the safe setting once across settings-page remounts", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, () => {
        calls += 1;
        return HttpResponse.json(direct);
      }),
    );
    const client = createTestQueryClient();
    const first = renderHook(() => useNetworkProxy(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    first.unmount();

    const second = renderHook(() => useNetworkProxy(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(client.getQueryData(networkProxyKeys.current())).toEqual(direct);
    expect(calls).toBe(1);
  });

  it("replaces the cache with the backend read-back after save", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_network_proxy_save`, () =>
        HttpResponse.json(proxied),
      ),
    );
    const client = createTestQueryClient();
    client.setQueryData(networkProxyKeys.current(), direct);
    const { result } = renderHook(() => useSaveNetworkProxy(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate(proxied.url);
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(networkProxyKeys.current())).toEqual(proxied);
  });
});
