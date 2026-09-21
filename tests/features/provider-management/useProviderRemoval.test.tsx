import { renderHook, waitFor } from "@testing-library/react";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { connectivityFor, healthKeys } from "@/entities/health";
import { providerKeys } from "@/entities/provider";
import { useRemoveProvider } from "@/features/provider-management";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

const remaining = {
  id: "anthropic",
  tool: "claude-code",
  name: "Anthropic",
  kind: "official",
  active: true,
  baseUrl: null,
  apiKey: null,
  websiteUrl: "https://anthropic.com",
  testable: false,
  canRemove: false,
};

describe("useRemoveProvider", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { services: en.services },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.error.mockClear();
    toastMocks.info.mockClear();
    toastMocks.success.mockClear();
  });

  it("sends only the stable target and reconciles every related cache", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_remove`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json([remaining]);
        },
      ),
    );
    const client = createTestQueryClient();
    client.setQueryData(healthKeys.connectivity(), {
      "claude-code": {
        relay: {
          providerId: "relay",
          reachability: "operational",
          responseTimeMs: 120,
          httpStatus: 200,
        },
        anthropic: {
          providerId: "anthropic",
          reachability: "degraded",
          responseTimeMs: 7100,
          httpStatus: 200,
        },
      },
    });
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useRemoveProvider(), {
      wrapper: withQueryClient(client),
    });

    result.current.mutate({
      tool: "claude-code",
      providerId: "relay",
      name: "My Relay",
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(body).toEqual({ tool: "claude-code", provider: "relay" });
    expect(client.getQueryData(providerKeys.list("claude-code"))).toEqual([
      remaining,
    ]);
    const connectivity = client.getQueryData(healthKeys.connectivity()) ?? {};
    expect(
      connectivityFor(connectivity, "claude-code", "relay"),
    ).toBeUndefined();
    expect(
      connectivityFor(connectivity, "claude-code", "anthropic"),
    ).toBeDefined();
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: healthKeys.snapshots,
    });
    expect(toastMocks.success).toHaveBeenCalledWith("Removed My Relay");
  });

  it("keeps removal pending until every uncertain state refresh finishes", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_remove`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.provider.removeFailed",
            technicalMessage: "/private/config token=do-not-show",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    let finishProviders: (() => void) | undefined;
    let finishHealth: (() => void) | undefined;
    const providerRefresh = new Promise<void>((resolve) => {
      finishProviders = resolve;
    });
    const healthRefresh = new Promise<void>((resolve) => {
      finishHealth = resolve;
    });
    invalidateSpy.mockImplementation((filters) =>
      JSON.stringify(filters?.queryKey) ===
      JSON.stringify(providerKeys.list("claude-code"))
        ? providerRefresh
        : healthRefresh,
    );
    const { result } = renderHook(() => useRemoveProvider(), {
      wrapper: withQueryClient(client),
    });

    result.current.mutate({
      tool: "claude-code",
      providerId: "relay",
      name: "My Relay",
    });
    await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(2));

    expect(result.current.isPending).toBe(true);
    expect(result.current.isError).toBe(false);
    finishProviders?.();
    await providerRefresh;
    expect(result.current.isPending).toBe(true);
    finishHealth?.();
    await waitFor(() => expect(result.current.isError).toBe(true));

    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: providerKeys.list("claude-code"),
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: healthKeys.snapshots,
    });
    expect(toastMocks.success).not.toHaveBeenCalled();
    expect(toastMocks.error).not.toHaveBeenCalled();
  });
});
