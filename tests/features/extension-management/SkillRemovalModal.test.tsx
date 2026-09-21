import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { delay, http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Extension } from "@/entities/extension";
import { SkillRemovalModal } from "@/features/extension-management";
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
  id: "anthropics/skills:skills/code-review",
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
      <SkillRemovalModal
        skill={SKILL}
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onOpenChange}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    ),
  };
}

describe("SkillRemovalModal", () => {
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

  it("defaults focus to Cancel and explains every removal boundary", async () => {
    mount();
    const dialog = screen.getByRole("dialog", { name: "Remove Code review?" });
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toHaveFocus();
    expect(dialog).toHaveTextContent(
      en.extensions.skill.remove.point.everywhere,
    );
    expect(dialog).toHaveTextContent(en.extensions.skill.remove.point.recovery);
    expect(dialog).toHaveTextContent(en.extensions.skill.remove.point.external);
  });

  it("keeps Cancel available but blocks removal when tool authority is stale", () => {
    mount(vi.fn(), true);
    const dialog = screen.getByRole("dialog", { name: "Remove Code review?" });
    expect(
      within(dialog).getByRole("alert", {
        name: en.extensions.actionsPaused.title,
      }),
    ).toHaveTextContent(en.extensions.actionsPaused.description);
    expect(
      within(dialog).getByRole("button", {
        name: en.extensions.skill.remove.confirm,
      }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeEnabled();
  });

  it("queues only the stable target and closes after the background task starts", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_remove`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json("op-remove-1");
      }),
    );
    const { onOpenChange } = mount();
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.skill.remove.confirm,
      }),
    );

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(body).toEqual({ tool: "claude-code", skill: SKILL.id });
    expect(toastMocks.info).toHaveBeenCalledWith("Removing Code review", {
      description: "You can keep working and follow progress in Tasks.",
    });
  });

  it("blocks buttons and Escape while the start request is in flight", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_remove`, async () => {
        await delay("infinite");
        return HttpResponse.json("never");
      }),
    );
    const { onOpenChange } = mount();
    const confirm = screen.getByRole("button", {
      name: en.extensions.skill.remove.confirm,
    });
    await userEvent.click(confirm);
    expect(confirm).toBeDisabled();
    expect(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    ).toBeDisabled();
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it("keeps a safe start failure inline and hides technical details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_remove`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "UNINSTALL_FAILED",
            messageKey: "error.skill.removeFailed",
            technicalMessage: "/private/path token=do-not-show",
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
        name: en.extensions.skill.remove.confirm,
      }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(en.error.skill.removeFailed);
    expect(alert).toHaveTextContent(en.extensions.skill.remove.errorRetry);
    expect(alert).not.toHaveTextContent("/private/path");
    expect(alert).not.toHaveTextContent("do-not-show");
  });
});
