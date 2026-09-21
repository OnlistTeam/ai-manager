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

function mount(
  tools: unknown[],
  onOpenTools = vi.fn(),
  onOpenExtensions = vi.fn(),
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
      onOpenExtensions={onOpenExtensions}
    />,
    {
      wrapper: withQueryClient(createTestQueryClient()),
    },
  );
  return onOpenTools;
}

describe("HomePage", () => {
  beforeEach(async () => {
    toastMocks.error.mockClear();
    toastMocks.info.mockClear();
    toastMocks.success.mockClear();
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
        home: en.home,
        preferences: { check: en.preferences.check },
        tools: en.tools,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("leads with the environment outcome instead of a time-of-day greeting", async () => {
    mount([tool()]);
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: en.home.card.allGood,
      }),
    ).toBeInTheDocument();
    for (const greeting of Object.values(en.home.greeting)) {
      expect(screen.queryByText(greeting)).toBeNull();
    }
  });

  it("shows the Ready outcome without repeating inventory counts", async () => {
    mount([tool(), tool({ id: "codex", name: "Codex" })]);
    expect(await screen.findByText(en.ds.status.ready)).toBeInTheDocument();
    const hero = screen
      .getByText(en.home.card.allGood)
      .closest('[aria-labelledby="home-environment-title"]');
    expect(hero).toHaveClass("min-h-[430px]", "min-w-0", "lg:min-h-[350px]");
    expect(hero).toHaveAttribute("data-model", "environment");
    expect(hero).toHaveAttribute("data-tone", "success");
    expect(hero).toHaveAttribute("data-spatial-stage");
    expect(hero?.querySelector('[data-model="environment"]')).not.toBeNull();
    expect(hero).not.toHaveTextContent("2 tools installed");
    expect(
      screen.queryByRole("list", { name: en.home.card.inventory.label }),
    ).toBeNull();
  });

  it("keeps service and MCP inventory out of the primary hero", async () => {
    const tools = [tool(), tool({ id: "codex", name: "Codex" })];
    mount(tools, vi.fn(), vi.fn(), {
      providers: [
        {
          tool: "claude-code",
          configured: true,
          configuredCount: 2,
          checkTargets: [],
        },
        {
          tool: "codex",
          configured: true,
          configuredCount: 1,
          checkTargets: [],
        },
      ],
      configs: [
        { tool: "claude-code", status: "readable" },
        { tool: "codex", status: "readable" },
      ],
      mcp: { total: 8, enabled: 2 },
    });

    expect(await screen.findByText(en.home.card.allGood)).toBeInTheDocument();
    expect(
      screen.queryByRole("list", { name: en.home.card.inventory.label }),
    ).toBeNull();
    expect(screen.queryByText("3 AI services configured")).toBeNull();
    expect(screen.queryByText("MCP enabled: 2 of 8")).toBeNull();
  });

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

    const start = await screen.findByRole("button", {
      name: en.home.card.startTool.replace("{{name}}", "Claude Code"),
    });
    expect(screen.queryByText(en.home.card.startHint)).toBeNull();
    await user.click(start);

    const dialog = screen.getByRole("dialog", { name: "Open Claude Code" });
    expect(
      within(dialog).getByText(en.tools.open.localTitle),
    ).toBeInTheDocument();
    expect(seen).toEqual([]);
    const confirm = within(dialog).getByRole("button", {
      name: en.tools.open.confirm,
    });
    expect(confirm).toHaveFocus();
    await user.click(confirm);

    await waitFor(() =>
      expect(seen).toEqual([{ tool: "claude-code", directoryMode: "default" }]),
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
    const client = createTestQueryClient();
    render(
      <HomePage
        onOpenTools={vi.fn()}
        onOpenServices={vi.fn()}
        onOpenExtensions={vi.fn()}
      />,
      { wrapper: withQueryClient(client) },
    );
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.home.card.startTool.replace("{{name}}", "Claude Code"),
      }),
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
    expect(screen.queryByText(en.home.card.startHint)).not.toBeInTheDocument();
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
    const client = createTestQueryClient();
    render(
      <HomePage
        onOpenTools={vi.fn()}
        onOpenServices={vi.fn()}
        onOpenExtensions={vi.fn()}
      />,
      { wrapper: withQueryClient(client) },
    );
    const startName = en.home.card.startTool.replace("{{name}}", "Claude Code");
    expect(
      await screen.findByRole("button", { name: startName }),
    ).toBeInTheDocument();

    void client.invalidateQueries({ queryKey: toolKeys.all });
    await waitFor(() =>
      expect(client.getQueryState(toolKeys.list())?.fetchStatus).toBe(
        "fetching",
      ),
    );
    expect(
      screen.queryByRole("button", { name: startName }),
    ).not.toBeInTheDocument();

    releaseRefresh();
    await waitFor(() =>
      expect(client.getQueryState(toolKeys.list())?.status).toBe("error"),
    );
    expect(screen.getByText(en.home.card.allGood)).toBeInTheDocument();
    const alert = screen.getByRole("alert", {
      name: en.home.refreshError.title,
    });
    expect(alert).toHaveTextContent(en.home.refreshError.description);
    expect(alert).toHaveClass("min-w-0", "flex-col", "sm:flex-row");
    expect(screen.getByRole("region", { name: en.home.pageLabel })).toHaveClass(
      "min-w-0",
    );
    expect(screen.queryByText(en.home.subtitle.stale)).toBeNull();
    const retry = screen.getByRole("button", {
      name: en.home.refreshError.action,
    });
    expect(
      screen.queryByRole("button", { name: startName }),
    ).not.toBeInTheDocument();

    await userEvent.click(retry);
    await waitFor(() => expect(listCalls).toBe(3));
    await waitFor(() => expect(retry).toHaveAttribute("aria-busy", "true"));
    expect(retry).toBeDisabled();
    expect(alert).toHaveAttribute("aria-busy", "true");
    const install = screen.getByRole("button", {
      name: en.home.quickActions.installTool,
    });
    install.focus();
    releaseRecovery();

    await waitFor(() => expect(alert).not.toBeInTheDocument());
    expect(
      await screen.findByRole("button", { name: startName }),
    ).toBeEnabled();
    expect(install).toHaveFocus();
  });

  it("pauses Update All when the retained task baseline cannot refresh", async () => {
    const tools = [tool({ status: "updateAvailable", latestVersion: "1.1.0" })];
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
    const client = createTestQueryClient();
    render(
      <HomePage
        onOpenTools={vi.fn()}
        onOpenServices={vi.fn()}
        onOpenExtensions={vi.fn()}
      />,
      { wrapper: withQueryClient(client) },
    );
    const updateAll = await screen.findByRole("button", {
      name: en.home.quickActions.updateAll,
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
    expect(
      screen.getByRole("button", {
        name: en.home.quickActions.installTool,
      }),
    ).toBeEnabled();
    expect(document.body).not.toHaveTextContent("private task payload");
  });

  it("shows the highest-priority next step and opens its real destination", async () => {
    const onOpenTools = vi.fn();
    mount(
      [tool({ status: "updateAvailable", latestVersion: "1.1.0" })],
      onOpenTools,
    );
    expect(await screen.findByText(en.ds.status.attention)).toBeInTheDocument();
    // Both the loading state and the "update available" state show Needs Attention
    // (see HomePage's honest-loading-state design), so the findByText above may match
    // during the loading state. Wait once more for the real count to render, to avoid
    // asserting before the fetch has actually landed.
    expect(
      await screen.findByText("1 item needs attention"),
    ).toBeInTheDocument();
    const heroHeading = screen.getByRole("heading", {
      name: "1 item needs attention",
    });
    const hero = heroHeading.closest<HTMLElement>(
      '[aria-labelledby="home-environment-title"]',
    );
    expect(hero).not.toBeNull();
    // The hero itself only carries the count and the primary action; the
    // health-check list below is the one place that names the problem —
    // even when there is exactly one, not only when there are several.
    expect(
      within(hero as HTMLElement).queryByText("Claude Code has an update"),
    ).not.toBeInTheDocument();
    const guide = screen.getByRole("region", { name: en.home.health.title });
    expect(
      within(guide).getByText("Claude Code has an update"),
    ).toBeInTheDocument();
    expect(
      within(guide).getByText(
        "Version 1.0.0 is installed; 1.1.0 is available.",
      ),
    ).toBeInTheDocument();
    const updateStatus = screen.getByText("1 update available");
    const actions = updateStatus.closest<HTMLElement>(
      '[data-slot="environment-hero-actions"]',
    );
    expect(actions).not.toBeNull();
    const review = within(actions as HTMLElement).getByRole("button", {
      name: en.home.card.actions.tools,
    });
    expect(updateStatus).toHaveClass("h-12", "rounded-lg");
    expect(review).toHaveClass(
      "h-12",
      "rounded-lg",
      "environment-hero__review",
    );
    expect(
      screen.queryByRole("list", { name: en.home.card.inventory.label }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: en.home.card.startTool.replace("{{name}}", "Claude Code"),
      }),
    ).not.toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: en.home.card.actions.tools }),
    );
    expect(onOpenTools).toHaveBeenCalledTimes(1);
  });

  it("takes a user with no service straight to AI Services", async () => {
    const onOpenServices = vi.fn();
    const onOpenExtensions = vi.fn();
    mount(
      [tool()],
      vi.fn(),
      onOpenExtensions,
      {
        providers: [
          {
            tool: "claude-code",
            configured: false,
            configuredCount: 0,
            checkTargets: [],
          },
        ],
        configs: [{ tool: "claude-code", status: "readable" }],
        mcp: { total: 0, enabled: 0 },
      },
      onOpenServices,
    );

    // The hero itself only carries the count and the primary action; the
    // health-check list below names the specific problem, including when
    // there is exactly one of them.
    const services = await screen.findByRole("button", {
      name: en.home.card.actions.services,
    });
    const guide = screen.getByRole("region", { name: en.home.health.title });
    expect(
      within(guide).getByText("Claude Code has no AI service configured"),
    ).toBeInTheDocument();
    await userEvent.click(services);
    expect(onOpenServices).toHaveBeenCalledOnce();
    expect(onOpenServices).toHaveBeenCalledWith("claude-code");
    expect(onOpenExtensions).not.toHaveBeenCalled();
  });

  it("says nothing is installed rather than claiming zero items need attention", async () => {
    mount([tool({ status: "notInstalled", version: null })]);
    expect(
      await screen.findByText(en.home.card.nothingInstalled),
    ).toBeInTheDocument();
    expect(screen.queryByText(/needs attention/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("list", { name: en.home.card.inventory.label }),
    ).not.toBeInTheDocument();
  });

  it("escalates to Action Required for a broken tool", async () => {
    mount([tool({ status: "broken" })]);
    expect(await screen.findByText(en.ds.status.action)).toBeInTheDocument();
  });

  it("pauses an open bulk review when a candidate is no longer updateable", async () => {
    let toolReads = 0;
    const updates: unknown[] = [];
    const tools = [tool({ status: "updateAvailable", latestVersion: "1.1.0" })];
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
    const client = createTestQueryClient();
    render(
      <HomePage
        onOpenTools={vi.fn()}
        onOpenServices={vi.fn()}
        onOpenExtensions={vi.fn()}
      />,
      { wrapper: withQueryClient(client) },
    );

    await userEvent.click(
      await screen.findByRole("button", {
        name: en.home.quickActions.updateAll,
      }),
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
      name: en.home.quickActions.updateAll,
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
      await screen.findByRole("button", {
        name: en.home.quickActions.updateAll,
      }),
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
      name: en.home.quickActions.updateAll,
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
    expect(updateAll).toHaveFocus();
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
      name: en.home.quickActions.updateAll,
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
      "1 update started in Tasks. Only the tool that did not start remains selected.",
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
      await screen.findByRole("button", {
        name: en.home.quickActions.updateAll,
      }),
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
        name: "Review this update in AI Tools",
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
      name: en.home.quickActions.updateAll,
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

  it("explains why Update All is disabled when everything is current", async () => {
    mount([tool({ latestVersion: "1.0.0" })]);
    expect(
      await screen.findByText("Everything is already up to date"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.home.quickActions.updateAll }),
    ).toBeDisabled();
  });

  /// When the version can't be looked up (offline, rate-limited, or not back yet),
  /// "everything is already up to date" is an unverified claim. In that case we only
  /// say "temporarily unable to confirm" instead of drawing a conclusion for the user.
  it("does not claim everything is current while no latest version is known", async () => {
    mount([tool({ latestVersion: null })]);
    expect(
      await screen.findByText(en.home.updateAll.hints.unavailable),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("Everything is already up to date"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.home.quickActions.updateAll }),
    ).toBeDisabled();
  });

  it("explains the temporary checking state before task data is ready", async () => {
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
    render(
      <HomePage
        onOpenTools={vi.fn()}
        onOpenServices={vi.fn()}
        onOpenExtensions={vi.fn()}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    );
    expect(
      await screen.findByText("Checking what can be updated"),
    ).toBeInTheDocument();

    expect(
      screen.queryByText(
        "Checking your setup before suggesting the next step.",
      ),
    ).toBeNull();
    expect(screen.queryByText(en.home.subtitle.attention)).toBeNull();

    const environmentStatus = screen.getByRole("status", {
      name: en.home.card.checking,
    });
    expect(environmentStatus).toHaveClass("min-h-[430px]", "lg:min-h-[350px]");
    expect(environmentStatus).toHaveAttribute("data-model", "environment");
    expect(environmentStatus).toHaveAttribute("data-tone", "neutral");
    expect(
      within(environmentStatus).getByRole("heading", { level: 1 }),
    ).toBeInTheDocument();
    expect(
      environmentStatus.querySelector('[data-loading="true"]'),
    ).not.toBeNull();
    expect(environmentStatus).toHaveTextContent(en.home.card.checking);
    expect(within(environmentStatus).queryByRole("button")).toBeNull();
  });

  it("sends capability-blocked updates to the tools review path", async () => {
    mount([
      tool({
        status: "updateAvailable",
        latestVersion: "1.1.0",
        capabilities: { ...CAPABILITIES, canUpdate: false },
      }),
    ]);

    expect(
      await screen.findByText("Review this update in AI Tools"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.home.quickActions.updateAll }),
    ).toBeDisabled();
  });

  it("says when task state is unavailable instead of risking a duplicate", async () => {
    const tools = [tool({ status: "updateAvailable", latestVersion: "1.1.0" })];
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
    render(
      <HomePage
        onOpenTools={vi.fn()}
        onOpenServices={vi.fn()}
        onOpenExtensions={vi.fn()}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    );

    expect(
      await screen.findByText("Update status is unavailable right now"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.home.quickActions.updateAll }),
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
      name: en.home.quickActions.updateAll,
    });
    expect(
      await screen.findByText("Updates are already running in Tasks"),
    ).toBeInTheDocument();
    expect(updateAll).toBeDisabled();
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
      name: en.home.quickActions.updateAll,
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
      name: en.home.quickActions.updateAll,
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

  it("opens AI Services from the Connect quick action", async () => {
    const tools = [tool()];
    const onOpenServices = vi.fn();
    mount(tools, vi.fn(), vi.fn(), healthySnapshot(tools), onOpenServices);
    const connect = await screen.findByRole("button", {
      name: en.home.quickActions.connectService,
    });
    expect(connect).toBeEnabled();
    await userEvent.click(connect);
    expect(onOpenServices).toHaveBeenCalledTimes(1);
    expect(onOpenServices).toHaveBeenCalledWith();
    expect(
      screen.getAllByRole("button").map((button) => button.textContent),
    ).not.toContain("Check Setup");
  });

  it("states the environment once, with the check's own timestamp beside it", async () => {
    mount([tool()]);
    expect(await screen.findByText(en.home.card.allGood)).toBeVisible();
    expect(screen.getByText(/Last checked/)).toBeVisible();
    // Nothing repeats the headline below the hero when all is well.
    expect(
      screen.queryByRole("region", { name: en.home.health.title }),
    ).toBeNull();
    expect(screen.queryByText(/local signal|Based on/)).toBeNull();
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
    render(
      <HomePage
        onOpenTools={vi.fn()}
        onOpenServices={vi.fn()}
        onOpenExtensions={vi.fn()}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    );
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

  it("lists the remaining problems only when there is more than one", async () => {
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
    // Three problems earn a list of their own; the hero states the count.
    expect(await screen.findByText("3 items need attention")).toBeVisible();
    const health = await screen.findByRole("region", {
      name: en.home.health.title,
    });
    expect(
      within(health).getByText("2 more").closest("details"),
    ).not.toHaveAttribute("open");
    await userEvent.click(
      within(health).getByRole("button", { name: "Review API Endpoints" }),
    );
    expect(onOpenServices).toHaveBeenCalledWith("claude-code");
  });

  it("combines tool, provider and config issues into the authoritative count", async () => {
    const installed = tool({
      status: "updateAvailable",
      latestVersion: "1.1.0",
    });
    mount([installed], vi.fn(), vi.fn(), {
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
    });
    expect(
      await screen.findByText("3 items need attention"),
    ).toBeInTheDocument();
    expect(await screen.findByText(en.ds.status.action)).toBeInTheDocument();
  });

  it("sends the user to the tools page from Install AI Tool", async () => {
    const onOpenTools = mount([tool()]);
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.home.quickActions.installTool,
      }),
    );
    expect(onOpenTools).toHaveBeenCalledTimes(1);
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
    render(
      <HomePage
        onOpenTools={vi.fn()}
        onOpenServices={vi.fn()}
        onOpenExtensions={vi.fn()}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    );
    const alert = await screen.findByRole("alert", {
      name: en.home.error.title,
    });
    expect(alert).toHaveClass("spatial-page-hero", "min-h-[430px]");
    expect(alert).toHaveAttribute("data-model", "environment");
    expect(alert).toHaveAttribute("data-tone", "danger");
    expect(alert).toHaveAttribute("data-spatial-stage");
    expect(alert.querySelector('[data-model="environment"]')).not.toBeNull();
    expect(
      within(alert).getByRole("heading", { level: 1 }),
    ).toBeInTheDocument();
    expect(screen.queryByText(en.home.subtitle.action)).toBeNull();
    expect(screen.queryByText(en.home.subtitle.checking)).toBeNull();
    expect(screen.queryByText(en.ds.status.ready)).not.toBeInTheDocument();
    const retry = screen.getByRole("button", {
      name: en.home.refreshError.action,
    });

    await userEvent.click(retry);
    await waitFor(() => expect(toolReads).toBe(2));
    await waitFor(() => expect(retry).toHaveAttribute("aria-busy", "true"));
    expect(retry).toBeDisabled();
    expect(alert).toHaveAttribute("aria-busy", "true");
    expect(alert.querySelector('[data-loading="true"]')).not.toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
    expect(document.body).not.toHaveTextContent("private setup path");

    releaseRetry();
    expect(await screen.findByText(en.home.card.allGood)).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByRole("region", { name: en.home.pageLabel }),
      ).toHaveFocus(),
    );
  });
});
