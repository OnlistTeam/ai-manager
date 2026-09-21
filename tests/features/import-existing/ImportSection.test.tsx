import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { importKeys } from "@/entities/import";
import en from "@/i18n/locales/en.json";
import { ImportSection } from "@/features/import-existing";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const SUMMARY = { services: 2, mcpServers: 3, skills: 4 };
const REFRESH_ERROR_TITLE = "Could not refresh the existing setup";
const REFRESH_ERROR_DESCRIPTION =
  "The last import summary is still shown. Import stays paused until a new check succeeds.";

function mount() {
  const client = createTestQueryClient();
  const view = render(<ImportSection />, {
    wrapper: withQueryClient(client),
  });

  return { ...view, client };
}

describe("ImportSection", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        common: en.common,
        ds: en.ds,
        error: en.error,
        preferences: en.preferences,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("uses an explicit local detection state for the first preview read", () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_import_preview`,
        () => new Promise<Response>(() => undefined),
      ),
    );
    mount();

    const status = screen.getByRole("status", {
      name: en.preferences.import.loading,
    });
    expect(status).toHaveAttribute("aria-busy", "true");
    expect(status).toHaveTextContent(en.preferences.import.reimport);
    expect(status.querySelector(".animate-pulse")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("shows a truthful not-found state", async () => {
    let checks = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () => {
        checks += 1;
        return HttpResponse.json({
          available: false,
          summary: { services: 0, mcpServers: 0, skills: 0 },
        });
      }),
    );
    mount();
    const notFound = await screen.findByRole("status", {
      name: en.preferences.import.notFound.title,
    });
    expect(notFound).toHaveTextContent(
      en.preferences.import.notFound.description,
    );
    expect(
      within(notFound).queryByRole("heading", {
        name: en.preferences.import.notFound.title,
      }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: en.preferences.import.action }),
    ).toBeNull();

    await userEvent.click(
      within(notFound).getByRole("button", {
        name: en.preferences.import.retry,
      }),
    );
    await waitFor(() => expect(checks).toBe(2));
  });

  it("keeps the row to one line and moves counts and safety into the review", async () => {
    let runCount = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({ available: true, summary: SUMMARY }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_import_run`, () => {
        runCount += 1;
        return HttpResponse.json({ imported: SUMMARY });
      }),
    );
    mount();
    const region = screen.getByRole("region", {
      name: en.preferences.import.title,
    });
    const importButton = await screen.findByRole("button", {
      name: en.preferences.import.action,
    });
    expect(region).toHaveTextContent(en.preferences.import.reimport);
    expect(region).toHaveTextContent(en.preferences.import.description);
    expect(region).not.toHaveTextContent("2 AI Services");
    expect(region).not.toHaveTextContent(en.preferences.import.collision);

    await userEvent.click(importButton);
    expect(runCount).toBe(0);
    const dialog = screen.getByRole("dialog", {
      name: en.preferences.import.confirm.title,
    });
    expect(dialog).toHaveTextContent(en.preferences.import.sourceDetail);
    expect(dialog).toHaveTextContent("2 AI Services");
    expect(dialog).toHaveTextContent("3 MCP Servers");
    expect(dialog).toHaveTextContent("4 Skills");
    expect(dialog).toHaveTextContent(
      en.preferences.import.safety.sourceUntouched,
    );
    expect(dialog).toHaveTextContent(en.preferences.import.safety.backupFirst);
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.preferences.import.action,
      }),
    );
    await waitFor(() => expect(runCount).toBe(1));
    expect(
      await screen.findByRole("status", {
        name: en.preferences.import.completion.title,
      }),
    ).toHaveTextContent(en.preferences.import.completion.description);
    expect(
      screen.getByRole("button", { name: en.preferences.import.again }),
    ).toBeInTheDocument();
  });

  it("keeps one discovery retry stable and restores section focus", async () => {
    let checks = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, async () => {
        checks += 1;
        if (checks === 1) {
          return HttpResponse.text("private source path", { status: 500 });
        }
        await new Promise<void>((resolve) => {
          releaseRetry = resolve;
        });
        return HttpResponse.json({ available: true, summary: SUMMARY });
      }),
    );
    mount();
    const alert = await screen.findByRole("alert", {
      name: en.preferences.import.error.title,
    });
    const retry = within(alert).getByRole("button", {
      name: en.preferences.import.retry,
    });
    expect(
      screen.getAllByRole("button", { name: en.preferences.import.retry }),
    ).toHaveLength(1);
    expect(
      screen.queryByRole("button", { name: en.preferences.import.action }),
    ).toBeNull();
    expect(document.body).not.toHaveTextContent("private source path");

    await userEvent.click(retry);
    await waitFor(() => expect(checks).toBe(2));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toBeInTheDocument();
    expect(
      screen.queryByRole("status", {
        name: en.preferences.import.loading,
      }),
    ).toBeNull();

    releaseRetry();
    expect(
      await screen.findByRole("button", { name: en.preferences.import.action }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByRole("region", { name: en.preferences.import.title }),
      ).toHaveFocus(),
    );
  });

  it("retains a review and pauses import after discovery refresh failure", async () => {
    let checks = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, async () => {
        checks += 1;
        if (checks === 2) {
          return HttpResponse.text("private refresh path", { status: 500 });
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
    const importButton = await screen.findByRole("button", {
      name: en.preferences.import.action,
    });
    await userEvent.click(importButton);

    await client.invalidateQueries({ queryKey: importKeys.all });
    const dialog = screen.getByRole("dialog", {
      name: en.preferences.import.confirm.title,
    });
    const warning = await within(dialog).findByRole("alert", {
      name: REFRESH_ERROR_TITLE,
    });
    expect(warning).toHaveTextContent(REFRESH_ERROR_DESCRIPTION);
    expect(dialog).toHaveTextContent("2 AI Services");
    expect(
      within(dialog).getByRole("button", {
        name: en.preferences.import.action,
      }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeEnabled();
    expect(document.body).not.toHaveTextContent("private refresh path");

    await userEvent.click(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    );
    const pageWarning = await screen.findByRole("alert", {
      name: REFRESH_ERROR_TITLE,
    });
    expect(importButton).toBeDisabled();

    const retry = within(pageWarning).getByRole("button", {
      name: en.preferences.import.retry,
    });
    await userEvent.click(retry);
    await waitFor(() => expect(checks).toBe(3));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(importButton).toBeDisabled();

    releaseRetry();
    await waitFor(() => expect(pageWarning).not.toBeInTheDocument());
    await waitFor(() => expect(importButton).toBeEnabled());
    await waitFor(() =>
      expect(
        screen.getByRole("region", { name: en.preferences.import.title }),
      ).toHaveFocus(),
    );
  });

  it("returns the modal import action after a successful discovery retry", async () => {
    let checks = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, async () => {
        checks += 1;
        if (checks === 2) {
          return HttpResponse.text("private dialog path", { status: 500 });
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
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.import.action,
      }),
    );

    await client.invalidateQueries({ queryKey: importKeys.all });
    const dialog = screen.getByRole("dialog", {
      name: en.preferences.import.confirm.title,
    });
    const warning = await within(dialog).findByRole("alert", {
      name: REFRESH_ERROR_TITLE,
    });
    const retry = within(warning).getByRole("button", {
      name: en.preferences.import.retry,
    });
    const confirm = within(dialog).getByRole("button", {
      name: en.preferences.import.action,
    });

    await userEvent.click(retry);
    await waitFor(() => expect(checks).toBe(3));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(confirm).toBeDisabled();
    releaseRetry();

    await waitFor(() => expect(warning).not.toBeInTheDocument());
    await waitFor(() => expect(confirm).toBeEnabled());
    await waitFor(() => expect(confirm).toHaveFocus());
    expect(document.body).not.toHaveTextContent("private dialog path");
  });

  it("moves from review to the no-source retry when the source disappears", async () => {
    let checks = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () => {
        checks += 1;
        if (checks === 2) {
          return HttpResponse.text("private vanished path", { status: 500 });
        }
        return HttpResponse.json(
          checks === 1
            ? { available: true, summary: SUMMARY }
            : {
                available: false,
                summary: { services: 0, mcpServers: 0, skills: 0 },
              },
        );
      }),
    );
    const { client } = mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.import.action,
      }),
    );

    await client.invalidateQueries({ queryKey: importKeys.all });
    const dialog = screen.getByRole("dialog", {
      name: en.preferences.import.confirm.title,
    });
    await userEvent.click(
      await within(dialog).findByRole("button", {
        name: en.preferences.import.retry,
      }),
    );

    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    const notFound = await screen.findByRole("status", {
      name: en.preferences.import.notFound.title,
    });
    await waitFor(() =>
      expect(
        within(notFound).getByRole("button", {
          name: en.preferences.import.retry,
        }),
      ).toHaveFocus(),
    );
    expect(document.body).not.toHaveTextContent("private vanished path");
  });

  it("retains the no-source result with one focused retry after refresh failure", async () => {
    let checks = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, async () => {
        checks += 1;
        if (checks === 2) {
          return HttpResponse.text("private no-source path", { status: 500 });
        }
        if (checks === 3) {
          await new Promise<void>((resolve) => {
            releaseRetry = resolve;
          });
        }
        return HttpResponse.json({
          available: false,
          summary: { services: 0, mcpServers: 0, skills: 0 },
        });
      }),
    );
    mount();
    const notFound = await screen.findByRole("status", {
      name: en.preferences.import.notFound.title,
    });

    await userEvent.click(
      within(notFound).getByRole("button", {
        name: en.preferences.import.retry,
      }),
    );
    await waitFor(() => expect(checks).toBe(2));
    const warning = await screen.findByRole("alert", {
      name: REFRESH_ERROR_TITLE,
    });
    expect(notFound).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: en.preferences.import.retry }),
    ).toHaveLength(1);
    const retry = within(warning).getByRole("button", {
      name: en.preferences.import.retry,
    });
    expect(retry).toHaveFocus();
    expect(document.body).not.toHaveTextContent("private no-source path");

    await userEvent.click(retry);
    await waitFor(() => expect(checks).toBe(3));
    expect(retry).toHaveAttribute("aria-busy", "true");
    releaseRetry();

    await waitFor(() => expect(warning).not.toBeInTheDocument());
    expect(notFound).toBeInTheDocument();
    await waitFor(() =>
      expect(
        within(notFound).getByRole("button", {
          name: en.preferences.import.retry,
        }),
      ).toHaveFocus(),
    );
  });

  it("does not steal focus when the user leaves during a discovery retry", async () => {
    let checks = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, async () => {
        checks += 1;
        if (checks === 1) {
          return HttpResponse.text("private source path", { status: 500 });
        }
        await new Promise<void>((resolve) => {
          releaseRetry = resolve;
        });
        return HttpResponse.json({ available: true, summary: SUMMARY });
      }),
    );
    const client = createTestQueryClient();
    render(
      <>
        <ImportSection />
        <button type="button">Outside import</button>
      </>,
      { wrapper: withQueryClient(client) },
    );
    const retry = await screen.findByRole("button", {
      name: en.preferences.import.retry,
    });
    const outside = screen.getByRole("button", { name: "Outside import" });

    await userEvent.click(retry);
    await waitFor(() => expect(checks).toBe(2));
    await userEvent.click(outside);
    expect(outside).toHaveFocus();
    releaseRetry();

    expect(
      await screen.findByRole("button", { name: en.preferences.import.action }),
    ).toBeInTheDocument();
    expect(outside).toHaveFocus();
  });

  it("keeps a completed import visible when discovery later fails", async () => {
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
    await userEvent.click(
      screen.getByRole("button", { name: en.preferences.import.action }),
    );
    expect(
      await screen.findByRole("status", {
        name: en.preferences.import.completion.title,
      }),
    ).toBeInTheDocument();

    await client.invalidateQueries({ queryKey: importKeys.all });
    expect(
      await screen.findByRole("alert", { name: REFRESH_ERROR_TITLE }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("status", {
        name: en.preferences.import.completion.title,
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.preferences.import.again }),
    ).toBeDisabled();
    expect(document.body).not.toHaveTextContent("private completed path");
  });

  it("starts review on the safe choice and returns focus when cancelled", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({ available: true, summary: SUMMARY }),
      ),
    );
    mount();
    const importButton = await screen.findByRole("button", {
      name: en.preferences.import.action,
    });
    await userEvent.click(importButton);

    const cancelButton = screen.getByRole("button", {
      name: en.ds.action.cancel,
    });
    await waitFor(() => expect(cancelButton).toHaveFocus());
    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(importButton).toHaveFocus();
  });

  it("keeps the review dialog locked while an import is running", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({ available: true, summary: SUMMARY }),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_import_run`,
        () => new Promise<Response>(() => undefined),
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.import.action,
      }),
    );
    const dialog = screen.getByRole("dialog", {
      name: en.preferences.import.confirm.title,
    });
    const importButton = within(dialog).getByRole("button", {
      name: en.preferences.import.action,
    });
    await userEvent.click(importButton);

    await waitFor(() => expect(importButton).toHaveAttribute("aria-busy"));
    expect(importButton).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeDisabled();
    expect(
      within(dialog).queryByRole("button", { name: en.ds.action.close }),
    ).toBeNull();

    await userEvent.keyboard("{Escape}");
    expect(
      screen.getByRole("dialog", {
        name: en.preferences.import.confirm.title,
      }),
    ).toBeInTheDocument();
  });

  it("keeps a failed repeat import in its review dialog with a real recovery action", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({ available: true, summary: SUMMARY }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_import_run`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "UPSTREAM_ERROR",
            messageKey: "error.import.mergeFailed",
            technicalMessage: "secret path /Users/alice",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.import.action,
      }),
    );
    const dialog = screen.getByRole("dialog", {
      name: en.preferences.import.confirm.title,
    });
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.preferences.import.action,
      }),
    );

    const alert = await within(dialog).findByRole("alert");
    expect(alert).toHaveTextContent(en.error.import.mergeFailed);
    expect(alert).toHaveTextContent(en.preferences.import.error.retry);
    expect(alert).not.toHaveTextContent(
      en.error.remediation.retryOrViewDetails,
    );
    expect(document.body).not.toHaveTextContent("secret path /Users/alice");
    expect(
      within(dialog).getByRole("button", {
        name: en.preferences.import.retryAction,
      }),
    ).toBeInTheDocument();
  });
});
