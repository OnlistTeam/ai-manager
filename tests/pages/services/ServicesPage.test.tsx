import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { delay, http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { providerKeys } from "@/entities/provider";
import { toolKeys, type ToolId } from "@/entities/tool";
import en from "@/i18n/locales/en.json";
import { ServicesPage } from "@/pages/services/ServicesPage";
import type { ServicesTab } from "@/pages/services/useServicesTab";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

function tool(id: string, name: string) {
  return {
    id,
    name,
    descriptionKey: `tool.${id}.description`,
    status: "installed",
    version: "1.0.0",
    latestVersion: null,
    sessionsInsideSettings: false,
    environment: "macos",
    capabilities: {
      canInstall: true,
      canUpdate: true,
      canUninstall: true,
      canRepair: false,
      canManageProvider: true,
      canManageMcp: true,
      canManageSkills: false,
      canManagePrompts: false,
      canManageVersion: true,
      canLaunch: true,
    },
  };
}

function service(overrides: Record<string, unknown> = {}) {
  return {
    id: "relay",
    tool: "claude-code",
    name: "My Relay",
    kind: "custom",
    active: false,
    baseUrl: "https://relay.example.com",
    apiKey: "sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F",
    websiteUrl: null,
    testable: true,
    canRemove: true,
    ...overrides,
  };
}

type ServicesResponder = () => Response | Promise<Response>;

function mount(
  services: unknown[] | ServicesResponder,
  tools: unknown[] = [tool("claude-code", "Claude Code")],
  /** The tool last left active. null = never selected. */
  toolScope: string | null = null,
  preferredToolId: ToolId | null = null,
  advancedMode = false,
  preferredTab: ServicesTab | null = null,
) {
  const client = createTestQueryClient();
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
      HttpResponse.json({
        advancedMode,
        importPromptSeen: true,
        toolScope,
        extensionKind: null,
      }),
    ),
    http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
      const body = (await request.json()) as {
        settings: Record<string, unknown>;
      };
      return HttpResponse.json(body.settings);
    }),
    http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
      HttpResponse.json(tools),
    ),
    http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
      typeof services === "function" ? services() : HttpResponse.json(services),
    ),
  );
  return {
    client,
    ...render(
      <ServicesPage
        preferredToolId={preferredToolId}
        preferredTab={preferredTab}
      />,
      { wrapper: withQueryClient(client) },
    ),
  };
}

