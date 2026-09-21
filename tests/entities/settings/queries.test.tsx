import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  settingsKeys,
  useProductSettings,
  useSaveProductSettings,
  type ProductSettings,
} from "@/entities/settings";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const STORED: ProductSettings = {
  advancedMode: false,
  importPromptSeen: false,
  toolScope: "claude-code",
  extensionScope: { kind: "tool", id: "claude-code" },
  extensionKind: null,
  downloadStrategy: "automatic",
  automaticProviderFailover: false,
  terminalApp: null,
};

function failure() {
  return HttpResponse.text(
    JSON.stringify({
      code: "CONFIG_WRITE_FAILED",
      messageKey: "error.settings.saveFailed",
      technicalMessage: null,
      remediation: "error.remediation.retryOrViewDetails",
      contextId: null,
    }),
    { status: 500 },
  );
}

describe("product settings", () => {
  it("reads the stored record", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(STORED),
      ),
    );
    const { result } = renderHook(() => useProductSettings(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(STORED);
  });

  it("merges the patch onto the cache and sends the whole record", async () => {
    let received: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(STORED),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        received = await request.json();
        return HttpResponse.json({ ...STORED, advancedMode: true });
      }),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const read = renderHook(() => useProductSettings(), { wrapper });
    await waitFor(() => expect(read.result.current.isSuccess).toBe(true));

    const { result } = renderHook(() => useSaveProductSettings(), { wrapper });
    result.current.mutate({ advancedMode: true });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(received).toEqual({ settings: { ...STORED, advancedMode: true } });
    // What gets written back is the record the backend **echoes**, not one the frontend merged itself.
    expect(client.getQueryData(settingsKeys.current())).toEqual({
      ...STORED,
      advancedMode: true,
    });
  });

  it("moves the cached value before the round trip finishes", async () => {
    let release: (() => void) | undefined;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(STORED),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async () => {
        await new Promise<void>((resolve) => {
          release = resolve;
        });
        return HttpResponse.json({ ...STORED, toolScope: "codex" });
      }),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const read = renderHook(() => useProductSettings(), { wrapper });
    await waitFor(() => expect(read.result.current.isSuccess).toBe(true));

    const { result } = renderHook(() => useSaveProductSettings(), { wrapper });
    result.current.mutate({ toolScope: "codex" });

    // The tab must move immediately. Waiting for a round trip before moving it makes the user think their click didn't register.
    await waitFor(() =>
      expect(
        client.getQueryData<ProductSettings>(settingsKeys.current())?.toolScope,
      ).toBe("codex"),
    );
    release?.();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
  });

  it("puts the old record back when the write fails", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(STORED),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, () => failure()),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const read = renderHook(() => useProductSettings(), { wrapper });
    await waitFor(() => expect(read.result.current.isSuccess).toBe(true));

    const { result } = renderHook(() => useSaveProductSettings(), { wrapper });
    result.current.mutate({ advancedMode: true });
    await waitFor(() => expect(result.current.isError).toBe(true));

    // A toggle stuck at the position the user pressed while the store holds a different value is worse than the toggle bouncing back on its own.
    expect(client.getQueryData(settingsKeys.current())).toEqual(STORED);
  });

  it("refuses to write when it has no snapshot to merge onto", async () => {
    // Final review C1: the backend does a full replace. Using defaults as the
    // baseline would fill in the fields we couldn't read with defaults — a
    // stray tab click after a failed settings read would wipe out the user's
    // real importPromptSeen value in the store.
    let saved = false;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.text("boom", { status: 500 }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, () => {
        saved = true;
        return HttpResponse.json(STORED);
      }),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const read = renderHook(() => useProductSettings(), { wrapper });
    await waitFor(() => expect(read.result.current.isError).toBe(true));

    const { result } = renderHook(() => useSaveProductSettings(), { wrapper });
    result.current.mutate({ advancedMode: true });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(saved).toBe(false);
  });

  it("leaves a failed read looking failed, not optimistically successful", async () => {
    // Final review I2: if the optimistic value stayed in the cache, the errored
    // query would be promoted to success, the error UI would disappear, and
    // staleTime: Infinity means it would never self-heal.
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.text("boom", { status: 500 }),
      ),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const read = renderHook(() => useProductSettings(), { wrapper });
    await waitFor(() => expect(read.result.current.isError).toBe(true));

    const { result } = renderHook(() => useSaveProductSettings(), { wrapper });
    result.current.mutate({ advancedMode: true });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(client.getQueryData(settingsKeys.current())).toBeUndefined();
    expect(read.result.current.isError).toBe(true);
  });

  it("runs overlapping saves one at a time and keeps every patch", async () => {
    // The backend does a full replace: with two concurrent saves, the later
    // write clobbers the earlier one, while the cache settles on whichever one
    // **returns** last. Each is independently non-deterministic, so the store
    // and the UI can end up showing two different values.
    const received: unknown[] = [];
    const releases: Array<() => void> = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(STORED),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        const body = (await request.json()) as { settings: ProductSettings };
        received.push(body);
        await new Promise<void>((resolve) => releases.push(resolve));
        return HttpResponse.json(body.settings);
      }),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const read = renderHook(() => useProductSettings(), { wrapper });
    await waitFor(() => expect(read.result.current.isSuccess).toBe(true));

    const first = renderHook(() => useSaveProductSettings(), { wrapper });
    const second = renderHook(() => useSaveProductSettings(), { wrapper });
    first.result.current.mutate({ toolScope: "codex" });
    second.result.current.mutate({ advancedMode: true });

    // Both optimistic values show up on screen immediately, but only the first save has actually been sent.
    await waitFor(() => expect(received).toHaveLength(1));
    expect(client.getQueryData(settingsKeys.current())).toEqual({
      ...STORED,
      toolScope: "codex",
      advancedMode: true,
    });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(received).toHaveLength(1);

    releases[0]?.();
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    // The first save's echoed response must not clobber the second save's optimistic value: the UI shouldn't revert and then jump forward again.
    expect(client.getQueryData(settingsKeys.current())).toEqual({
      ...STORED,
      toolScope: "codex",
      advancedMode: true,
    });

    // The second save resends on top of the first save's result, so the store
    // ends up with both changes. The first sent record carries at least its own
    // change; if the queued optimistic value has already landed in the cache by
    // then, it gets sent along too — that's still a change the user asked for,
    // so it's not wrong.
    await waitFor(() => expect(received).toHaveLength(2));
    expect(received[0]).toMatchObject({ settings: { toolScope: "codex" } });
    expect(received[1]).toEqual({
      settings: { ...STORED, toolScope: "codex", advancedMode: true },
    });

    releases[1]?.();
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(client.getQueryData(settingsKeys.current())).toEqual({
      ...STORED,
      toolScope: "codex",
      advancedMode: true,
    });
  });

  it("re-reads the stored record when the last of the overlapping saves fails", async () => {
    // The queued save's `previous` snapshot captures the earlier save's
    // optimistic value; once that earlier save fails and rolls back, the
    // snapshot is no longer the store's true value, so it has to be re-read.
    let reads = 0;
    let release: (() => void) | undefined;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () => {
        reads += 1;
        return HttpResponse.json(STORED);
      }),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async () => {
        // Only stall the first save so the second one genuinely queues behind it.
        if (release === undefined) {
          await new Promise<void>((resolve) => {
            release = resolve;
          });
        }
        return failure();
      }),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const read = renderHook(() => useProductSettings(), { wrapper });
    await waitFor(() => expect(read.result.current.isSuccess).toBe(true));
    expect(reads).toBe(1);

    const first = renderHook(() => useSaveProductSettings(), { wrapper });
    const second = renderHook(() => useSaveProductSettings(), { wrapper });
    first.result.current.mutate({ toolScope: "codex" });
    second.result.current.mutate({ advancedMode: true });
    await waitFor(() => expect(release).toBeDefined());
    release?.();

    await waitFor(() => expect(first.result.current.isError).toBe(true));
    await waitFor(() => expect(second.result.current.isError).toBe(true));
    await waitFor(() => expect(reads).toBe(2));
    await waitFor(() =>
      expect(client.getQueryData(settingsKeys.current())).toEqual(STORED),
    );
  });

  it("does not re-read after a lone failed save", async () => {
    let reads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () => {
        reads += 1;
        return HttpResponse.json(STORED);
      }),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, () => failure()),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const read = renderHook(() => useProductSettings(), { wrapper });
    await waitFor(() => expect(read.result.current.isSuccess).toBe(true));

    const { result } = renderHook(() => useSaveProductSettings(), { wrapper });
    result.current.mutate({ advancedMode: true });
    await waitFor(() => expect(result.current.isError).toBe(true));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(reads).toBe(1);
    expect(client.getQueryData(settingsKeys.current())).toEqual(STORED);
  });

  it("does not blank the shell by retrying a failed read when another page mounts", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () => {
        calls += 1;
        return HttpResponse.text("boom", { status: 500 });
      }),
    );
    const wrapper = withQueryClient(createTestQueryClient());
    const rootObserver = renderHook(() => useProductSettings(), { wrapper });
    await waitFor(() => expect(rootObserver.result.current.isError).toBe(true));

    const pageObserver = renderHook(() => useProductSettings(), { wrapper });
    await waitFor(() => expect(pageObserver.result.current.isError).toBe(true));
    expect(calls).toBe(1);
    expect(rootObserver.result.current.isPending).toBe(false);
  });
});
