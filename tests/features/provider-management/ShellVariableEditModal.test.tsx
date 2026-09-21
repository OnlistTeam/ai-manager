import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";

import en from "@/i18n/locales/en.json";
import { ShellVariableEditModal } from "@/features/provider-management";
import type { ShellVariableLocation } from "@/native";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const LOCATION: ShellVariableLocation = {
  variable: "ANTHROPIC_BASE_URL",
  path: "~/.config/zsh/secrets.zsh",
  line: 14,
  value: "https://old.example.test",
  editable: true,
};

function mount(location: ShellVariableLocation | null = LOCATION) {
  const onOpenChange = vi.fn();
  render(
    <ShellVariableEditModal
      location={location}
      tool="claude-code"
      onOpenChange={onOpenChange}
    />,
    { wrapper: withQueryClient(createTestQueryClient()) },
  );
  return { onOpenChange };
}

describe("ShellVariableEditModal", () => {
  beforeEach(async () => {
    i18n.addResourceBundle("en", "translation", en, true, true);
    await i18n.changeLanguage("en");
  });

  /**
   * The file and the line are shown rather than implied: this is the user's own
   * dotfile, and they are entitled to know which line is about to change.
   */
  it("names the file and line, and prefills the current value", async () => {
    mount();
    const dialog = await screen.findByRole("dialog");

    expect(
      within(dialog).getByText(
        en.services.shellVariable.at
          .replace("{{path}}", "~/.config/zsh/secrets.zsh")
          .replace("{{line}}", "14"),
      ),
    ).toBeVisible();
    expect(
      within(dialog).getByLabelText(en.services.shellVariable.value),
    ).toHaveValue("https://old.example.test");
  });

  it("sends the old value along so a line that moved on can be refused", async () => {
    const written: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_shell_variable_write`,
        async ({ request }) => {
          written.push(await request.json());
          return HttpResponse.json({
            path: "~/.config/zsh/secrets.zsh",
            line: 14,
          });
        },
      ),
    );
    const { onOpenChange } = mount();
    const dialog = await screen.findByRole("dialog");
    const input = within(dialog).getByLabelText(
      en.services.shellVariable.value,
    );

    await userEvent.clear(input);
    await userEvent.type(input, "https://new.example.test");
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.services.shellVariable.save,
      }),
    );

    await waitFor(() => expect(written).toHaveLength(1));
    expect(written[0]).toEqual({
      tool: "claude-code",
      update: {
        variable: "ANTHROPIC_BASE_URL",
        line: 14,
        expectedValue: "https://old.example.test",
        newValue: "https://new.example.test",
      },
    });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("cannot save a value that was not changed", async () => {
    mount();
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByRole("button", {
        name: en.services.shellVariable.save,
      }),
    ).toBeDisabled();
  });

  /**
   * The one thing the dialog has to say and cannot show: the variable was read
   * when that terminal started, and nothing here reaches back into it.
   */
  it("warns that an open terminal keeps the old value", async () => {
    mount();
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByText(
        en.services.shellVariable.openTerminalsKeepTheOldValue,
      ),
    ).toBeVisible();
  });

  it("reports a refused write and says nothing changed", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_shell_variable_write`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "OPERATION_CONFLICT",
            messageKey: "error.shellVariable.movedOn",
            technicalMessage:
              "the located line no longer holds the expected value",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    mount();
    const dialog = await screen.findByRole("dialog");
    const input = within(dialog).getByLabelText(
      en.services.shellVariable.value,
    );
    await userEvent.clear(input);
    await userEvent.type(input, "https://new.example.test");
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.services.shellVariable.save,
      }),
    );

    expect(
      await within(dialog).findByText(en.error.shellVariable.movedOn),
    ).toBeVisible();
    // No technical detail on screen, and the dialog stays open so the value
    // the user typed is not lost.
    expect(dialog).not.toHaveTextContent("the located line");
  });

  it("renders nothing until a line is chosen", () => {
    mount(null);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
