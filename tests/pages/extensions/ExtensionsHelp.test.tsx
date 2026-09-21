import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { describe, expect, it } from "vitest";
import { EXTENSION_TABS } from "@/features/extension-management";
import { ExtensionsHelp } from "@/pages/extensions/ExtensionsHelp";
import en from "@/i18n/locales/en.json";
import zh from "@/i18n/locales/zh.json";

describe("ExtensionsHelp", () => {
  it.each(["en", "zh"] as const)(
    "keeps prompt rules available on demand in %s",
    async (locale) => {
      const copy = locale === "zh" ? zh : en;
      i18n.addResourceBundle(locale, "translation", copy, true, true);
      await i18n.changeLanguage(locale);
      const tab = EXTENSION_TABS.find((item) => item.kind === "prompt")!;
      const user = userEvent.setup();
      render(<ExtensionsHelp tab={tab} />);
      const help = screen.getByText(copy.extensions.help);
      expect(
        screen.getByText(copy.extensions.prompt.exclusive),
      ).not.toBeVisible();
      await user.click(help);
      expect(screen.getByText(copy.extensions.prompt.exclusive)).toBeVisible();
      expect(
        screen.getByText(copy.extensions.location.promptNote),
      ).toBeVisible();
      await user.keyboard("{Escape}");
      expect(
        screen.getByText(copy.extensions.prompt.exclusive),
      ).not.toBeVisible();
      expect(help).toHaveFocus();
    },
  );
});
