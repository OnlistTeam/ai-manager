import { act, renderHook, waitFor } from "@testing-library/react";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { connectivityFor, healthKeys } from "@/entities/health";
import { providerKeys, type Provider } from "@/entities/provider";
import { useProviderConnectionFlow } from "@/features/provider-management";
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

const createdProvider = {
  id: "anthropic",
  tool: "claude-code",
  name: "Anthropic",
  kind: "custom",
  active: true,
  baseUrl: "https://api.anthropic.com",
  apiKey: "sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F",
  websiteUrl: "https://www.anthropic.com/claude-code",
  testable: true,
  canRemove: false,
} satisfies Provider;

function createResponse(provider: Provider = createdProvider) {
  return {
    providers: [provider],
    createdProviderId: provider.id,
  };
}

function request() {
  return {
    tool: "claude-code" as const,
    toolName: "Claude Code",
    draft: {
      presetId: "official",
      name: "Anthropic",
      apiKey: "sk-secret",
      model: "claude-sonnet-5",
    },
  };
}

function customRequest() {
  return {
    tool: "claude-code" as const,
    toolName: "Claude Code",
    draft: {
      name: "Private relay",
      apiKey: "sk-private",
      model: "model-a",
      baseUrl: "https://relay.example.test/v1",
    },
  };
}

