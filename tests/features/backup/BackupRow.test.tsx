import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BackupRow } from "@/features/backup/BackupRow";
import en from "@/i18n/locales/en.json";

const LONG_NAME =
  "ai-manager-backup-shared-team-production-workstation-common-configuration-file-2026-08-23T05-00-00.000Z.db";

describe("BackupRow", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, preferences: en.preferences },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("keeps a fallback restore-point label readable without raw file detail", async () => {
    render(
      <ul>
        <BackupRow
          file={{
            name: LONG_NAME,
            createdAt: "not-a-date",
            sizeBytes: 1_273_344,
          }}
          latest
          advancedMode
          busy={false}
          onRename={vi.fn()}
          onRestore={vi.fn()}
          onDelete={vi.fn()}
        />
      </ul>,
    );

    const row = screen.getByRole("listitem");
    const paragraphs = row.querySelectorAll("p");
    const label = paragraphs[0];

    expect(label).toHaveTextContent(LONG_NAME.replace(/\.db$/, ""));
    expect(label).toHaveClass("break-words", "flex-[1_1_12rem]");
    expect(label).not.toHaveClass("truncate");
    expect(paragraphs).toHaveLength(2);
    expect(label).not.toHaveAttribute("title");

    await userEvent.tab();
    expect(
      within(row).getByRole("button", {
        name: en.preferences.backup.renameNamed.replace(
          "{{name}}",
          LONG_NAME.replace(/\.db$/, ""),
        ),
      }),
    ).toHaveFocus();
    await userEvent.tab();
    expect(
      within(row).getByRole("button", {
        name: en.preferences.backup.restoreNamed.replace(
          "{{name}}",
          LONG_NAME.replace(/\.db$/, ""),
        ),
      }),
    ).toHaveFocus();
    await userEvent.tab();
    expect(
      within(row).getByRole("button", {
        name: en.preferences.backup.deleteNamed.replace(
          "{{name}}",
          LONG_NAME.replace(/\.db$/, ""),
        ),
      }),
    ).toHaveFocus();
  });
});
