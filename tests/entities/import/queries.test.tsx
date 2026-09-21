import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { backupKeys } from "@/entities/backup";
import { extensionKeys } from "@/entities/extension";
import {
  useImportPreview,
  useRunImport,
  type ImportPreview,
} from "@/entities/import";
import { providerKeys } from "@/entities/provider";
import { settingsKeys, type ProductSettings } from "@/entities/settings";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const PREVIEW: ImportPreview = {
  available: true,
  summary: { services: 2, mcpServers: 3, skills: 4 },
};
const SETTINGS: ProductSettings = {
  advancedMode: false,
  importPromptSeen: true,
  toolScope: null,
  extensionScope: null,
  extensionKind: null,
  downloadStrategy: "automatic",
  automaticProviderFailover: false,
  terminalApp: null,
};

describe("existing setup import queries", () => {
  it("reads the optional preview once per session until an explicit refresh", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () => {
        calls += 1;
        return HttpResponse.json(PREVIEW);
      }),
    );
    const client = createTestQueryClient();
    const first = renderHook(() => useImportPreview(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    expect(first.result.current.data).toEqual(PREVIEW);
    first.unmount();

    const second = renderHook(() => useImportPreview(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(second.result.current.data).toEqual(PREVIEW);
    expect(calls).toBe(1);
  });

  it("invalidates imported data and health but leaves settings authoritative", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_run`, () =>
        HttpResponse.json({ imported: PREVIEW.summary }),
      ),
    );
    const client = createTestQueryClient();
    const providerKey = providerKeys.list("claude-code");
    const extensionKey = extensionKeys.list("claude-code", "mcp");
    const backupKey = backupKeys.list();
    const healthKey = ["health", "snapshot"] as const;
    client.setQueryData(providerKey, ["cached provider"]);
    client.setQueryData(extensionKey, ["cached extension"]);
    client.setQueryData(backupKey, { files: [] });
    client.setQueryData(healthKey, { cached: true });
    client.setQueryData(settingsKeys.current(), SETTINGS);

    const { result } = renderHook(() => useRunImport(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    for (const key of [providerKey, extensionKey, backupKey, healthKey]) {
      expect(client.getQueryState(key)?.isInvalidated).toBe(true);
    }
    expect(client.getQueryState(settingsKeys.current())?.isInvalidated).toBe(
      false,
    );
    expect(client.getQueryData(settingsKeys.current())).toEqual(SETTINGS);
  });
});
