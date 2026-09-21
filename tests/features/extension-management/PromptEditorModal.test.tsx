import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { delay, http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Extension } from "@/entities/extension";
import { PromptEditorModal } from "@/features/extension-management";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const prompt: Extension = {
  kind: "prompt",
  id: "house-rules",
  scope: { kind: "tool", id: "claude-code" },
  name: "House rules",
  description: "Rules for every project.",
  management: "managed",
  enabled: true,
  canDisable: false,
};

const inventory = [{ ...prompt, name: "Updated rules" }];

function mount(
  editing: Extension | null = null,
  onOpenChange = vi.fn(),
  mutationsBlocked = false,
) {
  return {
    onOpenChange,
    ...render(
      <PromptEditorModal
        open
        tool="claude-code"
        toolName="Claude Code"
        prompt={editing}
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onOpenChange}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    ),
  };
}

describe("PromptEditorModal", () => {
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

  it("opens a beginner form without fetching a path or raw configuration", () => {
    const { container } = mount();
    const dialog = screen.getByRole("dialog", {
      name: en.extensions.prompt.editor.createTitle,
    });
    expect(
      within(dialog).getByLabelText(en.extensions.prompt.editor.name),
    ).toHaveFocus();
    expect(dialog).toHaveTextContent(en.extensions.prompt.editor.contentHint);
    expect(container.querySelector("pre, code")).toBeNull();
    expect(dialog).not.toHaveTextContent("CLAUDE.md");
    expect(dialog).not.toHaveTextContent("/Users/");
  });

  it("validates locally, then creates with no renderer-generated id", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_save`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(inventory);
      }),
    );
    const { onOpenChange } = mount();
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.prompt.editor.save,
      }),
    );
    expect(await screen.findAllByRole("alert")).toHaveLength(2);
    expect(body).toBeUndefined();

    await userEvent.type(
      screen.getByLabelText(en.extensions.prompt.editor.name),
      "Team rules",
    );
    await userEvent.type(
      screen.getByLabelText(en.extensions.prompt.editor.optionalDescription),
      "Shared instructions",
    );
    await userEvent.type(
      screen.getByLabelText(en.extensions.prompt.editor.content),
      "# Rules{enter}{enter}Run tests.",
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.prompt.editor.save,
      }),
    );

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(body).toEqual({
      tool: "claude-code",
      prompt: null,
      draft: {
        name: "Team rules",
        description: "Shared instructions",
        content: "# Rules\n\nRun tests.",
      },
    });
    expect(body).not.toHaveProperty("id");
    expect(JSON.stringify(body)).not.toContain("path");
  });

  it("loads explicit detail for editing and warns before changing the active file", async () => {
    const requests: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_get`, async ({ request }) => {
        requests.push(await request.json());
        return HttpResponse.json({
          id: prompt.id,
          tool: prompt.scope.kind === "tool" ? prompt.scope.id : "claude-code",
          name: prompt.name,
          description: prompt.description,
          content: "# Original rules",
          enabled: true,
        });
      }),
      http.post(`${TAURI_ENDPOINT}/app_prompt_save`, async ({ request }) => {
        requests.push(await request.json());
        return HttpResponse.json(inventory);
      }),
    );
    mount(prompt);
    expect(
      await screen.findByDisplayValue("# Original rules"),
    ).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toHaveTextContent(
      en.extensions.prompt.editor.activeBackup,
    );
    const name = screen.getByLabelText(en.extensions.prompt.editor.name);
    await userEvent.clear(name);
    await userEvent.type(name, "Updated rules");
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.prompt.editor.save,
      }),
    );
    await waitFor(() =>
      expect(requests.some((request) => "draft" in (request as object))).toBe(
        true,
      ),
    );
    expect(requests[0]).toEqual({
      tool: "claude-code",
      prompt: "house-rules",
    });
    const saveRequest = requests.find(
      (request) => "draft" in (request as object),
    );
    expect(saveRequest).toMatchObject({
      tool: "claude-code",
      prompt: "house-rules",
      draft: { name: "Updated rules", content: "# Original rules" },
    });
  });

  it("keeps safe save failures inline without revealing technical details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_save`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.prompt.saveRestoreFailed",
            technicalMessage: "/private/CLAUDE.md secret=do-not-show",
            remediation: "error.remediation.checkPromptSettings",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    mount();
    await userEvent.type(
      screen.getByLabelText(en.extensions.prompt.editor.name),
      "Rules",
    );
    await userEvent.type(
      screen.getByLabelText(en.extensions.prompt.editor.content),
      "Run tests.",
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.prompt.editor.save,
      }),
    );
    const alert = await screen.findByText(en.error.prompt.saveRestoreFailed);
    expect(alert).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toHaveTextContent(
      en.error.remediation.checkPromptSettings,
    );
    expect(screen.getByRole("dialog")).not.toHaveTextContent("/private/");
    expect(screen.getByRole("dialog")).not.toHaveTextContent("do-not-show");
  });

  it("keeps the draft editable but blocks save when authority is stale", async () => {
    mount(null, vi.fn(), true);
    expect(
      screen.getByRole("alert", { name: en.extensions.actionsPaused.title }),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText(en.extensions.prompt.editor.name),
    ).toBeEnabled();
    expect(
      screen.getByRole("button", {
        name: en.extensions.prompt.editor.save,
      }),
    ).toBeDisabled();
  });

  it("locks every exit while the save request is pending", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_save`, async () => {
        await delay("infinite");
        return HttpResponse.json([]);
      }),
    );
    const { onOpenChange } = mount();
    await userEvent.type(
      screen.getByLabelText(en.extensions.prompt.editor.name),
      "Rules",
    );
    await userEvent.type(
      screen.getByLabelText(en.extensions.prompt.editor.content),
      "Run tests.",
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.prompt.editor.save,
      }),
    );
    expect(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    ).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: en.ds.action.close }),
    ).toBeNull();
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
  });
});
