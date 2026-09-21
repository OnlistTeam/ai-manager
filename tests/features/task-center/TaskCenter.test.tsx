import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { delay, http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { operationKeys } from "@/entities/operation";
import { toolKeys } from "@/entities/tool";
import en from "@/i18n/locales/en.json";
import { TaskCenter } from "@/features/task-center";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  success: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

const FAILED = {
  id: "op-2",
  kind: "uninstall",
  tool: "codex",
  status: "failed",
  progress: 60,
  messageKey: "operation.phase.removing",
  error: {
    code: "UNINSTALL_FAILED",
    messageKey: "error.tool.uninstallIncomplete",
    technicalMessage: "/Users/x/.codex: Permission denied",
    remediation: "error.remediation.uninstallManually",
    contextId: "ctx-9",
  },
  startedAt: 2,
  finishedAt: 9,
};

const RUNNING = {
  id: "op-1",
  kind: "update",
  tool: "opencode",
  status: "running",
  progress: 72,
  messageKey: "operation.phase.downloading",
  error: null,
  startedAt: 5,
  finishedAt: null,
};

const CANCELLABLE_RUNNING = {
  ...RUNNING,
  id: "123e4567-e89b-42d3-a456-426614174000",
  progress: 20,
  canCancel: true,
};

const LOGGED_RUNNING = {
  ...RUNNING,
  logs: [
    {
      timestamp: 5,
      kind: "phase",
      messageKey: "operation.phase.downloading",
      detail: null,
    },
    {
      timestamp: 6,
      kind: "command",
      messageKey: null,
      detail: "npm install -g @openai/codex@latest",
    },
    {
      timestamp: 7,
      kind: "stderr",
      messageKey: null,
      detail: "registry token=*** timed out",
    },
  ],
};

const SUCCEEDED_INSTALL = {
  id: "op-3",
  kind: "install",
  tool: "codex",
  status: "success",
  progress: 100,
  messageKey: "operation.phase.ready",
  error: null,
  startedAt: 5,
  finishedAt: 10,
};

const FAILED_UPDATE_RECOVERY = {
  id: "op-update-failed",
  kind: "update",
  tool: "codex",
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
  startedAt: 5,
  finishedAt: 10,
};

const SKILL_INSTALL = {
  id: "op-skill",
  kind: "install",
  tool: "claude-code",
  extension: {
    kind: "skill",
    id: "anthropics/skills:code-review",
    name: "Code review",
  },
  status: "running",
  progress: 20,
  messageKey: "operation.phase.downloading",
  error: null,
  startedAt: 6,
  finishedAt: null,
};

const SKILL_REMOVE = {
  ...SKILL_INSTALL,
  id: "op-skill-remove",
  kind: "uninstall",
  progress: 25,
  messageKey: "operation.phase.removing",
};

const MCP_INSTALL = {
  ...SKILL_INSTALL,
  id: "op-mcp-install",
  extension: {
    kind: "mcp",
    id: "filesystem-a1b2c3d4",
    name: "Filesystem",
  },
  progress: 45,
  messageKey: "operation.phase.configuring",
};

const TOOLS = [
  {
    id: "opencode",
    name: "OpenCode",
    descriptionKey: "tool.opencode.description",
    status: "installed",
    version: "1.0.0",
    latestVersion: null,
    capabilities: {
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
    },
    sessionsInsideSettings: false,
    environment: null,
  },
  {
    id: "codex",
    name: "Codex",
    descriptionKey: "tool.codex.description",
    status: "installed",
    version: "1.0.0",
    latestVersion: null,
    capabilities: {
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
    },
    sessionsInsideSettings: false,
    environment: null,
  },
];

const BROKEN_TOOLS = TOOLS.map((tool) =>
  tool.id === "codex"
    ? {
        ...tool,
        status: "broken",
        version: "1.2.3",
        capabilities: { ...tool.capabilities, canLaunch: false },
      }
    : tool,
);

function renderTaskCenter(tools: unknown[] = TOOLS) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
      HttpResponse.json(tools),
    ),
  );
  const client = createTestQueryClient();
  render(<TaskCenter />, { wrapper: withQueryClient(client) });
  return client;
}

