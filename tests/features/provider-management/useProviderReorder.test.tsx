import { renderHook, waitFor } from "@testing-library/react";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { providerKeys, type Provider } from "@/entities/provider";
import { routingKeys } from "@/entities/routing";
import {
  orderProviders,
  useReorderProviders,
} from "@/features/provider-management";
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

function service(id: string, active = false): Provider {
  return {
    id,
    tool: "claude-code",
    name: id,
    kind: "custom",
    active,
    baseUrl: `https://${id}.example.test`,
    apiKey: null,
    websiteUrl: null,
    testable: true,
    canRemove: !active,
  };
}

const saved = [service("alpha", true), service("beta"), service("gamma")];
const ids = (providers: readonly Provider[] | undefined) =>
  (providers ?? []).map((provider) => provider.id);

describe("useReorderProviders", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { error: en.error, services: en.services },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.error.mockClear();
  });

  it("orders the cached services by id and ignores ids it does not know", () => {
    expect(ids(orderProviders(saved, ["gamma", "ghost", "alpha"]))).toEqual([
      "gamma",
      "alpha",
    ]);
  });

  it("shows the new order at once and keeps the saved list the backend returns", async () => {
    let body: unknown;
    let release = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_providers_reorder`,
        async ({ request }) => {
          body = await request.json();
          await held;
          return HttpResponse.json([saved[2], saved[0], saved[1]]);
        },
      ),
    );
    const client = createTestQueryClient();
    client.setQueryData(providerKeys.list("claude-code"), saved);
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useReorderProviders(), {
      wrapper: withQueryClient(client),
    });

    result.current.mutate({
      tool: "claude-code",
      providerIds: ["gamma", "alpha", "beta"],
    });

    await waitFor(() =>
      expect(
        ids(client.getQueryData(providerKeys.list("claude-code"))),
      ).toEqual(["gamma", "alpha", "beta"]),
    );
    // Still waiting on the backend: the order above came from the cache.
    expect(client.isMutating()).toBe(1);
    release();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(body).toEqual({
      tool: "claude-code",
      providers: ["gamma", "alpha", "beta"],
    });
    expect(
      client
        .getQueryData<Provider[]>(providerKeys.list("claude-code"))
        ?.find((provider) => provider.id === "alpha")?.active,
    ).toBe(true);
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: routingKeys.all });
    expect(toastMocks.error).not.toHaveBeenCalled();
  });

  it("puts the old order back and says so when saving fails", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_reorder`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.provider.reorderFailed",
            technicalMessage: "database is locked",
            remediation: null,
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
      // The rollback is followed by a reread in case the list changed.
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
        HttpResponse.json(saved),
      ),
    );
    const client = createTestQueryClient();
    client.setQueryData(providerKeys.list("claude-code"), saved);
    const { result } = renderHook(() => useReorderProviders(), {
      wrapper: withQueryClient(client),
    });

    result.current.mutate({
      tool: "claude-code",
      providerIds: ["beta", "gamma", "alpha"],
    });
    await waitFor(() => expect(result.current.isError).toBe(true));

    expect(ids(client.getQueryData(providerKeys.list("claude-code")))).toEqual([
      "alpha",
      "beta",
      "gamma",
    ]);
    expect(toastMocks.error).toHaveBeenCalledWith(
      en.error.provider.reorderFailed,
      { description: undefined },
    );
  });
});
