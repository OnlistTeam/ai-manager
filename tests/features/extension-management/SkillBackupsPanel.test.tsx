import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import {
  formatSkillBackupDate,
  SkillBackupsPanel,
} from "@/features/extension-management/SkillBackupsPanel";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const toastMocks = vi.hoisted(() => ({
  info: vi.fn(),
  success: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

const BACKUP = {
  id: "b".repeat(64),
  name: "Code review",
  description: "Reviews a change before it ships.",
  createdAt: 1_787_689_200,
  conflicts: false,
};

function mount(
  onRestoreStarted = vi.fn(),
  blocked = false,
  onBusyChange = vi.fn(),
) {
  const client = createTestQueryClient();
  return {
    client,
    onRestoreStarted,
    onBusyChange,
    ...render(
      <SkillBackupsPanel
        tool="claude-code"
        blocked={blocked}
        onBusyChange={onBusyChange}
        onRestoreStarted={onRestoreStarted}
      />,
      { wrapper: withQueryClient(client) },
    ),
  };
}

describe("SkillBackupsPanel", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        common: en.common,
        ds: en.ds,
        error: en.error,
        extensions: en.extensions,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.info.mockClear();
    toastMocks.success.mockClear();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_backups_list`, () =>
        HttpResponse.json([BACKUP]),
      ),
    );
  });

  it("shows only friendly local metadata and focuses manual refresh", async () => {
    const { container } = mount();
    const refresh = screen.getByRole("button", {
      name: en.extensions.skill.backups.refresh,
    });
    expect(await screen.findByText(BACKUP.name)).toBeInTheDocument();
    await waitFor(() => expect(refresh).toHaveFocus());
    expect(screen.getByText(BACKUP.description)).toBeInTheDocument();
    expect(container).not.toHaveTextContent("/Users/");
    expect(container).not.toHaveTextContent(BACKUP.id);
  });

  it("starts restore with the opaque reference and reports it through Tasks", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_backup_restore`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json("op-restore-1");
        },
      ),
    );
    const onRestoreStarted = vi.fn();
    mount(onRestoreStarted);
    await userEvent.click(
      await screen.findByRole("button", {
        name: `Restore ${BACKUP.name} from this recovery copy`,
      }),
    );

    await waitFor(() => expect(onRestoreStarted).toHaveBeenCalledTimes(1));
    expect(body).toEqual({ tool: "claude-code", backup: BACKUP.id });
    expect(toastMocks.info).toHaveBeenCalledWith(`Restoring ${BACKUP.name}`, {
      description: en.extensions.skill.backups.restoreQueuedDescription,
    });
  });

  it("disables restore for an already installed Skill but keeps cleanup explicit", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_backups_list`, () =>
        HttpResponse.json([{ ...BACKUP, conflicts: true }]),
      ),
    );
    mount();
    expect(await screen.findByText("Already installed")).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: `Restore ${BACKUP.name} from this recovery copy`,
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: `Delete the recovery copy for ${BACKUP.name}`,
      }),
    ).toBeEnabled();
  });

  it("deletes only after inline confirmation and refreshes the disk list", async () => {
    let copies = [BACKUP];
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_backups_list`, () =>
        HttpResponse.json(copies),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_skill_backup_delete`,
        async ({ request }) => {
          body = await request.json();
          copies = [];
          return HttpResponse.json(copies);
        },
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: `Delete the recovery copy for ${BACKUP.name}`,
      }),
    );
    const confirmation = screen.getByRole("alertdialog", {
      name: `Delete the recovery copy for ${BACKUP.name}?`,
    });
    expect(confirmation).toHaveTextContent(
      en.extensions.skill.backups.deleteDescription,
    );
    await userEvent.click(
      within(confirmation).getByRole("button", { name: "Delete copy" }),
    );

    await waitFor(() =>
      expect(screen.queryByText(BACKUP.name)).not.toBeInTheDocument(),
    );
    expect(body).toEqual({ backup: BACKUP.id });
    expect(toastMocks.success).toHaveBeenCalledWith(
      `Deleted the recovery copy for ${BACKUP.name}`,
    );
  });

  it("keeps deletion errors safe and re-reads potentially partial disk state", async () => {
    let listReads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_backups_list`, () => {
        listReads += 1;
        return HttpResponse.json([BACKUP]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_skill_backup_delete`, () =>
        HttpResponse.json(
          {
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.skill.backupDeleteFailed",
            technicalMessage: "/Users/private/recovery token=do-not-show",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    const { container } = mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: `Delete the recovery copy for ${BACKUP.name}`,
      }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Delete copy" }));

    const alert = await screen.findByRole("alert", {
      name: en.error.skill.backupDeleteFailed,
    });
    expect(alert).toHaveTextContent(en.error.skill.backupDeleteFailed);
    expect(container).not.toHaveTextContent("/Users/private");
    expect(container).not.toHaveTextContent("do-not-show");
    await waitFor(() => expect(listReads).toBeGreaterThanOrEqual(2));
  });

  it("formats a valid timestamp and safely handles an invalid one", () => {
    expect(formatSkillBackupDate(BACKUP.createdAt, "en")).not.toBe("—");
    expect(formatSkillBackupDate(Number.NaN, "en")).toBe("—");
  });
});