describe("useProviderConnectionFlow", () => {
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
    toastMocks.info.mockClear();
    toastMocks.success.mockClear();
  });

  it("checks the exact new service and reports address reachability without exposing the key", async () => {
    const checked: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_create`, () =>
        HttpResponse.json(createResponse()),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, async ({ request }) => {
        checked.push(await request.json());
        return HttpResponse.json({
          providerId: "anthropic",
          reachability: "operational",
          responseTimeMs: 240,
          httpStatus: 200,
        });
      }),
    );
    const client = createTestQueryClient();
    const onSaved = vi.fn();
    const { result } = renderHook(() => useProviderConnectionFlow(), {
      wrapper: withQueryClient(client),
    });

    act(() => result.current.connectProvider(request(), onSaved));

    await waitFor(() => expect(toastMocks.success).toHaveBeenCalledTimes(1));
    expect(onSaved).toHaveBeenCalledTimes(1);
    expect(checked).toEqual([{ tool: "claude-code", provider: "anthropic" }]);
    expect(client.getQueryData(providerKeys.list("claude-code"))).toEqual([
      createdProvider,
    ]);
    const connectivity = client.getQueryData(healthKeys.connectivity()) ?? {};
    expect(
      connectivityFor(connectivity, "claude-code", "anthropic"),
    ).toMatchObject({ reachability: "operational", responseTimeMs: 240 });
    expect(toastMocks.success).toHaveBeenCalledWith(
      en.services.connect.address.operational.replace("{{name}}", "Anthropic"),
      {
        description: en.services.connect.firstUse.replace(
          "{{tool}}",
          "Claude Code",
        ),
      },
    );
    expect(JSON.stringify(toastMocks.success.mock.calls)).not.toContain(
      "sk-secret",
    );
  });

  it("keeps a saved service distinct from an address that did not respond", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_create`, () =>
        HttpResponse.json(createResponse()),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.json({
          providerId: "anthropic",
          reachability: "failed",
          responseTimeMs: 8000,
          httpStatus: null,
        }),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useProviderConnectionFlow(), {
      wrapper: withQueryClient(client),
    });

    act(() => result.current.connectProvider(request()));

    await waitFor(() => expect(toastMocks.info).toHaveBeenCalledTimes(1));
    expect(toastMocks.info).toHaveBeenCalledWith(
      en.services.connect.address.failed.replace("{{name}}", "Anthropic"),
      { description: en.services.connect.addressRetry },
    );
    expect(client.getQueryData(providerKeys.list("claude-code"))).toEqual([
      createdProvider,
    ]);
  });

  it("uses the separate custom command and then the same explicit address check", async () => {
    const bodies: unknown[] = [];
    const custom = {
      ...createdProvider,
      id: "private-relay",
      name: "Private relay",
      baseUrl: "https://relay.example.test/v1",
      websiteUrl: null,
    };
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_custom_create`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json(createResponse(custom));
        },
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json({
          providerId: "private-relay",
          reachability: "operational",
          responseTimeMs: 90,
          httpStatus: 200,
        });
      }),
    );
    const { result } = renderHook(() => useProviderConnectionFlow(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    act(() => result.current.connectCustomProvider(customRequest()));

    await waitFor(() => expect(toastMocks.success).toHaveBeenCalledTimes(1));
    expect(bodies).toHaveLength(2);
    expect(bodies[0]).toMatchObject({
      tool: "claude-code",
      draft: customRequest().draft,
    });
    expect(bodies[1]).toEqual({
      tool: "claude-code",
      provider: "private-relay",
    });
    expect(JSON.stringify(toastMocks.success.mock.calls)).not.toContain(
      "sk-private",
    );
  });

  it("keeps a command failure attached to the exact service without a transient toast", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
        HttpResponse.json([createdProvider]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.json(
          {
            code: "PROVIDER_UNREACHABLE",
            messageKey: "error.provider.testFailed",
            technicalMessage:
              "connection refused at 10.0.0.1 token=never-render",
            remediation: "error.remediation.checkServiceSettings",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    const client = createTestQueryClient();
    client.setQueryData(healthKeys.connectivity(), {
      "claude-code": {
        anthropic: {
          providerId: "anthropic",
          reachability: "operational",
          responseTimeMs: 240,
          httpStatus: 200,
        },
      },
    });
    const { result } = renderHook(() => useProviderConnectionFlow(), {
      wrapper: withQueryClient(client),
    });

    act(() =>
      result.current.checkProvider({
        tool: "claude-code",
        providerId: "anthropic",
      }),
    );

    await waitFor(() => expect(result.current.checkFailure).not.toBeNull());
    expect(result.current.checkFailure).toMatchObject({
      tool: "claude-code",
      providerId: "anthropic",
      error: { messageKey: "error.provider.testFailed" },
    });
    expect(
      connectivityFor(
        client.getQueryData(healthKeys.connectivity()) ?? {},
        "claude-code",
        "anthropic",
      ),
    ).toBeUndefined();
    expect(toastMocks.error).not.toHaveBeenCalled();
  });

  it("does not invent a check for a service without a checkable address", async () => {
    let checkRequests = 0;
    const uncheckable = {
      ...createdProvider,
      kind: "custom" as const,
      baseUrl: null,
      apiKey: null,
      testable: false,
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_create`, () =>
        HttpResponse.json(createResponse(uncheckable)),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () => {
        checkRequests += 1;
        return HttpResponse.error();
      }),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useProviderConnectionFlow(), {
      wrapper: withQueryClient(client),
    });

    act(() => result.current.connectProvider(request()));

    await waitFor(() => expect(toastMocks.success).toHaveBeenCalledTimes(1));
    expect(checkRequests).toBe(0);
    expect(toastMocks.success).toHaveBeenCalledWith(
      en.services.connect.saved.replace("{{name}}", "Anthropic"),
      {
        description: en.services.connect.firstUse.replace(
          "{{tool}}",
          "Claude Code",
        ),
      },
    );
  });

  it("offers to activate a service the switch left only stored", async () => {
    const dormant = {
      ...createdProvider,
      active: false,
      kind: "custom" as const,
      baseUrl: null,
      apiKey: null,
      testable: false,
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_create`, () =>
        HttpResponse.json(createResponse(dormant)),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () => {
        throw new Error(
          "should not check a provider that was never stored active",
        );
      }),
    );
    const onActivate = vi.fn();
    const client = createTestQueryClient();
    const { result } = renderHook(
      () => useProviderConnectionFlow({ onActivate }),
      { wrapper: withQueryClient(client) },
    );

    act(() => result.current.connectProvider(request()));

    await waitFor(() => expect(toastMocks.success).toHaveBeenCalledTimes(1));
    expect(toastMocks.success).toHaveBeenCalledWith(
      en.services.connect.saved.replace("{{name}}", "Anthropic"),
      {
        description: en.services.connect.activateQuestion.replace(
          "{{tool}}",
          "Claude Code",
        ),
        action: {
          label: en.services.connect.activateNow,
          onClick: expect.any(Function),
        },
      },
    );
    const [, options] = toastMocks.success.mock.calls[0] as [
      string,
      { action: { onClick: () => void } },
    ];
    options.action.onClick();
    expect(onActivate).toHaveBeenCalledWith("anthropic");
  });

  it("does not offer to activate the service the tool is already using", async () => {
    const uncheckable = {
      ...createdProvider,
      kind: "custom" as const,
      baseUrl: null,
      apiKey: null,
      testable: false,
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_create`, () =>
        HttpResponse.json(createResponse(uncheckable)),
      ),
    );
    const onActivate = vi.fn();
    const client = createTestQueryClient();
    const { result } = renderHook(
      () => useProviderConnectionFlow({ onActivate }),
      { wrapper: withQueryClient(client) },
    );

    act(() => result.current.connectProvider(request()));

    await waitFor(() => expect(toastMocks.success).toHaveBeenCalledTimes(1));
    expect(toastMocks.success).toHaveBeenCalledWith(
      en.services.connect.saved.replace("{{name}}", "Anthropic"),
      {
        description: en.services.connect.firstUse.replace(
          "{{tool}}",
          "Claude Code",
        ),
      },
    );
  });

  it("reuses one opaque request id when the same connection is retried", async () => {
    const bodies: Array<Record<string, unknown>> = [];
    let attempts = 0;
    const uncheckable = {
      ...createdProvider,
      kind: "custom" as const,
      baseUrl: null,
      apiKey: null,
      testable: false,
    };
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_create`,
        async ({ request }) => {
          attempts += 1;
          bodies.push((await request.json()) as Record<string, unknown>);
          if (attempts === 1) {
            return HttpResponse.json(
              {
                code: "CONFIG_WRITE_FAILED",
                messageKey: "error.provider.createFailed",
                technicalMessage: "/private/config token=never-render",
                remediation: "error.remediation.checkConnectionSettings",
                contextId: null,
              },
              { status: 500 },
            );
          }
          return HttpResponse.json(createResponse(uncheckable));
        },
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useProviderConnectionFlow(), {
      wrapper: withQueryClient(client),
    });

    act(() => result.current.connectProvider(request()));
    await waitFor(() => expect(result.current.createError).not.toBeNull());
    act(() => result.current.connectProvider(request()));
    await waitFor(() => expect(toastMocks.success).toHaveBeenCalledTimes(1));

    expect(bodies).toHaveLength(2);
    const firstRequestId = bodies[0]?.requestId;
    expect(firstRequestId).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    expect(bodies[1]?.requestId).toBe(firstRequestId);
    expect(bodies[0]?.draft).toEqual(request().draft);
    expect(bodies[1]?.draft).toEqual(request().draft);
    expect(toastMocks.error).not.toHaveBeenCalled();
  });
});