describe("ServicesPage", () => {
  beforeEach(async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({
          advancedMode: false,
          importPromptSeen: true,
          toolScope: null,
          extensionKind: null,
        }),
      ),
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
              baseUrl: "https://api.example.test",
              websiteUrl: "https://www.anthropic.com",
              apiKeyUrl: "https://console.anthropic.com",
              official: true,
            },
          ],
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_edit_profile`, () =>
        HttpResponse.json({
          providerId: "relay",
          baseUrl: "https://relay.example.com",
          endpointCandidates: ["https://relay-backup.example.com"],
          endpointAutoSelect: true,
          models: ["claude-sonnet-5"],
          headerNames: [],
          capabilities: {
            canEditBaseUrl: true,
            canEditEndpoints: true,
            canEditModels: true,
            canEditHeaders: false,
            supportsMultipleModels: false,
          },
        }),
      ),
      // The test dialog reads the catalogue as soon as it opens.
      http.post(`${TAURI_ENDPOINT}/app_provider_models_list`, () =>
        HttpResponse.json({
          protocol: "anthropic",
          models: [{ id: "claude-sonnet-5", kind: "text" }],
          truncated: false,
          rejection: null,
        }),
      ),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        services: en.services,
        tool: en.tool,
        tools: en.tools,
        nav: en.nav,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.error.mockClear();
    toastMocks.info.mockClear();
    toastMocks.success.mockClear();
  });

  it("shows a loading status before anything arrives", () => {
    mount([service()]);
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("lists the services of the first tool that manages them", async () => {
    mount([service()]);
    expect(await screen.findByText("My Relay")).toBeInTheDocument();
    expect(
      screen.getByText("sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F"),
    ).toBeInTheDocument();
  });

  it("keeps an unsupported installed tool visible without reading its services", async () => {
    let providerReads = 0;
    const unsupported = {
      ...tool("kimi-code", "Kimi Code"),
      capabilities: {
        ...tool("kimi-code", "Kimi Code").capabilities,
        canManageProvider: false,
      },
    };
    mount(() => {
      providerReads += 1;
      return HttpResponse.json([]);
    }, [unsupported]);

    expect(
      await screen.findByText(
        en.services.scopeUnsupportedTitle.replace("{{tool}}", "Kimi Code"),
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: "Kimi Code, Not integrated" }),
    ).toBeInTheDocument();
    expect(providerReads).toBe(0);
    expect(
      screen.queryByRole("button", { name: en.services.action.add }),
    ).not.toBeInTheDocument();
  });

  it("reports OpenCode recent history and adds configuration without claiming a model switch", async () => {
    const saved = service({ tool: "opencode", additive: true, apiKey: null });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        HttpResponse.json({
          tool: "opencode",
          liveConfigPaths: ["~/.config/opencode/opencode.jsonc"],
          resources: [],
          storage: {
            totalBytes: 0,
            sessionBytes: 0,
            sessionCount: 0,
            measurementLimited: false,
          },
          effectiveConnection: {
            selection: "recentModel",
            model: "relay/openai/model",
            endpoint: "https://relay.example.com",
            endpointSource: {
              kind: "liveConfig",
              path: "~/.config/opencode/opencode.jsonc",
            },
            credential: "configured",
            credentialSource: {
              kind: "liveConfig",
              path: "~/.local/share/opencode/auth.json",
            },
            providerId: "relay",
            shellInspected: true,
          },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_activation_prepare`, () =>
        HttpResponse.json({
          status: "notChecked",
          originProviderId: null,
          activeProviderId: null,
          providers: [saved],
          checks: [],
        }),
      ),
    );
    mount([saved], [tool("opencode", "OpenCode")]);
    expect(
      await screen.findByText(en.services.card.recentModel),
    ).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("relay/openai/model");
    expect(screen.queryByText(en.services.card.inUse)).not.toBeInTheDocument();
    expect(
      screen.getByText(en.services.effective.credential.configured),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Add My Relay to tool" }),
    );
    await waitFor(() =>
      expect(toastMocks.success).toHaveBeenCalledWith(
        "My Relay added to tool configuration",
        expect.objectContaining({
          description: en.services.switch.chooseModelHint.replace(
            "{{tool}}",
            "OpenCode",
          ),
        }),
      ),
    );
    expect(screen.getByText(en.services.card.recentModel)).toBeInTheDocument();
  });

  it("puts the card actually in effect first and marks the DB selection overridden", async () => {
    const relayA = service({
      id: "relay-a",
      name: "Relay A",
      active: true,
      canRemove: false,
    });
    const relayB = service({ id: "relay-b", name: "Relay B", active: false });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        HttpResponse.json({
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
            endpoint: "https://relay-b.example.com",
            endpointSource: {
              kind: "liveConfig",
              path: "~/.claude/settings.json",
            },
            credential: "configured",
            credentialSource: {
              kind: "liveConfig",
              path: "~/.claude/settings.json",
            },
            providerId: "relay-b",
            shellInspected: true,
          },
        }),
      ),
    );
    mount([relayA, relayB]);
    await screen.findByText("Relay A");

    const inUseCard = screen.getByRole("article", { name: "Relay B" });
    expect(
      within(inUseCard).getByText(en.services.card.inUse),
    ).toBeInTheDocument();

    const overriddenCard = screen.getByRole("article", { name: "Relay A" });
    expect(
      within(overriddenCard).getByText(
        en.services.card.overriddenBy.replace(
          "{{source}}",
          "~/.claude/settings.json",
        ),
      ),
    ).toBeInTheDocument();

    const cards = screen.getAllByRole("article");
    expect(cards[0]).toBe(inUseCard);
    expect(cards[1]).toBe(overriddenCard);
    // liveConfig is the same config these cards already manage: the source note adds no new information, so don't repeat it.
    expect(
      within(inUseCard).queryByText("From ~/.claude/settings.json"),
    ).not.toBeInTheDocument();
  });

  it("no longer shows the local context and storage panel, only the card's own open-config action", async () => {
    const relayA = service({
      id: "relay-a",
      name: "Relay A",
      active: true,
      canRemove: false,
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        HttpResponse.json({
          tool: "claude-code",
          liveConfigPaths: ["~/.claude/settings.json"],
          resources: [
            {
              id: "live-config-0",
              kind: "configuration",
              scope: "global",
              path: "~/.claude/settings.json",
              exists: true,
              action: "browse",
              sizeBytes: 512,
              measurementLimited: false,
            },
          ],
          storage: {
            totalBytes: 512,
            sessionBytes: 0,
            sessionCount: 0,
            measurementLimited: false,
          },
          effectiveConnection: {
            endpoint: "https://relay-a.example.com",
            endpointSource: {
              kind: "liveConfig",
              path: "~/.claude/settings.json",
            },
            credential: "configured",
            credentialSource: {
              kind: "liveConfig",
              path: "~/.claude/settings.json",
            },
            providerId: "relay-a",
            shellInspected: true,
          },
        }),
      ),
    );
    mount([relayA]);
    await screen.findByText("Relay A");

    // That panel has moved to the "Local Data" page (ADR-0036); the endpoints page only
    // keeps the card's own "open config file" action — see ServicesOpenConfigAction and
    // tests/pages/data/ProviderRuntimeContextPanel.test.tsx.
    expect(screen.queryByText(en.services.runtime.title)).toBeNull();
    expect(
      await screen.findByRole("button", {
        name: en.services.runtime.openConfigFolder,
      }),
    ).toBeInTheDocument();
  });

  it("names the shell/env source on the card actually in effect, since the list alone can't explain it", async () => {
    const relayA = service({
      id: "relay-a",
      name: "Relay A",
      active: true,
      canRemove: false,
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        HttpResponse.json({
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
            providerId: "relay-a",
            shellInspected: true,
          },
        }),
      ),
    );
    mount([relayA]);
    await screen.findByText("Relay A");

    const inUseCard = screen.getByRole("article", { name: "Relay A" });
    expect(
      within(inUseCard).getByText(en.services.card.inUse),
    ).toBeInTheDocument();
    expect(
      within(inUseCard).getByText(
        "From ANTHROPIC_BASE_URL in ~/.config/zsh/secrets.zsh:3",
      ),
    ).toBeInTheDocument();
  });

  it("gives the unmatched live connection a card in the list instead of a region above it", async () => {
    const relayA = service({
      id: "relay-a",
      name: "Relay A",
      active: true,
      canRemove: false,
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        HttpResponse.json({
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
            endpoint: "https://owefskgisdes.game3.co/v1",
            endpointSource: {
              kind: "liveConfig",
              path: "~/.codex/config.toml",
            },
            credential: "configured",
            credentialSource: {
              kind: "liveConfig",
              path: "~/.codex/config.toml",
            },
            providerId: null,
            shellInspected: true,
          },
        }),
      ),
    );
    mount([relayA]);
    await screen.findByText("Relay A");

    const cards = screen.getAllByRole("article");
    expect(cards).toHaveLength(2);
    const liveCard = cards[0];
    expect(
      within(liveCard).getByText("https://owefskgisdes.game3.co/v1"),
    ).toBeInTheDocument();
    expect(
      within(liveCard).getByText(en.services.external.badge),
    ).toBeInTheDocument();
    expect(
      within(liveCard).getByText("From ~/.codex/config.toml"),
    ).toBeInTheDocument();
    expect(
      within(liveCard).getByText(en.services.card.inUse),
    ).toBeInTheDocument();
    // Every tool must share the same layout: no separate block above the list saying the same thing.
    expect(
      liveCard.compareDocumentPosition(cards[1]) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("does not retain an in-use badge when a runtime refresh fails with cached data", async () => {
    let failed = false;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        failed
          ? HttpResponse.text("unreadable fixture", { status: 500 })
          : HttpResponse.json({
              tool: "claude-code",
              liveConfigPaths: [],
              resources: [],
              storage: {
                totalBytes: 0,
                sessionBytes: 0,
                sessionCount: 0,
                measurementLimited: false,
              },
              effectiveConnection: {
                selection: "configuration",
                model: null,
                endpoint: "https://relay.example.com",
                endpointSource: {
                  kind: "liveConfig",
                  path: "~/.claude/settings.json",
                },
                credential: "configured",
                credentialSource: {
                  kind: "liveConfig",
                  path: "~/.claude/settings.json",
                },
                providerId: "relay",
                shellInspected: true,
              },
            }),
      ),
    );
    const { client } = mount([service({ active: true })]);
    expect(await screen.findByText(en.services.card.inUse)).toBeInTheDocument();
    failed = true;
    await act(async () => {
      await client.invalidateQueries({
        queryKey: providerKeys.runtimeContext("claude-code"),
      });
    });
    expect(
      await screen.findByText(en.services.card.unknown),
    ).toBeInTheDocument();
    expect(screen.queryByText(en.services.card.inUse)).not.toBeInTheDocument();
    expect(screen.getByText("My Relay")).toBeInTheDocument();
  });

  it("keeps the list alone when a saved endpoint explains the live connection", async () => {
    const relayA = service({
      id: "relay-a",
      name: "Relay A",
      active: true,
      canRemove: false,
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        HttpResponse.json({
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
              kind: "liveConfig",
              path: "~/.claude/settings.json",
            },
            credential: "configured",
            credentialSource: {
              kind: "liveConfig",
              path: "~/.claude/settings.json",
            },
            providerId: "relay-a",
            shellInspected: true,
          },
        }),
      ),
    );
    mount([relayA]);
    await screen.findByText("Relay A");

    expect(screen.getAllByRole("article")).toHaveLength(1);
    expect(
      screen.queryByText(en.services.external.badge),
    ).not.toBeInTheDocument();
  });

  it("says so when the terminal environment could not be read", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        HttpResponse.json({
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
              kind: "liveConfig",
              path: "~/.claude/settings.json",
            },
            credential: "configured",
            credentialSource: {
              kind: "liveConfig",
              path: "~/.claude/settings.json",
            },
            providerId: "relay-a",
            shellInspected: false,
          },
        }),
      ),
    );
    mount([service({ id: "relay-a", name: "Relay A", active: true })]);

    expect(
      await screen.findByRole("alert", {
        name: en.services.effective.shellNotInspectedTitle,
      }),
    ).toBeInTheDocument();
  });

  it("offers per-service checks without a Check all action", async () => {
    mount([service(), service({ id: "local", name: "Local" })]);
    await screen.findByText("My Relay");

    expect(
      screen.getByRole("button", {
        name: en.services.action.testNamed.replace("{{name}}", "My Relay"),
      }),
    ).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: en.services.action.testAll }),
    ).not.toBeInTheDocument();
  });

  it("uses a compact page heading and puts Add beside the endpoint count", async () => {
    mount([service()]);
    await screen.findByText("My Relay");
    const heading = screen.getByRole("heading", { name: en.services.title });
    const serviceCard = screen.getByRole("article", { name: "My Relay" });
    const connect = screen.getByRole("button", {
      name: en.services.action.add,
    });
    expect(connect).not.toHaveClass("w-full");
    expect(heading.closest(".spatial-page-hero")).toBeNull();
    expect(
      connect.compareDocumentPosition(serviceCard) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: en.services.action.testAll }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: en.services.refresh }),
    ).not.toBeInTheDocument();
  });

  it("says so honestly when a tool has no services yet", async () => {
    mount([]);
    expect(
      await screen.findByText(en.services.empty.title),
    ).toBeInTheDocument();
  });

  it("offers backend-owned presets as an optional fast-fill flow", async () => {
    mount([service()]);
    await screen.findByText("My Relay");
    await userEvent.click(
      screen.getByRole("button", { name: en.services.action.add }),
    );
    await screen.findByRole("dialog");
    const preset = await screen.findByRole("combobox", {
      name: en.services.connect.preset,
    });
    expect(preset).toHaveTextContent("Anthropic API");
    expect(screen.getByLabelText(en.services.connect.model)).toHaveValue(
      "claude-sonnet-5",
    );
  });

  it("shows connection setup loading without calling it a failure", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_connection_profile`,
        async () => {
          await delay("infinite");
          return HttpResponse.json({});
        },
      ),
    );
    mount([service()]);

    await screen.findByText("My Relay");
    expect(screen.queryByRole("alert")).toBeNull();
    const connect = screen.getByRole("button", {
      name: en.services.action.add,
    });
    expect(connect).toHaveAttribute("aria-busy", "true");
    expect(connect).toBeDisabled();
  });

  it("recovers when the safe connection template cannot be read", async () => {
    let profileReads = 0;
    let releaseRetry: (() => void) | undefined;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_connection_profile`,
        async () => {
          profileReads += 1;
          if (profileReads === 1) {
            return HttpResponse.json(
              {
                code: "UPSTREAM_ERROR",
                messageKey: "error.provider.listFailed",
                technicalMessage: "private template path",
                remediation: null,
                contextId: null,
              },
              { status: 500 },
            );
          }
          await new Promise<void>((resolve) => {
            releaseRetry = resolve;
          });
          return HttpResponse.json({
            defaultPresetId: "official",
            modelRequired: false,
            presets: [
              {
                id: "official",
                serviceName: "Anthropic API",
                defaultName: "Anthropic",
                defaultModel: "claude-sonnet-5",
                baseUrl: "https://api.example.test",
                websiteUrl: "https://www.anthropic.com",
                apiKeyUrl: "https://console.anthropic.com",
                official: true,
              },
            ],
          });
        },
      ),
    );
    mount([service()]);

    const alert = await screen.findByRole("alert", {
      name: en.services.connect.profileErrorTitle,
    });
    expect(alert).toHaveTextContent(en.services.connect.profileError);
    expect(alert).not.toHaveTextContent("private template path");
    expect(
      screen.getByRole("button", { name: en.services.action.add }),
    ).toBeDisabled();

    const retry = within(alert).getByRole("button", {
      name: en.services.connect.profileRetry,
    });
    await userEvent.click(retry);
    await waitFor(() => expect(profileReads).toBe(2));
    await waitFor(() => expect(retry).toHaveAttribute("aria-busy", "true"));
    expect(retry).toBeDisabled();

    releaseRetry?.();
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    const connect = screen.getByRole("button", {
      name: en.services.action.add,
    });
    expect(connect).toBeEnabled();
    await userEvent.click(connect);
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
  });

  it("connects a service from the empty state without exposing raw config", async () => {
    const seen: unknown[] = [];
    const checked: unknown[] = [];
    mount([]);
    await screen.findByText(en.services.empty.title);
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_create`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            providers: [
              service({
                id: "anthropic",
                name: "Anthropic",
                active: true,
                canRemove: false,
                baseUrl: "https://api.anthropic.com",
              }),
            ],
            createdProviderId: "anthropic",
          });
        },
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
    await userEvent.click(
      screen.getByRole("button", { name: en.services.action.add }),
    );
    await screen.findByRole("dialog");
    await screen.findByRole("combobox", {
      name: en.services.connect.preset,
    });
    await userEvent.type(
      screen.getByLabelText(en.services.connect.key),
      "sk-secret",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );
    await waitFor(() => expect(seen).toHaveLength(1));
    expect(seen).toEqual([
      {
        tool: "claude-code",
        requestId: expect.any(String),
        draft: {
          presetId: "official",
          name: "Anthropic",
          apiKey: "sk-secret",
          model: "claude-sonnet-5",
        },
      },
    ]);
    expect((seen[0] as { requestId?: unknown } | undefined)?.requestId).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    expect(JSON.stringify(seen)).not.toContain("settingsConfig");
    await waitFor(() =>
      expect(checked).toEqual([{ tool: "claude-code", provider: "anthropic" }]),
    );
    expect(
      await screen.findByText(en.services.card.unknown),
    ).toBeInTheDocument();
    expect(screen.getByText(en.services.test.operational)).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: en.services.start.action.replace("{{tool}}", "Claude Code"),
      }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(JSON.stringify(toastMocks.success.mock.calls)).not.toContain(
      "sk-secret",
    );
  });

  it("routes the default custom form through the separate typed command", async () => {
    const seen: unknown[] = [];
    mount([], [tool("claude-code", "Claude Code")], null, null, true);
    await screen.findByText(en.services.empty.title);
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_custom_create`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            providers: [
              service({
                id: "private-relay",
                name: "Private relay",
                active: true,
                canRemove: false,
                baseUrl: "https://relay.example.test/v1",
              }),
            ],
            createdProviderId: "private-relay",
          });
        },
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.json({
          providerId: "private-relay",
          reachability: "operational",
          responseTimeMs: 80,
          httpStatus: 200,
        }),
      ),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.services.action.add }),
    );
    const dialog = await screen.findByRole("dialog");
    // Name, address and model all arrive prefilled from the preset now, so a
    // typed value has to replace rather than append.
    const name = within(dialog).getByLabelText(en.services.connect.name);
    await userEvent.clear(name);
    await userEvent.type(name, "Private relay");
    const address = within(dialog).getByLabelText(en.services.connect.baseUrl);
    await userEvent.clear(address);
    await userEvent.type(address, "https://relay.example.test/v1");
    await userEvent.type(
      within(dialog).getByLabelText(en.services.connect.key),
      "sk-private",
    );
    const model = within(dialog).getByLabelText(en.services.connect.model);
    await userEvent.clear(model);
    await userEvent.type(model, "model-a");
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.ds.action.connect }),
    );

    await waitFor(() => expect(seen).toHaveLength(1));
    expect(seen).toEqual([
      {
        tool: "claude-code",
        requestId: expect.any(String),
        draft: {
          name: "Private relay",
          apiKey: "sk-private",
          model: "model-a",
          baseUrl: "https://relay.example.test/v1",
        },
      },
    ]);
    expect(JSON.stringify(seen)).not.toContain("settingsConfig");
    expect(JSON.stringify(seen)).not.toContain("headers");
    expect(await screen.findByText("Private relay")).toBeVisible();
  });

  it("refreshes a failed connection, keeps its draft inline, and retries one intent", async () => {
    const bodies: Array<Record<string, unknown>> = [];
    let attempts = 0;
    let requestId = "";
    let releaseRefresh: (() => void) | undefined;
    mount([service()]);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, async () => {
        await new Promise<void>((resolve) => {
          releaseRefresh = resolve;
        });
        return HttpResponse.json([
          service({
            id: `aimgr-connect-a-${requestId}`,
            name: "Saved Partially",
            active: true,
            canRemove: false,
          }),
        ]);
      }),
      http.post(
        `${TAURI_ENDPOINT}/app_provider_create`,
        async ({ request }) => {
          attempts += 1;
          const body = (await request.json()) as Record<string, unknown>;
          bodies.push(body);
          requestId = String(body.requestId ?? "");
          if (attempts === 1) {
            return HttpResponse.json(
              {
                code: "CONFIG_WRITE_FAILED",
                messageKey: "error.provider.createFailed",
                technicalMessage:
                  "permission denied at /private/tool.json token=never-render",
                remediation: "error.remediation.checkConnectionSettings",
                contextId: null,
              },
              { status: 500 },
            );
          }
          const id = `aimgr-connect-a-${requestId}`;
          return HttpResponse.json({
            providers: [
              service({
                id,
                name: "Anthropic",
                active: true,
                canRemove: false,
                baseUrl: "https://api.anthropic.com",
              }),
            ],
            createdProviderId: id,
          });
        },
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.json({
          providerId: `aimgr-connect-a-${requestId}`,
          reachability: "operational",
          responseTimeMs: 240,
          httpStatus: 200,
        }),
      ),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.services.action.add }),
    );
    await screen.findByRole("dialog");
    await screen.findByRole("combobox", {
      name: en.services.connect.preset,
    });
    const presetDialog = screen.getByRole("dialog");
    const key = within(presetDialog).getByLabelText(en.services.connect.key);
    await userEvent.type(key, "sk-typed-but-not-saved");
    const control = within(presetDialog).getByRole("button", {
      name: en.ds.action.connect,
    });
    await userEvent.click(control);

    await waitFor(() => expect(releaseRefresh).toBeTypeOf("function"));
    expect(requestId).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    expect(control).toBeDisabled();
    expect(within(presetDialog).queryByRole("alert")).toBeNull();

    releaseRefresh?.();
    const alert = await within(presetDialog).findByRole("alert", {
      name: "Could not finish connecting this service",
    });
    expect(alert).toHaveTextContent(en.error.provider.createFailed);
    expect(alert).toHaveTextContent(
      en.error.remediation.checkConnectionSettings,
    );
    expect(alert).not.toHaveTextContent("/private/tool.json");
    expect(alert).not.toHaveTextContent("never-render");
    expect(alert).not.toHaveTextContent("sk-typed-but-not-saved");
    expect(toastMocks.error).not.toHaveBeenCalled();
    expect(key).toHaveValue("sk-typed-but-not-saved");

    const retry = within(presetDialog).getByRole("button", {
      name: "Try connecting this service again",
    });
    expect(retry).toBe(control);
    expect(retry).toHaveFocus();
    await userEvent.click(retry);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(bodies).toHaveLength(2);
    expect(bodies[0]?.requestId).toBe(requestId);
    expect(bodies[1]?.requestId).toBe(requestId);
    expect(bodies[0]?.draft).toEqual({
      presetId: "official",
      name: "Anthropic",
      apiKey: "sk-typed-but-not-saved",
      model: "claude-sonnet-5",
    });
    expect(bodies[1]?.draft).toEqual(bodies[0]?.draft);
  });

  it("switches to a service but waits for runtime evidence before calling it in use", async () => {
    mount([service()]);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_activation_prepare`, () =>
        HttpResponse.json({
          status: "notChecked",
          originProviderId: "relay",
          activeProviderId: "relay",
          providers: [service({ active: true, canRemove: false })],
          checks: [],
        }),
      ),
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.action.useNamed.replace("{{name}}", "My Relay"),
      }),
    );
    await waitFor(() => expect(toastMocks.success).toHaveBeenCalled());
    expect(
      await screen.findByText(en.services.card.unknown),
    ).toBeInTheDocument();
  });

  it("tells the user to reopen the tool after a successful switch and offers to open it", async () => {
    mount([service()]);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_activation_prepare`, () =>
        HttpResponse.json({
          status: "ready",
          originProviderId: "relay",
          activeProviderId: "relay",
          providers: [service({ active: true, canRemove: false })],
          checks: [
            {
              providerId: "relay",
              reachability: "operational",
              responseTimeMs: 74,
              httpStatus: 200,
            },
          ],
        }),
      ),
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.action.useNamed.replace("{{name}}", "My Relay"),
      }),
    );
    await waitFor(() => expect(toastMocks.success).toHaveBeenCalled());
    expect(
      await screen.findByText(en.services.card.unknown),
    ).toBeInTheDocument();
    expect(toastMocks.success).toHaveBeenCalledWith(
      en.services.switch.appliedNamed.replace("{{name}}", "My Relay"),
      expect.objectContaining({
        description: en.services.switch.reopenHint.replace(
          "{{tool}}",
          "Claude Code",
        ),
        action: expect.objectContaining({ label: en.services.switch.openNow }),
      }),
    );

    const options = toastMocks.success.mock.calls.at(-1)?.[1] as {
      action: { onClick: () => void };
    };
    act(() => options.action.onClick());
    expect(
      await screen.findByRole("dialog", { name: "Open Claude Code" }),
    ).toBeInTheDocument();
  });

  it("does not announce a switch when the address check found the service unreachable", async () => {
    const relay = service();
    const primary = service({
      id: "primary",
      name: "Primary",
      active: true,
      canRemove: false,
    });
    const backup = service({ id: "backup", name: "Backup" });
    mount([relay, primary, backup]);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_activation_prepare`, () =>
        HttpResponse.json({
          status: "unreachable",
          originProviderId: "relay",
          activeProviderId: "primary",
          providers: [relay, primary, backup],
          checks: [
            {
              providerId: "relay",
              reachability: "failed",
              responseTimeMs: null,
              httpStatus: null,
            },
          ],
        }),
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "Use My Relay" }));
    const relayCard = screen.getByRole("article", { name: "My Relay" });
    await within(relayCard).findByRole("button", {
      name: en.services.failover.tryNextNamed.replace("{{name}}", "My Relay"),
    });
    expect(toastMocks.success).not.toHaveBeenCalled();
  });

  it("switches to the next reachable saved service in two explicit clicks", async () => {
    const relay = service();
    const primary = service({
      id: "primary",
      name: "Primary",
      active: true,
      canRemove: false,
    });
    const backup = service({ id: "backup", name: "Backup" });
    const activationBodies: unknown[] = [];
    const recoveryBodies: unknown[] = [];
    mount([relay, primary, backup]);
    await screen.findByText("My Relay");
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_activation_prepare`,
        async ({ request }) => {
          activationBodies.push(await request.json());
          return HttpResponse.json({
            status: "unreachable",
            originProviderId: "relay",
            activeProviderId: "primary",
            providers: [relay, primary, backup],
            checks: [
              {
                providerId: "relay",
                reachability: "failed",
                responseTimeMs: null,
                httpStatus: null,
              },
            ],
          });
        },
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_provider_next_healthy`,
        async ({ request }) => {
          recoveryBodies.push(await request.json());
          return HttpResponse.json({
            status: "failedOver",
            originProviderId: "relay",
            activeProviderId: "backup",
            providers: [
              relay,
              { ...primary, active: false, canRemove: true },
              { ...backup, active: true, canRemove: false },
            ],
            checks: [
              {
                providerId: "backup",
                reachability: "operational",
                responseTimeMs: 74,
                httpStatus: 200,
              },
            ],
          });
        },
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "Use My Relay" }));
    const relayCard = screen.getByRole("article", { name: "My Relay" });
    const tryNext = await within(relayCard).findByRole("button", {
      name: en.services.failover.tryNextNamed.replace("{{name}}", "My Relay"),
    });
    expect(activationBodies).toEqual([
      { tool: "claude-code", provider: "relay" },
    ]);
    expect(recoveryBodies).toEqual([]);

    await userEvent.click(tryNext);

    await waitFor(() =>
      expect(recoveryBodies).toEqual([
        { tool: "claude-code", failedProvider: "relay" },
      ]),
    );
    expect(
      within(screen.getByRole("article", { name: "Backup" })).getByText(
        en.services.card.unknown,
      ),
    ).toBeInTheDocument();
    expect(toastMocks.success).toHaveBeenCalledWith(
      "Switched from My Relay to the available service Backup.",
      expect.objectContaining({
        description: en.services.switch.reopenHint.replace(
          "{{tool}}",
          "Claude Code",
        ),
        action: expect.objectContaining({ label: en.services.switch.openNow }),
      }),
    );
  });

  it("keeps failed recovery feedback visible when no saved alternative responds", async () => {
    const relay = service();
    const primary = service({
      id: "primary",
      name: "Primary",
      active: true,
      canRemove: false,
    });
    const backup = service({ id: "backup", name: "Backup" });
    mount([relay, primary, backup]);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_activation_prepare`, () =>
        HttpResponse.json({
          status: "unreachable",
          originProviderId: "relay",
          activeProviderId: "primary",
          providers: [relay, primary, backup],
          checks: [
            {
              providerId: "relay",
              reachability: "failed",
              responseTimeMs: null,
              httpStatus: null,
            },
          ],
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_next_healthy`, () =>
        HttpResponse.json({
          status: "unreachable",
          originProviderId: "relay",
          activeProviderId: "primary",
          providers: [relay, primary, backup],
          checks: [
            {
              providerId: "backup",
              reachability: "failed",
              responseTimeMs: null,
              httpStatus: null,
            },
          ],
        }),
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "Use My Relay" }));
    const relayCard = screen.getByRole("article", { name: "My Relay" });
    await userEvent.click(
      await within(relayCard).findByRole("button", {
        name: en.services.failover.tryNextNamed.replace("{{name}}", "My Relay"),
      }),
    );

    expect(
      await within(relayCard).findByText(en.services.failover.noneAvailable),
    ).toBeVisible();
    expect(within(relayCard).getByText(en.services.test.failed)).toBeVisible();
    expect(
      within(relayCard).getByRole("button", { name: "Edit My Relay" }),
    ).toBeEnabled();
  });

  it("refreshes a failed service switch, keeps it on the right card, and retries the same service", async () => {
    const inventory = [
      service(),
      service({
        id: "primary",
        name: "Primary",
        active: true,
        canRemove: false,
      }),
    ];
    const refreshed = [
      service({ active: true, canRemove: false }),
      service({
        id: "primary",
        name: "Primary",
        active: false,
        canRemove: true,
      }),
    ];
    const bodies: unknown[] = [];
    let attempts = 0;
    let releaseRefresh: (() => void) | undefined;
    mount(inventory);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, async () => {
        await new Promise<void>((resolve) => {
          releaseRefresh = resolve;
        });
        return HttpResponse.json(refreshed);
      }),
      http.post(
        `${TAURI_ENDPOINT}/app_provider_activation_prepare`,
        async ({ request }) => {
          attempts += 1;
          bodies.push(await request.json());
          if (attempts === 1) {
            return HttpResponse.json(
              {
                code: "CONFIG_WRITE_FAILED",
                messageKey: "error.provider.switchFailed",
                technicalMessage: "permission denied at /private/tool.json",
                remediation: "error.remediation.checkPermissions",
                contextId: null,
              },
              { status: 500 },
            );
          }
          return HttpResponse.json({
            status: "notChecked",
            originProviderId: "relay",
            activeProviderId: "relay",
            providers: refreshed,
            checks: [],
          });
        },
      ),
    );
    const control = screen.getByRole("button", { name: "Use My Relay" });

    await userEvent.click(control);
    await waitFor(() => expect(releaseRefresh).toBeTypeOf("function"));
    const card = screen.getByRole("article", { name: "My Relay" });
    expect(card).toHaveAttribute("aria-busy", "true");
    expect(control).toBeDisabled();
    expect(within(card).queryByRole("alert")).toBeNull();
    const otherCard = screen.getByRole("article", { name: "Primary" });
    expect(otherCard).not.toHaveAttribute("aria-busy");
    expect(
      within(otherCard).getByRole("button", { name: "Edit Primary" }),
    ).toBeDisabled();

    releaseRefresh?.();
    const alert = await within(card).findByRole("alert", {
      name: "Could not finish switching to My Relay",
    });
    expect(alert).toHaveTextContent(en.error.provider.switchFailed);
    expect(alert).toHaveTextContent(en.error.remediation.checkPermissions);
    expect(alert).toHaveTextContent(
      "AI Manager refreshed the saved service state. Try the same switch again.",
    );
    expect(alert).not.toHaveTextContent("/private/tool.json");
    expect(within(otherCard).queryByRole("alert")).toBeNull();
    expect(toastMocks.error).not.toHaveBeenCalled();

    const retry = within(card).getByRole("button", {
      name: "Try switching to My Relay again",
    });
    expect(retry).toBe(control);
    expect(retry).toHaveFocus();
    expect(
      within(card).getByText(en.services.card.unknown),
    ).toBeInTheDocument();
    await userEvent.click(retry);
    await waitFor(() => expect(within(card).queryByRole("alert")).toBeNull());
    expect(
      within(card).queryByRole("button", {
        name: "Try switching to My Relay again",
      }),
    ).toBeNull();
    expect(bodies).toEqual([
      { tool: "claude-code", provider: "relay" },
      { tool: "claude-code", provider: "relay" },
    ]);
  });

  it("turns a saved service into the existing safe folder launch flow", async () => {
    const seen: unknown[] = [];
    mount([service()]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("launched");
      }),
    );

    const start = await screen.findByRole("button", {
      name: en.services.start.action.replace("{{tool}}", "Claude Code"),
    });
    expect(
      screen.getByRole("heading", {
        name: en.services.endpoints.title.replace("{{tool}}", "Claude Code"),
      }),
    ).toBeInTheDocument();
    await userEvent.click(start);

    const dialog = screen.getByRole("dialog", { name: "Open Claude Code" });
    expect(
      within(dialog).getByText(en.tools.open.localTitle),
    ).toBeInTheDocument();
    expect(seen).toEqual([]);

    await userEvent.click(
      within(dialog).getByRole("button", { name: en.tools.open.confirm }),
    );
    await waitFor(() =>
      expect(seen).toEqual([{ tool: "claude-code", directoryMode: "default" }]),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(toastMocks.success).toHaveBeenCalledWith(
      en.tools.open.launched.replace("{{name}}", "Claude Code"),
    );
  });

  it("does not offer Start when the tool cannot be launched on this platform", async () => {
    mount(
      [service()],
      [
        {
          ...tool("claude-code", "Claude Code"),
          capabilities: {
            ...tool("claude-code", "Claude Code").capabilities,
            canLaunch: false,
          },
        },
      ],
    );
    await screen.findByText("My Relay");

    expect(
      screen.queryByRole("button", {
        name: en.services.start.action.replace("{{tool}}", "Claude Code"),
      }),
    ).not.toBeInTheDocument();
  });

  it("does not show provider or global prompt controls for an uninstalled tool", async () => {
    mount(
      [service()],
      [{ ...tool("claude-code", "Claude Code"), status: "notInstalled" }],
    );

    expect(
      await screen.findByText(en.services.noTools.title),
    ).toBeInTheDocument();
    expect(screen.queryByText("My Relay")).not.toBeInTheDocument();
    expect(screen.queryByText("~/.claude/CLAUDE.md")).not.toBeInTheDocument();
  });

  it("keeps Start available when the installed tool has an update", async () => {
    mount(
      [service()],
      [
        {
          ...tool("claude-code", "Claude Code"),
          status: "updateAvailable",
          latestVersion: "1.1.0",
        },
      ],
    );

    expect(
      await screen.findByRole("button", {
        name: en.services.start.action.replace("{{tool}}", "Claude Code"),
      }),
    ).toBeInTheDocument();
  });

  it("reports the outcome of a check on the card that was checked", async () => {
    mount([service()]);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.json({
          providerId: "relay",
          reachability: "operational",
          responseTimeMs: 240,
          httpStatus: 200,
        }),
      ),
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.action.testNamed.replace("{{name}}", "My Relay"),
      }),
    );

    // The address check is the test dialog's first line, and it writes to the
    // same connectivity cache the card reads, so closing leaves the badge behind.
    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText(en.services.test.operational),
    ).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.ds.action.close }),
    );

    const card = await screen.findByRole("article", { name: "My Relay" });
    expect(
      within(card).getByText(en.services.test.operational),
    ).toBeInTheDocument();
  });

  it("keeps a failed address check on its card and retries the exact service", async () => {
    let attempts = 0;
    const checked: unknown[] = [];
    mount([service()]);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, async ({ request }) => {
        attempts += 1;
        checked.push(await request.json());
        if (attempts === 1) {
          return HttpResponse.json(
            {
              code: "PROVIDER_UNREACHABLE",
              messageKey: "error.provider.testFailed",
              technicalMessage:
                "connection refused at 10.0.0.1 token=never-render",
              remediation: "error.remediation.checkServiceSettings",
              contextId: null,
            },
            { status: 500 },
          );
        }
        return HttpResponse.json({
          providerId: "relay",
          reachability: "operational",
          responseTimeMs: 240,
          httpStatus: 200,
        });
      }),
    );

    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.action.testNamed.replace("{{name}}", "My Relay"),
      }),
    );

    const dialog = await screen.findByRole("dialog");
    const alert = await within(dialog).findByRole("alert", {
      name: "Could not check the address for My Relay",
    });
    expect(alert).toHaveTextContent(en.error.provider.testFailed);
    expect(alert).toHaveTextContent(en.error.remediation.checkServiceSettings);
    expect(alert).not.toHaveTextContent("10.0.0.1");
    expect(alert).not.toHaveTextContent("never-render");
    expect(toastMocks.error).not.toHaveBeenCalled();

    await userEvent.click(
      within(dialog).getByRole("button", { name: "Check My Relay again" }),
    );
    expect(
      await within(dialog).findByText(en.services.test.operational),
    ).toBeInTheDocument();
    expect(
      within(dialog).queryByRole("alert", {
        name: "Could not check the address for My Relay",
      }),
    ).not.toBeInTheDocument();
    expect(checked).toEqual([
      { tool: "claude-code", provider: "relay" },
      { tool: "claude-code", provider: "relay" },
    ]);
  });

  it("saves an edit through the dialog", async () => {
    mount([service()]);
    await screen.findByText("My Relay");
    const seen: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_save`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json([service({ name: "Renamed" })]);
      }),
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.action.editNamed.replace("{{name}}", "My Relay"),
      }),
    );
    const dialog = await screen.findByRole("dialog");
    const name = within(dialog).getByLabelText(en.services.form.name);
    await userEvent.clear(name);
    await userEvent.type(name, "Renamed");
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.services.form.save }),
    );
    await waitFor(() =>
      expect(seen).toEqual([
        {
          tool: "claude-code",
          provider: "relay",
          draft: {
            name: "Renamed",
            apiKey: null,
          },
        },
      ]),
    );
  });

  it("shows and saves backend-supported endpoint settings directly", async () => {
    const seen: unknown[] = [];
    mount([service()], undefined, null, null, true);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_save`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json([
          service({ baseUrl: "https://new.example.com/v1" }),
        ]);
      }),
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.action.editNamed.replace("{{name}}", "My Relay"),
      }),
    );
    const baseUrl = await screen.findByLabelText(en.services.form.baseUrl);
    await userEvent.clear(baseUrl);
    await userEvent.type(baseUrl, "https://new.example.com/v1");
    const model = screen.getByLabelText("Model 1");
    await userEvent.clear(model);
    await userEvent.type(model, "claude-opus-5");
    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.save }),
    );

    await waitFor(() =>
      expect(seen).toEqual([
        {
          tool: "claude-code",
          provider: "relay",
          draft: {
            name: "My Relay",
            apiKey: null,
            models: ["claude-opus-5"],
            advanced: {
              baseUrlChanged: true,
              baseUrl: "https://new.example.com/v1",
              endpointCandidates: [
                "https://relay-backup.example.com",
                "https://relay.example.com",
                "https://new.example.com/v1",
              ],
              endpointAutoSelect: null,
              headers: null,
            },
          },
        },
      ]),
    );
  });

  it("refreshes a failed edit, keeps its draft inline, and retries the same save", async () => {
    const bodies: unknown[] = [];
    let attempts = 0;
    let releaseRefresh: (() => void) | undefined;
    mount([service()]);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, async () => {
        await new Promise<void>((resolve) => {
          releaseRefresh = resolve;
        });
        return HttpResponse.json([service({ name: "Saved Partially" })]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_provider_save`, async ({ request }) => {
        attempts += 1;
        bodies.push(await request.json());
        if (attempts === 1) {
          return HttpResponse.json(
            {
              code: "CONFIG_WRITE_FAILED",
              messageKey: "error.provider.saveFailed",
              technicalMessage:
                "permission denied at /private/tool.json token=never-render",
              remediation: "error.remediation.checkServiceSettings",
              contextId: null,
            },
            { status: 500 },
          );
        }
        return HttpResponse.json([service()]);
      }),
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.action.editNamed.replace("{{name}}", "My Relay"),
      }),
    );
    const dialog = await screen.findByRole("dialog");
    const key = within(dialog).getByLabelText(en.services.form.key);
    await userEvent.clear(key);
    await userEvent.type(key, "sk-typed-but-not-saved");
    const control = within(dialog).getByRole("button", {
      name: en.services.form.save,
    });
    await userEvent.click(control);

    await waitFor(() => expect(releaseRefresh).toBeTypeOf("function"));
    expect(control).toBeDisabled();
    expect(within(dialog).queryByRole("alert")).toBeNull();

    releaseRefresh?.();
    const alert = await within(dialog).findByRole("alert", {
      name: "Could not finish saving this service",
    });
    expect(alert).toHaveTextContent(en.error.provider.saveFailed);
    expect(alert).toHaveTextContent(en.error.remediation.checkServiceSettings);
    expect(alert).toHaveTextContent(
      "AI Manager refreshed the saved service state. Review your changes, then try saving again.",
    );
    expect(alert).not.toHaveTextContent("/private/tool.json");
    expect(alert).not.toHaveTextContent("never-render");
    expect(alert).not.toHaveTextContent("sk-typed-but-not-saved");
    expect(toastMocks.error).not.toHaveBeenCalled();
    expect(key).toHaveValue("sk-typed-but-not-saved");

    const retry = within(dialog).getByRole("button", {
      name: "Try saving this service again",
    });
    expect(retry).toBe(control);
    expect(retry).toHaveFocus();
    await userEvent.click(retry);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(bodies).toEqual([
      {
        tool: "claude-code",
        provider: "relay",
        draft: {
          name: "My Relay",
          apiKey: "sk-typed-but-not-saved",
        },
      },
      {
        tool: "claude-code",
        provider: "relay",
        draft: {
          name: "My Relay",
          apiKey: "sk-typed-but-not-saved",
        },
      },
    ]);
  });

  it("removes a service only after showing its exact local impact", async () => {
    const seen: unknown[] = [];
    mount([service()]);
    await screen.findByText("My Relay");
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_remove`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([]);
        },
      ),
    );

    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.action.removeNamed.replace("{{name}}", "My Relay"),
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: en.services.remove.title.replace("{{name}}", "My Relay"),
    });
    expect(dialog).toHaveTextContent(
      en.services.remove.point.local.replace("{{tool}}", "Claude Code"),
    );
    expect(dialog).toHaveTextContent(en.services.remove.point.account);
    expect(dialog).toHaveTextContent(en.services.remove.point.otherTools);

    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.services.remove.confirm,
      }),
    );

    await waitFor(() =>
      expect(seen).toEqual([{ tool: "claude-code", provider: "relay" }]),
    );
    expect(JSON.stringify(seen)).not.toContain("apiKey");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(toastMocks.success).toHaveBeenCalledWith("Removed My Relay");
  });

  it("shows why the active service cannot be removed", async () => {
    mount([service({ active: true, canRemove: false })]);
    const card = await screen.findByRole("article", { name: "My Relay" });

    expect(
      within(card).getByText(en.services.remove.activeHint),
    ).toBeInTheDocument();
    expect(
      within(card).getByRole("button", {
        name: en.services.action.removeNamed.replace("{{name}}", "My Relay"),
      }),
    ).toBeDisabled();
  });

  it("locks dialogs, cards, and header actions while removal is pending", async () => {
    mount([service()]);
    const card = await screen.findByRole("article", { name: "My Relay" });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_remove`, async () => {
        await delay("infinite");
        return HttpResponse.json([]);
      }),
    );

    await userEvent.click(
      within(card).getByRole("button", {
        name: en.services.action.removeNamed.replace("{{name}}", "My Relay"),
      }),
    );
    const dialog = screen.getByRole("dialog");
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.services.remove.confirm,
      }),
    );

    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: en.ds.action.close }),
    ).toBeNull();
    expect(
      screen.getByRole("button", {
        name: en.services.action.add,
        hidden: true,
      }),
    ).toBeDisabled();
    expect(
      screen.queryByRole("button", {
        name: en.services.refresh,
        hidden: true,
      }),
    ).not.toBeInTheDocument();
    expect(
      within(card).getByRole("button", {
        name: en.services.action.editNamed.replace("{{name}}", "My Relay"),
        hidden: true,
      }),
    ).toBeDisabled();
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("keeps one initial list retry busy and restores page focus", async () => {
    let reads = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([tool("claude-code", "Claude Code")]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, async () => {
        reads += 1;
        if (reads === 1) {
          return HttpResponse.text("private provider database path", {
            status: 500,
          });
        }
        await new Promise<void>((resolve) => {
          releaseRetry = resolve;
        });
        return HttpResponse.json([service()]);
      }),
    );
    render(<ServicesPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    const alert = await screen.findByRole("alert", {
      name: en.services.error.title,
    });
    expect(
      screen.getAllByRole("button", { name: en.services.refresh }),
    ).toHaveLength(1);
    const retry = screen.getByRole("button", { name: en.services.refresh });

    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(2));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByRole("status")).toBeNull();
    expect(document.body).not.toHaveTextContent(
      "private provider database path",
    );

    releaseRetry();
    expect(await screen.findByText("My Relay")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByRole("region", { name: en.services.title }),
      ).toHaveFocus(),
    );
  });

  it("does not steal focus when someone leaves an initial list retry", async () => {
    let reads = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([tool("claude-code", "Claude Code")]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, async () => {
        reads += 1;
        if (reads === 1) {
          return HttpResponse.text("provider read failed", { status: 500 });
        }
        await new Promise<void>((resolve) => {
          releaseRetry = resolve;
        });
        return HttpResponse.json([service()]);
      }),
    );
    render(<ServicesPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await screen.findByRole("alert", { name: en.services.error.title });
    const retry = await screen.findByRole("button", {
      name: en.services.refresh,
    });
    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(2));

    const scopeTab = screen.getByRole("tab", { name: "Claude Code" });
    await userEvent.click(scopeTab);
    expect(scopeTab).toHaveFocus();
    releaseRetry();

    expect(await screen.findByText("My Relay")).toBeInTheDocument();
    expect(scopeTab).toHaveFocus();
  });

  it("retains saved services and pauses their actions after refresh failure", async () => {
    const { client } = mount([service()]);
    await screen.findByText("My Relay");
    let refreshReads = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, async () => {
        refreshReads += 1;
        if (refreshReads === 1) {
          return HttpResponse.text("private provider refresh target", {
            status: 500,
          });
        }
        await new Promise<void>((resolve) => {
          releaseRetry = resolve;
        });
        return HttpResponse.json([service({ active: true })]);
      }),
    );

    await client.invalidateQueries({
      queryKey: providerKeys.list("claude-code"),
    });
    const alert = await screen.findByRole("alert", {
      name: en.services.refreshError.title,
    });
    expect(refreshReads).toBe(1);
    expect(alert).toHaveTextContent(en.services.refreshError.description);
    expect(screen.getByText("My Relay")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", {
        name: en.services.endpoints.title.replace("{{tool}}", "Claude Code"),
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: en.services.action.useNamed.replace("{{name}}", "My Relay"),
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: en.services.action.testNamed.replace("{{name}}", "My Relay"),
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: en.services.action.editNamed.replace("{{name}}", "My Relay"),
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: en.services.action.removeNamed.replace("{{name}}", "My Relay"),
      }),
    ).toBeDisabled();
    expect(
      screen
        .getAllByRole("button", { name: en.services.action.add })
        .every((button) => button.hasAttribute("disabled")),
    ).toBe(true);
    expect(screen.getByRole("tab", { name: "Claude Code" })).toBeEnabled();
    const retry = within(alert).getByRole("button", {
      name: en.services.refresh,
    });
    expect(document.body).not.toHaveTextContent(
      "private provider refresh target",
    );

    await userEvent.click(retry);
    await waitFor(() => expect(refreshReads).toBe(2));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByRole("status")).toBeNull();

    releaseRetry();
    await waitFor(() => expect(alert).not.toBeInTheDocument());
    expect(
      screen.getByRole("region", { name: en.services.title }),
    ).toHaveFocus();
    expect(
      screen.getByRole("button", {
        name: en.services.action.testNamed.replace("{{name}}", "My Relay"),
      }),
    ).toBeEnabled();
  });

  it("retains service content and pauses every action while the tool baseline recovers", async () => {
    let reads = 0;
    let releaseFailure!: () => void;
    let releaseRecovery!: () => void;
    const failureGate = new Promise<void>((resolve) => {
      releaseFailure = resolve;
    });
    const recoveryGate = new Promise<void>((resolve) => {
      releaseRecovery = resolve;
    });
    const { client } = mount([service()]);
    await screen.findByText("My Relay");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, async () => {
        reads += 1;
        if (reads === 1) {
          await failureGate;
          return HttpResponse.text("private refreshed tool inventory", {
            status: 500,
          });
        }
        await recoveryGate;
        return HttpResponse.json([tool("claude-code", "Claude Code")]);
      }),
    );

    void client.invalidateQueries({ queryKey: toolKeys.all });
    await waitFor(() => expect(reads).toBe(1));
    expect(screen.getByText("My Relay")).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: en.services.start.action.replace("{{tool}}", "Claude Code"),
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: en.services.action.useNamed.replace("{{name}}", "My Relay"),
      }),
    ).toBeDisabled();

    releaseFailure();
    const alert = await screen.findByRole("alert", {
      name: "Could not refresh your tools",
    });
    expect(alert).toHaveTextContent(
      "The last tool check and services are still shown.",
    );
    expect(screen.getByText("My Relay")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", {
        name: en.services.endpoints.title.replace("{{tool}}", "Claude Code"),
      }),
    ).toBeInTheDocument();
    for (const name of [
      en.services.action.useNamed.replace("{{name}}", "My Relay"),
      en.services.action.testNamed.replace("{{name}}", "My Relay"),
      en.services.action.editNamed.replace("{{name}}", "My Relay"),
      en.services.action.removeNamed.replace("{{name}}", "My Relay"),
      en.services.start.action.replace("{{tool}}", "Claude Code"),
    ]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
    expect(
      screen
        .getAllByRole("button", { name: en.services.action.add })
        .every((button) => button.hasAttribute("disabled")),
    ).toBe(true);
    expect(screen.getByRole("tab", { name: "Claude Code" })).toBeEnabled();
    expect(document.body).not.toHaveTextContent(
      "private refreshed tool inventory",
    );

    const retry = within(alert).getByRole("button", {
      name: "Check tools again",
    });
    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(2));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toHaveAttribute("aria-busy", "true");
    const focusTarget = screen.getByRole("tab", { name: "Claude Code" });
    focusTarget.focus();

    releaseRecovery();
    await waitFor(() => expect(alert).not.toBeInTheDocument());
    expect(
      screen.getByRole("button", {
        name: en.services.action.testNamed.replace("{{name}}", "My Relay"),
      }),
    ).toBeEnabled();
    expect(focusTarget).toHaveFocus();
  });

  it("keeps an edited draft but blocks saving after the service baseline expires", async () => {
    const saves: unknown[] = [];
    const { client } = mount([service()]);
    const card = await screen.findByRole("article", { name: "My Relay" });
    await userEvent.click(
      within(card).getByRole("button", {
        name: en.services.action.editNamed.replace("{{name}}", "My Relay"),
      }),
    );
    const dialog = screen.getByRole("dialog");
    const name = within(dialog).getByLabelText(en.services.form.name);
    const key = within(dialog).getByLabelText(en.services.form.key);
    await userEvent.clear(name);
    await userEvent.type(name, "Draft Relay");
    await userEvent.clear(key);
    await userEvent.type(key, "sk-kept-locally");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
        HttpResponse.text("private provider refresh detail", { status: 500 }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_save`, async ({ request }) => {
        saves.push(await request.json());
        return HttpResponse.json([service()]);
      }),
    );

    void client.invalidateQueries({
      queryKey: providerKeys.list("claude-code"),
    });
    const paused = await within(dialog).findByRole("alert", {
      name: "Service actions are paused",
    });
    expect(paused).toHaveTextContent(
      "AI Manager must confirm the current tool and service state",
    );
    expect(name).toHaveValue("Draft Relay");
    expect(key).toHaveValue("sk-kept-locally");
    expect(
      within(dialog).getByRole("button", { name: en.services.form.save }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeEnabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.close }),
    ).toBeEnabled();
    name.focus();
    await userEvent.keyboard("{Enter}");
    expect(saves).toHaveLength(0);
    expect(dialog).not.toHaveTextContent("private provider refresh detail");
  });

  it("keeps an open launch confirmation safe when the tool baseline expires", async () => {
    const launches: unknown[] = [];
    const { client } = mount([service()]);
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.services.start.action.replace("{{tool}}", "Claude Code"),
      }),
    );
    const dialog = screen.getByRole("dialog", { name: "Open Claude Code" });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.text("private launch authority", { status: 500 }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
        launches.push(await request.json());
        return HttpResponse.json("launched");
      }),
    );

    void client.invalidateQueries({ queryKey: toolKeys.all });
    expect(
      await within(dialog).findByRole("alert", {
        name: "Service actions are paused",
      }),
    ).toBeInTheDocument();
    const confirm = within(dialog).getByRole("button", {
      name: en.tools.open.confirm,
    });
    expect(confirm).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeEnabled();
    await userEvent.click(confirm);
    expect(launches).toHaveLength(0);
    expect(dialog).not.toHaveTextContent("private launch authority");
  });

  it("retains an empty service result when its refresh fails", async () => {
    const { client } = mount([]);
    expect(
      await screen.findByText(en.services.empty.title),
    ).toBeInTheDocument();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
        HttpResponse.text("private empty refresh target", { status: 500 }),
      ),
    );

    await client.invalidateQueries({
      queryKey: providerKeys.list("claude-code"),
    });
    expect(
      await screen.findByRole("alert", {
        name: en.services.refreshError.title,
      }),
    ).toBeInTheDocument();
    expect(screen.getByText(en.services.empty.title)).toBeInTheDocument();
    expect(
      screen
        .getAllByRole("button", { name: en.services.action.add })
        .every((button) => button.hasAttribute("disabled")),
    ).toBe(true);
    expect(
      screen.getAllByRole("button", { name: en.services.refresh }),
    ).toHaveLength(1);
    expect(document.body).not.toHaveTextContent("private empty refresh target");
  });

  it("explains itself when no installed tool manages services", async () => {
    mount([], []);
    expect(
      await screen.findByText(en.services.noTools.title),
    ).toBeInTheDocument();
    // Important 1(a): a disabled providers query is always isPending under TanStack v5, so a
    // skeleton condition that only checks providers.isPending would spin forever alongside this empty state.
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("retains the no-tool explanation when an empty tool refresh fails", async () => {
    const { client } = mount([], []);
    const emptyTitle = await screen.findByText(en.services.noTools.title);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.text("private empty tool refresh", { status: 500 }),
      ),
    );

    void client.invalidateQueries({ queryKey: toolKeys.all });
    expect(
      await screen.findByRole("alert", {
        name: "Could not refresh your tools",
      }),
    ).toBeInTheDocument();
    expect(emptyTitle).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("private empty tool refresh");
  });

  it("shows an error with a working retry when the tool list itself cannot be read", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () => {
        calls += 1;
        return HttpResponse.text("boom", { status: 500 });
      }),
    );
    render(<ServicesPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    // Important 1(b): before this fix, a failing useTools() left a permanent skeleton with no
    // error message and no retry entry point (the header's Refresh is also disabled while active === null).
    expect(
      await screen.findByText(en.services.toolsError.title),
    ).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(calls).toBe(1);

    await userEvent.click(
      screen.getByRole("button", { name: en.services.refresh }),
    );
    await waitFor(() => expect(calls).toBeGreaterThan(1));
  });

  it("remembers which tool you were managing", async () => {
    let saved: unknown;
    mount(
      [service()],
      [tool("claude-code", "Claude Code"), tool("codex", "Codex")],
    );
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        const body = (await request.json()) as {
          settings: { toolScope: string };
        };
        saved = body.settings.toolScope;
        return HttpResponse.json(body.settings);
      }),
    );
    await userEvent.click(await screen.findByRole("tab", { name: "Codex" }));
    await waitFor(() => expect(saved).toBe("codex"));
  });

  it("keeps the chosen tool usable and offers a retry when remembering it fails", async () => {
    let attempts = 0;
    let releaseFirstAttempt: (() => void) | undefined;
    mount(
      [service()],
      [tool("claude-code", "Claude Code"), tool("codex", "Codex")],
      "claude-code",
    );
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        attempts += 1;
        const body = (await request.json()) as {
          settings: Record<string, unknown>;
        };
        if (attempts === 1) {
          await new Promise<void>((resolve) => {
            releaseFirstAttempt = resolve;
          });
          return HttpResponse.json(
            {
              code: "CONFIG_WRITE_FAILED",
              messageKey: "error.settings.saveFailed",
              technicalMessage: "permission denied at /private/settings.json",
              remediation: "error.remediation.retryOrViewDetails",
              contextId: null,
            },
            { status: 500 },
          );
        }
        return HttpResponse.json(body.settings);
      }),
    );

    await userEvent.click(await screen.findByRole("tab", { name: "Codex" }));
    await waitFor(() => expect(attempts).toBe(1));
    expect(
      screen.getByRole("status", {
        name: en.services.scopeSave.saving,
      }),
    ).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Claude Code" })).toBeDisabled();
    expect(screen.getByRole("tab", { name: "Codex" })).toBeDisabled();

    releaseFirstAttempt?.();
    const failure = await screen.findByRole("alert", {
      name: en.services.scopeSave.errorTitle,
    });
    expect(screen.getByRole("tab", { name: "Codex" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(failure).toHaveTextContent(
      en.services.scopeSave.errorDescription.replace("{{tool}}", "Codex"),
    );
    expect(failure).not.toHaveTextContent("permission denied");

    await userEvent.click(
      within(failure).getByRole("button", {
        name: en.services.scopeSave.retry,
      }),
    );
    await waitFor(() => expect(attempts).toBe(2));
    await waitFor(() =>
      expect(
        screen.queryByRole("alert", {
          name: en.services.scopeSave.errorTitle,
        }),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("tab", { name: "Codex" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("comes back to the tool you left off on", async () => {
    mount(
      [service()],
      [tool("claude-code", "Claude Code"), tool("codex", "Codex")],
      "codex",
    );
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Codex" })).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
  });

  it("uses a Home recommendation for this visit without losing explicit tab control", async () => {
    let saved: unknown;
    mount(
      [service()],
      [tool("claude-code", "Claude Code"), tool("codex", "Codex")],
      "codex",
      "claude-code",
    );
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        const body = (await request.json()) as {
          settings: { toolScope: string };
        };
        saved = body.settings.toolScope;
        return HttpResponse.json(body.settings);
      }),
    );

    expect(
      await screen.findByRole("tab", { name: "Claude Code" }),
    ).toHaveAttribute("aria-selected", "true");
    await userEvent.click(screen.getByRole("tab", { name: "Codex" }));

    await waitFor(() => expect(saved).toBe("codex"));
    expect(screen.getByRole("tab", { name: "Codex" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("falls back to the first tool when the stored one is not manageable here", async () => {
    // The stored value is a tool this page can't manage (e.g. it was uninstalled). The code comment specifically calls out this case.
    mount([service()], [tool("claude-code", "Claude Code")], "codex");
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Claude Code" })).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
  });

  it("keeps each completed connection check visible on its own card", async () => {
    mount([
      service({ id: "relay-a", name: "Relay A" }),
      service({ id: "relay-b", name: "Relay B" }),
    ]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, async ({ request }) => {
        const body = (await request.json()) as { provider: string };
        return HttpResponse.json({
          providerId: body.provider,
          reachability:
            body.provider === "relay-a" ? "operational" : "degraded",
          responseTimeMs: body.provider === "relay-a" ? 240 : 7100,
          httpStatus: 200,
        });
      }),
    );

    const relayA = await screen.findByRole("article", { name: "Relay A" });
    await userEvent.click(
      within(relayA).getByRole("button", {
        name: en.services.action.testNamed.replace("{{name}}", "Relay A"),
      }),
    );
    const firstDialog = await screen.findByRole("dialog");
    await within(firstDialog).findByText(en.services.test.operational);
    await userEvent.click(
      within(firstDialog).getByRole("button", {
        name: en.ds.action.close,
      }),
    );

    const relayB = await screen.findByRole("article", { name: "Relay B" });
    await userEvent.click(
      within(relayB).getByRole("button", {
        name: en.services.action.testNamed.replace("{{name}}", "Relay B"),
      }),
    );
    const secondDialog = await screen.findByRole("dialog");
    await within(secondDialog).findByText(en.services.test.degraded);
    await userEvent.click(
      within(secondDialog).getByRole("button", {
        name: en.ds.action.close,
      }),
    );

    expect(
      within(await screen.findByRole("article", { name: "Relay B" })).getByText(
        en.services.test.degraded,
      ),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole("article", { name: "Relay A" })).getByText(
        en.services.test.operational,
      ),
    ).toBeInTheDocument();
  });

  it("opens compatible services after an official address is unreachable", async () => {
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
              baseUrl: "https://api.example.test",
              websiteUrl: "https://www.anthropic.com",
              apiKeyUrl: "https://console.anthropic.com",
              official: true,
            },
            {
              id: "deepseek-safe",
              serviceName: "DeepSeek",
              defaultName: "DeepSeek",
              defaultModel: "deepseek-v4-pro",
              baseUrl: "https://api.example.test",
              websiteUrl: "https://platform.deepseek.com",
              apiKeyUrl: "https://platform.deepseek.com",
              official: false,
            },
          ],
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.json({
          providerId: "official",
          reachability: "failed",
          responseTimeMs: null,
          httpStatus: null,
        }),
      ),
    );
    mount([
      service({
        id: "official",
        name: "Anthropic Official",
        kind: "official",
        active: true,
        baseUrl: null,
        apiKey: null,
        testable: true,
        canRemove: false,
      }),
    ]);

    const card = await screen.findByRole("article", {
      name: "Anthropic Official",
    });
    await userEvent.click(
      within(card).getByRole("button", {
        name: en.services.action.testNamed.replace(
          "{{name}}",
          "Anthropic Official",
        ),
      }),
    );
    const probeDialog = await screen.findByRole("dialog");
    await within(probeDialog).findByText(en.services.test.failed);
    await userEvent.click(
      within(probeDialog).getByRole("button", {
        name: en.ds.action.close,
      }),
    );

    const checked = await screen.findByRole("article", {
      name: "Anthropic Official",
    });
    expect(
      within(checked).getByText(en.services.test.unreachableHint),
    ).toBeVisible();
    await userEvent.click(
      within(checked).getByRole("button", {
        name: en.services.test.browseCompatibleNamed.replace(
          "{{name}}",
          "Anthropic Official",
        ),
      }),
    );

    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByRole("combobox", {
        name: en.services.connect.preset,
      }),
    ).toHaveTextContent("DeepSeek");
  });
});

describe("ServicesPage tabs", () => {
  beforeEach(async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_connection_profile`, () =>
        HttpResponse.json({
          defaultPresetId: "official",
          modelRequired: false,
          presets: [],
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
        HttpResponse.json({
          running: false,
          address: null,
          port: null,
          activeConnections: 0,
          totalRequests: 0,
          successRequests: 0,
          failedRequests: 0,
          failoverCount: 0,
          targets: [],
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
        HttpResponse.json({
          periodDays: 30,
          startDate: "2026-08-06",
          endDate: "2026-09-05",
          summary: {
            requests: 0,
            estimatedCostUsd: "0.000000",
            tokens: 0,
            successRatePercent: 0,
            cacheHitRatePercent: 0,
          },
          byTool: [],
          trend: [],
        }),
      ),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        nav: en.nav,
        routing: en.routing,
        services: en.services,
        tool: en.tool,
        tools: en.tools,
        usage: en.usage,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("shows endpoints first and offers routing and usage as tabs of the same page", async () => {
    mount([service()]);
    const tabs = screen.getByRole("tablist", { name: en.services.title });
    expect(
      within(tabs)
        .getAllByRole("tab")
        .map((tab) => tab.textContent),
    ).toEqual([en.nav.services, en.nav.routing, en.nav.usage]);
    expect(
      within(tabs).getByRole("tab", { name: en.nav.services }),
    ).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByText("My Relay")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: en.routing.title })).toBeNull();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
  });

  it("opens directly on Local Routing when the shell asks for it", async () => {
    mount([service()], undefined, null, null, false, "routing");
    expect(
      await screen.findByRole("region", { name: en.routing.title }),
    ).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: en.nav.routing })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      screen.queryByRole("region", { name: en.services.title }),
    ).toBeNull();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
  });

  it("switches to Usage and back under one page heading", async () => {
    mount([service()]);
    expect(await screen.findByText("My Relay")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: en.nav.usage }));
    expect(
      await screen.findByRole("region", { name: en.usage.title }),
    ).toBeInTheDocument();
    expect(screen.queryByText("My Relay")).toBeNull();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);

    await userEvent.click(screen.getByRole("tab", { name: en.nav.services }));
    expect(await screen.findByText("My Relay")).toBeInTheDocument();
  });

  it("keeps the tool the shell asked for until its endpoints have been shown", async () => {
    mount(
      [],
      [tool("claude-code", "Claude Code"), tool("codex", "Codex")],
      "codex",
      "claude-code",
      false,
      "usage",
    );
    expect(
      await screen.findByRole("region", { name: en.usage.title }),
    ).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: en.nav.services }));
    expect(
      await screen.findByRole("tab", { name: "Claude Code" }),
    ).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Codex" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
  });

  it("lets the remembered tool win once the user leaves the endpoints tab", async () => {
    mount(
      [],
      [tool("claude-code", "Claude Code"), tool("codex", "Codex")],
      "codex",
      "claude-code",
    );
    expect(
      await screen.findByRole("tab", { name: "Claude Code" }),
    ).toHaveAttribute("aria-selected", "true");

    await userEvent.click(screen.getByRole("tab", { name: en.nav.usage }));
    await screen.findByRole("region", { name: en.usage.title });
    await userEvent.click(screen.getByRole("tab", { name: en.nav.services }));
    expect(await screen.findByRole("tab", { name: "Codex" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
