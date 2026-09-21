import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { backupKeys } from "@/entities/backup";
import en from "@/i18n/locales/en.json";
import { backupLabel, BackupSection } from "@/features/backup";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

// `vi.mock` factories are hoisted to the top of the file, so any object they
// reference must be built with `vi.hoisted` (existing page tests in this repo
// follow the same pattern). A plain const here would throw a ReferenceError.
const toastMocks = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn() }));
vi.mock("sonner", () => ({ toast: toastMocks }));

const FILE = {
  name: "db_backup_20260819_101500.db",
  createdAt: "2026-08-19T10:15:00Z",
  sizeBytes: 2_202_009,
};
const LIST = { files: [FILE] };

function restoreButtonName(): string {
  return en.preferences.backup.restoreNamed.replace(
    "{{name}}",
    backupLabel(FILE, i18n.language),
  );
}

function deleteButtonName(): string {
  return en.preferences.backup.deleteNamed.replace(
    "{{name}}",
    backupLabel(FILE, i18n.language),
  );
}

function renameButtonName(): string {
  return en.preferences.backup.renameNamed.replace(
    "{{name}}",
    backupLabel(FILE, i18n.language),
  );
}

function mount(advancedMode = false, onReviewServices?: () => void) {
  const client = createTestQueryClient();
  const view = render(
    <BackupSection
      advancedMode={advancedMode}
      onReviewServices={onReviewServices}
    />,
    {
      wrapper: withQueryClient(client),
    },
  );

  return { ...view, client };
}

