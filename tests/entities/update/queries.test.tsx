import { QueryClient } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { updateKeys, useUpdateStatus } from "@/entities/update";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const status = (
  phase: "unconfigured" | "checking" | "downloading" | "ready",
) => ({
  currentVersion: "1.0.0",
  availableVersion:
    phase === "downloading" || phase === "ready" ? "1.1.0" : null,
  channelReady: phase !== "unconfigured",
  phase,
  downloadedBytes: phase === "ready" ? 100 : phase === "downloading" ? 50 : 0,
  totalBytes: phase === "downloading" || phase === "ready" ? 100 : null,
  attempt: phase === "unconfigured" ? 0 : 1,
  maxAttempts: phase === "unconfigured" ? 0 : 3,
});

describe("useUpdateStatus", () => {
  it("starts the shared backend state only once for multiple consumers", async () => {
    let starts = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () => {
        starts += 1;
        return HttpResponse.json(status("unconfigured"));
      }),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const first = renderHook(() => useUpdateStatus(), { wrapper });
    const second = renderHook(() => useUpdateStatus(), { wrapper });

    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(starts).toBe(1);
    expect(client.getQueryData(updateKeys.status())).toEqual(
      status("unconfigured"),
    );
  });

  it("polls an active background download until the verified package is ready", async () => {
    let reads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () => {
        reads += 1;
        return HttpResponse.json(
          reads === 1
            ? status("checking")
            : reads === 2
              ? status("downloading")
              : status("ready"),
        );
      }),
    );
    const { result } = renderHook(() => useUpdateStatus(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    await waitFor(() => expect(result.current.data?.phase).toBe("ready"), {
      timeout: 2_500,
    });
    expect(reads).toBeGreaterThanOrEqual(3);
  });

  it("does not retry when the native update state cannot be read", async () => {
    const retrying = new QueryClient({
      defaultOptions: { queries: { retry: 3, retryDelay: 0 } },
    });
    let attempts = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () => {
        attempts += 1;
        return HttpResponse.text("private updater failure", { status: 500 });
      }),
    );
    const { result } = renderHook(() => useUpdateStatus(), {
      wrapper: withQueryClient(retrying),
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(attempts).toBe(1);
  });
});
