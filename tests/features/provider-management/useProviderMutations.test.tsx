import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { connectivityFor, healthKeys } from "@/entities/health";
import { providerKeys } from "@/entities/provider";
import {
  useCreateProvider,
  useSaveProvider,
  useSwitchProvider,
  useTestProvider,
} from "@/features/provider-management";

const TAURI_ENDPOINT = "http://tauri.local";
const CONNECTION_REQUEST_ID = "61ab9ef4-371e-4389-b75d-3bca1962228c";
const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

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

describe("provider mutations", () => {
  beforeEach(() => {
    toastMocks.error.mockClear();
  });

  it("writes a newly connected service into cache and refreshes setup health", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_create`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            providers: [wire],
            createdProviderId: "relay",
          });
        },
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useCreateProvider(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate({
      tool: "claude-code",
      requestId: CONNECTION_REQUEST_ID,
      draft: {
        presetId: "official",
        name: "Anthropic",
        apiKey: "sk-secret",
        model: "claude-sonnet-5",
      },
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(seen).toEqual([
      {
        tool: "claude-code",
        requestId: CONNECTION_REQUEST_ID,
        draft: {
          presetId: "official",
          name: "Anthropic",
          apiKey: "sk-secret",
          model: "claude-sonnet-5",
        },
      },
    ]);
    expect(client.getQueryData(providerKeys.list("claude-code"))).toEqual([
      wire,
    ]);
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: healthKeys.snapshots,
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: providerKeys.runtimeContext("claude-code"),
    });
  });

  it("awaits authoritative create refresh without a transient error toast", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_create`, () =>
        HttpResponse.text("boom", { status: 500 }),
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useCreateProvider(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate({
      tool: "claude-code",
      requestId: CONNECTION_REQUEST_ID,
      draft: {
        presetId: "official",
        name: "Anthropic",
        apiKey: "sk-secret",
        model: "claude-sonnet-5",
      },
    });
    await waitFor(() => expect(result.current.isError).toBe(true));

    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: providerKeys.list("claude-code"),
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: healthKeys.snapshots,
    });
    expect(toastMocks.error).not.toHaveBeenCalled();
  });

  it("writes the refreshed list straight into the cache after switching", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_switch`, () =>
        HttpResponse.json([wire]),
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useSwitchProvider(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate({ tool: "claude-code", providerId: "relay" });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(providerKeys.list("claude-code"))).toEqual([
      wire,
    ]);
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: healthKeys.snapshots,
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: providerKeys.runtimeContext("claude-code"),
    });
  });

  it("keeps the stored key when the draft leaves it out", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_save`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json([wire]);
      }),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    client.setQueryData(healthKeys.connectivity(), {
      "claude-code": {
        relay: {
          providerId: "relay",
          reachability: "operational",
          responseTimeMs: 120,
          httpStatus: 200,
        },
      },
    });
    const { result } = renderHook(() => useSaveProvider(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate({
      tool: "claude-code",
      providerId: "relay",
      draft: { name: "Renamed", apiKey: null },
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(seen).toEqual([
      {
        tool: "claude-code",
        provider: "relay",
        draft: { name: "Renamed", apiKey: null },
      },
    ]);
    const connectivity = client.getQueryData(healthKeys.connectivity()) ?? {};
    expect(
      connectivityFor(connectivity, "claude-code", "relay"),
    ).toBeUndefined();
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: providerKeys.editProfile("claude-code", "relay"),
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: providerKeys.runtimeContext("claude-code"),
    });
  });

  it("awaits authoritative provider and setup refresh without a transient switch toast", async () => {
    // Important 2: `switch` is a multi-step operation upstream (live config write
    // + DB is_current + proxy takeover); if it fails partway through, the cached
    // active badge drifts from real state. onError must invalidate this tool's
    // list, not just pop a toast and call it done.
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_switch`, () =>
        HttpResponse.text("boom", { status: 500 }),
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useSwitchProvider(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate({ tool: "claude-code", providerId: "relay" });
    await waitFor(() => expect(result.current.isError).toBe(true));

    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: providerKeys.list("claude-code"),
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: healthKeys.snapshots,
    });
    expect(toastMocks.error).not.toHaveBeenCalled();
  });

  it("refreshes save state without a transient error toast", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_save`, () =>
        HttpResponse.text("boom", { status: 500 }),
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useSaveProvider(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate({
      tool: "claude-code",
      providerId: "relay",
      draft: { name: "Renamed", apiKey: null },
    });
    await waitFor(() => expect(result.current.isError).toBe(true));

    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: providerKeys.list("claude-code"),
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: healthKeys.snapshots,
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: providerKeys.editProfiles("claude-code"),
    });
    expect(toastMocks.error).not.toHaveBeenCalled();
  });

  it("shows a translated sentence and never the technical detail on failure", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "PROVIDER_UNREACHABLE",
            messageKey: "error.provider.notTestable",
            technicalMessage: "connection refused to 10.0.0.1:443",
            remediation: "error.remediation.checkServiceSettings",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useTestProvider(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate({ tool: "claude-code", providerId: "relay" });
    await waitFor(() => expect(result.current.isError).toBe(true));

    expect(toastMocks.error).toHaveBeenCalledTimes(1);
    const [message, options] = toastMocks.error.mock.calls[0] ?? [];
    expect(String(message)).not.toContain("10.0.0.1");
    expect(options?.description).toBeDefined();
    expect(client.getQueryData(healthKeys.connectivity())).toEqual({});
  });

  it("shares a completed manual check with Quick Check", async () => {
    const checked = {
      providerId: "relay",
      reachability: "degraded",
      responseTimeMs: 7100,
      httpStatus: 200,
    } as const;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.json(checked),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useTestProvider(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate({ tool: "claude-code", providerId: "relay" });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    const cache = client.getQueryData(healthKeys.connectivity()) ?? {};
    expect(connectivityFor(cache, "claude-code", "relay")).toEqual(checked);
  });
});
