import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BackupImportModal } from "@/features/backup";
import en from "@/i18n/locales/en.json";

function mount(
  overrides: Partial<React.ComponentProps<typeof BackupImportModal>> = {},
) {
  const onOpenChange = vi.fn();
  const onImport = vi.fn();
  render(
    <BackupImportModal
      open
      blocked={false}
      importing={false}
      error={null}
      onOpenChange={onOpenChange}
      onImport={onImport}
      {...overrides}
    />,
  );
  return { onOpenChange, onImport };
}

describe("BackupImportModal", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, preferences: en.preferences },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("reads the consequences and the credential risk before choosing an archive", async () => {
    const { onImport } = mount();
    const dialog = screen.getByRole("dialog", {
      name: en.preferences.backup.transfer.confirmTitle,
    });
    for (const copy of [
      en.preferences.backup.transfer.secretDescription,
      en.preferences.backup.transfer.confirmReplace,
      en.preferences.backup.transfer.confirmSafety,
      en.preferences.backup.transfer.confirmTrust,
      en.preferences.backup.transfer.confirmInstalled,
    ]) {
      expect(dialog).toHaveTextContent(copy);
    }
    const cancel = within(dialog).getByRole("button", {
      name: en.ds.action.cancel,
    });
    await waitFor(() => expect(cancel).toHaveFocus());
    expect(onImport).not.toHaveBeenCalled();

    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.preferences.backup.transfer.chooseAndImport,
      }),
    );
    expect(onImport).toHaveBeenCalledTimes(1);
  });

  it("keeps technical details out of a visible import failure", () => {
    const { container } = render(
      <BackupImportModal
        open
        blocked={false}
        importing={false}
        error={{
          code: "CONFIG_PARSE_FAILED",
          messageKey: "error.backup.importFailed",
          remediationKey: "error.remediation.retryOrViewDetails",
          technicalMessage: "/Users/private/config.sql token=do-not-show",
          contextId: null,
        }}
        onOpenChange={vi.fn()}
        onImport={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("alert", { name: en.error.backup.importFailed }),
    ).toHaveTextContent(en.preferences.backup.transfer.errorHint);
    expect(container).not.toHaveTextContent("/Users/private");
    expect(container).not.toHaveTextContent("do-not-show");
  });

  it("does not allow the dialog to close while a native import is running", async () => {
    const onOpenChange = vi.fn();
    mount({ importing: true, onOpenChange });
    expect(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    ).toBeDisabled();
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
  });
});
