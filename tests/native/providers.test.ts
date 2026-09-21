import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { server } from "../msw/server";
import { native, NativeError } from "@/native";

const TAURI_ENDPOINT = "http://tauri.local";
const CONNECTION_REQUEST_ID = "173589d6-2b23-4bf5-b3e8-20ea6a62bce1";

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

describe("native.providers", () => {
  let seen: unknown[];

  beforeEach(() => {
    seen = [];
  });

  it("lists the services of one tool", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json([wire]);
      }),
    );
    const providers = await native.providers.list("claude-code");
    expect(seen).toEqual([{ tool: "claude-code" }]);
    expect(providers[0]?.apiKey).toBe(
      "sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F",
    );
    expect(providers[0]?.testable).toBe(true);
  });

  it("reads the effective connection without receiving credential values", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_runtime_context`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            tool: "claude-code",
            liveConfigPaths: ["~/.claude/settings.json"],
            resources: [],
            storage: {
              totalBytes: 0,
              sessionBytes: 0,
              sessionCount: 0,
              measurementLimited: false,
            },
            effectiveConnection: {
              endpoint: "https://api.example.test/",
              endpointSource: {
                kind: "shellFile",
                variable: "ANTHROPIC_BASE_URL",
                path: "~/.config/zsh/secrets.zsh:3",
              },
              credential: "configured",
              credentialSource: {
                kind: "environment",
                variable: "ANTHROPIC_AUTH_TOKEN",
              },
              providerId: null,
              shellInspected: true,
            },
          });
        },
      ),
    );
    const context = await native.providers.runtimeContext("claude-code");
    expect(seen).toEqual([{ tool: "claude-code" }]);
    expect(context.effectiveConnection?.endpointSource).toEqual({
      kind: "shellFile",
      variable: "ANTHROPIC_BASE_URL",
      path: "~/.config/zsh/secrets.zsh:3",
    });
    expect(JSON.stringify(context)).not.toContain("sk-");
  });

  it("rejects a runtime context that still carries the removed override list", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        HttpResponse.json({
          tool: "claude-code",
          liveConfigPaths: [],
          resources: [],
          storage: {
            totalBytes: 0,
            sessionBytes: 0,
            sessionCount: 0,
            measurementLimited: false,
          },
          effectiveConnection: null,
          externalOverrides: [],
        }),
      ),
    );
    await expect(
      native.providers.runtimeContext("claude-code"),
    ).rejects.toBeInstanceOf(NativeError);
  });

  it("opens a registered runtime resource by id rather than by path", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_runtime_resource_open`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json("folderOpened");
        },
      ),
    );

    await expect(
      native.providers.openRuntimeResource("claude-code", "session-data-0"),
    ).resolves.toBe("folderOpened");
    expect(seen).toEqual([{ tool: "claude-code", resource: "session-data-0" }]);
  });

  it("reads the backend-owned beginner connection profile", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_connection_profile`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
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
          });
        },
      ),
    );
    const profile = await native.providers.connectionProfile("claude-code");
    expect(seen).toEqual([{ tool: "claude-code" }]);
    expect(profile.presets[0]?.defaultModel).toBe("claude-sonnet-5");
  });

  it("accepts a compatible default that is honestly not vendor-official", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_connection_profile`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            defaultPresetId: "openrouter",
            modelRequired: true,
            presets: [
              {
                id: "openrouter",
                serviceName: "OpenRouter",
                defaultName: "OpenRouter",
                defaultModel: "anthropic/claude-sonnet-5",
                websiteUrl: "https://openrouter.ai",
                apiKeyUrl: "https://openrouter.ai",
                official: false,
              },
            ],
          });
        },
      ),
    );

    const profile = await native.providers.connectionProfile("pi");
    expect(seen).toEqual([{ tool: "pi" }]);
    expect(profile.defaultPresetId).toBe("openrouter");
    expect(profile.presets[0]?.official).toBe(false);
  });

  it("reads a safe edit profile without receiving keys or header values", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_edit_profile`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            providerId: "relay",
            baseUrl: "https://relay.example.com",
            endpointCandidates: ["https://relay-backup.example.com"],
            endpointAutoSelect: true,
            models: ["claude-sonnet-5"],
            headerNames: ["Authorization"],
            capabilities: {
              canEditBaseUrl: true,
              canEditEndpoints: true,
              canEditModels: true,
              canEditHeaders: false,
              supportsMultipleModels: false,
            },
          });
        },
      ),
    );
    const profile = await native.providers.editProfile("claude-code", "relay");
    expect(seen).toEqual([{ tool: "claude-code", provider: "relay" }]);
    expect(profile.models).toEqual(["claude-sonnet-5"]);
    expect(profile.endpointCandidates).toEqual([
      "https://relay-backup.example.com",
    ]);
    expect(JSON.stringify(profile)).not.toContain("apiKey");
    expect(JSON.stringify(profile)).not.toContain("headerValues");
  });

  it("creates a service without sending raw provider configuration", async () => {
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
    const created = await native.providers.create(
      "claude-code",
      CONNECTION_REQUEST_ID,
      {
        presetId: "official",
        name: "Anthropic",
        apiKey: "sk-secret",
        model: "claude-sonnet-5",
      },
    );
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
    expect(JSON.stringify(seen)).not.toContain("settingsConfig");
    expect(created.createdProviderId).toBe("relay");
    expect(created.providers).toEqual([wire]);
  });

  it("rejects missing presets and raw configuration before invoking create", () => {
    expect(() =>
      native.providers.create("claude-code", CONNECTION_REQUEST_ID, {
        name: "Anthropic",
        apiKey: "sk-secret",
        model: "claude-sonnet-5",
      } as never),
    ).toThrow();
    expect(() =>
      native.providers.create("claude-code", CONNECTION_REQUEST_ID, {
        presetId: "official",
        name: "Anthropic",
        apiKey: "sk-secret",
        model: "claude-sonnet-5",
        settingsConfig: { env: { ANTHROPIC_API_KEY: "sk-secret" } },
      } as never),
    ).toThrow();
    expect(seen).toEqual([]);
  });

  it("rejects a create response that adds a plaintext key field", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_create`, () =>
        HttpResponse.json({
          providers: [wire],
          createdProviderId: "relay",
          apiKey: "sk-secret",
        }),
      ),
    );

    await expect(
      native.providers.create("claude-code", CONNECTION_REQUEST_ID, {
        presetId: "official",
        name: "Anthropic",
        apiKey: "sk-secret",
        model: "claude-sonnet-5",
      }),
    ).rejects.toBeInstanceOf(NativeError);
  });

  it("creates an Advanced custom service through its separate strict command", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_custom_create`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            providers: [wire],
            createdProviderId: "relay",
          });
        },
      ),
    );

    await expect(
      native.providers.createCustom("claude-code", CONNECTION_REQUEST_ID, {
        name: "Private relay",
        apiKey: "sk-secret",
        model: "model-a",
        baseUrl: "https://relay.example.com/v1",
      }),
    ).resolves.toMatchObject({ createdProviderId: "relay" });
    expect(seen).toEqual([
      {
        tool: "claude-code",
        requestId: CONNECTION_REQUEST_ID,
        draft: {
          name: "Private relay",
          apiKey: "sk-secret",
          model: "model-a",
          baseUrl: "https://relay.example.com/v1",
        },
      },
    ]);
    expect(JSON.stringify(seen)).not.toContain("settingsConfig");
    expect(JSON.stringify(seen)).not.toContain("headers");
  });

  it("rejects unsafe or raw custom configuration before native invocation", () => {
    const valid = {
      name: "Private relay",
      apiKey: "sk-secret",
      model: "model-a",
      baseUrl: "https://relay.example.com/v1",
    };
    for (const draft of [
      { ...valid, baseUrl: "http://relay.example.com/v1" },
      { ...valid, baseUrl: "https://relay.example.com/v1?token=secret" },
      { ...valid, settingsConfig: { auth: "sk-secret" } },
    ]) {
      expect(() =>
        native.providers.createCustom(
          "claude-code",
          CONNECTION_REQUEST_ID,
          draft as never,
        ),
      ).toThrow();
    }
    expect(seen).toEqual([]);
  });

  it("switches and gets the refreshed list back", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_switch`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([{ ...wire, active: true }]);
        },
      ),
    );
    const refreshed = await native.providers.switch("claude-code", "relay");
    expect(seen).toEqual([{ tool: "claude-code", provider: "relay" }]);
    expect(refreshed[0]?.active).toBe(true);
  });

  it("prepares a Use action with only the saved provider id", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_activation_prepare`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            status: "ready",
            originProviderId: "relay",
            activeProviderId: "relay",
            providers: [wire],
            checks: [
              {
                providerId: "relay",
                reachability: "operational",
                responseTimeMs: 81,
                httpStatus: 200,
              },
            ],
          });
        },
      ),
    );

    const outcome = await native.providers.prepareActivation(
      "claude-code",
      "relay",
    );

    expect(seen).toEqual([{ tool: "claude-code", provider: "relay" }]);
    expect(JSON.stringify(seen)).not.toContain("url");
    expect(JSON.stringify(seen)).not.toContain("automaticProviderFailover");
    expect(outcome.status).toBe("ready");
  });

  it("prepares Open without sending a provider address", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_launch_prepare`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            status: "notChecked",
            originProviderId: null,
            activeProviderId: null,
            providers: [],
            checks: [],
          });
        },
      ),
    );

    await expect(
      native.providers.prepareLaunch("codex"),
    ).resolves.toMatchObject({ status: "notChecked" });
    expect(seen).toEqual([{ tool: "codex" }]);
  });

  it("tries the next saved service without accepting renderer-owned candidates", async () => {
    const next = {
      ...wire,
      id: "relay-next",
      name: "Next Relay",
      active: true,
    };
    const origin = { ...wire, active: false, canRemove: true };
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_next_healthy`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            status: "failedOver",
            originProviderId: "relay",
            activeProviderId: "relay-next",
            providers: [origin, next],
            checks: [
              {
                providerId: "relay-next",
                reachability: "degraded",
                responseTimeMs: 710,
                httpStatus: 429,
              },
            ],
          });
        },
      ),
    );

    const outcome = await native.providers.tryNextHealthy(
      "claude-code",
      "relay",
    );

    expect(seen).toEqual([{ tool: "claude-code", failedProvider: "relay" }]);
    expect(JSON.stringify(seen)).not.toContain("candidates");
    expect(outcome.activeProviderId).toBe("relay-next");
  });

  it("rejects inconsistent or secret-bearing preflight responses", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_launch_prepare`, () =>
        HttpResponse.json({
          status: "failedOver",
          originProviderId: "relay",
          activeProviderId: "relay",
          providers: [{ ...wire, apiKey: "sk-secret" }],
          checks: [
            {
              providerId: "relay",
              reachability: "failed",
              responseTimeMs: null,
              httpStatus: null,
              targetUrl: "https://private.example.test",
            },
          ],
        }),
      ),
    );

    await expect(
      native.providers.prepareLaunch("claude-code"),
    ).rejects.toBeInstanceOf(NativeError);
  });

  it("sends a null key when the user did not change it", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_save`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json([wire]);
      }),
    );
    await native.providers.save("claude-code", "relay", {
      name: "Renamed",
      apiKey: null,
    });
    expect(seen).toEqual([
      {
        tool: "claude-code",
        provider: "relay",
        draft: { name: "Renamed", apiKey: null },
      },
    ]);
  });

  it("sends only the explicit advanced edit shape", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_save`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json([wire]);
      }),
    );
    await native.providers.save("opencode", "relay", {
      name: "Renamed",
      apiKey: null,
      models: ["model-a", "model-b"],
      advanced: {
        baseUrlChanged: true,
        baseUrl: "https://relay.example.com/v1",
        endpointCandidates: [
          "https://relay.example.com/v1",
          "https://relay-backup.example.com/v1",
        ],
        endpointAutoSelect: false,
        headers: [
          { name: "Authorization", value: null },
          { name: "X-Tenant", value: "tenant-secret" },
        ],
      },
    });
    expect(seen).toEqual([
      {
        tool: "opencode",
        provider: "relay",
        draft: {
          name: "Renamed",
          apiKey: null,
          models: ["model-a", "model-b"],
          advanced: {
            baseUrlChanged: true,
            baseUrl: "https://relay.example.com/v1",
            endpointCandidates: [
              "https://relay.example.com/v1",
              "https://relay-backup.example.com/v1",
            ],
            endpointAutoSelect: false,
            headers: [
              { name: "Authorization", value: null },
              { name: "X-Tenant", value: "tenant-secret" },
            ],
          },
        },
      },
    ]);
    expect(JSON.stringify(seen)).not.toContain("settingsConfig");
  });

  it("rejects raw provider configuration before invoking native save", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_save`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json([wire]);
      }),
    );
    expect(() =>
      native.providers.save("claude-code", "relay", {
        name: "Relay",
        apiKey: null,
        settingsConfig: { auth: "secret" },
      } as never),
    ).toThrow();
    expect(seen).toEqual([]);
  });

  it("rejects an edit profile that tries to return header values", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_edit_profile`, () =>
        HttpResponse.json({
          providerId: "relay",
          baseUrl: null,
          endpointCandidates: [],
          endpointAutoSelect: true,
          models: [],
          headerNames: ["Authorization"],
          headerValues: ["secret"],
          capabilities: {
            canEditBaseUrl: true,
            canEditEndpoints: true,
            canEditModels: true,
            canEditHeaders: true,
            supportsMultipleModels: true,
          },
        }),
      ),
    );
    await expect(
      native.providers.editProfile("opencode", "relay"),
    ).rejects.toBeInstanceOf(NativeError);
  });

  it("removes a service using only its tool and stable id", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_remove`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([]);
        },
      ),
    );

    const remaining = await native.providers.remove("claude-code", "relay");

    expect(seen).toEqual([{ tool: "claude-code", provider: "relay" }]);
    expect(remaining).toEqual([]);
    expect(JSON.stringify(seen)).not.toContain("settingsConfig");
    expect(JSON.stringify(seen)).not.toContain("apiKey");
  });

  it("reads a check result", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.json({
          providerId: "relay",
          reachability: "degraded",
          responseTimeMs: 7100,
          httpStatus: 200,
        }),
      ),
    );
    const result = await native.providers.test("claude-code", "relay");
    expect(result.reachability).toBe("degraded");
    expect(result.responseTimeMs).toBe(7100);
  });

  it("starts a native-owned check-all task using only the tool id", async () => {
    const operationId = "123e4567-e89b-42d3-a456-426614174000";
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_test_all`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json(operationId);
        },
      ),
    );

    await expect(native.providers.testAll("claude-code")).resolves.toBe(
      operationId,
    );
    expect(seen).toEqual([{ tool: "claude-code" }]);
    expect(JSON.stringify(seen)).not.toContain("provider");
    expect(JSON.stringify(seen)).not.toContain("url");
    expect(JSON.stringify(seen)).not.toContain("key");
  });

  it("tests several credential-free endpoints in one native request", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_endpoints_test`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([
            {
              candidateId: "official-global",
              latencyMs: 42,
              httpStatus: 204,
              failure: null,
            },
            {
              candidateId: "official-cn",
              latencyMs: null,
              httpStatus: null,
              failure: "timeout",
            },
          ]);
        },
      ),
    );

    const results = await native.providers.testEndpoints("claude-code", [
      { id: "official-global", url: "https://api.example.com/v1" },
      { id: "official-cn", url: "https://api.example.cn/v1" },
    ]);

    expect(seen).toEqual([
      {
        tool: "claude-code",
        candidates: [
          { id: "official-global", url: "https://api.example.com/v1" },
          { id: "official-cn", url: "https://api.example.cn/v1" },
        ],
      },
    ]);
    expect(results).toEqual([
      {
        candidateId: "official-global",
        latencyMs: 42,
        httpStatus: 204,
        failure: null,
      },
      {
        candidateId: "official-cn",
        latencyMs: null,
        httpStatus: null,
        failure: "timeout",
      },
    ]);
    expect(JSON.stringify(results)).not.toContain("url");
  });

  it("refuses endpoint credentials before invoking native code", () => {
    expect(() =>
      native.providers.testEndpoints("claude-code", [
        {
          id: "unsafe",
          url: "https://user:secret@example.com/v1?token=hidden",
        },
      ]),
    ).toThrow();
    expect(seen).toEqual([]);
  });

  it("rejects raw endpoint errors or echoed URLs from native code", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_endpoints_test`, () =>
        HttpResponse.json([
          {
            candidateId: "official",
            latencyMs: null,
            httpStatus: null,
            failure: "request timed out",
            url: "https://secret.example.test",
          },
        ]),
      ),
    );

    await expect(
      native.providers.testEndpoints("claude-code", [
        { id: "official", url: "https://api.example.com" },
      ]),
    ).rejects.toBeInstanceOf(NativeError);
  });

  it("tests reviewed presets by tool without sending endpoint URLs", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_presets_test`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([
            {
              candidateId: "official",
              latencyMs: 180,
              httpStatus: 200,
              failure: null,
            },
            {
              candidateId: "deepseek-safe",
              latencyMs: null,
              httpStatus: null,
              failure: "connection",
            },
          ]);
        },
      ),
    );

    const results = await native.providers.testPresets("claude-code");

    expect(seen).toEqual([{ tool: "claude-code" }]);
    expect(JSON.stringify(seen)).not.toContain("url");
    expect(results[0]).toEqual({
      candidateId: "official",
      latencyMs: 180,
      httpStatus: 200,
      failure: null,
    });
    expect(JSON.stringify(results)).not.toContain("url");
  });

  it("surfaces a refusal as a NativeError with a stable code", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "PROVIDER_NOT_FOUND",
            messageKey: "error.provider.notFound",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    await expect(
      native.providers.test("claude-code", "gone"),
    ).rejects.toBeInstanceOf(NativeError);
  });

  it("refuses a payload whose key field is not a string", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
        HttpResponse.json([{ ...wire, apiKey: 42 }]),
      ),
    );
    await expect(native.providers.list("claude-code")).rejects.toBeInstanceOf(
      NativeError,
    );
  });
});
