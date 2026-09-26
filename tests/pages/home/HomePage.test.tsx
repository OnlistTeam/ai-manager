import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { delay, http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { operationKeys } from "@/entities/operation";
import { toolKeys } from "@/entities/tool";
import en from "@/i18n/locales/en.json";
import { HomePage } from "@/pages/home/HomePage";
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

const CAPABILITIES = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: false,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: true,
  canLaunch: true,
};

const RUNNING_UPDATE = {
  id: "op-running",
  kind: "update",
  tool: "claude-code",
  status: "running",
  progress: 42,
  messageKey: "operation.phase.downloading",
  error: null,
  startedAt: 5,
  finishedAt: null,
};

function tool(overrides: Record<string, unknown> = {}) {
  return {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "tool.claude-code.description",
    status: "installed",
    version: "1.0.0",
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: null,
    ...overrides,
  };
}

function provider(overrides: Record<string, unknown> = {}) {
  return {
    id: "relay",
    tool: "claude-code",
    name: "Relay",
    kind: "custom",
    active: false,
    baseUrl: "https://relay.example.test",
    apiKey: null,
    websiteUrl: null,
    testable: true,
    canRemove: true,
    ...overrides,
  };
}

function editProfile(providerId: string, models: string[]) {
  return {
    providerId,
    baseUrl: null,
    endpointCandidates: [],
    endpointAutoSelect: false,
    models,
    headerNames: [],
    capabilities: {
      canEditBaseUrl: true,
      canEditEndpoints: true,
      canEditModels: true,
      canEditHeaders: false,
      supportsMultipleModels: false,
    },
    baseUrlTakesNoVersion: false,
  };
}

function readyUpdatePreview(toolId: string, fingerprint: string) {
  return {
    state: "ready",
    preview: {
      tool: toolId,
      previewFingerprint: fingerprint,
      targetVersion: "2.0.0",
      source: "nativeInstaller",
      installations: [
        {
          source: "nativeInstaller",
          version: "1.0.0",
          runnable: true,
          isDefault: true,
          location: `/Users/test/.local/bin/${toolId}`,
        },
      ],
      attempts: [
        {
          method: "nativeSelfUpdate",
          commands: [`/Users/test/.local/bin/${toolId} update`],
        },
      ],
      multipleInstallations: false,
    },
  };
}

function blockedUpdatePreview(toolId: string) {
  return {
    state: "blocked",
    tool: toolId,
    reason: "ambiguousInstallation",
  };
}

function healthySnapshot(tools: unknown[]) {
  const installed = (tools as ReturnType<typeof tool>[]).filter(
    (entry) =>
      entry.status === "installed" || entry.status === "updateAvailable",
  );
  return {
    providers: installed.map((entry) => ({
      tool: entry.id,
      configured: true,
      configuredCount: 1,
      checkTargets: [],
    })),
    configs: installed.map((entry) => ({
      tool: entry.id,
      status: "readable",
    })),
    mcp: { total: 0, enabled: 0 },
  };
}