function mount(operations: unknown[], tools: unknown[] = TOOLS) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
      HttpResponse.json(operations),
    ),
  );
  return renderTaskCenter(tools);
}

describe("TaskCenter", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        common: en.common,
        ds: en.ds,
        taskCenter: en.taskCenter,
        error: en.error,
        operation: en.operation,
        tools: en.tools,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.error.mockClear();
    toastMocks.success.mockClear();
  });

  it("counts the running tasks on the trigger (spec section 40)", async () => {
    mount([RUNNING, FAILED]);
    const trigger = screen.getByRole("button", { name: /Activity/ });
    await waitFor(() => expect(trigger).toHaveTextContent("1"));
  });

  it("lists each task with its progress and its phase", async () => {
    mount([RUNNING, FAILED]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    expect(panel.querySelector(".overflow-y-auto")).toHaveClass(
      "scrollbar-subtle",
    );
    expect(panel).toHaveAccessibleDescription("1 activity running");
    expect(
      within(panel).getByRole("heading", {
        name: en.taskCenter.sections.active,
      }),
    ).toBeInTheDocument();
    expect(
      within(panel).getByRole("heading", {
        name: en.taskCenter.sections.recent,
      }),
    ).toBeInTheDocument();
    expect(within(panel).getByText("Update OpenCode")).toBeInTheDocument();
    expect(within(panel).getByText("72%")).toBeInTheDocument();
    expect(
      within(panel).getByText(en.operation.phase.downloading),
    ).toBeInTheDocument();
  });

  it("requests cancellation only when native marks the running phase safe", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_operation_cancel`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({
            ...CANCELLABLE_RUNNING,
            canCancel: false,
            messageKey: "operation.phase.cancelling",
          });
        },
      ),
    );
    mount([CANCELLABLE_RUNNING]);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });

    await user.click(
      within(panel).getByRole("button", { name: en.ds.action.cancel }),
    );

    await waitFor(() =>
      expect(body).toEqual({ operationId: CANCELLABLE_RUNNING.id }),
    );
    expect(
      within(panel).getByText(en.operation.phase.cancelling),
    ).toBeVisible();
    expect(
      within(panel).queryByRole("button", { name: en.ds.action.cancel }),
    ).toBeNull();
  });

  it("does not offer cancel after native closes the safe phase", async () => {
    mount([{ ...RUNNING, canCancel: false }]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    expect(
      within(panel).queryByRole("button", { name: en.ds.action.cancel }),
    ).toBeNull();
  });

  it("keeps bounded process logs collapsed until the user asks for them", async () => {
    mount([LOGGED_RUNNING]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    const command = within(panel).getByText(
      "npm install -g @openai/codex@latest",
    );
    expect(command).not.toBeVisible();
    expect(within(panel).queryByText(/do-not-show/)).not.toBeInTheDocument();

    await userEvent.click(
      within(panel).getByText(
        en.taskCenter.logs.title.replace("{{count}}", "3"),
      ),
    );
    expect(command).toBeVisible();
    expect(
      within(panel).getByText("registry token=*** timed out"),
    ).toBeVisible();
    expect(within(panel).getByText(en.taskCenter.logs.privacy)).toBeVisible();
  });

  it("names concurrent progress bars after their exact tasks", async () => {
    mount([
      RUNNING,
      {
        ...RUNNING,
        id: "op-4",
        kind: "install",
        tool: "codex",
        progress: 31,
      },
    ]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });

    expect(
      within(panel).getByRole("progressbar", { name: "Update OpenCode" }),
    ).toHaveAttribute("aria-valuenow", "72");
    expect(
      within(panel).getByRole("progressbar", { name: "Install Codex" }),
    ).toHaveAttribute("aria-valuenow", "31");
  });

  it("names Skill work after the Skill rather than the locking tool", async () => {
    mount([SKILL_INSTALL]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    expect(
      within(panel).getByText("Install Code review Skill"),
    ).toBeInTheDocument();
    expect(within(panel).queryByText("Install Claude Code")).toBeNull();
    expect(
      within(panel)
        .getByText("Install Code review Skill")
        .closest("li")
        ?.querySelector('[data-extension-artwork="skill"]'),
    ).not.toBeNull();
  });

  it("names Skill removal and keeps the Skill artwork in Tasks", async () => {
    mount([SKILL_REMOVE]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    const title = within(panel).getByText("Remove Code review Skill");
    expect(title).toBeInTheDocument();
    expect(
      title.closest("li")?.querySelector('[data-extension-artwork="skill"]'),
    ).not.toBeNull();
  });

  it("shows guided MCP installation with its connection artwork and phase", async () => {
    mount([MCP_INSTALL]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    const title = within(panel).getByText("Add Filesystem MCP connection");
    expect(title).toBeInTheDocument();
    expect(
      within(panel).getByText(en.operation.phase.configuring),
    ).toBeVisible();
    expect(
      title.closest("li")?.querySelector('[data-extension-artwork="mcp"]'),
    ).not.toBeNull();
  });

  it("keeps the complete name of a long MCP task in the compact panel", async () => {
    const name =
      "SharedTeamProductionFilesystemConnectionUsedByEveryWorkstationAnywhere";
    const taskName = `Add ${name} MCP connection`;
    mount([
      {
        ...MCP_INSTALL,
        extension: { ...MCP_INSTALL.extension, name },
      },
    ]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });

    const title = within(panel).getByText(taskName);
    expect(title).toHaveClass("break-words");
    expect(title).not.toHaveClass("truncate");
    expect(
      within(panel).getByRole("progressbar", { name: taskName }),
    ).toHaveAttribute("aria-valuenow", "45");
  });

  it("shows the error, not the phase the task died in (spec section 42)", async () => {
    mount([FAILED]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    expect(
      within(panel).getByText(en.error.tool.uninstallIncomplete),
    ).toBeInTheDocument();
    expect(
      within(panel).queryByText(en.operation.phase.removing),
    ).not.toBeInTheDocument();
  });

  it("keeps the technical detail behind View Details", async () => {
    mount([FAILED]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    expect(
      within(panel).queryByText(/Permission denied/),
    ).not.toBeInTheDocument();

    await userEvent.click(
      within(panel).getByRole("button", {
        name: "View details: Remove Codex",
      }),
    );
    expect(
      await screen.findByRole("dialog", {
        name: en.taskCenter.details.title,
      }),
    ).toHaveAccessibleDescription("Remove Codex");
    expect(await screen.findByText(/Permission denied/)).toBeInTheDocument();
    expect(screen.getByText("UNINSTALL_FAILED")).toBeInTheDocument();
    expect(screen.getByText("ctx-9")).toBeInTheDocument();
    expect(
      screen.getByText(en.error.remediation.uninstallManually),
    ).toBeInTheDocument();
  });

  it("confirms an owned verified restore after a Broken update", async () => {
    const writes: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_tool_install_version`,
        async ({ request }) => {
          writes.push(await request.json());
          return HttpResponse.json("op-recovery");
        },
      ),
    );
    mount([FAILED_UPDATE_RECOVERY], BROKEN_TOOLS);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    const restore = within(panel).getByRole("button", {
      name: "Restore Codex to 1.2.3",
    });
    await user.click(restore);

    const modal = await screen.findByRole("dialog", {
      name: "Restore Codex to 1.2.3?",
    });
    expect(writes).toEqual([]);
    await user.click(
      within(modal).getByRole("button", { name: "Restore 1.2.3" }),
    );
    await waitFor(() =>
      expect(writes).toEqual([{ tool: "codex", version: "1.2.3" }]),
    );
    expect(panel).toBeInTheDocument();
  });

  it("explains an ownership mismatch without offering a restore command", async () => {
    mount(
      [
        {
          ...FAILED_UPDATE_RECOVERY,
          updateRecovery: { kind: "ownershipChanged" },
        },
      ],
      BROKEN_TOOLS,
    );
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    expect(
      within(panel).getByText(
        en.tools.updateRecovery.advisory.ownershipChanged,
      ),
    ).toBeVisible();
    expect(
      within(panel).queryByRole("button", { name: /Restore Codex/ }),
    ).toBeNull();
  });

  it("stays out of the global chrome when nothing has run yet", async () => {
    mount([]);
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: en.taskCenter.open }),
      ).toBeNull(),
    );
  });

  it("shows a truthful loading state instead of an empty history", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, async () => {
        await delay("infinite");
        return HttpResponse.json([]);
      }),
    );
    renderTaskCenter();

    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    expect(
      await screen.findByRole("status", {
        name: en.taskCenter.loading.label,
      }),
    ).toBeInTheDocument();
    expect(screen.getByText(en.common.detecting)).toBeInTheDocument();
    expect(document.querySelector(".animate-pulse")).toBeNull();
    expect(
      screen.queryByText(en.taskCenter.empty.title),
    ).not.toBeInTheDocument();
  });

  it("explains a load failure and retries without pretending the list is empty", async () => {
    let requests = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () => {
        requests += 1;
        return requests === 1
          ? HttpResponse.text("not available", { status: 500 })
          : HttpResponse.json([]);
      }),
    );
    renderTaskCenter();

    const user = userEvent.setup();
    const trigger = await screen.findByRole("button", {
      name: en.taskCenter.openUnavailable,
    });
    expect(trigger.querySelector("[data-task-warning]")).not.toBeNull();
    await user.click(trigger);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.taskCenter.unavailable.title,
    );
    expect(
      screen.queryByText(en.taskCenter.empty.title),
    ).not.toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: en.taskCenter.unavailable.retry }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: en.taskCenter.open }),
      ).toBeNull(),
    );
    expect(requests).toBe(2);
  });

  it("labels retained task history stale and restores focus through retry recovery", async () => {
    let requests = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () => {
        requests += 1;
        if (requests === 1 || requests === 4) {
          return HttpResponse.json([RUNNING]);
        }
        return HttpResponse.text("not available", { status: 500 });
      }),
    );
    const client = renderTaskCenter();
    const user = userEvent.setup();

    expect(
      await screen.findByRole("button", {
        name: "Activity, 1 operation running",
      }),
    ).toBeInTheDocument();
    await act(async () => {
      await client.invalidateQueries({ queryKey: operationKeys.all });
    });
    expect(requests).toBe(2);
    expect(client.getQueryState(operationKeys.list())).toMatchObject({
      status: "error",
      fetchStatus: "idle",
    });

    const trigger = await screen.findByRole("button", {
      name: "Activity, showing last known status",
    });
    expect(trigger.querySelector("[data-task-warning]")).not.toBeNull();
    await user.click(trigger);

    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    expect(panel).toHaveAccessibleDescription(
      "Showing the last known activity status",
    );
    const warning = within(panel).getByRole("alert", {
      name: "Activity couldn't refresh",
    });
    expect(warning).toHaveTextContent(
      "The last known activity is still shown. New progress may be missing.",
    );
    expect(within(panel).getByText("Update OpenCode")).toBeInTheDocument();

    const retry = within(warning).getByRole("button", {
      name: "Refresh activity",
    });
    await user.click(retry);
    await waitFor(() => expect(retry).toHaveFocus());
    expect(requests).toBe(3);

    await user.click(retry);
    await waitFor(() =>
      expect(
        within(panel).getByRole("heading", {
          level: 2,
          name: en.taskCenter.title,
        }),
      ).toHaveFocus(),
    );
    expect(within(panel).queryByRole("alert")).not.toBeInTheDocument();
    expect(requests).toBe(4);
  });

  it("focuses the named panel first and restores the trigger on Escape", async () => {
    mount([RUNNING]);
    const user = userEvent.setup();
    const trigger = screen.getByRole("button", { name: /Activity/ });
    await user.click(trigger);

    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    expect(
      within(panel).getByRole("heading", {
        level: 2,
        name: en.taskCenter.title,
      }),
    ).toHaveFocus();

    await user.keyboard("{Escape}");
    await waitFor(() => expect(trigger).toHaveFocus());
    expect(panel).not.toBeInTheDocument();
  });

  it("keeps the panel mounted and returns focus to the exact details button", async () => {
    mount([FAILED]);
    const user = userEvent.setup();
    const trigger = screen.getByRole("button", { name: /Activity/ });
    await user.click(trigger);
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    const detailsButton = within(panel).getByRole("button", {
      name: "View details: Remove Codex",
    });

    await user.click(detailsButton);
    expect(
      await screen.findByRole("dialog", {
        name: en.taskCenter.details.title,
      }),
    ).toBeInTheDocument();
    await user.keyboard("{Escape}");

    await waitFor(() => expect(detailsButton).toHaveFocus());
    expect(panel).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it("gives repeated details controls unique accessible names", async () => {
    mount([
      FAILED,
      { ...FAILED, id: "op-3", tool: "opencode", finishedAt: 10 },
    ]);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });

    expect(
      within(panel).getByRole("button", {
        name: "View details: Remove Codex",
      }),
    ).toBeInTheDocument();
    expect(
      within(panel).getByRole("button", {
        name: "View details: Remove OpenCode",
      }),
    ).toBeInTheDocument();
  });

  it("opens a verified completed install without closing Task Center", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("cancelled");
      }),
    );
    mount([SUCCEEDED_INSTALL]);
    const user = userEvent.setup();
    const trigger = screen.getByRole("button", { name: /Activity/ });
    await user.click(trigger);
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    const openButton = within(panel).getByRole("button", {
      name: "Open Codex",
    });

    await user.click(openButton);
    const modal = await screen.findByRole("dialog", { name: "Open Codex" });
    expect(
      within(modal).getByText(en.tools.open.localTitle),
    ).toBeInTheDocument();
    expect(seen).toEqual([]);
    const confirm = within(modal).getByRole("button", {
      name: en.tools.open.confirm,
    });
    expect(confirm).toHaveFocus();

    await user.click(confirm);
    await waitFor(() =>
      expect(seen).toEqual([{ tool: "codex", directoryMode: "default" }]),
    );
    await waitFor(() => expect(openButton).toHaveFocus());
    expect(panel).toBeInTheDocument();
    expect(toastMocks.success).not.toHaveBeenCalled();

    await user.keyboard("{Escape}");
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it("does not offer Open until the fresh inventory verifies the tool", async () => {
    const unverifiedTools = TOOLS.map((tool) =>
      tool.id === "codex" ? { ...tool, status: "broken" } : tool,
    );
    mount([SUCCEEDED_INSTALL], unverifiedTools);
    await userEvent.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });

    expect(
      within(panel).queryByRole("button", { name: "Open Codex" }),
    ).not.toBeInTheDocument();
  });

  it("keeps task history visible but pauses Open until tool authority recovers", async () => {
    let toolRequests = 0;
    let releaseRefresh: (() => void) | undefined;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, async () => {
        toolRequests += 1;
        if (toolRequests === 1 || toolRequests === 4) {
          return HttpResponse.json(TOOLS);
        }
        if (toolRequests === 2) {
          await new Promise<void>((resolve) => {
            releaseRefresh = resolve;
          });
        }
        return HttpResponse.text("private detector path", { status: 500 });
      }),
    );
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([SUCCEEDED_INSTALL]),
      ),
    );
    const client = createTestQueryClient();
    render(<TaskCenter />, { wrapper: withQueryClient(client) });
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    const openButton = await within(panel).findByRole("button", {
      name: "Open Codex",
    });
    expect(openButton).toBeEnabled();

    act(() => {
      void client.invalidateQueries({ queryKey: toolKeys.all });
    });
    await waitFor(() => expect(toolRequests).toBe(2));
    expect(openButton).toBeDisabled();

    act(() => releaseRefresh?.());
    const warning = await within(panel).findByRole("alert", {
      name: "Couldn't refresh your tools",
    });
    expect(warning).toHaveTextContent(
      "The last tool details are still shown. Opening a completed tool stays paused until a new check succeeds.",
    );
    expect(within(panel).getByText("Install Codex")).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("private detector path");
    expect(
      screen.getByRole("button", {
        name: "Activity: No activity is running; tool actions unavailable",
      }),
    ).toBeInTheDocument();

    const retry = within(warning).getByRole("button", {
      name: "Check tools again",
    });
    await user.click(retry);
    await waitFor(() => expect(toolRequests).toBe(3));
    await waitFor(() => expect(retry).toHaveFocus());
    expect(openButton).toBeDisabled();

    await user.click(retry);
    await waitFor(() => expect(toolRequests).toBe(4));
    await waitFor(() =>
      expect(
        within(panel).getByRole("heading", {
          level: 2,
          name: en.taskCenter.title,
        }),
      ).toHaveFocus(),
    );
    expect(within(panel).queryByRole("alert")).not.toBeInTheDocument();
    expect(openButton).toBeEnabled();
  });

  it("keeps an open launch confirmation safe when tool authority expires", async () => {
    let toolRequests = 0;
    let releaseRefresh: (() => void) | undefined;
    const launches: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, async () => {
        toolRequests += 1;
        if (toolRequests === 1) return HttpResponse.json(TOOLS);
        await new Promise<void>((resolve) => {
          releaseRefresh = resolve;
        });
        return HttpResponse.text("private detector path", { status: 500 });
      }),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([SUCCEEDED_INSTALL]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
        launches.push(await request.json());
        return HttpResponse.json("cancelled");
      }),
    );
    const client = createTestQueryClient();
    render(<TaskCenter />, { wrapper: withQueryClient(client) });
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    const openButton = await within(panel).findByRole("button", {
      name: "Open Codex",
    });
    await user.click(openButton);
    const modal = await screen.findByRole("dialog", { name: "Open Codex" });
    const confirm = within(modal).getByRole("button", {
      name: en.tools.open.confirm,
    });

    act(() => {
      void client.invalidateQueries({ queryKey: toolKeys.all });
    });
    await waitFor(() => expect(toolRequests).toBe(2));
    await waitFor(() => expect(confirm).toBeDisabled());
    expect(within(modal).getByText("Opening tools is paused")).toBeVisible();
    expect(
      within(modal).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeEnabled();

    act(() => releaseRefresh?.());
    await within(modal).findByRole("alert", {
      name: "Opening tools is paused",
    });
    await user.click(confirm);
    expect(launches).toEqual([]);

    await user.click(
      within(modal).getByRole("button", { name: en.ds.action.cancel }),
    );
    await waitFor(() => expect(modal).not.toBeInTheDocument());
    expect(panel).toBeInTheDocument();
    expect(
      within(panel).getByRole("heading", {
        level: 2,
        name: en.taskCenter.title,
      }),
    ).toHaveFocus();
    expect(openButton).toBeDisabled();
  });

  it("keeps a launch confirmation blocked when fresh tools revoke launchability", async () => {
    let toolRequests = 0;
    const launches: unknown[] = [];
    const brokenTools = TOOLS.map((tool) =>
      tool.id === "codex" ? { ...tool, status: "broken" } : tool,
    );
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () => {
        toolRequests += 1;
        return HttpResponse.json(toolRequests === 1 ? TOOLS : brokenTools);
      }),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([SUCCEEDED_INSTALL]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
        launches.push(await request.json());
        return HttpResponse.json("cancelled");
      }),
    );
    const client = createTestQueryClient();
    render(<TaskCenter />, { wrapper: withQueryClient(client) });
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    await user.click(
      await within(panel).findByRole("button", { name: "Open Codex" }),
    );
    const modal = await screen.findByRole("dialog", { name: "Open Codex" });

    await act(async () => {
      await client.invalidateQueries({ queryKey: toolKeys.all });
    });
    expect(toolRequests).toBe(2);
    const alert = await within(modal).findByRole("alert", {
      name: "This tool is no longer ready to open",
    });
    expect(alert).toHaveTextContent(
      "The latest check no longer shows this tool as a launchable install. Cancel this window and review AI Tools.",
    );
    const confirm = within(modal).getByRole("button", {
      name: en.tools.open.confirm,
    });
    expect(confirm).toBeDisabled();
    expect(
      within(panel).queryByRole("button", { name: "Open Codex" }),
    ).toBeNull();
    await user.click(confirm);
    expect(launches).toEqual([]);

    await user.click(
      within(modal).getByRole("button", { name: en.ds.action.cancel }),
    );
    await waitFor(() => expect(modal).not.toBeInTheDocument());
    expect(
      within(panel).getByRole("heading", {
        level: 2,
        name: en.taskCenter.title,
      }),
    ).toHaveFocus();
  });

  it("keeps authoritative history readable when the first tool check fails", async () => {
    let toolRequests = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () => {
        toolRequests += 1;
        return toolRequests === 1
          ? HttpResponse.text("private detector path", { status: 500 })
          : HttpResponse.json(TOOLS);
      }),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([SUCCEEDED_INSTALL]),
      ),
    );
    const client = createTestQueryClient();
    render(<TaskCenter />, { wrapper: withQueryClient(client) });
    const user = userEvent.setup();

    await user.click(
      await screen.findByRole("button", {
        name: "Activity: No activity is running; tool actions unavailable",
      }),
    );
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    const warning = within(panel).getByRole("alert", {
      name: "Tool actions are unavailable",
    });
    expect(warning).toHaveTextContent(
      "Task history is still shown, but AI Manager couldn't confirm which tools can open.",
    );
    expect(within(panel).getByText("Install codex")).toBeInTheDocument();
    expect(within(panel).queryByRole("button", { name: /Open/ })).toBeNull();

    await user.click(
      within(warning).getByRole("button", { name: "Check tools again" }),
    );
    await waitFor(() => expect(toolRequests).toBe(2));
    await waitFor(() =>
      expect(
        within(panel).getByRole("heading", {
          level: 2,
          name: en.taskCenter.title,
        }),
      ).toHaveFocus(),
    );
    expect(within(panel).getByText("Install Codex")).toBeInTheDocument();
    expect(
      within(panel).getByRole("button", { name: "Open Codex" }),
    ).toBeEnabled();
  });

  it("confirms a completed terminal handoff with the existing success feedback", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, () =>
        HttpResponse.json("launched"),
      ),
    );
    mount([SUCCEEDED_INSTALL]);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Activity/ }));
    const panel = await screen.findByRole("dialog", {
      name: en.taskCenter.title,
    });
    await user.click(within(panel).getByRole("button", { name: "Open Codex" }));
    await user.click(
      within(screen.getByRole("dialog", { name: "Open Codex" })).getByRole(
        "button",
        { name: en.tools.open.confirm },
      ),
    );

    await waitFor(() =>
      expect(toastMocks.success).toHaveBeenCalledWith(
        en.tools.open.launched.replace("{{name}}", "Codex"),
      ),
    );
    expect(panel).toBeInTheDocument();
  });
});
