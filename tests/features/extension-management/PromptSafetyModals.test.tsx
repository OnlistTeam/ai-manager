import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Extension } from "@/entities/extension";
import {
  PromptImportModal,
  PromptRemovalModal,
} from "@/features/extension-management";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const prompt: Extension = {
  kind: "prompt",
  id: "draft-rules",
  scope: { kind: "tool", id: "claude-code" },
  name: "Draft rules",
  description: null,
  management: "managed",
  enabled: false,
  canDisable: false,
};

const wrapper = () => withQueryClient(createTestQueryClient());

describe("Prompt safety confirmations", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, extensions: en.extensions },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("defaults removal focus to Cancel and explains that live instructions stay untouched", () => {
    render(
      <PromptRemovalModal
        prompt={prompt}
        mutationsBlocked={false}
        onOpenChange={vi.fn()}
      />,
      { wrapper: wrapper() },
    );
    const dialog = screen.getByRole("dialog", {
      name: en.extensions.prompt.remove.title.replace("{{name}}", prompt.name),
    });
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toHaveFocus();
    expect(dialog).toHaveTextContent(en.extensions.prompt.remove.point.current);
    expect(dialog).toHaveTextContent(en.extensions.prompt.remove.point.file);
    expect(dialog).toHaveTextContent(en.extensions.prompt.remove.point.undo);
  });

  it("removes only by stable id and closes after authoritative refresh", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_remove`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json([]);
      }),
    );
    const onOpenChange = vi.fn();
    render(
      <PromptRemovalModal
        prompt={prompt}
        mutationsBlocked={false}
        onOpenChange={onOpenChange}
      />,
      { wrapper: wrapper() },
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.prompt.remove.confirm,
      }),
    );
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(body).toEqual({ tool: "claude-code", prompt: "draft-rules" });
    expect(JSON.stringify(body)).not.toContain("content");
  });

  it("imports only after confirmation and sends no file path or content", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_prompt_import_current`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json([prompt]);
        },
      ),
    );
    const onOpenChange = vi.fn();
    render(
      <PromptImportModal
        open
        tool="claude-code"
        toolName="Claude Code"
        mutationsBlocked={false}
        onOpenChange={onOpenChange}
      />,
      { wrapper: wrapper() },
    );
    const dialog = screen.getByRole("dialog", {
      name: en.extensions.prompt.import.title,
    });
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toHaveFocus();
    expect(body).toBeUndefined();
    expect(dialog).toHaveTextContent(
      en.extensions.prompt.import.point.unchanged,
    );
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.extensions.prompt.import.confirm,
      }),
    );
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(body).toEqual({ tool: "claude-code" });
    expect(JSON.stringify(body)).not.toMatch(/path|content|file/i);
  });

  it("keeps import failures inline and redacts native technical details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_import_current`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_PARSE_FAILED",
            messageKey: "error.prompt.importMissing",
            technicalMessage: "/Users/alice/.claude/CLAUDE.md",
            remediation: "error.remediation.openToolManually",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    render(
      <PromptImportModal
        open
        tool="claude-code"
        toolName="Claude Code"
        mutationsBlocked={false}
        onOpenChange={vi.fn()}
      />,
      { wrapper: wrapper() },
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.prompt.import.confirm,
      }),
    );
    const dialog = screen.getByRole("dialog");
    expect(
      await screen.findByText(en.error.prompt.importMissing),
    ).toBeInTheDocument();
    expect(dialog).not.toHaveTextContent("/Users/alice");
    expect(dialog).not.toHaveTextContent("CLAUDE.md");
  });
});
