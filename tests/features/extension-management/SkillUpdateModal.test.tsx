import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Extension } from "@/entities/extension";
import { SkillUpdateModal } from "@/features/extension-management";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const toastMocks = vi.hoisted(() => ({ info: vi.fn() }));
vi.mock("sonner", () => ({ toast: toastMocks }));

const SKILL: Extension = {
  kind: "skill",
  id: "anthropics/skills:code-review",
  scope: { kind: "tool", id: "claude-code" },
  name: "Code review",
  description: "Reviews a change before it ships.",
  management: "managed",
  enabled: true,
  canDisable: true,
};

function mount(onOpenChange = vi.fn(), mutationsBlocked = false) {
  return {
    onOpenChange,
    ...render(
      <SkillUpdateModal
        skill={SKILL}
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onOpenChange}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    ),
  };
}

describe("SkillUpdateModal", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, extensions: en.extensions },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.info.mockClear();
  });

  it("defaults to Cancel and explains download, backup, and tool sync", () => {
    mount();
    const dialog = screen.getByRole("dialog", { name: "Update Code review?" });
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toHaveFocus();
    expect(dialog).toHaveTextContent(en.extensions.skill.update.point.download);
    expect(dialog).toHaveTextContent(en.extensions.skill.update.point.backup);
    expect(dialog).toHaveTextContent(en.extensions.skill.update.point.tools);
  });

  it("queues only the tool and stable Skill id, then closes", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_update`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json("op-update-1");
      }),
    );
    const { onOpenChange } = mount();
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.skill.update.confirm,
      }),
    );
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(body).toEqual({ tool: "claude-code", skill: SKILL.id });
    expect(toastMocks.info).toHaveBeenCalledWith("Updating Code review", {
      description: "You can keep working and follow progress in Tasks.",
    });
  });

  it("shows a safe native failure and never exposes technical details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_update`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "UPDATE_FAILED",
            messageKey: "error.skill.updateFailed",
            technicalMessage: "/private/repo token=do-not-show",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    mount();
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.skill.update.confirm,
      }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(en.error.skill.updateFailed);
    expect(alert).toHaveTextContent(en.extensions.skill.update.errorRetry);
    expect(alert).not.toHaveTextContent("/private/repo");
    expect(alert).not.toHaveTextContent("do-not-show");
  });
});
