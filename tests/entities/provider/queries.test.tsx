import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";
import {
  providerKeys,
  useProviderConnectionProfile,
  useProviderEditProfile,
  useProviders,
} from "@/entities/provider";

const TAURI_ENDPOINT = "http://tauri.local";

const wire = {
  id: "relay",
  tool: "claude-code",
  name: "My Relay",
  kind: "custom",
  active: true,
  baseUrl: "https://relay.example.com",
  apiKey: "sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F",
  websiteUrl: null,
  testable: true,
  canRemove: false,
};

describe("useProviders", () => {
  it("keys the cache by tool so two tools never share a list", () => {
    expect(providerKeys.list("claude-code")).toEqual([
      "providers",
      "list",
      "claude-code",
    ]);
    expect(providerKeys.list("codex")).not.toEqual(
      providerKeys.list("claude-code"),
    );
    expect(providerKeys.list("codex")[0]).toBe(providerKeys.all[0]);
  });

  it("loads the services of the selected tool", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
        HttpResponse.json([wire]),
      ),
    );
    const { result } = renderHook(() => useProviders("claude-code"), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.[0]?.name).toBe("My Relay");
  });

  it("does not call the backend before a tool is chosen", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, () => {
        calls += 1;
        return HttpResponse.json([]);
      }),
    );
    const { result } = renderHook(() => useProviders(null), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.fetchStatus).toBe("idle"));
    expect(calls).toBe(0);
  });

  it("loads and separately caches the backend-owned connection profile", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_connection_profile`, () =>
        HttpResponse.json({
          defaultPresetId: "official",
          modelRequired: false,
          presets: [
            {
              id: "official",
              serviceName: "Anthropic API",
              defaultName: "Anthropic",
              defaultModel: "claude-sonnet-5",
              websiteUrl: "https://www.anthropic.com",
              apiKeyUrl: "https://console.anthropic.com",
              official: true,
            },
          ],
        }),
      ),
    );
    const { result } = renderHook(
      () => useProviderConnectionProfile("claude-code"),
      { wrapper: withQueryClient(createTestQueryClient()) },
    );
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.presets[0]?.serviceName).toBe("Anthropic API");
    expect(providerKeys.connectionProfile("claude-code")).not.toEqual(
      providerKeys.list("claude-code"),
    );
  });

  it("loads a per-service safe edit profile only after both targets exist", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_edit_profile`, () => {
        calls += 1;
        return HttpResponse.json({
          providerId: "relay",
          baseUrl: "https://relay.example.com",
          endpointCandidates: ["https://relay-backup.example.com"],
          endpointAutoSelect: true,
          models: ["model-a"],
          headerNames: [],
          capabilities: {
            canEditBaseUrl: true,
            canEditEndpoints: true,
            canEditModels: true,
            canEditHeaders: false,
            supportsMultipleModels: false,
          },
        });
      }),
    );
    const client = createTestQueryClient();
    const disabled = renderHook(() => useProviderEditProfile(null, null), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() =>
      expect(disabled.result.current.fetchStatus).toBe("idle"),
    );
    expect(calls).toBe(0);

    const enabled = renderHook(
      () => useProviderEditProfile("claude-code", "relay"),
      { wrapper: withQueryClient(client) },
    );
    await waitFor(() => expect(enabled.result.current.isSuccess).toBe(true));
    expect(enabled.result.current.data?.models).toEqual(["model-a"]);
    expect(calls).toBe(1);
    expect(providerKeys.editProfile("claude-code", "relay")).not.toEqual(
      providerKeys.editProfile("claude-code", "other"),
    );
  });
});