/** Saved endpoints per tool; every tool without an entry has none. */
function serveProviders(byTool: Record<string, unknown[]> = {}) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_providers_list`, async ({ request }) => {
      const { tool: toolId } = (await request.json()) as { tool: string };
      return HttpResponse.json(byTool[toolId] ?? []);
    }),
    http.post(
      `${TAURI_ENDPOINT}/app_provider_edit_profile`,
      async ({ request }) => {
        const body = (await request.json()) as { provider: string };
        return HttpResponse.json(editProfile(body.provider, []));
      },
    ),
  );
}

function mount(
  tools: unknown[],
  onOpenTools = vi.fn(),
  onOpenMcp = vi.fn(),
  snapshot = healthySnapshot(tools),
  onOpenServices = vi.fn(),
  operations: unknown[] = [],
) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
      HttpResponse.json(tools),
    ),
    http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
      HttpResponse.json(snapshot),
    ),
    http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
      HttpResponse.json(operations),
    ),
  );
  render(
    <HomePage
      onOpenTools={onOpenTools}
      onOpenServices={onOpenServices}
      onOpenMcp={onOpenMcp}
    />,
    {
      wrapper: withQueryClient(createTestQueryClient()),
    },
  );
  return onOpenTools;
}

function renderHome(onOpenServices = vi.fn()) {
  const client = createTestQueryClient();
  render(
    <HomePage
      onOpenTools={vi.fn()}
      onOpenServices={onOpenServices}
      onOpenMcp={vi.fn()}
    />,
    { wrapper: withQueryClient(client) },
  );
  return client;
}

async function findRow(name: string): Promise<HTMLElement> {
  return screen.findByRole("article", { name });
}

const startName = (name: string) =>
  en.home.status.startTool.replace("{{name}}", name);

describe("HomePage", () => {
  beforeEach(async () => {
    toastMocks.error.mockClear();
    toastMocks.info.mockClear();
    toastMocks.success.mockClear();
    serveProviders();
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_tools_update_preview`,
        async ({ request }) => {
          const body = (await request.json()) as { tools: string[] };
          return HttpResponse.json(
            body.tools.map((toolId, index) =>
              readyUpdatePreview(
                toolId,
                String.fromCharCode("a".charCodeAt(0) + index).repeat(64),
              ),
            ),
          );
        },
      ),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        home: en.home,
        preferences: { check: en.preferences.check },
        services: en.services,
        tools: en.tools,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  describe("status line", () => {
    it("states the environment once, with the check's own timestamp beside it", async () => {
      mount([tool()]);
      const headline = await screen.findByRole("heading", {
        level: 1,
        name: en.home.status.allGood,
      });
      expect(headline).toBeVisible();
      expect(screen.getByText(/Last checked/)).toBeVisible();
      expect(screen.getAllByText(en.home.status.allGood)).toHaveLength(1);
      expect(screen.queryByText(/needs attention|need attention/)).toBeNull();
      // The findings list only appears when there is something to act on.
      expect(
        screen.queryByRole("region", { name: en.home.health.title }),
      ).toBeNull();
    });

    it("reads as one compact line rather than a spatial hero", async () => {
      mount([tool()]);
      const headline = await screen.findByRole("heading", {
        level: 1,
        name: en.home.status.allGood,
      });
      const line = headline.parentElement as HTMLElement;
      expect(line).toHaveAttribute("data-status", "ready");
      expect(line.querySelector("[data-model]")).toBeNull();
      expect(line.querySelector("[data-spatial-stage]")).toBeNull();
      expect(
        within(line).getByRole("button", { name: en.home.health.recheck }),
      ).toBeInTheDocument();
    });

    it("says nothing is installed and offers installing instead of a tool list", async () => {
      const onOpenTools = vi.fn();
      mount([tool({ status: "notInstalled", version: null })], onOpenTools);
      expect(
        await screen.findByRole("heading", {
          level: 1,
          name: en.home.status.nothingInstalled,
        }),
      ).toBeInTheDocument();
      expect(screen.queryByText(/needs attention/)).not.toBeInTheDocument();
      expect(
        screen.queryByRole("heading", { name: en.home.tools.title }),
      ).toBeNull();
      await userEvent.click(
        screen.getByRole("button", { name: en.home.tools.install }),
      );
      expect(onOpenTools).toHaveBeenCalledTimes(1);
    });

    it("escalates to Action Required for a broken tool", async () => {
      mount([tool({ status: "broken" })]);
      expect(await screen.findByText(en.ds.status.action)).toBeInTheDocument();
    });

    it("counts findings, names updates, and opens the recommended destination", async () => {
      const onOpenTools = vi.fn();
      mount(
        [tool({ status: "updateAvailable", latestVersion: "1.1.0" })],
        onOpenTools,
      );
      expect(
        await screen.findByRole("heading", {
          level: 1,
          name: "1 item needs attention",
        }),
      ).toBeInTheDocument();
      expect(screen.getByText("1 update available")).toBeVisible();
      expect(
        screen.queryByRole("button", { name: startName("Claude Code") }),
      ).not.toBeInTheDocument();
      await userEvent.click(
        screen.getByRole("button", { name: en.home.status.actions.tools }),
      );
      expect(onOpenTools).toHaveBeenCalledTimes(1);
    });

    it("explains the checking state before the first answer", async () => {
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tools_list`, async () => {
          await delay("infinite");
          return HttpResponse.json([]);
        }),
        http.post(`${TAURI_ENDPOINT}/app_operations_list`, async () => {
          await delay("infinite");
          return HttpResponse.json([]);
        }),
      );
      renderHome();

      const status = await screen.findByRole("status", {
        name: en.home.status.checking,
      });
      expect(
        within(status).getByRole("heading", { level: 1 }),
      ).toHaveTextContent(en.home.status.checking);
      expect(within(status).queryByRole("button")).toBeNull();
      expect(
        screen.queryByRole("button", { name: en.home.tools.updateAll }),
      ).toBeNull();
    });

    it("rechecks on demand and then tests the saved service addresses once", async () => {
      let snapshotReads = 0;
      const connectionChecks: unknown[] = [];
      const tools = [tool()];
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
          HttpResponse.json(tools),
        ),
        http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () => {
          snapshotReads += 1;
          return HttpResponse.json({
            ...healthySnapshot(tools),
            providers: [
              {
                tool: "claude-code",
                configured: true,
                configuredCount: 1,
                checkTargets: [{ providerId: "relay", name: "Relay" }],
              },
            ],
          });
        }),
        http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
          HttpResponse.json([]),
        ),
        http.post(
          `${TAURI_ENDPOINT}/app_provider_test_all`,
          async ({ request }) => {
            connectionChecks.push(await request.json());
            return HttpResponse.json("00000000-0000-4000-8000-000000000001");
          },
        ),
      );
      renderHome();
      const recheck = await screen.findByRole("button", {
        name: en.home.health.recheck,
      });
      expect(snapshotReads).toBe(1);
      expect(connectionChecks).toEqual([]);

      await userEvent.click(recheck);

      await waitFor(() => expect(snapshotReads).toBe(2));
      await waitFor(() =>
        expect(connectionChecks).toEqual([{ tool: "claude-code" }]),
      );
    });

    it("keeps one initial retry busy and restores Home focus", async () => {
      let toolReads = 0;
      let releaseRetry!: () => void;
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tools_list`, async () => {
          toolReads += 1;
          if (toolReads === 1) {
            return HttpResponse.text("private setup path", { status: 500 });
          }
          await new Promise<void>((resolve) => {
            releaseRetry = resolve;
          });
          return HttpResponse.json([tool()]);
        }),
        http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
          HttpResponse.json(healthySnapshot([tool()])),
        ),
        http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
          HttpResponse.json([]),
        ),
      );
      renderHome();
      const alert = await screen.findByRole("alert", {
        name: en.home.error.title,
      });
      expect(alert.querySelector("[data-model]")).toBeNull();
      expect(
        within(alert).getByRole("heading", { level: 1 }),
      ).toBeInTheDocument();
      expect(screen.queryByText(en.ds.status.ready)).not.toBeInTheDocument();
      const retry = screen.getByRole("button", {
        name: en.home.refreshError.action,
      });

      await userEvent.click(retry);
      await waitFor(() => expect(toolReads).toBe(2));
      await waitFor(() => expect(retry).toHaveAttribute("aria-busy", "true"));
      expect(retry).toBeDisabled();
      expect(alert).toHaveAttribute("aria-busy", "true");
      expect(screen.queryByRole("status")).toBeNull();
      expect(document.body).not.toHaveTextContent("private setup path");

      releaseRetry();
      expect(
        await screen.findByText(en.home.status.allGood),
      ).toBeInTheDocument();
      await waitFor(() =>
        expect(
          screen.getByRole("region", { name: en.home.pageLabel }),
        ).toHaveFocus(),
      );
    });
  });

  describe("start", () => {
    it("starts a Ready tool through the existing private folder flow", async () => {
      const seen: unknown[] = [];
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json("launched");
        }),
      );
      mount([tool()]);
      const user = userEvent.setup();

      await user.click(
        await screen.findByRole("button", { name: startName("Claude Code") }),
      );

      const dialog = screen.getByRole("dialog", { name: "Open Claude Code" });
      expect(
        within(dialog).getByRole("button", {
          name: en.tools.open.chooseFolder,
        }),
      ).toBeInTheDocument();
      expect(seen).toEqual([]);
      const confirm = within(dialog).getByRole("button", {
        name: en.tools.open.confirm,
      });
      expect(confirm).toHaveFocus();
      await user.click(confirm);

      await waitFor(() =>
        expect(seen).toEqual([
          { tool: "claude-code", directoryMode: "default" },
        ]),
      );
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    });

    it("pauses an already-open Start flow when task authority becomes unavailable", async () => {
      const tools = [tool()];
      let operationReads = 0;
      let releaseRefresh!: () => void;
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
          HttpResponse.json(tools),
        ),
        http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
          HttpResponse.json(healthySnapshot(tools)),
        ),
        http.post(`${TAURI_ENDPOINT}/app_operations_list`, async () => {
          operationReads += 1;
          if (operationReads === 2) {
            await new Promise<void>((resolve) => {
              releaseRefresh = resolve;
            });
            return HttpResponse.text("private running task", { status: 500 });
          }
          return HttpResponse.json([]);
        }),
      );
      const client = renderHome();
      await userEvent.click(
        await screen.findByRole("button", { name: startName("Claude Code") }),
      );
      const dialog = screen.getByRole("dialog");
      const confirm = within(dialog).getByRole("button", {
        name: en.tools.open.confirm,
      });
      expect(confirm).toBeEnabled();

      void client.invalidateQueries({ queryKey: operationKeys.all });
      await waitFor(() => expect(operationReads).toBe(2));
      expect(
        within(dialog).getByRole("status", {
          name: en.home.refreshing.title,
        }),
      ).toHaveAttribute("aria-busy", "true");
      expect(confirm).toBeDisabled();

      releaseRefresh();
      expect(
        await within(dialog).findByRole("alert", {
          name: en.home.refreshError.title,
        }),
      ).toBeInTheDocument();
      expect(confirm).toBeDisabled();
      expect(
        within(dialog).getByRole("button", { name: en.ds.action.cancel }),
      ).toBeEnabled();
      expect(document.body).not.toHaveTextContent("private running task");
    });

    it("does not offer Start when a Ready tool cannot launch", async () => {
      mount([
        tool({
          capabilities: { ...CAPABILITIES, canLaunch: false },
        }),
      ]);

      expect(await screen.findByText(en.ds.status.ready)).toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: /Start with/ }),
      ).not.toBeInTheDocument();
    });

    it("does not combine a preserved Ready result with a refreshing tool list", async () => {
      const tools = [tool()];
      let listCalls = 0;
      let releaseRefresh!: () => void;
      let releaseRecovery!: () => void;
      const refreshGate = new Promise<void>((resolve) => {
        releaseRefresh = resolve;
      });
      const recoveryGate = new Promise<void>((resolve) => {
        releaseRecovery = resolve;
      });
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tools_list`, async () => {
          listCalls += 1;
          if (listCalls === 2) {
            await refreshGate;
            return HttpResponse.text("refresh failed", { status: 500 });
          }
          if (listCalls === 3) await recoveryGate;
          return HttpResponse.json(tools);
        }),
        http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
          HttpResponse.json(healthySnapshot(tools)),
        ),
        http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
          HttpResponse.json([]),
        ),
      );
      const client = renderHome();
      const start = startName("Claude Code");
      expect(
        await screen.findByRole("button", { name: start }),
      ).toBeInTheDocument();

      void client.invalidateQueries({ queryKey: toolKeys.all });
      await waitFor(() =>
        expect(client.getQueryState(toolKeys.list())?.fetchStatus).toBe(
          "fetching",
        ),
      );
      expect(
        screen.queryByRole("button", { name: start }),
      ).not.toBeInTheDocument();

      releaseRefresh();
      await waitFor(() =>
        expect(client.getQueryState(toolKeys.list())?.status).toBe("error"),
      );
      expect(screen.getByText(en.home.status.allGood)).toBeInTheDocument();
      const alert = screen.getByRole("alert", {
        name: en.home.refreshError.title,
      });
      expect(alert).toHaveTextContent(en.home.refreshError.description);
      expect(alert).toHaveClass("min-w-0", "flex-col", "sm:flex-row");
      expect(
        screen.getByRole("region", { name: en.home.pageLabel }),
      ).toHaveClass("min-w-0");
      const retry = screen.getByRole("button", {
        name: en.home.refreshError.action,
      });
      expect(
        screen.queryByRole("button", { name: start }),
      ).not.toBeInTheDocument();

      await userEvent.click(retry);
      await waitFor(() => expect(listCalls).toBe(3));
      await waitFor(() => expect(retry).toHaveAttribute("aria-busy", "true"));
      expect(retry).toBeDisabled();
      expect(alert).toHaveAttribute("aria-busy", "true");
      const install = screen.getByRole("button", {
        name: en.home.tools.install,
      });
      install.focus();
      releaseRecovery();

      await waitFor(() => expect(alert).not.toBeInTheDocument());
      expect(await screen.findByRole("button", { name: start })).toBeEnabled();
      expect(install).toHaveFocus();
    });
  });

  describe("my tools", () => {
    it("lists each installed tool that can use an endpoint, and nothing else", async () => {
      mount([
        tool(),
        tool({ id: "codex", name: "Codex" }),
        tool({ id: "opencode", name: "OpenCode", status: "notInstalled" }),
        tool({
          id: "kimi-code",
          name: "Kimi Code",
          capabilities: { ...CAPABILITIES, canManageProvider: false },
        }),
      ]);

      const list = await screen.findByRole("heading", {
        name: en.home.tools.title,
      });
      const section = list.closest("section") as HTMLElement;
      expect(await findRow("Claude Code")).toBeInTheDocument();
      expect(await findRow("Codex")).toBeInTheDocument();
      expect(within(section).getAllByRole("article")).toHaveLength(2);
      expect(screen.queryByRole("article", { name: "OpenCode" })).toBeNull();
      expect(screen.queryByRole("article", { name: "Kimi Code" })).toBeNull();
    });

    it("names the endpoint in use and the model it pins", async () => {
      serveProviders({
        "claude-code": [
          provider({ id: "kimi", name: "Kimi", active: true }),
          provider(),
        ],
      });
      server.use(
        http.post(
          `${TAURI_ENDPOINT}/app_provider_edit_profile`,
          async ({ request }) => {
            const body = (await request.json()) as { provider: string };
            return HttpResponse.json(editProfile(body.provider, ["kimi-k2"]));
          },
        ),
      );
      mount([tool()]);

      const row = await findRow("Claude Code");
      expect(await within(row).findByText("Kimi")).toBeVisible();
      expect(await within(row).findByText("kimi-k2")).toBeVisible();
    });

    it("says Official sign-in when the official entry or the tool's own login is in effect", async () => {
      serveProviders({
        "claude-code": [
          provider({
            id: "official",
            name: "Claude Official",
            kind: "official",
            active: true,
          }),
        ],
      });
      mount([
        tool(),
        tool({
          id: "gemini-cli",
          name: "Gemini CLI",
          discovery: {
            publisher: "Google",
            access: "vendorOrProvider",
            useCases: ["officialCoding"],
          },
        }),
      ]);

      const claude = await findRow("Claude Code");
      expect(
        await within(claude).findByText(en.home.tools.official),
      ).toBeVisible();
      expect(within(claude).queryByText("Claude Official")).toBeNull();
      const gemini = await findRow("Gemini CLI");
      expect(
        await within(gemini).findByText(en.home.tools.official),
      ).toBeVisible();
    });

    it("says Not connected and offers to connect when a tool needs an endpoint", async () => {
      const onOpenServices = vi.fn();
      mount(
        [
          tool({
            id: "opencode",
            name: "OpenCode",
            discovery: {
              publisher: "SST",
              access: "provider",
              useCases: ["modelChoice"],
            },
          }),
        ],
        vi.fn(),
        vi.fn(),
        undefined,
        onOpenServices,
      );

      const row = await findRow("OpenCode");
      expect(
        await within(row).findByText(en.home.tools.notConnected),
      ).toBeVisible();
      await userEvent.click(
        within(row).getByRole("button", {
          name: "Connect an API endpoint to OpenCode",
        }),
      );
      expect(onOpenServices).toHaveBeenCalledWith("opencode");
    });

    it("counts added endpoints for a tool that picks its model itself", async () => {
      serveProviders({
        opencode: [
          provider({ id: "a", tool: "opencode", additive: true }),
          provider({ id: "b", tool: "opencode", name: "B", additive: true }),
        ],
      });
      mount([tool({ id: "opencode", name: "OpenCode" })]);

      const row = await findRow("OpenCode");
      expect(await within(row).findByText("2 endpoints added")).toBeVisible();
      expect(within(row).getByText("Model chosen in OpenCode")).toBeVisible();
      expect(within(row).queryByRole("combobox")).toBeNull();
    });

    it("opens the tool's API endpoints from the row", async () => {
      const onOpenServices = vi.fn();
      mount(
        [tool(), tool({ id: "codex", name: "Codex" })],
        vi.fn(),
        vi.fn(),
        undefined,
        onOpenServices,
      );

      const row = await findRow("Codex");
      await userEvent.click(
        within(row).getByRole("button", {
          name: "Open API endpoints for Codex",
        }),
      );
      expect(onOpenServices).toHaveBeenCalledWith("codex");
    });

    it("switches through the shared preflight and announces the reopen hint", async () => {
      const activations: unknown[] = [];
      serveProviders({
        "claude-code": [
          provider({ id: "kimi", name: "Kimi", active: true }),
          provider(),
        ],
      });
      server.use(
        http.post(
          `${TAURI_ENDPOINT}/app_provider_activation_prepare`,
          async ({ request }) => {
            activations.push(await request.json());
            return HttpResponse.json({
              status: "notChecked",
              originProviderId: "relay",
              activeProviderId: "relay",
              providers: [
                provider({ id: "kimi", name: "Kimi" }),
                provider({ active: true }),
              ],
              checks: [],
            });
          },
        ),
      );
      mount([tool()]);

      const row = await findRow("Claude Code");
      const select = await within(row).findByRole("combobox", {
        name: "Switch the endpoint Claude Code uses",
      });
      await waitFor(() => expect(select).toBeEnabled());
      expect(
        within(select)
          .getAllByRole("option")
          .map((option) => option.textContent),
      ).toEqual([en.home.tools.switchTo, "Relay"]);

      await userEvent.selectOptions(select, "relay");

      await waitFor(() =>
        expect(activations).toEqual([
          { tool: "claude-code", provider: "relay" },
        ]),
      );
      expect(await within(row).findByText("Relay")).toBeVisible();
      await waitFor(() =>
        expect(toastMocks.success).toHaveBeenCalledWith(
          "Switched to Relay",
          expect.objectContaining({
            description: en.services.switch.reopenHint.replace(
              "{{tool}}",
              "Claude Code",
            ),
          }),
        ),
      );
    });

    it("says nothing changed when the chosen endpoint does not respond", async () => {
      const onOpenServices = vi.fn();
      serveProviders({
        "claude-code": [
          provider({ id: "kimi", name: "Kimi", active: true }),
          provider(),
        ],
      });
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_provider_activation_prepare`, () =>
          HttpResponse.json({
            status: "unreachable",
            originProviderId: "kimi",
            activeProviderId: "kimi",
            providers: [
              provider({ id: "kimi", name: "Kimi", active: true }),
              provider(),
            ],
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
      mount([tool()], vi.fn(), vi.fn(), undefined, onOpenServices);

      const row = await findRow("Claude Code");
      const select = await within(row).findByRole("combobox");
      await waitFor(() => expect(select).toBeEnabled());
      await userEvent.selectOptions(select, "relay");

      expect(
        await within(row).findByText(
          "Relay did not respond, so nothing was changed.",
        ),
      ).toBeVisible();
      expect(within(row).getByText("Kimi")).toBeVisible();
      expect(toastMocks.success).not.toHaveBeenCalled();
      await userEvent.click(
        within(row).getByRole("button", {
          name: en.home.status.actions.services,
        }),
      );
      expect(onOpenServices).toHaveBeenCalledWith("claude-code");
    });

    it("keeps a failed switch on the row without leaking its detail", async () => {
      serveProviders({
        "claude-code": [
          provider({ id: "kimi", name: "Kimi", active: true }),
          provider(),
        ],
      });
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_provider_activation_prepare`, () =>
          HttpResponse.json(
            {
              code: "CONFIG_WRITE_FAILED",
              messageKey: "error.provider.switchFailed",
              technicalMessage: "private path /Users/alice/.claude",
              remediation: "error.remediation.retryOrViewDetails",
              contextId: null,
            },
            { status: 500 },
          ),
        ),
      );
      mount([tool()]);

      const row = await findRow("Claude Code");
      const select = await within(row).findByRole("combobox");
      await waitFor(() => expect(select).toBeEnabled());
      await userEvent.selectOptions(select, "relay");

      expect(
        await within(row).findByRole("alert", {
          name: "Could not finish switching to Relay",
        }),
      ).toBeVisible();
      expect(toastMocks.success).not.toHaveBeenCalled();
    });

    it("updates one tool from its row through the same review", async () => {
      const seen: unknown[] = [];
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json("op-1");
        }),
      );
      mount([
        tool({ status: "updateAvailable", latestVersion: "1.1.0" }),
        tool({
          id: "codex",
          name: "Codex",
          status: "updateAvailable",
          latestVersion: "2.0.0",
        }),
      ]);
      const user = userEvent.setup();
      const row = await findRow("Codex");
      const update = within(row).getByRole("button", { name: "Update Codex" });
      await waitFor(() => expect(update).toBeEnabled());
      await user.click(update);

      const dialog = await screen.findByRole("dialog");
      await user.click(
        await within(dialog).findByRole("button", { name: /Update/ }),
      );
      await waitFor(() =>
        expect(seen).toEqual([
          { tool: "codex", previewFingerprint: "a".repeat(64) },
        ]),
      );
    });

    it("leaves updates and missing endpoints to the rows instead of repeating them as findings", async () => {
      const onOpenServices = vi.fn();
      mount(
        [tool({ status: "updateAvailable", latestVersion: "1.1.0" })],
        vi.fn(),
        vi.fn(),
        {
          providers: [
            {
              tool: "claude-code",
              configured: false,
              configuredCount: 0,
              checkTargets: [],
            },
          ],
          configs: [{ tool: "claude-code", status: "unreadable" }],
          mcp: { total: 2, enabled: 1 },
        },
        onOpenServices,
      );
      // The unreadable config and the pending update; the absent service is
      // informational and is not counted.
      expect(
        await screen.findByRole("heading", {
          level: 1,
          name: "2 items need attention",
        }),
      ).toBeVisible();
      const findings = await screen.findByRole("region", {
        name: en.home.health.title,
      });
      expect(within(findings).getAllByRole("listitem")).toHaveLength(1);
      expect(
        within(findings).queryByText("Claude Code has an update"),
      ).toBeNull();
      expect(
        within(findings).queryByText(
          "Claude Code has no API endpoint configured",
        ),
      ).toBeNull();
      const row = await findRow("Claude Code");
      expect(
        within(row).getByRole("button", { name: "Update Claude Code" }),
      ).toBeVisible();

      await userEvent.click(
        within(findings).getByRole("button", { name: "Review API Endpoints" }),
      );
      expect(onOpenServices).toHaveBeenCalledWith("claude-code");
    });
  });

  describe("update all", () => {
    it("is hidden while nothing has an update", async () => {
      mount([tool({ latestVersion: "1.0.0" })]);
      expect(await findRow("Claude Code")).toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: en.home.tools.updateAll }),
      ).toBeNull();
      expect(
        screen.getByRole("button", { name: en.home.tools.install }),
      ).toBeEnabled();
    });

    it("pauses Update All when the retained task baseline cannot refresh", async () => {
      const tools = [
        tool({ status: "updateAvailable", latestVersion: "1.1.0" }),
      ];
      let operationReads = 0;
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
          HttpResponse.json(tools),
        ),
        http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
          HttpResponse.json(healthySnapshot(tools)),
        ),
        http.post(`${TAURI_ENDPOINT}/app_operations_list`, () => {
          operationReads += 1;
          return operationReads === 2
            ? HttpResponse.text("private task payload", { status: 500 })
            : HttpResponse.json([]);
        }),
      );
      const client = renderHome();
      const updateAll = await screen.findByRole("button", {
        name: en.home.tools.updateAll,
      });
      await waitFor(() => expect(updateAll).toBeEnabled());
      await userEvent.click(updateAll);
      const dialog = screen.getByRole("dialog");
      const confirm = await within(dialog).findByRole("button", {
        name: "Update 1 tool",
      });
      expect(confirm).toBeEnabled();

      await client.invalidateQueries({ queryKey: operationKeys.all });
      expect(
        await within(dialog).findByRole("alert", {
          name: en.home.refreshError.title,
        }),
      ).toBeInTheDocument();
      expect(confirm).toBeDisabled();
      const cancel = within(dialog).getByRole("button", {
        name: en.ds.action.cancel,
      });
      expect(cancel).toBeEnabled();
      await userEvent.click(cancel);

      expect(
        screen.getByRole("alert", { name: en.home.refreshError.title }),
      ).toBeInTheDocument();
      expect(updateAll).toBeDisabled();
      expect(updateAll).toHaveAccessibleDescription(
        en.home.updateAll.hints.unavailable,
      );
      expect(
        screen.getByRole("button", { name: en.home.tools.install }),
      ).toBeEnabled();
      expect(document.body).not.toHaveTextContent("private task payload");
    });

    it("pauses an open bulk review when a candidate is no longer updateable", async () => {
      let toolReads = 0;
      const updates: unknown[] = [];
      const tools = [
        tool({ status: "updateAvailable", latestVersion: "1.1.0" }),
      ];
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tools_list`, () => {
          toolReads += 1;
          return HttpResponse.json([
            tool({
              status: toolReads === 1 ? "updateAvailable" : "installed",
              latestVersion: toolReads === 1 ? "1.1.0" : null,
            }),
          ]);
        }),
        http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
          HttpResponse.json(healthySnapshot(tools)),
        ),
        http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
          HttpResponse.json([]),
        ),
        http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
          updates.push(await request.json());
          return HttpResponse.json("00000000-0000-4000-8000-000000000001");
        }),
      );
      const client = renderHome();

      await userEvent.click(
        await screen.findByRole("button", { name: en.home.tools.updateAll }),
      );
      const dialog = await screen.findByRole("dialog", {
        name: "Review 1 tool update",
      });
      const confirm = await within(dialog).findByRole("button", {
        name: "Update 1 tool",
      });
      expect(confirm).toBeEnabled();

      await client.invalidateQueries({ queryKey: toolKeys.all });
      await waitFor(() => expect(toolReads).toBe(2));
      await waitFor(() => expect(confirm).toBeDisabled());
      await userEvent.click(confirm);
      expect(updates).toEqual([]);
    });

    it("previews every candidate and starts only ready updates", async () => {
      const previews: unknown[] = [];
      const updates: unknown[] = [];
      mount([
        tool({ status: "updateAvailable", latestVersion: "1.1.0" }),
        tool({
          id: "codex",
          name: "Codex",
          status: "updateAvailable",
          latestVersion: "2.0.0",
        }),
      ]);
      server.use(
        http.post(
          `${TAURI_ENDPOINT}/app_tools_update_preview`,
          async ({ request }) => {
            previews.push(await request.json());
            return HttpResponse.json([
              readyUpdatePreview("claude-code", "a".repeat(64)),
              blockedUpdatePreview("codex"),
            ]);
          },
        ),
        http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
          updates.push(await request.json());
          return HttpResponse.json("00000000-0000-4000-8000-000000000001");
        }),
      );

      const user = userEvent.setup();
      const updateAll = await screen.findByRole("button", {
        name: en.home.tools.updateAll,
      });
      await waitFor(() => expect(updateAll).toBeEnabled());
      await user.click(updateAll);
      await waitFor(() =>
        expect(previews).toEqual([{ tools: ["claude-code", "codex"] }]),
      );

      const dialog = await screen.findByRole("dialog", {
        name: "Review 2 tool updates",
      });
      expect(dialog).toHaveTextContent("Claude Code will update");
      expect(dialog).toHaveTextContent("Codex will be skipped");
      expect(updates).toEqual([]);
      await user.click(
        within(dialog).getByRole("button", { name: "Update 1 tool" }),
      );

      await waitFor(() =>
        expect(updates).toEqual([
          {
            tool: "claude-code",
            previewFingerprint: "a".repeat(64),
          },
        ]),
      );
    });

    it("keeps every blocked candidate visible without starting an update", async () => {
      const updates: unknown[] = [];
      let previewReads = 0;
      mount([
        tool({ status: "updateAvailable", latestVersion: "1.1.0" }),
        tool({
          id: "codex",
          name: "Codex",
          status: "updateAvailable",
          latestVersion: "2.0.0",
        }),
      ]);
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tools_update_preview`, () => {
          previewReads += 1;
          return HttpResponse.json([
            blockedUpdatePreview("claude-code"),
            blockedUpdatePreview("codex"),
          ]);
        }),
        http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
          updates.push(await request.json());
          return HttpResponse.json("00000000-0000-4000-8000-000000000001");
        }),
      );

      await userEvent.click(
        await screen.findByRole("button", { name: en.home.tools.updateAll }),
      );
      const dialog = await screen.findByRole("dialog", {
        name: "Review 2 tool updates",
      });
      expect(dialog).toHaveTextContent("Claude Code will be skipped");
      expect(dialog).toHaveTextContent("Codex will be skipped");
      expect(
        within(dialog).getByRole("button", { name: "Update 0 tools" }),
      ).toBeDisabled();
      await userEvent.click(
        within(dialog).getByRole("button", { name: "Check again" }),
      );
      await waitFor(() => expect(previewReads).toBe(2));
      expect(updates).toEqual([]);
    });

    it("reviews every out-of-date tool before scheduling updates", async () => {
      const seen: unknown[] = [];
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json("op-1");
        }),
      );
      mount([
        tool({ status: "updateAvailable", latestVersion: "1.1.0" }),
        tool({
          id: "codex",
          name: "Codex",
          status: "updateAvailable",
          latestVersion: "2.0.0",
        }),
        tool({ id: "opencode", name: "OpenCode" }),
      ]);
      const user = userEvent.setup();
      const updateAll = await screen.findByRole("button", {
        name: en.home.tools.updateAll,
      });
      await waitFor(() => expect(updateAll).toBeEnabled());
      await user.click(updateAll);

      const dialog = screen.getByRole("dialog", {
        name: "Review 2 tool updates",
      });
      expect(dialog).toHaveAccessibleDescription(
        "2 can update now. 0 will be skipped until its installation is reviewed.",
      );
      expect(seen).toEqual([]);
      const confirm = within(dialog).getByRole("button", {
        name: "Update 2 tools",
      });
      expect(confirm).toHaveFocus();
      await user.click(confirm);

      await waitFor(() => expect(seen).toHaveLength(2));
      expect(seen).toEqual([
        { tool: "claude-code", previewFingerprint: "a".repeat(64) },
        { tool: "codex", previewFingerprint: "b".repeat(64) },
      ]);
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    });

    it("keeps only failed updates selected and retries them in place", async () => {
      const seen: unknown[] = [];
      let codexAttempts = 0;
      let previewReads = 0;
      server.use(
        http.post(
          `${TAURI_ENDPOINT}/app_tools_update_preview`,
          async ({ request }) => {
            previewReads += 1;
            const body = (await request.json()) as { tools: string[] };
            return HttpResponse.json(
              body.tools.map((toolId, index) =>
                readyUpdatePreview(
                  toolId,
                  previewReads === 1
                    ? String.fromCharCode("a".charCodeAt(0) + index).repeat(64)
                    : "c".repeat(64),
                ),
              ),
            );
          },
        ),
        http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
          const body = await request.json();
          seen.push(body);
          if ((body as { tool?: string }).tool === "codex") {
            codexAttempts += 1;
            if (codexAttempts === 1) {
              return HttpResponse.json(
                {
                  code: "UPDATE_FAILED",
                  messageKey: "error.tool.updateFailed",
                  technicalMessage: "private updater path /Users/alice/tool",
                  remediation: "error.remediation.checkInternetConnection",
                  contextId: null,
                },
                { status: 500 },
              );
            }
          }
          return HttpResponse.json("op-update");
        }),
      );
      mount([
        tool({ status: "updateAvailable", latestVersion: "1.1.0" }),
        tool({
          id: "codex",
          name: "Codex",
          status: "updateAvailable",
          latestVersion: "2.0.0",
        }),
      ]);
      const user = userEvent.setup();
      const updateAll = await screen.findByRole("button", {
        name: en.home.tools.updateAll,
      });
      await waitFor(() => expect(updateAll).toBeEnabled());
      await user.click(updateAll);
      await user.click(
        within(screen.getByRole("dialog")).getByRole("button", {
          name: "Update 2 tools",
        }),
      );

      const alert = await screen.findByRole("alert", {
        name: "Some updates are now running",
      });
      expect(alert).toHaveTextContent(
        "1 update started in Activity. Only the tool that did not start remains selected.",
      );
      expect(alert).not.toHaveTextContent("/Users/alice/tool");
      const retryDialog = screen.getByRole("dialog", {
        name: "Review 1 tool update",
      });
      expect(retryDialog).toHaveAccessibleDescription(
        "1 can update now. 0 will be skipped until its installation is reviewed.",
      );
      expect(toastMocks.error).not.toHaveBeenCalled();

      await waitFor(() => expect(previewReads).toBe(2));
      expect(seen).toHaveLength(2);
      const refreshedConfirm = await within(retryDialog).findByRole("button", {
        name: "Update 1 tool",
      });
      await user.click(refreshedConfirm);
      await waitFor(() =>
        expect(seen).toEqual([
          { tool: "claude-code", previewFingerprint: "a".repeat(64) },
          { tool: "codex", previewFingerprint: "b".repeat(64) },
          { tool: "codex", previewFingerprint: "c".repeat(64) },
        ]),
      );
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    });

    it("keeps skipped tools visible after a failed ready item later succeeds", async () => {
      let previewReads = 0;
      let codexAttempts = 0;
      const onOpenTools = vi.fn();
      mount(
        [
          tool({ status: "updateAvailable", latestVersion: "1.1.0" }),
          tool({
            id: "codex",
            name: "Codex",
            status: "updateAvailable",
            latestVersion: "2.0.0",
          }),
          tool({
            id: "opencode",
            name: "OpenCode",
            status: "updateAvailable",
            latestVersion: "3.0.0",
          }),
        ],
        onOpenTools,
      );
      server.use(
        http.post(
          `${TAURI_ENDPOINT}/app_tools_update_preview`,
          async ({ request }) => {
            previewReads += 1;
            const body = (await request.json()) as { tools: string[] };
            if (previewReads === 1) {
              return HttpResponse.json([
                readyUpdatePreview("claude-code", "a".repeat(64)),
                readyUpdatePreview("codex", "b".repeat(64)),
                blockedUpdatePreview("opencode"),
              ]);
            }
            expect(body).toEqual({ tools: ["codex"] });
            return HttpResponse.json([
              readyUpdatePreview("codex", "c".repeat(64)),
            ]);
          },
        ),
        http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
          const body = (await request.json()) as { tool: string };
          if (body.tool === "codex" && codexAttempts++ === 0) {
            return HttpResponse.json(
              {
                code: "UPDATE_FAILED",
                messageKey: "error.tool.updateFailed",
                technicalMessage: null,
                remediation: "error.remediation.retryOrViewDetails",
                contextId: null,
              },
              { status: 500 },
            );
          }
          return HttpResponse.json("00000000-0000-4000-8000-000000000001");
        }),
      );

      await userEvent.click(
        await screen.findByRole("button", { name: en.home.tools.updateAll }),
      );
      let dialog = await screen.findByRole("dialog", {
        name: "Review 3 tool updates",
      });
      await userEvent.click(
        within(dialog).getByRole("button", { name: "Update 2 tools" }),
      );
      dialog = await screen.findByRole("dialog", {
        name: "Review 1 tool update",
      });
      await waitFor(() => expect(previewReads).toBe(2));
      await userEvent.click(
        await within(dialog).findByRole("button", { name: "Update 1 tool" }),
      );

      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      const skipped = screen.getByRole("alert");
      expect(skipped).toHaveTextContent(
        "1 tool was skipped because its update method could not be confirmed.",
      );
      await userEvent.click(
        within(skipped).getByRole("button", {
          name: "Review this update in Software",
        }),
      );
      expect(onOpenTools).toHaveBeenCalledOnce();
    });

    it("keeps every failed update in the confirmation instead of closing it", async () => {
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tool_update`, () =>
          HttpResponse.json(
            {
              code: "UPDATE_FAILED",
              messageKey: "error.tool.updateFailed",
              technicalMessage: "private updater exit code 127",
              remediation: "error.remediation.checkInternetConnection",
              contextId: null,
            },
            { status: 500 },
          ),
        ),
      );
      mount([tool({ status: "updateAvailable", latestVersion: "1.1.0" })]);
      const user = userEvent.setup();
      const updateAll = await screen.findByRole("button", {
        name: en.home.tools.updateAll,
      });
      await waitFor(() => expect(updateAll).toBeEnabled());
      await user.click(updateAll);
      const dialog = screen.getByRole("dialog", {
        name: "Review 1 tool update",
      });
      await user.click(
        within(dialog).getByRole("button", { name: "Update 1 tool" }),
      );

      const alert = await screen.findByRole("alert", {
        name: "Updates could not start",
      });
      expect(alert).toHaveTextContent(
        "No update task was created. Retry checks this tool again before starting anything.",
      );
      expect(alert).not.toHaveTextContent("exit code 127");
      expect(
        await within(dialog).findByRole("button", { name: "Update 1 tool" }),
      ).toBeEnabled();
      expect(toastMocks.error).not.toHaveBeenCalled();
    });

    it("sends capability-blocked updates to the tools review path", async () => {
      const onOpenTools = vi.fn();
      mount(
        [
          tool({
            status: "updateAvailable",
            latestVersion: "1.1.0",
            capabilities: { ...CAPABILITIES, canUpdate: false },
          }),
        ],
        onOpenTools,
      );

      expect(
        await screen.findByText(en.home.updateAll.hints.review),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: en.home.tools.updateAll }),
      ).toBeDisabled();
      const row = await findRow("Claude Code");
      await userEvent.click(
        within(row).getByRole("button", { name: "Update Claude Code" }),
      );
      expect(onOpenTools).toHaveBeenCalledTimes(1);
      expect(screen.queryByRole("dialog")).toBeNull();
    });

    it("says when task state is unavailable instead of risking a duplicate", async () => {
      const tools = [
        tool({ status: "updateAvailable", latestVersion: "1.1.0" }),
      ];
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
          HttpResponse.json(tools),
        ),
        http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
          HttpResponse.json(healthySnapshot(tools)),
        ),
        http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
          HttpResponse.text("not available", { status: 500 }),
        ),
      );
      renderHome();

      expect(
        await screen.findByText(en.home.updateAll.hints.unavailable),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: en.home.tools.updateAll }),
      ).toBeDisabled();
    });

    it("does not schedule a tool whose update task is already running", async () => {
      mount(
        [tool({ status: "updateAvailable", latestVersion: "1.1.0" })],
        vi.fn(),
        vi.fn(),
        undefined,
        vi.fn(),
        [RUNNING_UPDATE],
      );

      const updateAll = await screen.findByRole("button", {
        name: en.home.tools.updateAll,
      });
      expect(
        await screen.findByText(en.home.updateAll.hints.running),
      ).toBeInTheDocument();
      expect(updateAll).toBeDisabled();
      const row = await findRow("Claude Code");
      expect(
        within(row).getByRole("button", { name: "Update Claude Code" }),
      ).toHaveAttribute("aria-busy", "true");
    });

    it("updates free tools while leaving an already-running tool alone", async () => {
      const seen: unknown[] = [];
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json("op-codex");
        }),
      );
      const tools = [
        tool({ status: "updateAvailable", latestVersion: "1.1.0" }),
        tool({
          id: "codex",
          name: "Codex",
          status: "updateAvailable",
          latestVersion: "2.0.0",
        }),
      ];
      mount(tools, vi.fn(), vi.fn(), healthySnapshot(tools), vi.fn(), [
        RUNNING_UPDATE,
      ]);

      const user = userEvent.setup();
      const updateAll = await screen.findByRole("button", {
        name: en.home.tools.updateAll,
      });
      await waitFor(() => expect(updateAll).toBeEnabled());
      await user.click(updateAll);
      const dialog = screen.getByRole("dialog", {
        name: "Review 1 tool update",
      });
      expect(dialog).toHaveAccessibleDescription(
        "1 can update now. 0 will be skipped until its installation is reviewed.",
      );
      await user.click(
        within(dialog).getByRole("button", { name: "Update 1 tool" }),
      );

      await waitFor(() =>
        expect(seen).toEqual([
          { tool: "codex", previewFingerprint: "a".repeat(64) },
        ]),
      );
    });

    it("lets the user leave while update tasks are still being scheduled", async () => {
      let release!: () => void;
      const scheduled = new Promise<void>((resolve) => {
        release = resolve;
      });
      const seen: unknown[] = [];
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
          seen.push(await request.json());
          await scheduled;
          return HttpResponse.json("00000000-0000-4000-8000-000000000001");
        }),
      );
      mount([tool({ status: "updateAvailable", latestVersion: "1.1.0" })]);
      const user = userEvent.setup();
      const updateAll = await screen.findByRole("button", {
        name: en.home.tools.updateAll,
      });
      await waitFor(() => expect(updateAll).toBeEnabled());
      await user.click(updateAll);
      const dialog = screen.getByRole("dialog", {
        name: "Review 1 tool update",
      });
      const confirm = within(dialog).getByRole("button", {
        name: "Update 1 tool",
      });
      await user.click(confirm);

      await waitFor(() => expect(confirm).toHaveAttribute("aria-busy", "true"));

      // Scheduling can take seconds on a restricted network. The user is not
      // held here for it: leaving does not undo the hand-off, and the task
      // still lands in the activity panel.
      await user.keyboard("{Escape}");
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

      release();
      await waitFor(() => expect(seen).toHaveLength(1));
      expect(screen.queryByRole("dialog")).toBeNull();
    });
  });
});
