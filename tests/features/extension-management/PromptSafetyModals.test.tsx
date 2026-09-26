import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Extension } from "@/entities/extension";
import { PromptRemovalModal } from "@/features/extension-management";
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
    expect(dialog).toHaveTextContent(en.extensions.prompt.remove.description);
    expect(within(dialog).queryByRole("list")).toBeNull();
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
});
