import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { delay, http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toolKeys } from "@/entities/tool";
import { extensionKeys, type ExtensionKind } from "@/entities/extension";
import en from "@/i18n/locales/en.json";
import { ExtensionsPage } from "@/pages/extensions/ExtensionsPage";
import type { ExtensionsTab } from "@/pages/extensions/useExtensionsWorkspaceTab";
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

function tool(
  id: string,
  name: string,
  capabilities: Record<string, boolean> = {},
) {
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
      ...capabilities,
    },
  };
}

const CLAUDE = tool("claude-code", "Claude Code", {
  canManageSkills: true,
  canManagePrompts: true,
  canLaunch: true,
});
const CODEX = tool("codex", "Codex", {
  canManageSkills: true,
  canManagePrompts: true,
});

type OperationsResponder = () => Response | Promise<Response>;
type ExtensionsResponder = (request: Request) => Response | Promise<Response>;
type LocalInventoryResponder = () => Response | Promise<Response>;
type ToolsResponder = () => Response | Promise<Response>;
type SkillUpdatesResponder = () => Response | Promise<Response>;

function extension(overrides: Record<string, unknown> = {}) {
  return {
    kind: "skill",
    id: "anthropics/skills:code-review",
    scope: { kind: "tool", id: "claude-code" },
    name: "Code review",
    description: "Checks a diff before you push.",
    enabled: false,
    canDisable: true,
    management: "managed",
    ...overrides,
  };
}

function mount(
  extensions: unknown[] | ExtensionsResponder,
  tools: unknown[] | ToolsResponder = [CLAUDE, CODEX],
  operations: unknown[] | OperationsResponder = [],
  localInventory: Record<string, unknown> | LocalInventoryResponder = {
    items: [],
    scopes: [],
    truncated: false,
  },
  skillUpdates: unknown[] | SkillUpdatesResponder = [],
  desktopApps: unknown[] = [],
  preferredTab: ExtensionsTab | null = null,
  preferredKind: ExtensionKind | null = null,
  fixedKind?: ExtensionKind,
) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
      HttpResponse.json({
        advancedMode: false,
        importPromptSeen: true,
        toolScope: null,
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
      typeof tools === "function" ? tools() : HttpResponse.json(tools),
    ),
    http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
      HttpResponse.json(desktopApps),
    ),
    http.post(`${TAURI_ENDPOINT}/app_extensions_list`, ({ request }) =>
      typeof extensions === "function"
        ? extensions(request)
        : HttpResponse.json(extensions),
    ),
    http.post(`${TAURI_ENDPOINT}/app_extensions_local_inventory`, () =>
      typeof localInventory === "function"
        ? localInventory()
        : HttpResponse.json(localInventory),
    ),
    http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
      typeof operations === "function"
        ? operations()
        : HttpResponse.json(operations),
    ),
    http.post(`${TAURI_ENDPOINT}/app_skill_updates_check`, () =>
      typeof skillUpdates === "function"
        ? skillUpdates()
        : HttpResponse.json(skillUpdates),
    ),
    // The configuration-file row above the list. Skills answer null: each one
    // is its own directory, so there is no shared file to name.
    http.post(
      `${TAURI_ENDPOINT}/app_extension_location_describe`,
      async ({ request }) => {
        const body = (await request.json()) as { kind: ExtensionKind };
        return HttpResponse.json(
          body.kind === "skill"
            ? null
            : { kind: body.kind, path: "~/.claude.json", exists: true },
        );
      },
    ),
  );
  const client = createTestQueryClient();
  return {
    client,
    ...render(
      <ExtensionsPage
        fixedKind={fixedKind}
        preferredTab={preferredTab}
        preferredKind={preferredKind}
      />,
      { wrapper: withQueryClient(client) },
    ),
  };
}

