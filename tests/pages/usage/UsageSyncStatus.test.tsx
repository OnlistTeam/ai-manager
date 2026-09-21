import { act, render, screen } from "@testing-library/react";
import i18n from "i18next";
import { afterEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import zh from "@/i18n/locales/zh.json";
import { UsageSyncStatus } from "@/pages/usage/UsageSyncStatus";

afterEach(() => vi.useRealTimers());

describe("UsageSyncStatus", () => {
  it.each(["en", "zh"] as const)(
    "explains a long scan and cleans up its timer in %s",
    async (locale) => {
      const copy = locale === "en" ? en : zh;
      i18n.addResourceBundle(
        locale,
        "translation",
        { usage: copy.usage },
        true,
        true,
      );
      await i18n.changeLanguage(locale);
      vi.useFakeTimers();
      vi.setSystemTime(new Date("2026-09-20T00:00:00Z"));
      const view = render(<UsageSyncStatus startedAt={Date.now()} />);
      expect(screen.getByText(copy.usage.sync.runningHint)).toBeInTheDocument();
      act(() => vi.advanceTimersByTime(10_000));
      expect(
        screen.getByText(copy.usage.sync.largeHistory),
      ).toBeInTheDocument();
      expect(
        screen.getByText(i18n.t("usage.sync.elapsed", { seconds: 10 })),
      ).toBeInTheDocument();
      view.unmount();
      expect(vi.getTimerCount()).toBe(0);
    },
  );
});
