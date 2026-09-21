import { render, screen, waitFor, within } from "@testing-library/react";
import type { ComponentProps } from "react";
import userEvent from "@testing-library/user-event";
import { delay, http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toolKeys } from "@/entities/tool";
import en from "@/i18n/locales/en.json";
import { ToolsPage } from "@/pages/tools/ToolsPage";
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
    status: "notInstalled",
    version: null,
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: null,
    ...overrides,
  };
}

function readyUpdatePreview(
  toolId = "claude-code",
  fingerprint = "a".repeat(64),
) {
  return {
    state: "ready",
    preview: {
      tool: toolId,
      previewFingerprint: fingerprint,
      targetVersion: "2.4.0",
      source: "nativeInstaller",
      installations: [
        {
          source: "nativeInstaller",
          version: "2.3.1",
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

function mount(
  tools: unknown[],
  operations: unknown[] = [],
  props: ComponentProps<typeof ToolsPage> = {},
) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
      HttpResponse.json(tools),
    ),
    http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
      HttpResponse.json(operations),
    ),
    http.post(
      `${TAURI_ENDPOINT}/app_tools_update_preview`,
      async ({ request }) => {
        const body = (await request.json()) as { tools: string[] };
        return HttpResponse.json(
          body.tools.map((toolId) => readyUpdatePreview(toolId)),
        );
      },
    ),
    http.post(
      `${TAURI_ENDPOINT}/app_tool_uninstall_preview`,
      async ({ request }) => {
        const body = (await request.json()) as { tool: string };
        return HttpResponse.json({
          tool: body.tool,
          app: [
            {
              kind: "command",
              value:
                "/Users/test/.nvm/versions/node/v22/bin/npm uninstall -g @anthropic-ai/claude-code",
              canRemoveAutomatically: true,
            },
          ],
          settings: [
            {
              kind: "directory",
              value: "/Users/test/.claude",
              canRemoveAutomatically: true,
            },
            {
              kind: "file",
              value: "/Users/test/.claude.json",
              canRemoveAutomatically: true,
            },
          ],
          cache: [
            {
              kind: "directory",
              value: "/Users/test/.claude/projects",
              canRemoveAutomatically: true,
            },
          ],
        });
      },
    ),
  );
  return render(<ToolsPage {...props} />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

async function openToolMore(name: string) {
  await userEvent.click(
    await screen.findByRole("button", { name: `More actions for ${name}` }),
  );
}

async function chooseRemove(name: string) {
  await openToolMore(name);
  await userEvent.click(screen.getByRole("menuitem", { name: "Remove" }));
}

describe("ToolsPage", () => {
  beforeEach(async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json([
          {
            id: "codex-app",
            name: "ChatGPT / Codex",
            status: "notInstalled",
            version: null,
            relatedTool: "codex",
            configurationRelationship: "sharedConfiguration",
            canLaunch: false,
            environment: "macos",
            installerHandoff: "directOfficialPackage",
            uninstallHandoff: "revealApplication",
            updatesManagedByVendor: true,
            canRollback: false,
            canManageMcp: false,
          },
          {
            id: "claude-desktop",
            name: "Claude Desktop",
            status: "notInstalled",
            version: null,
            relatedTool: "claude-code",
            configurationRelationship: "separateConfiguration",
            canLaunch: false,
            environment: "macos",
            installerHandoff: "directOfficialPackage",
            uninstallHandoff: "revealApplication",
            updatesManagedByVendor: true,
            canRollback: false,
            canManageMcp: false,
          },
        ]),
      ),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        operation: en.operation,
        taskCenter: en.taskCenter,
        tool: en.tool,
        tools: en.tools,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.error.mockClear();
    toastMocks.info.mockClear();
    toastMocks.success.mockClear();
  });

  it("shows a loading state before the list arrives", () => {
    mount([tool()]);
    expect(
      screen.getByRole("status", { name: en.tools.loading }),
    ).toBeInTheDocument();
  });

  it("renders one card per tool with the translated description", async () => {
    mount([
      tool(),
      tool({
        id: "codex",
        name: "Codex",
        descriptionKey: "tool.codex.description",
      }),
    ]);
    expect(await screen.findByText("Claude Code")).toBeInTheDocument();
    expect(screen.getByText("Codex")).toBeInTheDocument();
    expect(
      screen.getByText(en.tool["claude-code"].description),
    ).toBeInTheDocument();
  });

  it("no longer offers an inner tablist now that sessions live on their own page", async () => {
    mount([tool({ status: "installed", version: "1.0.0" })]);
    await screen.findByText("Claude Code");
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
  });

  it("keeps the software list primary without a recommendation or summary block", async () => {
    const { container } = mount([
      tool({ status: "installed", version: "2.3.1" }),
      tool({
        id: "codex",
        name: "Codex",
        descriptionKey: "tool.codex.description",
        status: "updateAvailable",
        version: "0.95.0",
        latestVersion: "0.96.0",
      }),
      tool({
        id: "opencode",
        name: "OpenCode",
        descriptionKey: "tool.opencode.description",
        status: "notInstalled",
      }),
      tool({
        id: "gemini-cli",
        name: "Gemini CLI",
        descriptionKey: "tool.gemini-cli.description",
        status: "unknown",
      }),
    ]);

    await screen.findByText("Gemini CLI");
    expect(container.querySelectorAll("[data-tool-card]")).toHaveLength(4);
    expect(
      screen.queryByRole("region", { name: en.tools.summary.title }),
    ).toBeNull();
    expect(screen.queryByText(en.tools.discovery.title)).toBeNull();
  });

  it("keeps CLI management before the secondary desktop-app inventory", async () => {
    mount([tool({ status: "installed", version: "2.3.1" })]);

    const cliCard = (await screen.findByText("Claude Code")).closest(
      "[data-tool-card]",
    );
    const desktopApps = await screen.findByRole("heading", {
      name: en.tools.desktopApps.title,
    });

    expect(
      (cliCard as HTMLElement).compareDocumentPosition(desktopApps) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("keeps desktop relationships in the desktop app inventory", async () => {
    const { container } = mount(
      [tool({ status: "installed", version: "2.3.1" })],
      [],
    );

    const cli = await waitFor(() => {
      const card = container.querySelector<HTMLElement>(
        '[data-tool-card="claude-code"]',
      );
      expect(card).not.toBeNull();
      return card as HTMLElement;
    });
    expect(cli).not.toHaveTextContent("Claude Desktop keeps separate settings");
    expect(
      within(cli).queryByRole("button", { name: "Manage CLI settings" }),
    ).toBeNull();

    const app = screen.getByRole("article", { name: "Claude Desktop" });
    await userEvent.click(
      within(app).getByRole("button", {
        name: "More actions for Claude Desktop",
      }),
    );
    await userEvent.click(
      screen.getByRole("menuitem", { name: "View Claude Code" }),
    );
    await waitFor(() =>
      expect(
        within(cli).getByRole("button", { name: "Open Claude Code" }),
      ).toHaveFocus(),
    );
  });

  it("routes a manageable desktop configuration to its exact MCP scope", async () => {
    const onOpenExtensions = vi.fn();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json([
          {
            id: "claude-desktop",
            name: "Claude Desktop",
            status: "installed",
            version: "1.0.0",
            relatedTool: "claude-code",
            configurationRelationship: "separateConfiguration",
            canLaunch: true,
            environment: "macos",
            installerHandoff: "directOfficialPackage",
            uninstallHandoff: "revealApplication",
            updatesManagedByVendor: true,
            canRollback: false,
            canManageMcp: true,
          },
        ]),
      ),
    );
    mount([tool({ status: "installed", version: "2.3.1" })], [], {
      onOpenExtensions,
    });

    const app = await screen.findByRole("article", { name: "Claude Desktop" });
    await userEvent.click(
      within(app).getByRole("button", {
        name: "More actions for Claude Desktop",
      }),
    );
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Manage Claude Desktop MCP" }),
    );
    expect(onOpenExtensions).toHaveBeenCalledWith({
      kind: "desktopApp",
      id: "claude-desktop",
    });
  });

  it("uses distinct local artwork for every supported tool", async () => {
    const { container } = mount([
      tool(),
      tool({
        id: "codex",
        name: "Codex",
        descriptionKey: "tool.codex.description",
      }),
      tool({
        id: "opencode",
        name: "OpenCode",
        descriptionKey: "tool.opencode.description",
      }),
      tool({
        id: "gemini-cli",
        name: "Gemini CLI",
        descriptionKey: "tool.gemini-cli.description",
      }),
    ]);

    await screen.findByText("Gemini CLI");
    const artwork = [
      ...container.querySelectorAll("[data-tool-card] [data-tool-artwork]"),
    ];
    expect(artwork).toHaveLength(4);
    expect(
      new Set(
        artwork.map((item) => item.querySelector("img")?.getAttribute("src")),
      ),
    ).toHaveProperty("size", 4);
    for (const item of artwork) {
      expect(item).toHaveAttribute("aria-hidden", "true");
      expect(item.querySelector("img")).toHaveAttribute("alt", "");
    }
  });

  it("loads update details before a single tool can update", async () => {
    const previews: unknown[] = [];
    const updates: unknown[] = [];
    mount([
      tool({
        status: "updateAvailable",
        version: "2.3.1",
        latestVersion: "2.4.0",
      }),
    ]);
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_tools_update_preview`,
        async ({ request }) => {
          previews.push(await request.json());
          return HttpResponse.json([readyUpdatePreview()]);
        },
      ),
      http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
        updates.push(await request.json());
        return HttpResponse.json("00000000-0000-4000-8000-000000000001");
      }),
    );

    await userEvent.click(
      await screen.findByRole("button", { name: "Update Claude Code" }),
    );
    await waitFor(() => expect(previews).toEqual([{ tools: ["claude-code"] }]));
    expect(
      await screen.findByText("Uses Claude Code's own updater"),
    ).toBeVisible();
    expect(updates).toEqual([]);

    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "Update",
      }),
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

  it("pauses an open update when the latest inventory no longer authorizes it", async () => {
    let toolReads = 0;
    const updates: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () => {
        toolReads += 1;
        return HttpResponse.json([
          tool({
            status: toolReads === 1 ? "updateAvailable" : "installed",
            version: toolReads === 1 ? "2.3.1" : "2.4.0",
            latestVersion: null,
          }),
        ]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_tools_update_preview`, () =>
        HttpResponse.json([readyUpdatePreview()]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
        updates.push(await request.json());
        return HttpResponse.json("00000000-0000-4000-8000-000000000001");
      }),
    );
    const client = createTestQueryClient();
    render(<ToolsPage />, { wrapper: withQueryClient(client) });

    await userEvent.click(
      await screen.findByRole("button", { name: "Update Claude Code" }),
    );
    const dialog = await screen.findByRole("dialog");
    const confirm = await within(dialog).findByRole("button", {
      name: "Update",
    });
    expect(confirm).toBeEnabled();

    await client.invalidateQueries({ queryKey: toolKeys.all });
    await waitFor(() => expect(toolReads).toBe(2));
    await waitFor(() => expect(confirm).toBeDisabled());
    await userEvent.click(confirm);
    expect(updates).toEqual([]);
    expect(
      within(dialog).getByRole("button", { name: "Cancel" }),
    ).toBeEnabled();
  });

  it("rechecks a stale update before it can submit a new fingerprint", async () => {
    let previewReads = 0;
    const updates: unknown[] = [];
    mount([
      tool({
        status: "updateAvailable",
        version: "2.3.1",
        latestVersion: "2.4.0",
      }),
    ]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_update_preview`, () => {
        previewReads += 1;
        return HttpResponse.json([
          readyUpdatePreview(
            "claude-code",
            (previewReads === 1 ? "a" : "b").repeat(64),
          ),
        ]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
        const body = await request.json();
        updates.push(body);
        if (updates.length === 1) {
          return HttpResponse.json(
            {
              code: "UPDATE_PREVIEW_STALE",
              messageKey: "error.tool.updatePreviewStale",
              technicalMessage: "private changed path",
              remediation: "error.remediation.recheckUpdate",
              contextId: null,
            },
            { status: 500 },
          );
        }
        return HttpResponse.json("00000000-0000-4000-8000-000000000001");
      }),
    );

    await userEvent.click(
      await screen.findByRole("button", { name: "Update Claude Code" }),
    );
    const updateButton = await screen.findByRole("button", { name: "Update" });
    await userEvent.click(updateButton);
    expect(
      await screen.findByRole("alert", {
        name: en.error.tool.updatePreviewStale,
      }),
    ).toBeVisible();
    expect(document.body).not.toHaveTextContent("private changed path");

    await userEvent.click(screen.getByRole("button", { name: "Check again" }));
    await waitFor(() => expect(previewReads).toBe(2));
    await waitFor(() => expect(updateButton).toBeEnabled());
    await userEvent.click(updateButton);

    await waitFor(() => expect(updates).toHaveLength(2));
    expect(updates).toEqual([
      { tool: "claude-code", previewFingerprint: "a".repeat(64) },
      { tool: "claude-code", previewFingerprint: "b".repeat(64) },
    ]);
  });

  it("lets the user leave a slow hand-off without undoing it", async () => {
    let release!: () => void;
    const scheduled = new Promise<void>((resolve) => {
      release = resolve;
    });
    const updates: unknown[] = [];
    mount([
      tool({
        status: "updateAvailable",
        version: "2.3.1",
        latestVersion: "2.4.0",
      }),
    ]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
        updates.push(await request.json());
        await scheduled;
        return HttpResponse.json("00000000-0000-4000-8000-000000000001");
      }),
    );

    await userEvent.click(
      await screen.findByRole("button", { name: "Update Claude Code" }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Update" }),
    );
    await waitFor(() => expect(updates).toHaveLength(1));

    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // The hand-off was never undone: it finishes on its own after the user
    // walked away, and the task is registered.
    release();
    await waitFor(() => expect(updates).toHaveLength(1));
    expect(toastMocks.error).not.toHaveBeenCalled();
  });

  it("notifies about a hand-off that fails after the dialog was dismissed", async () => {
    let release!: () => void;
    const scheduled = new Promise<void>((resolve) => {
      release = resolve;
    });
    mount([
      tool({
        status: "updateAvailable",
        version: "2.3.1",
        latestVersion: "2.4.0",
      }),
    ]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_update`, async () => {
        await scheduled;
        return HttpResponse.json(
          {
            code: "UPDATE_PREVIEW_STALE",
            messageKey: "error.tool.updatePreviewStale",
            technicalMessage: "private changed path",
            remediation: "error.remediation.recheckUpdate",
            contextId: null,
          },
          { status: 500 },
        );
      }),
    );

    await userEvent.click(
      await screen.findByRole("button", { name: "Update Claude Code" }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Update" }),
    );
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // The in-dialog error area is gone, so the outcome has to travel by
    // notification instead of vanishing.
    release();
    await waitFor(() =>
      expect(toastMocks.error).toHaveBeenCalledWith(
        en.error.tool.updatePreviewStale,
        expect.objectContaining({
          description: en.error.remediation.recheckUpdate,
        }),
      ),
    );
    expect(document.body).not.toHaveTextContent("private changed path");
  });

  it("asks for confirmation before installing (spec section 31)", async () => {
    const seen: unknown[] = [];
    mount([tool()]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_install`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-1");
      }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Install Claude Code" }),
    );
    expect(screen.getByText("Install Claude Code?")).toBeInTheDocument();
    expect(seen).toEqual([]);

    const dialog = screen.getByRole("dialog");
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Install" }),
    );
    await waitFor(() => expect(seen).toEqual([{ tool: "claude-code" }]));
  });

  it("returns keyboard focus to the action that opened a confirmation", async () => {
    mount([
      tool({
        status: "updateAvailable",
        version: "2.3.1",
        latestVersion: "2.4.0",
      }),
    ]);
    const updateButton = await screen.findByRole("button", {
      name: "Update Claude Code",
    });

    await userEvent.click(updateButton);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(updateButton).toHaveFocus();
  });

  it("shows the real task phase and progress inside the active tool card", async () => {
    mount(
      [
        tool({
          status: "updateAvailable",
          version: "1.0.0",
          latestVersion: "1.1.0",
        }),
      ],
      [
        {
          id: "op-1",
          kind: "update",
          tool: "claude-code",
          status: "running",
          progress: 40,
          messageKey: "operation.phase.installing",
          error: null,
          startedAt: 1,
          finishedAt: null,
          logs: [
            {
              timestamp: 2,
              kind: "command",
              messageKey: null,
              detail: "npm install -g @anthropic-ai/claude-code@latest",
            },
          ],
        },
      ],
    );
    expect(await screen.findByText("Update Claude Code")).toBeInTheDocument();
    expect(screen.getByText(en.operation.phase.installing)).toBeInTheDocument();
    expect(screen.getByText("40%")).toBeInTheDocument();
    expect(
      screen.getByRole("progressbar", { name: "Update Claude Code" }),
    ).toHaveAttribute("aria-valuenow", "40");
    expect(
      screen.queryByRole("button", { name: "Update Claude Code" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Remove Claude Code" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "More actions for Claude Code" }),
    ).not.toBeInTheDocument();
    const command = screen.getByText(
      "npm install -g @anthropic-ai/claude-code@latest",
    );
    expect(command).not.toBeVisible();
    await userEvent.click(
      screen.getByText(en.taskCenter.logs.title.replace("{{count}}", "1")),
    );
    expect(command).toBeVisible();
  });

  it("cancels a tool card only while native marks its phase safe", async () => {
    const operationId = "123e4567-e89b-42d3-a456-426614174000";
    let body: unknown;
    const operation = {
      id: operationId,
      kind: "update",
      tool: "claude-code",
      extension: null,
      status: "running",
      progress: 20,
      messageKey: "operation.phase.downloading",
      canCancel: true,
      logs: [],
      updateRecovery: null,
      error: null,
      startedAt: 1,
      finishedAt: null,
    };
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_operation_cancel`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({
            ...operation,
            canCancel: false,
            messageKey: "operation.phase.cancelling",
          });
        },
      ),
    );
    const { container } = mount(
      [
        tool({
          status: "updateAvailable",
          version: "1.0.0",
          latestVersion: "1.1.0",
        }),
      ],
      [operation],
    );

    await screen.findByText(en.operation.phase.downloading);
    const card = container.querySelector('[data-tool-card="claude-code"]');
    expect(card).not.toBeNull();
    await userEvent.click(
      within(card as HTMLElement).getByRole("button", {
        name: en.ds.action.cancel,
      }),
    );

    await waitFor(() => expect(body).toEqual({ operationId }));
    expect(
      within(card as HTMLElement).getByText(en.operation.phase.cancelling),
    ).toBeVisible();
    expect(
      within(card as HTMLElement).queryByRole("button", {
        name: en.ds.action.cancel,
      }),
    ).toBeNull();
  });

  it("keeps another tool actionable while one tool is updating", async () => {
    mount(
      [
        tool({
          status: "updateAvailable",
          version: "1.0.0",
          latestVersion: "1.1.0",
        }),
        tool({
          id: "codex",
          name: "Codex",
          descriptionKey: "tool.codex.description",
          status: "updateAvailable",
          version: "2.0.0",
          latestVersion: "2.1.0",
        }),
      ],
      [
        {
          id: "op-1",
          kind: "update",
          tool: "claude-code",
          status: "running",
          progress: 40,
          messageKey: "operation.phase.installing",
          error: null,
          startedAt: 1,
          finishedAt: null,
        },
      ],
    );

    expect(await screen.findByText("Update Claude Code")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update Codex" })).toBeEnabled();
  });

  it("retains failed operation logs until explicit dismissal without locking other tools", async () => {
    const { container } = mount(
      [
        tool({
          status: "updateAvailable",
          version: "1.0.0",
          latestVersion: "1.1.0",
        }),
        tool({
          id: "codex",
          name: "Codex",
          descriptionKey: "tool.codex.description",
          status: "updateAvailable",
          version: "2.0.0",
          latestVersion: "2.1.0",
        }),
      ],
      [
        {
          id: "op-failed",
          kind: "update",
          tool: "claude-code",
          extension: null,
          status: "failed",
          progress: 72,
          messageKey: "operation.phase.installing",
          logs: [
            {
              timestamp: 2,
              kind: "command",
              messageKey: null,
              detail: "npm install -g @anthropic-ai/claude-code@latest",
            },
            {
              timestamp: 3,
              kind: "stderr",
              messageKey: null,
              detail: "request failed behind the current network",
            },
          ],
          error: {
            code: "UPDATE_FAILED",
            messageKey: "error.tool.updateFailed",
            technicalMessage: "private technical detail",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          },
          startedAt: 1,
          finishedAt: 4,
        },
      ],
    );

    await screen.findByText(en.error.tool.updateFailed);
    const failedCard = container.querySelector(
      '[data-tool-card="claude-code"]',
    );
    expect(failedCard).not.toBeNull();
    expect(failedCard).not.toHaveAttribute("aria-busy");
    expect(screen.getByRole("button", { name: "Update Codex" })).toBeEnabled();

    const command = within(failedCard as HTMLElement).getByText(
      "npm install -g @anthropic-ai/claude-code@latest",
    );
    expect(command).not.toBeVisible();
    await userEvent.click(
      within(failedCard as HTMLElement).getByText(
        en.taskCenter.logs.title.replace("{{count}}", "2"),
      ),
    );
    expect(command).toBeVisible();
    expect(within(failedCard as HTMLElement).getByRole("list")).toHaveAttribute(
      "data-selectable-text",
    );

    await userEvent.click(
      within(failedCard as HTMLElement).getByRole("button", {
        name: en.ds.action.close,
      }),
    );
    expect(
      screen.getByRole("button", { name: "Update Claude Code" }),
    ).toBeEnabled();
  });

  it("restores a Broken failed update only after explicit confirmation", async () => {
    const installs: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_tool_install_version`,
        async ({ request }) => {
          installs.push(await request.json());
          return HttpResponse.json("op-update-recovery");
        },
      ),
    );
    mount(
      [tool({ status: "broken", version: "1.2.3" })],
      [
        {
          id: "op-broken-update",
          kind: "update",
          tool: "claude-code",
          extension: null,
          status: "failed",
          progress: 64,
          messageKey: "operation.phase.checking",
          logs: [],
          updateRecovery: { kind: "available", targetVersion: "1.2.3" },
          error: {
            code: "UPDATE_FAILED",
            messageKey: "error.tool.updateFailed",
            technicalMessage: null,
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          },
          startedAt: 1,
          finishedAt: 4,
        },
      ],
    );

    const restore = await screen.findByRole("button", {
      name: "Restore Claude Code to 1.2.3",
    });
    expect(restore).toBeEnabled();
    await userEvent.click(restore);
    const modal = await screen.findByRole("dialog", {
      name: "Restore Claude Code to 1.2.3?",
    });
    expect(installs).toEqual([]);
    await userEvent.click(
      within(modal).getByRole("button", { name: "Restore 1.2.3" }),
    );
    await waitFor(() =>
      expect(installs).toEqual([{ tool: "claude-code", version: "1.2.3" }]),
    );
  });

  it("keeps retained inventory visible but pauses actions during a background refresh", async () => {
    let toolReads = 0;
    let releaseRefresh!: () => void;
    const refreshGate = new Promise<void>((resolve) => {
      releaseRefresh = resolve;
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, async () => {
        toolReads += 1;
        if (toolReads > 1) await refreshGate;
        return HttpResponse.json([
          tool(
            toolReads > 1
              ? { status: "installed", version: "1.0.0" }
              : undefined,
          ),
        ]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([]),
      ),
    );
    render(<ToolsPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    const install = await screen.findByRole("button", {
      name: "Install Claude Code",
    });
    await waitFor(() => expect(install).toBeEnabled());

    await userEvent.click(
      screen.getByRole("button", { name: en.tools.refresh }),
    );
    await waitFor(() => expect(toolReads).toBe(2));
    expect(screen.getByText("Claude Code")).toBeVisible();
    expect(install).toBeDisabled();

    releaseRefresh();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Open Claude Code" }),
      ).toBeEnabled(),
    );
  });

  it("keeps an unavailable inventory retry busy and restores page focus", async () => {
    let toolReads = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, async () => {
        toolReads += 1;
        if (toolReads === 1) {
          return HttpResponse.text("private detector path", { status: 500 });
        }
        await new Promise<void>((resolve) => {
          releaseRetry = resolve;
        });
        return HttpResponse.json([tool()]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([]),
      ),
    );
    render(<ToolsPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await screen.findByText(en.tools.error.title);

    const retry = screen.getByRole("button", { name: en.tools.refresh });
    await userEvent.click(retry);
    await waitFor(() => expect(toolReads).toBe(2));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(screen.getByText(en.tools.error.title)).toBeInTheDocument();
    expect(screen.queryByRole("status")).toBeNull();
    expect(document.body).not.toHaveTextContent("private detector path");

    releaseRetry();
    expect(
      await screen.findByRole("button", { name: "Install Claude Code" }),
    ).toBeEnabled();
    await waitFor(() =>
      expect(
        screen.getByRole("region", { name: en.tools.title }),
      ).toHaveFocus(),
    );
  });

  it("retains the last inventory but pauses actions after refresh failure", async () => {
    let toolReads = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, async () => {
        toolReads += 1;
        if (toolReads === 2) {
          return HttpResponse.text("private refresh target", { status: 500 });
        }
        if (toolReads === 3) {
          await new Promise<void>((resolve) => {
            releaseRetry = resolve;
          });
        }
        return HttpResponse.json([
          tool(
            toolReads === 3
              ? { status: "installed", version: "1.0.0" }
              : undefined,
          ),
        ]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([]),
      ),
    );
    render(<ToolsPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    const install = await screen.findByRole("button", {
      name: "Install Claude Code",
    });
    expect(install).toBeEnabled();

    await userEvent.click(
      screen.getByRole("button", { name: en.tools.refresh }),
    );
    const alert = await screen.findByRole("alert", {
      name: en.tools.refreshError.title,
    });
    expect(toolReads).toBe(2);
    expect(alert).toHaveTextContent(en.tools.refreshError.description);
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    expect(screen.queryByText(en.tools.discovery.title)).toBeNull();
    expect(install).toBeDisabled();
    expect(document.body).not.toHaveTextContent("private refresh target");

    const retry = screen.getByRole("button", { name: en.tools.refresh });
    await userEvent.click(retry);
    await waitFor(() => expect(toolReads).toBe(3));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(install).toBeDisabled();
    expect(alert).toBeInTheDocument();
    expect(screen.queryByRole("status")).toBeNull();

    releaseRetry();
    expect(
      await screen.findByRole("button", { name: "Open Claude Code" }),
    ).toBeEnabled();
    await waitFor(() => expect(alert).not.toBeInTheDocument());
    await waitFor(() =>
      expect(
        screen.getByRole("region", { name: en.tools.title }),
      ).toHaveFocus(),
    );
  });

  it("pauses lifecycle actions until the task baseline is known", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([tool()]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, async () => {
        await delay("infinite");
        return HttpResponse.json([]);
      }),
    );
    render(<ToolsPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    const loading = (
      await screen.findByText(en.taskCenter.loading.summary)
    ).closest('[role="status"]');
    const indicator = loading?.querySelector("svg");
    expect(indicator).toHaveClass("motion-safe:animate-spin");
    expect(indicator?.classList.contains("animate-spin")).toBe(false);
    expect(
      screen.getByRole("button", { name: "Install Claude Code" }),
    ).toBeDisabled();
  });

  it("explains unavailable task state and unlocks actions after retry", async () => {
    let operationReads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([tool()]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () => {
        operationReads += 1;
        return operationReads === 1
          ? HttpResponse.text("not available", { status: 500 })
          : HttpResponse.json([]);
      }),
    );
    render(<ToolsPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    expect(
      await screen.findByText(en.taskCenter.unavailable.title),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Install Claude Code" }),
    ).toBeDisabled();

    await userEvent.click(
      screen.getByRole("button", { name: en.taskCenter.unavailable.retry }),
    );

    await waitFor(() => expect(operationReads).toBe(2));
    await waitFor(() =>
      expect(
        screen.queryByText(en.taskCenter.unavailable.title),
      ).not.toBeInTheDocument(),
    );
    expect(
      screen.getByRole("button", { name: "Install Claude Code" }),
    ).toBeEnabled();
  });

  it("keeps confirmation busy until card progress owns the handoff", async () => {
    let operationReads = 0;
    let releaseRefresh!: () => void;
    const refreshReady = new Promise<void>((resolve) => {
      releaseRefresh = resolve;
    });
    const runningInstall = {
      id: "op-install",
      kind: "install",
      tool: "claude-code",
      status: "running",
      progress: 12,
      messageKey: "operation.phase.installing",
      error: null,
      startedAt: 2,
      finishedAt: null,
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([tool()]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, async () => {
        operationReads += 1;
        if (operationReads > 1) await refreshReady;
        return HttpResponse.json(operationReads > 1 ? [runningInstall] : []);
      }),
      http.post(`${TAURI_ENDPOINT}/app_tool_install`, () =>
        HttpResponse.json("op-install"),
      ),
    );
    render(<ToolsPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    const user = userEvent.setup();
    const install = await screen.findByRole("button", {
      name: "Install Claude Code",
    });
    await waitFor(() => expect(install).toBeEnabled());
    await user.click(install);
    const dialog = screen.getByRole("dialog");
    const confirm = within(dialog).getByRole("button", { name: "Install" });
    await user.click(confirm);

    await waitFor(() => expect(operationReads).toBe(2));
    expect(confirm).toHaveAttribute("aria-busy", "true");
    expect(dialog).toBeInTheDocument();

    releaseRefresh();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(await screen.findByText("Install Claude Code")).toBeInTheDocument();
    expect(
      screen.getByRole("progressbar", { name: "Install Claude Code" }),
    ).toHaveAttribute("aria-valuenow", "12");
  });

  it("keeps a failed install confirmation open for an inline retry", async () => {
    let attempts = 0;
    mount([tool()]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_install`, () => {
        attempts += 1;
        return attempts === 1
          ? HttpResponse.json(
              {
                code: "INSTALL_FAILED",
                messageKey: "error.tool.installFailed",
                technicalMessage: "private installer path and exit code 127",
                remediation: "error.remediation.checkInternetConnection",
                contextId: null,
              },
              { status: 500 },
            )
          : HttpResponse.json("op-install");
      }),
    );

    await userEvent.click(
      await screen.findByRole("button", { name: "Install Claude Code" }),
    );
    let dialog = screen.getByRole("dialog");
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Install" }),
    );

    const alert = await screen.findByRole("alert", {
      name: en.error.tool.installFailed,
    });
    expect(dialog).toBeInTheDocument();
    expect(alert).toHaveTextContent(
      en.error.remediation.checkInternetConnection,
    );
    expect(alert).not.toHaveTextContent("private installer path");
    expect(toastMocks.error).not.toHaveBeenCalled();

    dialog = screen.getByRole("dialog");
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Install" }),
    );
    await waitFor(() => expect(attempts).toBe(2));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("re-checks the tool instead of guessing when the status is unknown", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () => {
        calls += 1;
        return HttpResponse.json([tool({ status: "unknown" })]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([]),
      ),
    );
    render(<ToolsPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await userEvent.click(
      await screen.findByRole("button", { name: "Check Claude Code" }),
    );
    await waitFor(() => expect(calls).toBeGreaterThan(1));
  });

  it("explains when a broken tool has no safe automatic repair", async () => {
    mount([tool({ status: "broken", version: "2.3.1" })]);

    expect(
      await screen.findByText(en.ds.tool.repairUnavailable),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Fix Claude Code" }),
    ).toBeNull();
    await openToolMore("Claude Code");
    expect(screen.getByRole("menuitem", { name: "Remove" })).toBeVisible();
    expect(toastMocks.info).not.toHaveBeenCalled();
  });

  it("confirms and starts a safe repair for a broken npm-owned Codex", async () => {
    const seen: unknown[] = [];
    mount([
      tool({
        id: "codex",
        name: "Codex CLI",
        descriptionKey: "tool.codex.description",
        status: "broken",
        version: "0.149.0",
        capabilities: { ...CAPABILITIES, canRepair: true },
      }),
    ]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_repair`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-repair");
      }),
    );

    await userEvent.click(
      await screen.findByRole("button", { name: "Fix Codex CLI" }),
    );
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("Repair Codex CLI?")).toBeInTheDocument();
    expect(seen).toEqual([]);

    await userEvent.click(
      within(dialog).getByRole("button", { name: "Repair" }),
    );
    await waitFor(() => expect(seen).toEqual([{ tool: "codex" }]));
  });

  it("starts in the native-owned default directory without sending a path", async () => {
    const seen: unknown[] = [];
    mount([tool({ status: "installed", version: "1.0.0" })]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("launched");
      }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Open Claude Code" }),
    );
    const dialog = screen.getByRole("dialog");
    expect(
      within(dialog).getByText(en.tools.open.localTitle),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(en.tools.open.localHint),
    ).toBeInTheDocument();
    expect(seen).toEqual([]);

    const confirm = within(dialog).getByRole("button", {
      name: en.tools.open.confirm,
    });
    expect(confirm).toHaveFocus();
    await userEvent.click(confirm);
    await waitFor(() =>
      expect(seen).toEqual([{ tool: "claude-code", directoryMode: "default" }]),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(toastMocks.success).toHaveBeenCalledWith(
      en.tools.open.launched.replace("{{name}}", "Claude Code"),
    );
    expect(toastMocks.info).not.toHaveBeenCalled();
  });

  it("can choose a directory and closes quietly when the picker is cancelled", async () => {
    const seen: unknown[] = [];
    mount([tool({ status: "installed", version: "1.0.0" })]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("cancelled");
      }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Open Claude Code" }),
    );
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: en.tools.open.chooseFolder,
      }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(seen).toEqual([{ tool: "claude-code", directoryMode: "choose" }]);
    expect(toastMocks.success).not.toHaveBeenCalled();
    expect(toastMocks.error).not.toHaveBeenCalled();
  });

  it("locks dismissal and shows progress while the folder picker is open", async () => {
    let release!: () => void;
    const pickerClosed = new Promise<void>((resolve) => {
      release = resolve;
    });
    mount([tool({ status: "installed", version: "1.0.0" })]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async () => {
        await pickerClosed;
        return HttpResponse.json("cancelled");
      }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Open Claude Code" }),
    );
    const confirm = within(screen.getByRole("dialog")).getByRole("button", {
      name: en.tools.open.confirm,
    });
    await userEvent.click(confirm);

    await waitFor(() => expect(confirm).toHaveAttribute("aria-busy", "true"));
    expect(confirm).toBeDisabled();
    expect(
      within(screen.getByRole("dialog")).queryByRole("button", {
        name: en.ds.action.close,
      }),
    ).toBeNull();
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    release();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("keeps a terminal handoff failure inline and retries the exact tool", async () => {
    let attempts = 0;
    const seen: unknown[] = [];
    mount([tool({ status: "installed", version: "1.0.0" })]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
        attempts += 1;
        seen.push(await request.json());
        if (attempts === 1) {
          return HttpResponse.json(
            {
              code: "LAUNCH_FAILED",
              messageKey: "error.tool.launchFailed",
              technicalMessage: "private path must not be rendered",
              remediation: "error.remediation.openToolManually",
              contextId: null,
            },
            { status: 500 },
          );
        }
        return HttpResponse.json("launched");
      }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Open Claude Code" }),
    );
    const dialog = screen.getByRole("dialog");
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.tools.open.confirm }),
    );

    const alert = await within(dialog).findByRole("alert", {
      name: en.error.tool.launchFailed,
    });
    expect(alert).toHaveTextContent(en.error.remediation.openToolManually);
    expect(alert).not.toHaveTextContent("private path");
    expect(toastMocks.error).not.toHaveBeenCalled();
    expect(screen.getByText("Open Claude Code")).toBeInTheDocument();

    const retry = within(dialog).getByRole("button", {
      name: "Try opening Claude Code again",
    });
    expect(retry).toHaveTextContent("Try again");
    expect(retry).toHaveFocus();
    await userEvent.click(retry);

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(seen).toEqual([
      { tool: "claude-code", directoryMode: "default" },
      { tool: "claude-code", directoryMode: "default" },
    ]);
    expect(toastMocks.success).toHaveBeenCalledWith(
      en.tools.open.launched.replace("{{name}}", "Claude Code"),
    );
  });

  it("offers a retry when the list cannot be read", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.text("boom", { status: 500 }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([]),
      ),
    );
    render(<ToolsPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    expect(await screen.findByText(en.tools.error.title)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.tools.refresh }),
    ).toBeInTheDocument();
  });

  it("shows an empty state instead of a blank page", async () => {
    mount([]);
    expect(await screen.findByText(en.tools.empty.title)).toBeInTheDocument();
  });

  it("opens the three-option removal dialog and defaults to the app only", async () => {
    const seen: unknown[] = [];
    mount([tool({ status: "installed", version: "1.0.0" })]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_uninstall`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-9");
      }),
    );
    await chooseRemove("Claude Code");
    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText("/Users/test/.claude"),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "/Users/test/.nvm/versions/node/v22/bin/npm uninstall -g @anthropic-ai/claude-code",
      ),
    ).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.tools.uninstall.submit }),
    );
    await waitFor(() =>
      expect(seen).toEqual([
        {
          tool: "claude-code",
          options: { removeSettings: false, removeCache: false },
        },
      ]),
    );
  });

  it("shows removal for a package-manager-owned Grok installation", async () => {
    mount([
      tool({
        id: "grok-build",
        name: "Grok Build",
        descriptionKey: "tool.grok-build.description",
        status: "installed",
        version: "0.1.4",
        capabilities: { ...CAPABILITIES, canUninstall: true },
      }),
    ]);

    await openToolMore("Grok Build");
    expect(screen.getByRole("menuitem", { name: "Remove" })).toBeVisible();
  });

  it("keeps removal choices after a start failure and retries in place", async () => {
    let attempts = 0;
    const seen: unknown[] = [];
    mount([
      tool({
        status: "installed",
        version: "1.0.0",
        sessionsInsideSettings: true,
      }),
    ]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_uninstall`, async ({ request }) => {
        attempts += 1;
        seen.push(await request.json());
        return attempts === 1
          ? HttpResponse.json(
              {
                code: "UNINSTALL_FAILED",
                messageKey: "error.tool.uninstallFailed",
                technicalMessage: "permission denied at /private/tool",
                remediation: "error.remediation.checkPermissions",
                contextId: null,
              },
              { status: 500 },
            )
          : HttpResponse.json("op-uninstall");
      }),
    );

    await chooseRemove("Claude Code");
    let dialog = screen.getByRole("dialog");
    await waitFor(() =>
      expect(
        within(dialog).getByRole("checkbox", {
          name: en.tools.uninstall.option.settings,
        }),
      ).toBeEnabled(),
    );
    await userEvent.click(
      within(dialog).getByRole("checkbox", {
        name: en.tools.uninstall.option.settings,
      }),
    );
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.tools.uninstall.submit,
      }),
    );
    expect(within(dialog).getByText("/Users/test/.claude")).toBeVisible();
    expect(
      within(dialog).getByText(
        "/Users/test/.nvm/versions/node/v22/bin/npm uninstall -g @anthropic-ai/claude-code",
      ),
    ).toBeVisible();
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.tools.uninstall.confirm,
      }),
    );

    const alert = await screen.findByRole("alert", {
      name: en.error.tool.uninstallFailed,
    });
    expect(alert).toHaveTextContent(en.error.remediation.checkPermissions);
    expect(alert).not.toHaveTextContent("/private/tool");
    expect(toastMocks.error).not.toHaveBeenCalled();
    expect(
      screen.getByText(en.tools.uninstall.warning.sessionsInsideSettings),
    ).toBeInTheDocument();

    dialog = screen.getByRole("dialog");
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.tools.uninstall.back }),
    );
    expect(
      within(dialog).getByRole("checkbox", {
        name: en.tools.uninstall.option.settings,
      }),
    ).toBeChecked();
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.tools.uninstall.submit,
      }),
    );
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.tools.uninstall.confirm,
      }),
    );

    await waitFor(() => expect(attempts).toBe(2));
    expect(seen).toEqual([
      {
        tool: "claude-code",
        options: { removeSettings: true, removeCache: false },
      },
      {
        tool: "claude-code",
        options: { removeSettings: true, removeCache: false },
      },
    ]);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("shows an empty cache target explicitly and does not offer a no-op deletion", async () => {
    mount([
      tool({
        id: "gemini-cli",
        name: "Gemini CLI",
        descriptionKey: "tool.gemini-cli.description",
        status: "installed",
        version: "1.0.0",
      }),
    ]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_uninstall_preview`, () =>
        HttpResponse.json({
          tool: "gemini-cli",
          app: [
            {
              kind: "command",
              value: "npm uninstall -g @google/gemini-cli",
              canRemoveAutomatically: true,
            },
          ],
          settings: [
            {
              kind: "directory",
              value: "/Users/test/.gemini",
              canRemoveAutomatically: true,
            },
          ],
          cache: [],
        }),
      ),
    );

    await chooseRemove("Gemini CLI");
    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText("/Users/test/.gemini"),
    ).toBeVisible();
    expect(
      within(dialog).getByText(en.tools.uninstall.reason.nothingToRemove),
    ).toBeVisible();
    expect(
      within(dialog).getByRole("checkbox", {
        name: en.tools.uninstall.option.cache,
      }),
    ).toBeDisabled();
  });

  it("offers to open the location of every path it is about to delete", async () => {
    mount([
      tool({
        id: "gemini-cli",
        name: "Gemini CLI",
        descriptionKey: "tool.gemini-cli.description",
        status: "installed",
        version: "1.0.0",
      }),
    ]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_uninstall_preview`, () =>
        HttpResponse.json({
          tool: "gemini-cli",
          app: [
            {
              kind: "command",
              value: "npm uninstall -g @google/gemini-cli",
              canRemoveAutomatically: true,
            },
          ],
          settings: [
            {
              kind: "directory",
              value: "/Users/test/.gemini",
              canRemoveAutomatically: true,
            },
          ],
          cache: [],
        }),
      ),
    );
    const revealed: string[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_reveal_path`, async ({ request }) => {
        const body = (await request.json()) as { path: string };
        revealed.push(body.path);
        return HttpResponse.json(null);
      }),
    );

    await chooseRemove("Gemini CLI");
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("/Users/test/.gemini");

    // A command has no location to open, so only the directory offers one.
    const reveals = within(dialog).getAllByRole("button", {
      name: /Show .* in the file manager/u,
    });
    expect(reveals).toHaveLength(1);

    await userEvent.click(reveals[0]);
    await waitFor(() => expect(revealed).toEqual(["/Users/test/.gemini"]));
  });

  it("blocks automatic removal when the native preview rejects the app path", async () => {
    mount([tool({ status: "installed", version: "1.0.0" })]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_uninstall_preview`, () =>
        HttpResponse.json({
          tool: "claude-code",
          app: [
            {
              kind: "directory",
              value: "/Applications/Claude Code.app",
              canRemoveAutomatically: false,
            },
          ],
          settings: [],
          cache: [],
        }),
      ),
    );

    await chooseRemove("Claude Code");
    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText(en.tools.uninstall.preview.protected),
    ).toBeVisible();
    expect(
      within(dialog).getByRole("button", {
        name: en.tools.uninstall.submit,
      }),
    ).toBeDisabled();
  });
});