describe("BackupSection", () => {
  beforeEach(async () => {
    toastMocks.success.mockClear();
    toastMocks.error.mockClear();
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, preferences: en.preferences },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.json(LIST),
      ),
    );
  });

  it("announces that it is loading", () => {
    mount();
    expect(
      screen.getByRole("status", {
        name: en.preferences.backup.loading,
      }),
    ).toBeInTheDocument();
  });

  it("says when a backup was made, never what the file is called", async () => {
    mount();
    expect(
      await screen.findByText(en.preferences.backup.summary.title),
    ).toBeInTheDocument();
    expect(
      screen.getByText(en.preferences.backup.summary.title).closest(".ds-card"),
    ).toHaveClass("overflow-hidden", "rounded-xl");
    expect(
      screen.getByText(en.preferences.backup.summary.latestBadge),
    ).toBeInTheDocument();
    const latestSummary = screen.getByText(
      en.preferences.backup.summary.latest.replace(
        "{{name}}",
        backupLabel(FILE, i18n.language),
      ),
    );
    expect(latestSummary).toHaveClass("break-words");
    expect(latestSummary).not.toHaveClass("truncate");
    expect(screen.queryByText(/db_backup_/)).toBeNull();
    expect(screen.getByText(en.preferences.backup.folder)).not.toBeVisible();
  });

  it("keeps the storage note behind a small disclosure without any folder path", async () => {
    mount();
    const summary = await screen.findByText(en.preferences.backup.details);
    const folder = screen.getByText(en.preferences.backup.folder);
    expect(folder).not.toBeVisible();
    await userEvent.click(summary);
    expect(folder).toBeVisible();
    expect(folder).toHaveClass("break-words");
    expect(en.preferences.backup.folder).not.toContain("{{");
    expect(document.body).not.toHaveTextContent(/\/Users\//u);
  });

  it("offers one clear first-backup action inside a compact local-protection status", async () => {
    let created = false;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.json({ ...LIST, files: [] }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_backup_create`, () => {
        created = true;
        return HttpResponse.json(LIST);
      }),
    );
    mount();
    const empty = await screen.findByRole("status", {
      name: en.preferences.backup.empty.title,
    });
    expect(empty).toHaveTextContent(en.preferences.backup.empty.description);
    expect(empty).toHaveTextContent(en.preferences.backup.empty.local);
    expect(
      screen.queryByRole("heading", {
        name: en.preferences.backup.empty.title,
      }),
    ).toBeNull();
    expect(
      screen.getAllByRole("button", {
        name: en.preferences.backup.create,
      }),
    ).toHaveLength(1);

    await userEvent.click(
      within(empty).getByRole("button", {
        name: en.preferences.backup.create,
      }),
    );
    await waitFor(() => expect(created).toBe(true));
    expect(
      await screen.findByText(en.preferences.backup.completion.created.title),
    ).toBeInTheDocument();
  });

  it("says up front that backups run daily and lets the user switch them off", async () => {
    let received: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_backup_schedule_save`,
        async ({ request }) => {
          received = await request.json();
          return HttpResponse.json({ automatic: false, retainCount: 10 });
        },
      ),
    );
    mount();
    const toggle = await screen.findByRole("switch", {
      name: en.preferences.backup.automatic,
    });
    expect(toggle).toBeChecked();
    expect(toggle).toHaveAccessibleDescription(
      en.preferences.backup.summaryLine_other.replace("{{count}}", "10"),
    );

    await userEvent.click(toggle);

    await waitFor(() =>
      expect(received).toEqual({
        schedule: { automatic: false, retainCount: 10 },
      }),
    );
    expect(toggle).not.toBeChecked();
    expect(
      screen.getByText(en.preferences.backup.automaticOff),
    ).toBeInTheDocument();
  });

  it("exports to another computer through the native picker and keeps cancellation quiet", async () => {
    let exports = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_export`, () => {
        exports += 1;
        return HttpResponse.json(
          exports === 1 ? { status: "cancelled" } : { status: "exported" },
        );
      }),
    );
    mount();
    const exportButton = await screen.findByRole("button", {
      name: en.preferences.backup.exportToOther,
    });
    expect(screen.queryByRole("dialog")).toBeNull();

    await userEvent.click(exportButton);
    await waitFor(() => expect(exports).toBe(1));
    expect(
      screen.queryByText(en.preferences.backup.completion.exported.title),
    ).toBeNull();

    await userEvent.click(exportButton);
    expect(
      await screen.findByText(en.preferences.backup.completion.exported.title),
    ).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("requires review before importing from another computer and reports a live-sync warning", async () => {
    let imports = 0;
    const onReviewServices = vi.fn();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_import`, () => {
        imports += 1;
        return HttpResponse.json({
          status: "imported",
          backups: LIST,
          toolsOutOfSync: true,
        });
      }),
    );
    mount(false, onReviewServices);
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.backup.importFromOther,
      }),
    );
    const dialog = screen.getByRole("dialog", {
      name: en.preferences.backup.transfer.confirmTitle,
    });
    expect(imports).toBe(0);
    expect(dialog).toHaveTextContent(
      en.preferences.backup.transfer.confirmReplace,
    );
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.preferences.backup.transfer.chooseAndImport,
      }),
    );

    const warning = await screen.findByRole("alert", {
      name: en.preferences.backup.completion.importWarning.title,
    });
    expect(imports).toBe(1);
    expect(warning).toHaveTextContent(
      en.preferences.backup.completion.importWarning.description,
    );
    await userEvent.click(
      within(warning).getByRole("button", {
        name: en.preferences.backup.completion.reviewServices,
      }),
    );
    expect(onReviewServices).toHaveBeenCalledTimes(1);
  });

  it("keeps one unavailable-list retry stable and restores section focus", async () => {
    let reads = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, async () => {
        reads += 1;
        if (reads === 1) {
          return HttpResponse.text(
            JSON.stringify({
              code: "UPSTREAM_ERROR",
              messageKey: "error.backup.listFailed",
              technicalMessage: "sqlite says no",
              remediation: "error.remediation.retryOrViewDetails",
              contextId: null,
            }),
            { status: 500 },
          );
        }
        await new Promise<void>((resolve) => {
          releaseRetry = resolve;
        });
        return HttpResponse.json(LIST);
      }),
    );
    mount();
    const alert = await screen.findByRole("alert", {
      name: en.preferences.backup.error.title,
    });
    const retry = within(alert).getByRole("button", {
      name: en.preferences.backup.retry,
    });
    expect(
      screen.getAllByRole("button", { name: en.preferences.backup.retry }),
    ).toHaveLength(1);
    expect(
      screen.queryByRole("button", { name: en.preferences.backup.create }),
    ).toBeNull();
    // Spec §42: not a single word of technical detail may reach the screen.
    expect(screen.queryByText(/sqlite/i)).toBeNull();

    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(2));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toBeInTheDocument();
    expect(
      screen.queryByRole("status", {
        name: en.preferences.backup.loading,
      }),
    ).toBeNull();

    releaseRetry();
    expect(
      await screen.findByText(en.preferences.backup.summary.title),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByRole("region", { name: en.preferences.backup.title }),
      ).toHaveFocus(),
    );
  });

  it("returns focus to the same retry after another read failure", async () => {
    let reads = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, async () => {
        reads += 1;
        if (reads === 2) {
          await new Promise<void>((resolve) => {
            releaseRetry = resolve;
          });
        }
        return HttpResponse.text("private backup path", { status: 500 });
      }),
    );
    mount();
    const alert = await screen.findByRole("alert", {
      name: en.preferences.backup.error.title,
    });
    const retry = within(alert).getByRole("button", {
      name: en.preferences.backup.retry,
    });

    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(2));
    releaseRetry();

    await waitFor(() => expect(retry).not.toHaveAttribute("aria-busy"));
    expect(retry).toHaveFocus();
    expect(alert).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("private backup path");
  });

  it("does not steal focus when the user leaves during a retry", async () => {
    let reads = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, async () => {
        reads += 1;
        if (reads === 1) {
          return HttpResponse.text("private backup path", { status: 500 });
        }
        await new Promise<void>((resolve) => {
          releaseRetry = resolve;
        });
        return HttpResponse.json(LIST);
      }),
    );
    const client = createTestQueryClient();
    render(
      <>
        <BackupSection advancedMode={false} />
        <button type="button">Outside backup</button>
      </>,
      { wrapper: withQueryClient(client) },
    );
    const retry = await screen.findByRole("button", {
      name: en.preferences.backup.retry,
    });
    const outside = screen.getByRole("button", { name: "Outside backup" });

    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(2));
    await userEvent.click(outside);
    expect(outside).toHaveFocus();
    releaseRetry();

    expect(
      await screen.findByText(en.preferences.backup.summary.title),
    ).toBeInTheDocument();
    expect(outside).toHaveFocus();
  });

  it("retains the last backup list and pauses actions after refresh failure", async () => {
    let reads = 0;
    let releaseRetry!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, async () => {
        reads += 1;
        if (reads === 2) {
          return HttpResponse.text("private backup directory", { status: 500 });
        }
        if (reads === 3) {
          await new Promise<void>((resolve) => {
            releaseRetry = resolve;
          });
        }
        return HttpResponse.json(LIST);
      }),
    );
    const { client } = mount();
    const create = await screen.findByRole("button", {
      name: en.preferences.backup.create,
    });
    const restore = await screen.findByRole("button", {
      name: restoreButtonName(),
    });
    const remove = screen.getByRole("button", {
      name: deleteButtonName(),
    });

    await client.invalidateQueries({ queryKey: backupKeys.all });
    const alert = await screen.findByRole("alert", {
      name: en.preferences.backup.refreshError.title,
    });
    expect(reads).toBe(2);
    expect(alert).toHaveTextContent(
      en.preferences.backup.refreshError.description,
    );
    expect(
      screen.getByText(backupLabel(FILE, i18n.language)),
    ).toBeInTheDocument();
    expect(
      screen.getByText(en.preferences.backup.summary.title),
    ).toBeInTheDocument();
    expect(create).toBeDisabled();
    expect(restore).toBeDisabled();
    expect(remove).toBeDisabled();
    expect(document.body).not.toHaveTextContent("private backup directory");

    const retry = within(alert).getByRole("button", {
      name: en.preferences.backup.retry,
    });
    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(3));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toBeInTheDocument();
    expect(create).toBeDisabled();

    releaseRetry();
    await waitFor(() => expect(alert).not.toBeInTheDocument());
    await waitFor(() => expect(create).toBeEnabled());
    expect(restore).toBeEnabled();
    expect(remove).toBeEnabled();
    await waitFor(() =>
      expect(
        screen.getByRole("region", { name: en.preferences.backup.title }),
      ).toHaveFocus(),
    );
  });

  it("pauses the first-backup action when an empty list becomes stale", async () => {
    let reads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () => {
        reads += 1;
        return reads === 1
          ? HttpResponse.json({ ...LIST, files: [] })
          : HttpResponse.text("private empty backup directory", {
              status: 500,
            });
      }),
    );
    const { client } = mount();
    const create = await screen.findByRole("button", {
      name: en.preferences.backup.create,
    });

    await client.invalidateQueries({ queryKey: backupKeys.all });
    expect(
      await screen.findByRole("alert", {
        name: en.preferences.backup.refreshError.title,
      }),
    ).toBeInTheDocument();
    expect(create).toBeDisabled();
    expect(document.body).not.toHaveTextContent(
      "private empty backup directory",
    );
  });

  it("pauses a failed create retry when the backup list becomes stale", async () => {
    let reads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () => {
        reads += 1;
        return reads === 1
          ? HttpResponse.json(LIST)
          : HttpResponse.text("private backup directory", { status: 500 });
      }),
      http.post(`${TAURI_ENDPOINT}/app_backup_create`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "UPSTREAM_ERROR",
            messageKey: "error.backup.createFailed",
            technicalMessage: "private create target",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const { client } = mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.backup.create,
      }),
    );
    const createAlert = await screen.findByRole("alert");
    const createRetry = within(createAlert).getByRole("button", {
      name: en.preferences.backup.retry,
    });
    expect(createRetry).toBeEnabled();

    await client.invalidateQueries({ queryKey: backupKeys.all });
    expect(
      await screen.findByRole("alert", {
        name: en.preferences.backup.refreshError.title,
      }),
    ).toBeInTheDocument();
    expect(createRetry).toBeDisabled();
    expect(document.body).not.toHaveTextContent(/private .* target/i);
  });

  it("blocks a pending confirmation while the backup list is refreshing", async () => {
    let reads = 0;
    let releaseRefresh!: () => void;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, async () => {
        reads += 1;
        if (reads === 2) {
          await new Promise<void>((resolve) => {
            releaseRefresh = resolve;
          });
        }
        return HttpResponse.json(LIST);
      }),
    );
    const { client } = mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: restoreButtonName(),
      }),
    );

    const refresh = client.invalidateQueries({ queryKey: backupKeys.all });
    await waitFor(() => expect(reads).toBe(2));
    expect(
      screen.getByRole("button", {
        name: en.preferences.backup.restoreConfirm,
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    ).toBeEnabled();

    releaseRefresh();
    await refresh;
    await waitFor(() =>
      expect(
        screen.getByRole("button", {
          name: en.preferences.backup.restoreConfirm,
        }),
      ).toBeEnabled(),
    );
  });

  it("spells out what a restore does before doing it", async () => {
    let restored: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_restore`, async ({ request }) => {
        restored = await request.json();
        return HttpResponse.json({ backups: LIST, toolsOutOfSync: false });
      }),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: restoreButtonName(),
      }),
    );
    expect(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    ).toHaveFocus();
    for (const point of Object.values(en.preferences.backup.restorePoint)) {
      expect(screen.getByText(point)).toBeInTheDocument();
    }
    expect(restored).toBeUndefined();

    await userEvent.click(
      screen.getByRole("button", {
        name: en.preferences.backup.restoreConfirm,
      }),
    );
    await waitFor(() => expect(restored).toEqual({ backup: FILE.name }));
    expect(
      await screen.findByText(en.preferences.backup.completion.restored.title),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/AI Manager is now using the restore point from/),
    ).toBeInTheDocument();
    expect(toastMocks.success).not.toHaveBeenCalled();
  });

  it("locks a restore in place while it is running", async () => {
    let finishRestore!: (value: {
      backups: typeof LIST;
      toolsOutOfSync: boolean;
    }) => void;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_backup_restore`,
        () =>
          new Promise((resolve) => {
            finishRestore = (value) => resolve(HttpResponse.json(value));
          }),
      ),
    );

    mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: restoreButtonName(),
      }),
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.preferences.backup.restoreConfirm,
      }),
    );

    expect(
      screen.getByRole("button", {
        name: en.preferences.backup.restoreConfirm,
      }),
    ).toHaveAttribute("aria-busy", "true");
    expect(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    ).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: en.ds.action.close }),
    ).toBeNull();
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    finishRestore({ backups: LIST, toolsOutOfSync: false });
    expect(
      await screen.findByText(en.preferences.backup.completion.restored.title),
    ).toBeInTheDocument();
  });

  it("says so when the tools did not pick the restore up", async () => {
    const reviewServices = vi.fn();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_restore`, () =>
        HttpResponse.json({ backups: LIST, toolsOutOfSync: true }),
      ),
    );
    mount(false, reviewServices);
    await userEvent.click(
      await screen.findByRole("button", {
        name: restoreButtonName(),
      }),
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.preferences.backup.restoreConfirm,
      }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      en.preferences.backup.completion.warning.title,
    );
    expect(alert).toHaveTextContent(
      en.preferences.backup.completion.warning.description,
    );
    await userEvent.click(
      within(alert).getByRole("button", {
        name: en.preferences.backup.completion.reviewServices,
      }),
    );
    expect(reviewServices).toHaveBeenCalledOnce();
    expect(toastMocks.success).not.toHaveBeenCalled();
  });

  it("reuses the native backup rename and keeps the chosen name visible", async () => {
    let received: unknown;
    const renamed = { ...FILE, name: "before-upgrading-claude.db" };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_rename`, async ({ request }) => {
        received = await request.json();
        return HttpResponse.json({ ...LIST, files: [renamed] });
      }),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: renameButtonName() }),
    );
    const dialog = screen.getByRole("dialog");
    const input = within(dialog).getByRole("textbox", {
      name: en.preferences.backup.nameLabel,
    });
    await userEvent.type(input, "before-upgrading-claude");
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.preferences.backup.renameConfirm,
      }),
    );

    await waitFor(() =>
      expect(received).toEqual({
        backup: FILE.name,
        name: "before-upgrading-claude",
      }),
    );
    const result = await screen.findByRole("status", {
      name: en.preferences.backup.completion.renamed.title,
    });
    expect(result).toHaveTextContent("before-upgrading-claude");
    expect(
      screen.getByRole("button", {
        name: en.preferences.backup.renameNamed.replace(
          "{{name}}",
          "before-upgrading-claude",
        ),
      }),
    ).toBeInTheDocument();
  });

  it("explains an unsafe backup name before making a native request", async () => {
    const rename = vi.fn();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_rename`, () => {
        rename();
        return HttpResponse.json(LIST);
      }),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: renameButtonName() }),
    );
    const dialog = screen.getByRole("dialog");
    const input = within(dialog).getByRole("textbox", {
      name: en.preferences.backup.nameLabel,
    });
    await userEvent.type(input, "../outside");
    await userEvent.tab();
    expect(dialog).toHaveTextContent(
      en.preferences.backup.renameValidation.invalid,
    );
    expect(
      within(dialog).getByRole("button", {
        name: en.preferences.backup.renameConfirm,
      }),
    ).toBeDisabled();
    expect(rename).not.toHaveBeenCalled();
  });

  it("asks before deleting and keeps the dialog open when it fails", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_delete`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "BACKUP_NOT_FOUND",
            messageKey: "error.backup.notFound",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: deleteButtonName() }),
    );
    expect(
      screen.getByText(en.preferences.backup.deletePoint.permanent),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: en.preferences.backup.deleteConfirm }),
    );
    const dialog = screen.getByRole("dialog");
    const alert = await within(dialog).findByRole("alert");
    expect(alert).toHaveTextContent(en.error.backup.notFound);
    expect(alert).not.toHaveTextContent(/sqlite|technical/i);
    expect(toastMocks.error).not.toHaveBeenCalled();
    expect(dialog).toBeInTheDocument();
  });

  it("keeps the deleted restore point visible as a durable result", async () => {
    let removed: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_delete`, async ({ request }) => {
        removed = await request.json();
        return HttpResponse.json({ ...LIST, files: [] });
      }),
    );
    mount();
    const label = backupLabel(FILE, i18n.language);
    await userEvent.click(
      await screen.findByRole("button", { name: deleteButtonName() }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.preferences.backup.deleteConfirm }),
    );

    await waitFor(() => expect(removed).toEqual({ backup: FILE.name }));
    const result = await screen.findByRole("status", {
      name: en.preferences.backup.completion.deleted.title,
    });
    expect(result).toHaveTextContent(label);
    expect(result).not.toHaveTextContent(FILE.name);
    expect(
      screen.getByRole("status", {
        name: en.preferences.backup.empty.title,
      }),
    ).toBeInTheDocument();
    expect(toastMocks.success).not.toHaveBeenCalled();
  });

  it("actually makes a backup when the button is pressed", async () => {
    // Mutation guard: swap onClick for a no-op and every prior case would still
    // pass — the entire "create backup now" flow had zero coverage before this.
    let created = false;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_create`, () => {
        created = true;
        return HttpResponse.json(LIST);
      }),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: en.preferences.backup.create }),
    );
    await waitFor(() => expect(created).toBe(true));
    expect(
      await screen.findByText(en.preferences.backup.completion.created.title),
    ).toBeInTheDocument();
    expect(
      screen.getByText(en.preferences.backup.completion.created.description),
    ).toBeInTheDocument();
    expect(toastMocks.success).not.toHaveBeenCalled();
  });

  it("keeps a create failure visible and lets the user retry it", async () => {
    let attempts = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_create`, () => {
        attempts += 1;
        if (attempts === 1) {
          return HttpResponse.text(
            JSON.stringify({
              code: "UPSTREAM_ERROR",
              messageKey: "error.backup.createFailed",
              technicalMessage: "sqlite disk detail",
              remediation: "error.remediation.retryOrViewDetails",
              contextId: null,
            }),
            { status: 500 },
          );
        }
        return HttpResponse.json(LIST);
      }),
    );

    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: en.preferences.backup.create }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(en.error.backup.createFailed);
    expect(alert).toHaveTextContent(en.preferences.backup.error.actionRetry);
    expect(alert).not.toHaveTextContent(/sqlite/i);

    await userEvent.click(
      within(alert).getByRole("button", {
        name: en.preferences.backup.retry,
      }),
    );
    expect(
      await screen.findByText(en.preferences.backup.completion.created.title),
    ).toBeInTheDocument();
    expect(attempts).toBe(2);
  });

  it("makes everything the database feeds re-read itself after a restore", async () => {
    // A restore swaps out the entire database. This pins down that the
    // invalidation actually happens — swap the predicate for `() => false` and
    // this case would still pass, while the whole UI would be stuck on stale data.
    const client = createTestQueryClient();
    client.setQueryData(["tools", "list"], []);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_restore`, () =>
        HttpResponse.json({ backups: LIST, toolsOutOfSync: false }),
      ),
    );
    render(<BackupSection advancedMode={false} />, {
      wrapper: withQueryClient(client),
    });
    await userEvent.click(
      (
        await screen.findAllByRole("button", {
          name: restoreButtonName(),
        })
      )[0]!,
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.preferences.backup.restoreConfirm,
      }),
    );
    await waitFor(() =>
      expect(client.getQueryState(["tools", "list"])?.isInvalidated).toBe(true),
    );
    // The update check is unrelated to the database and shouldn't be invalidated along with it.
    expect(client.getQueryState(["update", "status"])?.isInvalidated).not.toBe(
      true,
    );
  });
});
