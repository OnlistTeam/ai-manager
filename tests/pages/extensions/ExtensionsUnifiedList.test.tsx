import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Extension, ExtensionScope } from "@/entities/extension";
import {
  unifiedExtensionRows,
  type ExtensionScopeOption,
} from "@/features/extension-management";
import en from "@/i18n/locales/en.json";
import { ExtensionsUnifiedList } from "@/pages/extensions/ExtensionsUnifiedList";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const toastMocks = vi.hoisted(() => ({ success: vi.fn(), info: vi.fn() }));
vi.mock("sonner", () => ({ toast: toastMocks }));

const CLAUDE: ExtensionScope = { kind: "tool", id: "claude-code" };
const CODEX: ExtensionScope = { kind: "tool", id: "codex" };

function target(scope: ExtensionScope, name: string): ExtensionScopeOption {
  return {
    key: `${scope.kind}:${scope.id}`,
    scope,
    name,
    supported: true,
    tool: null,
    desktopApp: null,
  };
}

const TARGETS = [target(CLAUDE, "Claude Code"), target(CODEX, "Codex")];

function entry(overrides: Partial<Extension>): Extension {
  return {
    kind: "mcp",
    id: "files-a1b2c3d4",
    scope: CLAUDE,
    name: "Files",
    description: null,
    detail: null,
    portability: null,
    management: "managed",
    enabled: true,
    canDisable: true,
    ...overrides,
  };
}

/** The same item as each app lists it: on in `on`, off elsewhere. */
function everywhere(item: Partial<Extension>, on: readonly ExtensionScope[]) {
  return TARGETS.map((option) => [
    entry({
      ...item,
      scope: option.scope,
      enabled: on.some((scope) => scope.id === option.scope.id),
    }),
  ]);
}

function mount(kind: "skill" | "mcp", lists: Extension[][], onEdit = vi.fn()) {
  render(
    <ExtensionsUnifiedList
      kind={kind}
      rows={unifiedExtensionRows(lists)}
      targets={TARGETS}
      operations={[]}
      actionsBlocked={false}
      skillUpdateIds={new Set()}
      onRemove={vi.fn()}
      onUpdate={vi.fn()}
      onEdit={onEdit}
    />,
    { wrapper: withQueryClient(createTestQueryClient()) },
  );
  return { onEdit };
}

describe("ExtensionsUnifiedList rows (ADR-0062)", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, extensions: en.extensions },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.success.mockClear();
  });

  it("names every app column above the switches, in their order", () => {
    mount("mcp", everywhere({}, [CLAUDE]));
    const columns = document.querySelectorAll("[data-scope-column]");
    expect([...columns].map((column) => column.textContent)).toEqual([
      "Claude",
      "Codex",
    ]);
    expect(
      [...columns].map((column) => column.getAttribute("data-scope-column")),
    ).toEqual(["tool:claude-code", "tool:codex"]);
  });

  it("shows the command line under an MCP row and opens it for editing", async () => {
    const { onEdit } = mount(
      "mcp",
      everywhere({ detail: "npx -y @playwright/mcp@latest" }, [CLAUDE]),
    );
    const row = screen.getByRole("article", { name: "Files" });
    expect(row).toHaveTextContent("npx -y @playwright/mcp@latest");

    await userEvent.click(
      within(row).getByRole("button", {
        name: "Edit Files, item 1 of 1",
      }),
    );
    expect(onEdit).toHaveBeenCalledWith({
      id: "files-a1b2c3d4",
      name: "Files",
      scope: CLAUDE,
      found: [],
    });
  });

  it("edits a found MCP connection by taking it over from every app it is in", async () => {
    const found = [
      [entry({ management: "detected", canDisable: false, scope: CLAUDE })],
      [entry({ management: "detected", canDisable: false, scope: CODEX })],
    ];
    const { onEdit } = mount("mcp", found);
    const row = screen.getByRole("article", { name: "Files" });
    expect(within(row).queryByRole("button", { name: /^Remove/ })).toBeNull();
    await userEvent.click(
      within(row).getByRole("button", { name: "Edit Files, item 1 of 1" }),
    );
    expect(onEdit).toHaveBeenCalledWith(
      expect.objectContaining({ found: [CLAUDE, CODEX] }),
    );
  });

  it("opens a managed Skill's stored folder and SKILL.md by id only", async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_resource_open`,
        async ({ request }) => {
          const body = (await request.json()) as { action: string };
          bodies.push(body);
          return HttpResponse.json(
            body.action === "edit" ? "editorOpened" : "folderOpened",
          );
        },
      ),
    );
    mount(
      "skill",
      everywhere(
        {
          kind: "skill",
          id: "anthropics/skills:pdf",
          name: "PDF",
          detail: "~/.agents/skills/pdf · anthropics/skills",
        },
        [CLAUDE],
      ),
    );
    const row = screen.getByRole("article", { name: "PDF" });
    expect(row).toHaveTextContent("~/.agents/skills/pdf · anthropics/skills");

    await userEvent.click(
      within(row).getByRole("button", { name: "Open the location of PDF" }),
    );
    await waitFor(() => expect(bodies).toHaveLength(1));
    await userEvent.click(
      within(row).getByRole("button", { name: "Edit the SKILL.md for PDF" }),
    );
    await waitFor(() =>
      expect(bodies).toEqual([
        { skill: "anthropics/skills:pdf", action: "browse" },
        { skill: "anthropics/skills:pdf", action: "edit" },
      ]),
    );
    expect(toastMocks.success).toHaveBeenCalledTimes(2);
  });

  it("warns once when the item is on in an app it may not work in", () => {
    const portability = {
      reason: "envReference" as const,
      worksIn: [CLAUDE],
    };
    mount("mcp", everywhere({ portability }, [CLAUDE, CODEX]));
    const row = screen.getByRole("article", { name: "Files" });
    const sentence = i18n.t("extensions.portability.envReference", {
      tools: ["Codex"],
    });
    expect(within(row).getAllByRole("button", { name: sentence })).toHaveLength(
      1,
    );
    expect(sentence).toContain("Codex");
  });

  it("stays quiet while the item is only on where it works, but cautions the other switch", () => {
    const portability = {
      reason: "toolHome" as const,
      worksIn: [CLAUDE],
    };
    mount("mcp", everywhere({ portability }, [CLAUDE]));
    const row = screen.getByRole("article", { name: "Files" });
    expect(
      within(row).queryByRole("button", {
        name: /another app's folder/,
      }),
    ).toBeNull();
    expect(
      within(row).getByRole("button", {
        name: new RegExp(en.extensions.portability.switchCaution),
        pressed: false,
      }),
    ).toBeInTheDocument();
    expect(
      within(row)
        .getByRole("button", { pressed: true })
        .getAttribute("aria-label"),
    ).not.toContain(en.extensions.portability.switchCaution);
  });
});