describe("ExtensionsPage", () => {
  it.each(["skill", "mcp", "prompt"] as const)(
    "fixes the independent %s destination regardless of a remembered kind",
    async (kind) => {
      const requested: unknown[] = [];
      mount(
        async (request) => {
          requested.push(await request.json());
          return HttpResponse.json([]);
        },
        undefined,
        undefined,
        undefined,
        undefined,
        undefined,
        null,
        "mcp",
        kind,
      );
      expect(
        await screen.findByRole("heading", {
          level: 1,
          name: en.extensions[kind].title,
        }),
      ).toBeInTheDocument();
      await waitFor(() =>
        expect(requested).toContainEqual({
          scope: { kind: "tool", id: "claude-code" },
          kind,
        }),
      );
      expect(
        screen.queryByRole("tablist", { name: en.extensions.kindScope }),
      ).toBeNull();
      expect(screen.queryByRole("tab", { name: en.nav.workspace })).toBeNull();
      if (kind !== "skill")
        expect(
          await screen.findByRole("button", {
            name: en.extensions.location.action,
          }),
        ).toBeEnabled();
      expect(screen.getByText(en.extensions[kind].explainer)).not.toBeVisible();
      await userEvent.click(screen.getByText(en.extensions.help));
      expect(screen.getByText(en.extensions[kind].explainer)).toBeVisible();
      if (kind === "prompt")
        expect(screen.getByText(en.extensions.prompt.exclusive)).toBeVisible();
    },
  );

  it("does not expose file reveal for an unsupported scope", async () => {
    mount(
      [],
      [tool("kimi-code", "Kimi", { canManageMcp: false })],
      undefined,
      undefined,
      undefined,
      undefined,
      null,
      null,
      "mcp",
    );
    await screen.findByRole("tab", { name: "Kimi, Not integrated" });
    expect(
      screen.queryByRole("button", { name: en.extensions.location.action }),
    ).toBeNull();
  });

  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        extensions: en.extensions,
        nav: en.nav,
        operation: en.operation,
        taskCenter: en.taskCenter,
        tool: en.tool,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.error.mockClear();
    toastMocks.info.mockClear();
  });

  it("shows a loading status before anything arrives", () => {
    mount([extension()]);
    expect(screen.getByRole("status")).toBeInTheDocument();
    for (const tab of screen.getAllByRole("tab")) {
      expect(tab).toHaveAttribute("aria-selected", "false");
    }
  });

  it("offers spec 36's three inner pages, in its order, each with its own words", async () => {
    mount([extension()]);
    const kinds = screen.getByRole("tablist", {
      name: en.extensions.kindScope,
    });
    expect(
      within(kinds)
        .getAllByRole("tab")
        .map((tab) => tab.textContent),
    ).toEqual([
      en.extensions.skill.title,
      en.extensions.mcp.title,
      en.extensions.prompt.title,
    ]);
    expect(
      await screen.findByText(en.extensions.skill.explainer),
    ).toBeInTheDocument();
  });

  it("keeps installed tools visible across every supported extension kind", async () => {
    mount([extension()]);
    const tools = await screen.findByRole("tablist", {
      name: en.extensions.toolScope,
    });
    // Claude Code and Codex both have stable local Skills directories.
    expect(
      within(tools)
        .getAllByRole("tab")
        .map((tab) => tab.textContent),
    ).toEqual(["Claude Code", "Codex"]);

    await userEvent.click(
      screen.getByRole("tab", { name: en.extensions.mcp.title }),
    );
    await waitFor(() =>
      expect(
        within(
          screen.getByRole("tablist", { name: en.extensions.toolScope }),
        ).getAllByRole("tab"),
      ).toHaveLength(2),
    );
    expect(screen.getByText(en.extensions.mcp.explainer)).toBeInTheDocument();

    await userEvent.click(
      screen.getByRole("tab", { name: en.extensions.prompt.title }),
    );
    const promptTools = screen.getByRole("tablist", {
      name: en.extensions.toolScope,
    });
    expect(within(promptTools).getAllByRole("tab")).toHaveLength(2);
    expect(
      within(promptTools).getByRole("tab", { name: "Codex" }),
    ).toBeInTheDocument();
  });

  it("offers installed Claude Desktop as a distinct MCP target", async () => {
    const seen: unknown[] = [];
    mount(
      async (request) => {
        seen.push(await request.json());
        return HttpResponse.json([
          extension({
            kind: "mcp",
            id: "filesystem",
            name: "Project files",
            scope: { kind: "desktopApp", id: "claude-desktop" },
          }),
        ]);
      },
      [CLAUDE],
      [],
      { items: [], scopes: [], truncated: false },
      [],
      [
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
      ],
    );

    await userEvent.click(
      screen.getByRole("tab", { name: en.extensions.mcp.title }),
    );
    const targetTabs = await screen.findByRole("tablist", {
      name: en.extensions.toolScope,
    });
    await userEvent.click(
      within(targetTabs).getByRole("tab", { name: "Claude Desktop" }),
    );

    expect(await screen.findByText("Project files")).toBeVisible();
    await waitFor(() =>
      expect(seen).toContainEqual({
        scope: { kind: "desktopApp", id: "claude-desktop" },
        kind: "mcp",
      }),
    );
  });

  it("opens a routed desktop-app scope directly on MCP", async () => {
    const seen: unknown[] = [];
    mount(
      async (request) => {
        seen.push(await request.json());
        return HttpResponse.json([]);
      },
      [CLAUDE],
      [],
      { items: [], scopes: [], truncated: false },
      [],
      [
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
      ],
      { kind: "desktopApp", id: "claude-desktop" },
    );

    expect(
      await screen.findByRole("tab", {
        name: en.extensions.mcp.title,
        selected: true,
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: "Claude Desktop", selected: true }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(seen).toContainEqual({
        scope: { kind: "desktopApp", id: "claude-desktop" },
        kind: "mcp",
      }),
    );
  });

  it("lists what the tool already has", async () => {
    mount([extension()]);
    expect(await screen.findByText("Code review")).toBeInTheDocument();
    expect(
      screen.getByText("Checks a diff before you push."),
    ).toBeInTheDocument();
  });

  it("keeps the add action in the header above the current extension list", async () => {
    mount([extension()]);
    const item = await screen.findByText("Code review");
    const add = screen.getByRole("button", {
      name: en.extensions.skill.catalog.add,
    });

    expect(
      add.compareDocumentPosition(item) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      screen
        .getByRole("heading", { name: en.extensions.title })
        .closest(".spatial-page-hero"),
    ).toBeNull();
  });

  it("offers a safe update only for a Skill known to have one", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_update`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json("op-skill-update-1");
      }),
    );
    mount(
      [extension()],
      [CLAUDE, CODEX],
      [],
      { items: [], scopes: [], truncated: false },
      [{ id: "anthropics/skills:code-review", name: "Code review" }],
    );

    const update = await screen.findByRole("button", {
      name: "Update Code review, item 1 of 1",
    });
    expect(screen.getByText(en.extensions.card.updateAvailable)).toBeVisible();
    await userEvent.click(update);
    const dialog = screen.getByRole("dialog", { name: "Update Code review?" });
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.extensions.skill.update.confirm,
      }),
    );
    await waitFor(() =>
      expect(body).toEqual({
        tool: "claude-code",
        skill: "anthropics/skills:code-review",
      }),
    );
  });

  it("does not show a routine refresh beside the compact add action", async () => {
    mount([extension()], [CLAUDE]);
    await screen.findByText("Code review");
    const add = screen.getByRole("button", {
      name: en.extensions.skill.catalog.add,
    });
    expect(add).not.toHaveClass("w-full");
    expect(
      screen.queryByRole("button", { name: en.extensions.refresh }),
    ).not.toBeInTheDocument();
  });

  it("reports an incomplete update check without claiming Skills are current", async () => {
    mount(
      [extension()],
      [CLAUDE],
      [],
      { items: [], scopes: [], truncated: false },
      () => HttpResponse.text("private repository", { status: 500 }),
    );
    const alert = await screen.findByRole("alert", {
      name: en.extensions.skill.update.checkErrorTitle,
    });
    expect(alert).toHaveTextContent(
      en.extensions.skill.update.checkErrorDescription,
    );
    expect(alert).not.toHaveTextContent("private repository");
    expect(document.body).not.toHaveTextContent("Up to date");
  });

  it("keeps detected Skills in place and opens their folder or SKILL.md without exposing a path", async () => {
    const bodies: unknown[] = [];
    const detected = extension({
      id: "mobile-app-release",
      name: "Mobile App Release",
      enabled: true,
      canDisable: false,
      management: "detected",
    });
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_detected_skill_resource_open`,
        async ({ request }) => {
          const body = await request.json();
          bodies.push(body);
          return HttpResponse.json(
            (body as { action?: string }).action === "edit"
              ? "editorOpened"
              : "folderOpened",
          );
        },
      ),
    );
    mount([detected]);

    const openLocation = await screen.findByRole("button", {
      name: "Open the location of Mobile App Release",
    });
    const editDocument = screen.getByRole("button", {
      name: "Edit the SKILL.md for Mobile App Release",
    });
    expect(screen.queryByText(/Add .* to AI Manager/i)).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();

    await userEvent.click(openLocation);
    await waitFor(() => expect(bodies).toHaveLength(1));
    await userEvent.click(editDocument);

    await waitFor(() =>
      expect(bodies).toEqual([
        {
          scope: { kind: "tool", id: "claude-code" },
          skill: "mobile-app-release",
          action: "browse",
        },
        {
          scope: { kind: "tool", id: "claude-code" },
          skill: "mobile-app-release",
          action: "edit",
        },
      ]),
    );
    expect(JSON.stringify(bodies)).not.toContain("/Users/");
  });

  it("opens the safe Skill removal confirmation and sends only its stable id", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_remove`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json("op-skill-remove-1");
      }),
    );
    mount([extension()]);
    await userEvent.click(
      await screen.findByRole("button", {
        name: i18n.t("extensions.card.removeNamed", {
          name: "Code review",
          position: 1,
          total: 1,
        }),
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Remove Code review?",
    });
    expect(dialog).toHaveTextContent(en.extensions.skill.remove.point.recovery);
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.extensions.skill.remove.confirm,
      }),
    );
    await waitFor(() =>
      expect(body).toEqual({
        tool: "claude-code",
        skill: "anthropics/skills:code-review",
      }),
    );
    expect(toastMocks.info).toHaveBeenCalledWith("Removing Code review", {
      description: "You can keep working and follow progress in Tasks.",
    });
  });

  it("locks Skill controls and shows background removal progress", async () => {
    mount(
      [extension()],
      [CLAUDE, CODEX],
      [
        {
          id: "op-remove",
          kind: "uninstall",
          tool: "claude-code",
          extension: {
            kind: "skill",
            id: "anthropics/skills:code-review",
            name: "Code review",
          },
          status: "running",
          progress: 25,
          messageKey: "operation.phase.removing",
          error: null,
          startedAt: 10,
          finishedAt: null,
        },
      ],
    );
    expect(await screen.findByText("Remove Code review Skill")).toBeVisible();
    expect(screen.getByText("25%")).toBeVisible();
    expect(
      screen.getByRole("button", { name: en.extensions.skill.catalog.add }),
    ).toBeDisabled();
    expect(
      screen.getByRole("switch", {
        name: i18n.t("extensions.card.itemLabel", {
          name: "Code review",
          position: 1,
          total: 1,
        }),
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: i18n.t("extensions.card.removeNamed", {
          name: "Code review",
          position: 1,
          total: 1,
        }),
      }),
    ).toBeDisabled();
  });

  it("pauses extension mutations until the task baseline is known", async () => {
    mount([extension()], [CLAUDE, CODEX], async () => {
      await delay("infinite");
      return HttpResponse.json([]);
    });

    expect(
      await screen.findByText(en.taskCenter.loading.summary),
    ).toBeInTheDocument();
    // The task baseline and the extension list are independent queries. The
    // summary above only proves the first one is still loading, so the card has
    // to be awaited on its own before its controls can be asserted on.
    await screen.findByText("Code review");
    expect(
      screen.getByRole("button", { name: en.extensions.skill.catalog.add }),
    ).toBeDisabled();
    expect(
      screen.getByRole("switch", {
        name: i18n.t("extensions.card.itemLabel", {
          name: "Code review",
          position: 1,
          total: 1,
        }),
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: i18n.t("extensions.card.removeNamed", {
          name: "Code review",
          position: 1,
          total: 1,
        }),
      }),
    ).toBeDisabled();
  });

  it("explains unavailable task state and unlocks extensions after retry", async () => {
    const user = userEvent.setup();
    let operationReads = 0;
    mount([extension()], [CLAUDE, CODEX], () => {
      operationReads += 1;
      return operationReads === 1
        ? HttpResponse.text("not available", { status: 500 })
        : HttpResponse.json([]);
    });

    expect(
      await screen.findByText(en.taskCenter.unavailable.title),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.extensions.skill.catalog.add }),
    ).toBeDisabled();

    const retry = screen.getByRole("button", {
      name: en.taskCenter.unavailable.retry,
    });
    for (
      let step = 0;
      step < 12 && document.activeElement !== retry;
      step += 1
    ) {
      await user.tab();
    }
    expect(retry).toHaveFocus();
    await user.keyboard("{Enter}");

    await waitFor(() => expect(operationReads).toBe(2));
    await waitFor(() =>
      expect(
        screen.queryByText(en.taskCenter.unavailable.title),
      ).not.toBeInTheDocument(),
    );
    expect(
      screen.getByRole("button", { name: en.extensions.skill.catalog.add }),
    ).toBeEnabled();
    expect(
      screen.getByRole("switch", {
        name: i18n.t("extensions.card.itemLabel", {
          name: "Code review",
          position: 1,
          total: 1,
        }),
      }),
    ).toBeEnabled();
  });

  it("labels both tab panels", async () => {
    mount([
      extension({ enabled: true }),
      extension({ id: "second", name: "Second" }),
    ]);
    const kindTab = await screen.findByRole("tab", {
      name: en.extensions.skill.title,
    });
    expect(kindTab).toHaveAttribute("aria-controls", "extensions-kind-panel");
    const kindPanel = await screen.findByRole("tabpanel", {
      name: en.extensions.skill.title,
    });
    expect(kindPanel).toHaveAttribute("role", "tabpanel");
    expect(kindPanel).toHaveAttribute("aria-labelledby", kindTab.id);

    const toolTab = screen.getByRole("tab", { name: "Claude Code" });
    expect(toolTab).toHaveAttribute("aria-controls", "extensions-tool-panel");
    const toolPanel = await screen.findByRole("tabpanel", {
      name: "Claude Code",
    });
    expect(toolPanel).toHaveAttribute("role", "tabpanel");
    expect(toolPanel).toHaveAttribute("aria-labelledby", toolTab.id);
  });

  it("separates local discoveries from items managed by AI Manager", async () => {
    mount([
      extension({ id: "managed", name: "Managed Skill" }),
      extension({
        id: "local",
        name: "Local Skill",
        management: "detected",
        enabled: true,
        canDisable: false,
      }),
    ]);

    const detected = await screen.findByRole("region", {
      name: en.extensions.inventory.detected.title,
    });
    const managed = screen.getByRole("region", {
      name: en.extensions.inventory.managed.title,
    });
    expect(detected).toHaveTextContent("Local Skill");
    expect(detected).not.toHaveTextContent("Managed Skill");
    expect(managed).toHaveTextContent("Managed Skill");
    expect(managed).not.toHaveTextContent("Local Skill");
    expect(
      within(detected).getByRole("button", {
        name: en.extensions.inventory.detected.rescan,
      }),
    ).toBeInTheDocument();
    expect(
      within(managed).queryByRole("button", {
        name: en.extensions.inventory.detected.rescan,
      }),
    ).toBeNull();
  });

  it("prints where a detected Skill actually lives instead of a stock reassurance", async () => {
    const local = extension({
      id: "unity-cli",
      name: "Unity CLI",
      management: "detected",
      enabled: true,
      canDisable: false,
    });
    mount([local], [CLAUDE, CODEX], [], {
      items: [local, { ...local, scope: { kind: "tool", id: "codex" } }],
      scopes: [
        { tool: "claude-code", kind: "skill", status: "ready" },
        { tool: "codex", kind: "skill", status: "ready" },
      ],
      truncated: false,
    });

    expect(
      await screen.findByText(
        i18n.t("extensions.card.alsoIn", { tools: ["Claude Code", "Codex"] }),
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(en.extensions.card.detectedDescription),
    ).toBeNull();
  });

  it("keeps the neutral wording while the local inventory is unreadable", async () => {
    mount(
      [
        extension({
          id: "unity-cli",
          name: "Unity CLI",
          management: "detected",
          enabled: true,
          canDisable: false,
        }),
      ],
      [CLAUDE, CODEX],
      [],
      () => HttpResponse.text("private local inventory path", { status: 500 }),
    );

    expect(
      await screen.findByText(en.extensions.card.detectedDescription),
    ).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("private local inventory path");
  });

  it("copies a detected Skill into the tool the user picks and leaves the rest alone", async () => {
    const local = extension({
      id: "unity-cli",
      name: "Unity CLI",
      management: "detected",
      enabled: true,
      canDisable: false,
    });
    const sent: unknown[] = [];
    mount([local], [CLAUDE, CODEX, tool("gemini-cli", "Gemini CLI")], [], {
      items: [local],
      scopes: [{ tool: "claude-code", kind: "skill", status: "ready" }],
      truncated: false,
    });
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_detected_skill_copy`,
        async ({ request }) => {
          sent.push(await request.json());
          return HttpResponse.json([local]);
        },
      ),
    );

    await userEvent.click(
      await screen.findByRole("button", {
        name: i18n.t("extensions.card.copyToNamed", { name: "Unity CLI" }),
      }),
    );
    const dialog = await screen.findByRole("dialog");
    // Gemini CLI cannot hold Skills, so it is not a destination at all.
    expect(dialog).not.toHaveTextContent("Gemini CLI");
    // The tool it was found in is not offered either.
    expect(within(dialog).queryByLabelText("Claude Code")).toBeNull();

    await userEvent.click(within(dialog).getByLabelText("Codex"));
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.extensions.copy.confirm,
      }),
    );

    await waitFor(() =>
      expect(sent).toEqual([
        {
          scope: { kind: "tool", id: "claude-code" },
          target: "codex",
          skill: "unity-cli",
        },
      ]),
    );
    expect(
      await within(dialog).findByText(en.extensions.copy.copied),
    ).toBeInTheDocument();
  });

  it("greys out a tool that already carries the Skill instead of offering to overwrite it", async () => {
    const local = extension({
      id: "unity-cli",
      name: "Unity CLI",
      management: "detected",
      enabled: true,
      canDisable: false,
    });
    mount([local], [CLAUDE, CODEX], [], {
      items: [local, { ...local, scope: { kind: "tool", id: "codex" } }],
      scopes: [
        { tool: "claude-code", kind: "skill", status: "ready" },
        { tool: "codex", kind: "skill", status: "ready" },
      ],
      truncated: false,
    });

    // Every eligible tool already has it, so there is nothing to offer.
    await screen.findByText(
      i18n.t("extensions.card.alsoIn", { tools: ["Claude Code", "Codex"] }),
    );
    expect(
      screen.queryByRole("button", {
        name: i18n.t("extensions.card.copyToNamed", { name: "Unity CLI" }),
      }),
    ).toBeNull();
  });

  it("gives two same-named switches distinct accessible names", async () => {
    mount([
      extension({ id: "first", name: "Repeated" }),
      extension({ id: "second", name: "Repeated" }),
    ]);
    expect(
      await screen.findByRole("switch", {
        name: i18n.t("extensions.card.itemLabel", {
          name: "Repeated",
          position: 1,
          total: 2,
        }),
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("switch", {
        name: i18n.t("extensions.card.itemLabel", {
          name: "Repeated",
          position: 2,
          total: 2,
        }),
      }),
    ).toBeInTheDocument();
  });

  it("says so honestly when a kind has nothing yet", async () => {
    mount([]);
    expect(
      await screen.findByText(en.extensions.skill.empty.title),
    ).toBeInTheDocument();
  });

  it("offers guided add flows only for the extension kinds that support them", async () => {
    mount([extension()]);
    await screen.findByText("Code review");
    expect(
      screen.getByRole("button", { name: en.extensions.skill.catalog.add }),
    ).toBeInTheDocument();
    expect(screen.queryByText(en.extensions.addLaterTitle)).toBeNull();

    await userEvent.click(
      screen.getByRole("tab", { name: en.extensions.mcp.title }),
    );
    expect(
      await screen.findByRole("button", {
        name: en.extensions.mcp.install.add,
      }),
    ).toBeInTheDocument();
    expect(screen.queryByText(en.extensions.addLaterTitle)).toBeNull();
    expect(
      screen.queryByRole("button", { name: en.extensions.skill.catalog.add }),
    ).toBeNull();

    await userEvent.click(
      screen.getByRole("tab", { name: en.extensions.prompt.title }),
    );
    expect(
      await screen.findByRole("button", {
        name: en.extensions.prompt.editor.add,
      }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: en.extensions.prompt.import.open,
      }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(en.extensions.addLaterTitle)).toBeNull();
    expect(
      screen.queryByRole("button", { name: en.extensions.mcp.install.add }),
    ).toBeNull();
  });

  it("opens the guided MCP form instead of asking for raw configuration", async () => {
    const { container } = mount([]);
    await userEvent.click(
      await screen.findByRole("tab", { name: en.extensions.mcp.title }),
    );
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.extensions.mcp.install.add,
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: en.extensions.mcp.install.title,
    });
    expect(
      within(dialog).getByLabelText(en.extensions.mcp.install.name),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole("radio", { name: /Local command/ }),
    ).toBeChecked();
    expect(container.querySelector("pre, code")).toBeNull();
  });

  it("offers a global MCP removal confirmation and submits only its stable id", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_remove`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json("op-mcp-remove-1");
      }),
    );
    mount([
      extension({
        kind: "mcp",
        id: "filesystem-a1b2c3d4",
        name: "Project files",
      }),
    ]);
    await userEvent.click(
      await screen.findByRole("tab", { name: en.extensions.mcp.title }),
    );
    await userEvent.click(
      await screen.findByRole("button", {
        name: i18n.t("extensions.card.removeNamed", {
          name: "Project files",
          position: 1,
          total: 1,
        }),
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Remove Project files?",
    });
    expect(dialog).toHaveTextContent(en.extensions.mcp.remove.point.everywhere);
    expect(dialog).toHaveTextContent(
      en.extensions.mcp.remove.point.keepsTargets,
    );
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.extensions.mcp.remove.confirm,
      }),
    );
    await waitFor(() =>
      expect(body).toEqual({
        scope: { kind: "tool", id: "claude-code" },
        mcp: "filesystem-a1b2c3d4",
      }),
    );
  });

  it("locks MCP controls and shows global removal progress", async () => {
    mount(
      [
        extension({
          kind: "mcp",
          id: "filesystem-a1b2c3d4",
          name: "Project files",
        }),
      ],
      [CLAUDE, CODEX],
      [
        {
          id: "op-mcp-remove",
          kind: "uninstall",
          tool: "claude-code",
          extension: {
            kind: "mcp",
            id: "filesystem-a1b2c3d4",
            name: "Project files",
          },
          status: "running",
          progress: 25,
          messageKey: "operation.phase.removing",
          error: null,
          startedAt: 10,
          finishedAt: null,
        },
      ],
    );
    await userEvent.click(
      await screen.findByRole("tab", { name: en.extensions.mcp.title }),
    );
    expect(
      await screen.findByText("Remove Project files MCP connection"),
    ).toBeVisible();
    expect(screen.getByText("25%")).toBeVisible();
    expect(
      screen.getByRole("button", { name: en.extensions.mcp.install.add }),
    ).toBeDisabled();
    expect(
      screen.getByRole("switch", {
        name: i18n.t("extensions.card.itemLabel", {
          name: "Project files",
          position: 1,
          total: 1,
        }),
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: i18n.t("extensions.card.removeNamed", {
          name: "Project files",
          position: 1,
          total: 1,
        }),
      }),
    ).toBeDisabled();
  });

  it("opens a searchable Skill picker instead of a raw configuration editor", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, () =>
        HttpResponse.json([
          {
            id: "anthropics/skills:code-review",
            name: "Code review from catalog",
            description: "Reviews changes.",
            source: {
              owner: "anthropics",
              repository: "skills",
              branch: "main",
              directory: "code-review",
            },
            installed: false,
            mirrorUsed: false,
          },
        ]),
      ),
    );
    const { container } = mount([]);
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.extensions.skill.catalog.add,
      }),
    );
    expect(
      await screen.findByRole("dialog", {
        name: en.extensions.skill.catalog.title,
      }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText("Code review from catalog"),
    ).toBeInTheDocument();
    expect(container.querySelector("textarea")).toBeNull();
  });

  it("switches one on and shows the refreshed state", async () => {
    mount([extension()]);
    await screen.findByText("Code review");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extension_set_enabled`, () =>
        HttpResponse.json([extension({ enabled: true })]),
      ),
    );
    const switchName = i18n.t("extensions.card.itemLabel", {
      name: "Code review",
      position: 1,
      total: 1,
    });
    await userEvent.click(
      await screen.findByRole("switch", { name: switchName }),
    );
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: switchName })).toBeChecked(),
    );
  });

  it("refreshes a failed toggle, keeps it on the right card, and retries the intended state", async () => {
    const inventory = [
      extension(),
      extension({
        id: "example-org/skills:release-notes",
        name: "Release notes",
        description: "Drafts a summary after a change.",
      }),
    ];
    const bodies: unknown[] = [];
    let attempts = 0;
    let releaseRefresh: (() => void) | undefined;
    mount(inventory);
    await screen.findByText("Code review");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extensions_list`, async () => {
        await new Promise<void>((resolve) => {
          releaseRefresh = resolve;
        });
        // Simulate a partial-success scenario: tool-side write fails but the DB state
        // has already changed. The retry must reuse the user's original target value,
        // not resubmit based on the toggle's current state.
        return HttpResponse.json([extension({ enabled: true }), inventory[1]]);
      }),
      http.post(
        `${TAURI_ENDPOINT}/app_extension_set_enabled`,
        async ({ request }) => {
          attempts += 1;
          bodies.push(await request.json());
          if (attempts === 1) {
            return HttpResponse.json(
              {
                code: "CONFIG_WRITE_FAILED",
                messageKey: "error.extension.toggleFailed",
                technicalMessage: "permission denied at /private/tool.json",
                remediation: "error.remediation.checkPermissions",
                contextId: null,
              },
              { status: 500 },
            );
          }
          return HttpResponse.json([
            extension({ enabled: true }),
            inventory[1],
          ]);
        },
      ),
    );
    const switchName = i18n.t("extensions.card.itemLabel", {
      name: "Code review",
      position: 1,
      total: 2,
    });
    const control = await screen.findByRole("switch", { name: switchName });

    await userEvent.click(control);
    await waitFor(() => expect(releaseRefresh).toBeTypeOf("function"));
    const card = screen.getByRole("article", { name: "Code review" });
    expect(card).toHaveAttribute("aria-busy", "true");
    expect(control).toBeDisabled();
    expect(within(card).queryByRole("alert")).toBeNull();

    releaseRefresh?.();
    const alert = await within(card).findByRole("alert", {
      name: "Could not turn on Code review",
    });
    expect(control).toBeChecked();
    expect(control).toHaveFocus();
    expect(alert).toHaveTextContent(en.error.extension.toggleFailed);
    expect(alert).toHaveTextContent(en.error.remediation.checkPermissions);
    expect(alert).toHaveTextContent(
      "AI Manager refreshed the status it can read. Try the same change again.",
    );
    expect(alert).not.toHaveTextContent("/private/tool.json");
    expect(
      within(
        screen.getByRole("article", { name: "Release notes" }),
      ).queryByRole("alert"),
    ).toBeNull();
    expect(toastMocks.error).not.toHaveBeenCalled();

    const retryControl = screen.getByRole("switch", {
      name: "Try turning on Code review again, item 1 of 2",
    });
    expect(retryControl).toBe(control);
    expect(retryControl).toHaveFocus();
    await userEvent.click(retryControl);
    const completedControl = await screen.findByRole("switch", {
      name: switchName,
    });
    await waitFor(() => expect(completedControl).toBeChecked());
    expect(completedControl).toHaveFocus();
    expect(bodies).toEqual([
      {
        scope: { kind: "tool", id: "claude-code" },
        kind: "skill",
        extension: "anthropics/skills:code-review",
        enabled: true,
      },
      {
        scope: { kind: "tool", id: "claude-code" },
        kind: "skill",
        extension: "anthropics/skills:code-review",
        enabled: true,
      },
    ]);
    expect(within(card).queryByRole("alert")).toBeNull();
  });

  it("tells the user in plain words when the list cannot be read", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({
          advancedMode: false,
          importPromptSeen: true,
          toolScope: null,
          extensionKind: null,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([CLAUDE]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_extensions_list`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "UPSTREAM_ERROR",
            messageKey: "error.extension.listFailed",
            technicalMessage: "unable to read config",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    render(<ExtensionsPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    expect(
      await screen.findByText(en.extensions.error.title),
    ).toBeInTheDocument();
    // Technical details never surface on the main UI (§42).
    expect(screen.queryByText("unable to read config")).toBeNull();
  });

  it("keeps one initial read retry busy without losing its focus", async () => {
    let reads = 0;
    let releaseRetry!: () => void;
    mount(async () => {
      reads += 1;
      if (reads === 1) {
        return HttpResponse.text("private extension directory", {
          status: 500,
        });
      }
      await new Promise<void>((resolve) => {
        releaseRetry = resolve;
      });
      return HttpResponse.json([extension()]);
    }, [CLAUDE]);
    await screen.findByText(en.extensions.error.title);

    const refreshButtons = screen.getAllByRole("button", {
      name: en.extensions.refresh,
    });
    expect(refreshButtons).toHaveLength(1);
    const retry = refreshButtons[0];
    expect(
      screen.getByRole("button", { name: en.extensions.skill.catalog.add }),
    ).toBeDisabled();

    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(2));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(screen.getByText(en.extensions.error.title)).toBeInTheDocument();
    expect(screen.queryByRole("status")).toBeNull();
    expect(document.body).not.toHaveTextContent("private extension directory");

    releaseRetry();
    expect(await screen.findByText("Code review")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: en.extensions.refresh }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.extensions.skill.catalog.add }),
    ).toBeEnabled();
  });

  it("retains the scoped list and pauses mutations after refresh failure", async () => {
    let reads = 0;
    let releaseRetry!: () => void;
    const { client } = mount(async () => {
      reads += 1;
      if (reads === 2) {
        return HttpResponse.text("private extension refresh target", {
          status: 500,
        });
      }
      if (reads === 3) {
        await new Promise<void>((resolve) => {
          releaseRetry = resolve;
        });
      }
      return HttpResponse.json([extension({ enabled: reads === 3 })]);
    }, [CLAUDE]);
    await screen.findByText("Code review");
    const switchName = i18n.t("extensions.card.itemLabel", {
      name: "Code review",
      position: 1,
      total: 1,
    });
    const control = await screen.findByRole("switch", { name: switchName });
    await client.invalidateQueries({
      queryKey: extensionKeys.list("tool:claude-code", "skill"),
    });
    const alert = await screen.findByRole("alert", {
      name: en.extensions.refreshError.title,
    });
    expect(reads).toBe(2);
    expect(alert).toHaveTextContent(en.extensions.refreshError.description);
    expect(screen.getByText("Code review")).toBeInTheDocument();
    expect(
      screen.getByRole("region", {
        name: en.extensions.inventory.managed.title,
      }),
    ).toBeInTheDocument();
    expect(control).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: i18n.t("extensions.card.removeNamed", {
          name: "Code review",
          position: 1,
          total: 1,
        }),
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("tab", { name: en.extensions.skill.title }),
    ).toBeEnabled();
    expect(screen.getByRole("tab", { name: "Claude Code" })).toBeEnabled();
    expect(
      screen.getByRole("button", { name: en.extensions.skill.catalog.add }),
    ).toBeDisabled();
    const retry = within(alert).getByRole("button", {
      name: en.extensions.refresh,
    });
    expect(document.body).not.toHaveTextContent(
      "private extension refresh target",
    );

    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(3));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByRole("status")).toBeNull();

    releaseRetry();
    await waitFor(() => expect(control).toBeChecked());
    await waitFor(() => expect(alert).not.toBeInTheDocument());
  });

  it("keeps an initial tool retry stable and moves focus to the named page after recovery", async () => {
    let reads = 0;
    let releaseRecovery!: () => void;
    const recoveryGate = new Promise<void>((resolve) => {
      releaseRecovery = resolve;
    });
    mount([extension()], async () => {
      reads += 1;
      if (reads === 1) {
        return HttpResponse.text("private tool detector path", {
          status: 500,
        });
      }
      await recoveryGate;
      return HttpResponse.json([CLAUDE]);
    });

    const alert = await screen.findByRole("alert", {
      name: en.extensions.toolsError.title,
    });
    const page = screen.getByRole("region", { name: en.extensions.title });
    expect(page).toHaveClass("min-w-0");
    for (const tab of screen.getAllByRole("tab")) {
      expect(tab).toBeDisabled();
    }
    const retry = within(alert).getByRole("button", {
      name: en.extensions.refresh,
    });

    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(2));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toBeInTheDocument();
    expect(screen.queryByRole("status")).toBeNull();
    expect(document.body).not.toHaveTextContent("private tool detector path");

    releaseRecovery();
    expect(await screen.findByText("Code review")).toBeInTheDocument();
    await waitFor(() => expect(page).toHaveFocus());
  });

  it("retains extension content and pauses every mutation while the tool baseline recovers", async () => {
    let reads = 0;
    let releaseFailure!: () => void;
    let releaseRecovery!: () => void;
    const failureGate = new Promise<void>((resolve) => {
      releaseFailure = resolve;
    });
    const recoveryGate = new Promise<void>((resolve) => {
      releaseRecovery = resolve;
    });
    const { client } = mount([extension()], async () => {
      reads += 1;
      if (reads === 2) {
        await failureGate;
        return HttpResponse.text("private refreshed tool inventory", {
          status: 500,
        });
      }
      if (reads === 3) await recoveryGate;
      return HttpResponse.json([CLAUDE]);
    });
    await screen.findByText("Code review");
    const add = screen.getByRole("button", {
      name: en.extensions.skill.catalog.add,
    });
    const switchName = i18n.t("extensions.card.itemLabel", {
      name: "Code review",
      position: 1,
      total: 1,
    });
    const control = await screen.findByRole("switch", { name: switchName });

    void client.invalidateQueries({ queryKey: toolKeys.all });
    await waitFor(() => expect(reads).toBe(2));
    expect(screen.getByText("Code review")).toBeInTheDocument();
    expect(add).toBeDisabled();
    expect(control).toBeDisabled();

    releaseFailure();
    const alert = await screen.findByRole("alert", {
      name: en.extensions.toolsRefreshError.title,
    });
    expect(alert).toHaveTextContent(
      en.extensions.toolsRefreshError.description,
    );
    expect(screen.getByText("Code review")).toBeInTheDocument();
    expect(
      screen.getByRole("region", {
        name: en.extensions.inventory.managed.title,
      }),
    ).toBeInTheDocument();
    expect(add).toBeDisabled();
    expect(control).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: i18n.t("extensions.card.removeNamed", {
          name: "Code review",
          position: 1,
          total: 1,
        }),
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("tab", { name: en.extensions.skill.title }),
    ).toBeEnabled();
    expect(screen.getByRole("tab", { name: "Claude Code" })).toBeEnabled();
    expect(document.body).not.toHaveTextContent(
      "private refreshed tool inventory",
    );

    const retry = within(alert).getByRole("button", {
      name: en.extensions.toolsRefreshError.action,
    });
    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(3));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toHaveAttribute("aria-busy", "true");
    const focusTarget = screen.getByRole("tab", {
      name: en.extensions.skill.title,
    });
    focusTarget.focus();

    releaseRecovery();
    await waitFor(() => expect(alert).not.toBeInTheDocument());
    expect(add).toBeEnabled();
    expect(control).toBeEnabled();
    expect(focusTarget).toHaveFocus();
  });

  it("fails closed when the tool refresh fails after the Skill picker opens", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, () =>
        HttpResponse.json([
          {
            id: "anthropics/skills:code-review",
            name: "Code review from catalog",
            description: "Reviews changes.",
            source: {
              owner: "anthropics",
              repository: "skills",
              branch: "main",
              directory: "code-review",
            },
            installed: false,
            mirrorUsed: false,
          },
        ]),
      ),
    );
    let reads = 0;
    const { client } = mount([extension()], () => {
      reads += 1;
      return reads === 1
        ? HttpResponse.json([CLAUDE])
        : HttpResponse.text("private tool refresh detail", { status: 500 });
    });
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.extensions.skill.catalog.add,
      }),
    );
    await screen.findByText("Code review from catalog");

    void client.invalidateQueries({ queryKey: toolKeys.all });
    await waitFor(() => expect(reads).toBe(2));
    const dialog = await screen.findByRole("dialog", {
      name: en.extensions.skill.catalog.title,
    });
    const alert = await within(dialog).findByRole("alert", {
      name: en.extensions.actionsPaused.title,
    });
    expect(alert).toHaveTextContent(en.extensions.actionsPaused.description);
    expect(
      within(dialog).getByRole("button", {
        name: "Install Code review from catalog",
      }),
    ).toBeDisabled();
    for (const close of within(dialog).getAllByRole("button", {
      name: en.ds.action.close,
    })) {
      expect(close).toBeEnabled();
    }
    expect(dialog).not.toHaveTextContent("private tool refresh detail");
  });

  it("explains an unsupported kind instead of hiding the installed tool", async () => {
    let reads = 0;
    mount(() => {
      reads += 1;
      return HttpResponse.json([]);
    }, [
      tool("openclaw", "OpenClaw", {
        canManageMcp: false,
        canManagePrompts: true,
      }),
    ]);
    // OpenClaw's native extension surface is its managed AGENTS.md.
    expect(
      await screen.findByText(en.extensions.prompt.explainer),
    ).toBeInTheDocument();
    expect(reads).toBe(1);
    await userEvent.click(
      screen.getByRole("tab", { name: en.extensions.skill.title }),
    );
    expect(
      await screen.findByText(
        en.extensions.scope.unsupportedTitle
          .replace("{{kind}}", en.extensions.skill.title)
          .replace("{{tool}}", "OpenClaw"),
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("tab", {
        name: `OpenClaw, ${en.extensions.scope.unsupportedShort}`,
      }),
    ).toBeInTheDocument();
    expect(reads).toBe(1);
  });

  it("never puts raw configuration on the first screen", async () => {
    const { container } = mount([extension()]);
    await screen.findByText("Code review");
    // Spec §36.
    expect(container.querySelector("textarea")).toBeNull();
    expect(container.querySelector("pre")).toBeNull();
    expect(container.querySelector("code")).toBeNull();
  });

  /**
   * When the local data page hands back a global prompt, it must land on the
   * Prompts tab — falling back to a remembered Skills tab means the handoff
   * never happened (ADR-0037).
   */
  it("opens on the kind the shell asked for, and yields to the next manual choice", async () => {
    mount(
      [extension()],
      [CLAUDE],
      [],
      undefined,
      [],
      [],
      { kind: "tool", id: "claude-code" },
      "prompt",
    );

    await waitFor(() =>
      expect(
        screen.getByRole("tab", { name: en.extensions.prompt.title }),
      ).toHaveAttribute("aria-selected", "true"),
    );

    await userEvent.click(
      screen.getByRole("tab", { name: en.extensions.skill.title }),
    );

    await waitFor(() =>
      expect(
        screen.getByRole("tab", { name: en.extensions.skill.title }),
      ).toHaveAttribute("aria-selected", "true"),
    );
  });

  it("remembers which kind you were looking at", async () => {
    let saved: unknown;
    mount([extension()]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        const body = (await request.json()) as {
          settings: { extensionKind: string };
        };
        saved = body.settings.extensionKind;
        return HttpResponse.json(body.settings);
      }),
    );
    await userEvent.click(
      await screen.findByRole("tab", { name: en.extensions.mcp.title }),
    );
    await waitFor(() => expect(saved).toBe("mcp"));
  });

  it("keeps both scope choices usable and retries them together after save failures", async () => {
    let attempts = 0;
    let saved: Record<string, unknown> | undefined;
    const releases: Array<(() => void) | undefined> = [];
    mount([extension()]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        attempts += 1;
        const body = (await request.json()) as {
          settings: Record<string, unknown>;
        };
        if (attempts <= 2) {
          await new Promise<void>((resolve) => {
            releases[attempts - 1] = resolve;
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
        saved = body.settings;
        return HttpResponse.json(body.settings);
      }),
    );

    await userEvent.click(
      screen.getByRole("tab", { name: en.extensions.mcp.title }),
    );
    await waitFor(() => expect(releases[0]).toBeTypeOf("function"));
    expect(
      screen.getByRole("status", { name: en.extensions.scopeSave.saving }),
    ).toBeInTheDocument();
    for (const tab of screen.getAllByRole("tab")) {
      expect(tab).toBeDisabled();
    }

    releases[0]?.();
    let failure = await screen.findByRole("alert", {
      name: en.extensions.scopeSave.errorTitle,
    });
    expect(
      screen.getByRole("tab", { name: en.extensions.mcp.title }),
    ).toHaveAttribute("aria-selected", "true");
    expect(failure).not.toHaveTextContent("permission denied");

    await userEvent.click(screen.getByRole("tab", { name: "Codex" }));
    await waitFor(() => expect(releases[1]).toBeTypeOf("function"));
    expect(
      screen.getByRole("status", { name: en.extensions.scopeSave.saving }),
    ).toBeInTheDocument();
    for (const tab of screen.getAllByRole("tab")) {
      expect(tab).toBeDisabled();
    }

    releases[1]?.();
    failure = await screen.findByRole("alert", {
      name: en.extensions.scopeSave.errorTitle,
    });
    expect(screen.getByRole("tab", { name: "Codex" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    await userEvent.click(
      within(failure).getByRole("button", {
        name: en.extensions.scopeSave.retry,
      }),
    );
    await waitFor(() => expect(attempts).toBe(3));
    expect(saved).toMatchObject({ extensionKind: "mcp", toolScope: "codex" });
    await waitFor(() =>
      expect(
        screen.queryByRole("alert", {
          name: en.extensions.scopeSave.errorTitle,
        }),
      ).not.toBeInTheDocument(),
    );
    expect(
      screen.getByRole("tab", { name: en.extensions.mcp.title }),
    ).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Codex" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});

describe("ExtensionsPage OpenClaw workspace tab", () => {
  const OPENCLAW = tool("openclaw", "OpenClaw", {
    canManageSkills: false,
    canManageMcp: false,
    canManagePrompts: true,
  });

  beforeEach(async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_overview`, () =>
        HttpResponse.json({
          files: [],
          existingFiles: 0,
          dailyMemoryCount: 0,
          dailyMemoryBytes: 0,
          totalBytes: 0,
          limited: false,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_openclaw_daily_memories`, () =>
        HttpResponse.json({
          items: [],
          totalCount: 0,
          totalBytes: 0,
          limited: false,
        }),
      ),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        common: en.common,
        ds: en.ds,
        error: en.error,
        extensions: en.extensions,
        nav: en.nav,
        openClawWorkspace: en.openClawWorkspace,
        operation: en.operation,
        taskCenter: en.taskCenter,
        tool: en.tool,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("does not offer the workspace while OpenClaw is not installed", async () => {
    mount([extension()]);
    await screen.findByText(en.extensions.skill.explainer);
    const kinds = screen.getByRole("tablist", {
      name: en.extensions.kindScope,
    });
    expect(
      within(kinds).queryByRole("tab", { name: en.nav.workspace }),
    ).toBeNull();
    expect(
      screen.queryByRole("region", { name: en.openClawWorkspace.title }),
    ).toBeNull();
  });

  it("adds the workspace as a fourth tab once OpenClaw is installed, without writing settings", async () => {
    let settingsWrites = 0;
    mount([extension()], [CLAUDE, CODEX, OPENCLAW]);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        settingsWrites += 1;
        const body = (await request.json()) as {
          settings: Record<string, unknown>;
        };
        return HttpResponse.json(body.settings);
      }),
    );
    await screen.findByText(en.extensions.skill.explainer);
    const kinds = screen.getByRole("tablist", {
      name: en.extensions.kindScope,
    });
    expect(
      within(kinds)
        .getAllByRole("tab")
        .map((tab) => tab.textContent),
    ).toEqual([
      en.extensions.skill.title,
      en.extensions.mcp.title,
      en.extensions.prompt.title,
      en.nav.workspace,
    ]);

    await userEvent.click(
      within(kinds).getByRole("tab", { name: en.nav.workspace }),
    );
    expect(
      await screen.findByRole("region", { name: en.openClawWorkspace.title }),
    ).toBeInTheDocument();
    expect(
      within(kinds).getByRole("tab", { name: en.nav.workspace }),
    ).toHaveAttribute("aria-selected", "true");
    expect(
      within(kinds).getByRole("tab", { name: en.extensions.skill.title }),
    ).toHaveAttribute("aria-selected", "false");
    expect(screen.queryByText(en.extensions.skill.explainer)).toBeNull();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(settingsWrites).toBe(0);

    await userEvent.click(
      within(kinds).getByRole("tab", { name: en.extensions.skill.title }),
    );
    expect(
      await screen.findByText(en.extensions.skill.explainer),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: en.openClawWorkspace.title }),
    ).toBeNull();
  });

  it("opens directly on the workspace when the shell asks for it", async () => {
    mount(
      [extension()],
      [CLAUDE, OPENCLAW],
      [],
      undefined,
      [],
      [],
      "workspace",
    );
    expect(
      await screen.findByRole("region", { name: en.openClawWorkspace.title }),
    ).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: en.nav.workspace })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
