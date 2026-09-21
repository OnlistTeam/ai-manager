import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  desktopPreferencesKeys,
  useDesktopPreferences,
  useSaveDesktopPreferences,
} from "@/entities/desktop-preferences";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const initial = {
  launchOnStartup: false,
  silentStartup: false,
  showInTray: true,
  minimizeToTrayOnClose: true,
};
const saved = { ...initial, launchOnStartup: true };

describe("desktop preference queries", () => {
  it("shares the device read across remounts", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_preferences_get`, () => {
        calls += 1;
        return HttpResponse.json(initial);
      }),
    );
    const client = createTestQueryClient();
    const first = renderHook(() => useDesktopPreferences(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    first.unmount();

    const second = renderHook(() => useDesktopPreferences(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(calls).toBe(1);
  });

  it("replaces cached state with the verified backend read-back", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_preferences_save`, () =>
        HttpResponse.json(saved),
      ),
    );
    const client = createTestQueryClient();
    client.setQueryData(desktopPreferencesKeys.current(), initial);
    const hook = renderHook(() => useSaveDesktopPreferences(), {
      wrapper: withQueryClient(client),
    });
    hook.result.current.mutate(saved);
    await waitFor(() => expect(hook.result.current.isSuccess).toBe(true));
    expect(client.getQueryData(desktopPreferencesKeys.current())).toEqual(
      saved,
    );
  });
});
