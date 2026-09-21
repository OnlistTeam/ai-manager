import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { importKeys } from "@/entities/import";
import en from "@/i18n/locales/en.json";
import { ImportPromptModal } from "@/features/import-existing";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const SUMMARY = { services: 12, mcpServers: 8, skills: 15 };
const REFRESH_ERROR_TITLE = "Could not refresh the existing setup";
const DECISION_ERROR_TITLE = "Could not save this choice";
const DECISION_ERROR_DESCRIPTION =
  "Try again. This step stays open until AI Manager saves your choice.";

function mount(onResolve = vi.fn(async () => undefined)) {
  const client = createTestQueryClient();
  const view = render(<ImportPromptModal enabled onResolve={onResolve} />, {
    wrapper: withQueryClient(client),
  });
  return { ...view, client, onResolve };
}

describe("ImportPromptModal", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, preferences: en.preferences },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({ available: true, summary: SUMMARY }),
      ),
    );
  });

  it("has exactly Import and Skip decisions and cannot be dismissed around them", async () => {
    const { onResolve } = mount();
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getAllByRole("button")).toHaveLength(2);
    expect(
      within(dialog).getByRole("button", {
        name: en.preferences.import.action,
      }),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", {
        name: en.preferences.import.skip,
      }),
    ).toBeInTheDocument();
    expect(within(dialog).queryByLabelText(en.ds.action.close)).toBeNull();

    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.preferences.import.skip,
      }),
    );
    expect(onResolve).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("keeps a durable completion state and resolves only when the user continues", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_run`, () =>
        HttpResponse.json({ imported: SUMMARY }),
      ),
    );
    const { onResolve } = mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.import.action,
      }),
    );

    expect(
      await screen.findByRole("heading", {
        name: en.preferences.import.completion.title,
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(en.preferences.import.completion.nextStep),
    ).toBeInTheDocument();
    expect(onResolve).not.toHaveBeenCalled();
    const continueButton = screen.getByRole("button", {
      name: en.preferences.import.completion.continue,
    });
    await waitFor(() => expect(continueButton).toHaveFocus());

    await userEvent.click(continueButton);
    expect(onResolve).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("keeps completion recoverable until its continue choice is saved", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_run`, () =>
        HttpResponse.json({ imported: SUMMARY }),
      ),
    );
    let saves = 0;
    const onResolve = vi.fn(async () => {
      saves += 1;
      if (saves === 1) throw new Error("private completed settings path");
    });
    mount(onResolve);
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.import.action,
      }),
    );
    const dialog = await screen.findByRole("dialog");
    const continueButton = within(dialog).getByRole("button", {
      name: en.preferences.import.completion.continue,
    });

    await userEvent.click(continueButton);
    expect(
      await within(dialog).findByRole("alert", {
        name: DECISION_ERROR_TITLE,
      }),
    ).toHaveTextContent(DECISION_ERROR_DESCRIPTION);
    expect(
      within(dialog).getByRole("heading", {
        name: en.preferences.import.completion.title,
      }),
    ).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent(
      "private completed settings path",
    );
    await waitFor(() => expect(continueButton).toHaveFocus());

    await userEvent.click(continueButton);
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(onResolve).toHaveBeenCalledTimes(2);
  });

  it("stays open on failure and never renders technical detail", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_run`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "UPSTREAM_ERROR",
            messageKey: "error.import.mergeFailed",
            technicalMessage: "secret-key-at-/Users/alice",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const { onResolve } = mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.import.action,
      }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.error.import.mergeFailed,
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      en.preferences.import.error.retry,
    );
    expect(screen.getByRole("alert")).not.toHaveTextContent(
      en.error.remediation.retryOrViewDetails,
    );
    expect(document.body).not.toHaveTextContent("secret-key-at-/Users/alice");
    expect(onResolve).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: en.preferences.import.retryAction,
      }),
    ).toBeInTheDocument();
  });

  it("explains both safety boundaries before either decision", async () => {
    mount();
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByRole("region", {
        name: en.preferences.import.safety.title,
      }),
    ).toHaveTextContent(en.preferences.import.safety.sourceUntouched);
    expect(dialog).toHaveTextContent(en.preferences.import.safety.backupFirst);
  });

  it("keeps both decisions locked until a skip choice is saved", async () => {
    let releaseSave!: () => void;
    const onResolve = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          releaseSave = resolve;
        }),
    );
    mount(onResolve);
    const dialog = await screen.findByRole("dialog");
    const skip = within(dialog).getByRole("button", {
      name: en.preferences.import.skip,
    });
    const importButton = within(dialog).getByRole("button", {
      name: en.preferences.import.action,
    });

    await userEvent.click(skip);
    await waitFor(() => expect(onResolve).toHaveBeenCalledTimes(1));
    expect(skip).toHaveAttribute("aria-busy", "true");
    expect(skip).toBeDisabled();
    expect(importButton).toBeDisabled();
    expect(dialog).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(dialog).toBeInTheDocument();

    releaseSave();
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
  });

  it("keeps the choice open with a safe retry when persistence fails", async () => {
    let saves = 0;
    const onResolve = vi.fn(async () => {
      saves += 1;
      if (saves === 1) throw new Error("private settings path");
    });
    mount(onResolve);
    const dialog = await screen.findByRole("dialog");
    const skip = within(dialog).getByRole("button", {
      name: en.preferences.import.skip,
    });

    await userEvent.click(skip);
    const alert = await within(dialog).findByRole("alert", {
      name: DECISION_ERROR_TITLE,
    });
    expect(alert).toHaveTextContent(DECISION_ERROR_DESCRIPTION);
    expect(dialog).toBeInTheDocument();
    expect(skip).toBeEnabled();
    expect(document.body).not.toHaveTextContent("private settings path");

    await userEvent.click(skip);
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(onResolve).toHaveBeenCalledTimes(2);
  });

  it("retains the review and pauses import after discovery refresh failure", async () => {
    let checks = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, async () => {
        checks += 1;
        if (checks === 2) {
          return HttpResponse.text("private prompt path", { status: 500 });
        }
        if (checks === 3) {
          await new Promise<void>((resolve) => {
            releaseRetry = resolve;
          });
        }
        return HttpResponse.json({ available: true, summary: SUMMARY });
      }),
    );
    const { client } = mount();
    const dialog = await screen.findByRole("dialog");

    await client.invalidateQueries({ queryKey: importKeys.all });
    const warning = await within(dialog).findByRole("alert", {
      name: REFRESH_ERROR_TITLE,
    });
    const retry = within(warning).getByRole("button", {
      name: en.preferences.import.retry,
    });
    const importButton = within(dialog).getByRole("button", {
      name: en.preferences.import.action,
    });
    expect(dialog).toHaveTextContent("12 AI Services");
    expect(importButton).toBeDisabled();
    expect(
      within(dialog).getByRole("button", {
        name: en.preferences.import.skip,
      }),
    ).toBeEnabled();
    expect(document.body).not.toHaveTextContent("private prompt path");

    await userEvent.click(retry);
    await waitFor(() => expect(checks).toBe(3));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(importButton).toBeDisabled();
    releaseRetry();

    await waitFor(() => expect(warning).not.toBeInTheDocument());
    await waitFor(() => expect(importButton).toBeEnabled());
    await waitFor(() => expect(importButton).toHaveFocus());
  });

  it("keeps completion visible if discovery later fails", async () => {
    let checks = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () => {
        checks += 1;
        return checks === 1
          ? HttpResponse.json({ available: true, summary: SUMMARY })
          : HttpResponse.text("private completed path", { status: 500 });
      }),
      http.post(`${TAURI_ENDPOINT}/app_import_run`, () =>
        HttpResponse.json({ imported: SUMMARY }),
      ),
    );
    const { client } = mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.import.action,
      }),
    );
    expect(
      await screen.findByRole("heading", {
        name: en.preferences.import.completion.title,
      }),
    ).toBeInTheDocument();

    await client.invalidateQueries({ queryKey: importKeys.all });
    await waitFor(() =>
      expect(client.getQueryState(importKeys.preview())?.status).toBe("error"),
    );
    expect(
      screen.getByRole("heading", {
        name: en.preferences.import.completion.title,
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: en.preferences.import.completion.continue,
      }),
    ).toBeEnabled();
    expect(document.body).not.toHaveTextContent("private completed path");
  });
});
