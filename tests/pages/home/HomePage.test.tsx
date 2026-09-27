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
) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
      HttpResponse.json(tools),
    ),
    http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
      HttpResponse.json(snapshot),
    ),
    http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
      HttpResponse.json([]),
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
      expect(screen.queryByRole("article")).toBeNull();
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
    beforeEach(() => {
      // cmdk keeps the highlighted option in view; jsdom has no layout.
      Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
        configurable: true,
        value: vi.fn(),
      });
    });

    const pickerFor = (row: HTMLElement, tool: string) =>
      within(row).findByRole("button", {
        name: new RegExp(`^Endpoint ${tool} uses: `),
      });

    async function openPicker(row: HTMLElement, tool: string) {
      const pill = await pickerFor(row, tool);
      await waitFor(() => expect(pill).toBeEnabled());
      await userEvent.click(pill);
      return screen.findByRole("listbox", { name: `Endpoints for ${tool}` });
    }

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

    it("names the endpoint in use and the model it pins on the row's picker", async () => {
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
      const pill = await pickerFor(row, "Claude Code");
      expect(await within(pill).findByText("Kimi")).toBeVisible();
      expect(await within(pill).findByText("kimi-k2")).toBeVisible();
    });

    it("names the official entry as the list does, and says Official sign-in for the tool's own login", async () => {
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
      expect(await within(claude).findByText("Claude Official")).toBeVisible();
      expect(within(claude).queryByText(en.home.tools.official)).toBeNull();
      const gemini = await findRow("Gemini CLI");
      expect(
        await within(gemini).findByText(en.home.tools.official),
      ).toBeVisible();
    });

    it("lists the endpoint in use first and checked, then the rest in saved order", async () => {
      serveProviders({
        "claude-code": [
          provider({ id: "relay", name: "Relay" }),
          provider({ id: "kimi", name: "Kimi", active: true }),
          provider({ id: "backup", name: "Backup" }),
        ],
      });
      mount([tool()]);

      const listbox = await openPicker(
        await findRow("Claude Code"),
        "Claude Code",
      );
      const options = within(listbox).getAllByRole("option");
      expect(options.map((option) => option.textContent)).toEqual([
        "Kimi",
        "Relay",
        "Backup",
        en.home.tools.manageEndpoints,
      ]);
      expect(options[0]).toHaveAttribute("aria-current", "true");
      expect(options[1]).not.toHaveAttribute("aria-current");
      // A short list has no filter field.
      expect(screen.queryByPlaceholderText(en.home.tools.filter)).toBeNull();
    });

    it("switches through the shared preflight when an endpoint is picked", async () => {
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
      const listbox = await openPicker(row, "Claude Code");
      await userEvent.click(
        within(listbox).getByRole("option", { name: "Relay" }),
      );

      await waitFor(() =>
        expect(activations).toEqual([
          { tool: "claude-code", provider: "relay" },
        ]),
      );
      expect(screen.queryByRole("listbox")).toBeNull();
      expect(
        await within(await pickerFor(row, "Claude Code")).findByText("Relay"),
      ).toBeVisible();
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

    it("opens the tool's API endpoints from the picker", async () => {
      const onOpenServices = vi.fn();
      serveProviders({
        codex: [provider({ id: "team", tool: "codex", active: true })],
      });
      mount(
        [tool(), tool({ id: "codex", name: "Codex" })],
        vi.fn(),
        vi.fn(),
        undefined,
        onOpenServices,
      );

      const listbox = await openPicker(await findRow("Codex"), "Codex");
      await userEvent.click(
        within(listbox).getByRole("option", {
          name: en.home.tools.manageEndpoints,
        }),
      );
      expect(onOpenServices).toHaveBeenCalledWith("codex");
      expect(screen.queryByRole("listbox")).toBeNull();
    });

    it("says there are no endpoints yet and offers to add one", async () => {
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
      const listbox = await openPicker(row, "OpenCode");
      expect(screen.getByText(en.home.tools.noEndpoints)).toBeVisible();
      const options = within(listbox).getAllByRole("option");
      expect(options.map((option) => option.textContent)).toEqual([
        en.home.tools.addEndpoint,
      ]);
      await userEvent.click(options[0]);
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
      const listbox = await openPicker(row, "OpenCode");
      // Nothing to pick here: the tool chooses the model itself.
      expect(screen.getByText("Model chosen in OpenCode")).toBeVisible();
      expect(
        within(listbox)
          .getAllByRole("option")
          .map((option) => option.textContent),
      ).toEqual([en.home.tools.manageEndpoints]);
    });

    it("filters a long list of endpoints", async () => {
      serveProviders({
        "claude-code": Array.from({ length: 8 }, (_, index) =>
          provider({
            id: `relay-${index}`,
            name: index === 5 ? "Team Gateway" : `Relay ${index}`,
            active: index === 0,
          }),
        ),
      });
      mount([tool()]);

      const listbox = await openPicker(
        await findRow("Claude Code"),
        "Claude Code",
      );
      const filter = screen.getByPlaceholderText(en.home.tools.filter);
      await waitFor(() => expect(filter).toHaveFocus());
      await userEvent.type(filter, "gateway");
      await waitFor(() =>
        expect(
          within(listbox)
            .getAllByRole("option")
            .map((option) => option.textContent),
        ).toEqual(["Team Gateway", en.home.tools.manageEndpoints]),
      );
    });

    it("shows an unreadable list as plain text instead of a picker", async () => {
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
          HttpResponse.text("private path", { status: 500 }),
        ),
      );
      mount([tool()]);

      const row = await findRow("Claude Code");
      expect(
        await within(row).findByText(en.home.tools.unavailable),
      ).toBeVisible();
      expect(within(row).queryByRole("button")).toBeNull();
    });

    it("has no update button and no whole-row target", async () => {
      const onOpenTools = vi.fn();
      mount(
        [tool({ status: "updateAvailable", latestVersion: "1.1.0" })],
        onOpenTools,
      );

      const row = await findRow("Claude Code");
      await pickerFor(row, "Claude Code");
      expect(within(row).getAllByRole("button")).toHaveLength(1);
      expect(within(row).queryByRole("button", { name: /Update/ })).toBeNull();
      expect(screen.queryByRole("button", { name: /Update all/i })).toBeNull();
      await userEvent.click(
        screen.getByRole("button", { name: "1 update available" }),
      );
      expect(onOpenTools).toHaveBeenCalledTimes(1);
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
      const listbox = await openPicker(row, "Claude Code");
      await userEvent.click(
        within(listbox).getByRole("option", { name: "Relay" }),
      );

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
      const listbox = await openPicker(row, "Claude Code");
      await userEvent.click(
        within(listbox).getByRole("option", { name: "Relay" }),
      );

      expect(
        await within(row).findByRole("alert", {
          name: "Could not finish switching to Relay",
        }),
      ).toBeVisible();
      expect(toastMocks.success).not.toHaveBeenCalled();
    });

    it("leaves updates and missing endpoints out of the findings", async () => {
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
      expect(
        within(findings).queryByRole("button", { name: /Update/ }),
      ).toBeNull();

      await userEvent.click(
        within(findings).getByRole("button", { name: "Review API Endpoints" }),
      );
      expect(onOpenServices).toHaveBeenCalledWith("claude-code");
    });
  });
});
