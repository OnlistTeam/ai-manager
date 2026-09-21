import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTw from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

const locales = { en, ja, "zh-TW": zhTw, zh } as const;

describe("Home health check locale contract", () => {
  for (const [locale, messages] of Object.entries(locales)) {
    it(`${locale} keeps every health card outcome and its one action complete`, () => {
      const health = messages.home.health;

      for (const [key, value] of [
        ["title", health.title],
        ["recheck", health.recheck],
        ["allGood", health.allGood],
        ["issues_one", health.issues_one],
        ["issues_other", health.issues_other],
        ["lastChecked", health.lastChecked],
        ["connectionError", health.connectionError],
        ["noTools.title", health.noTools.title],
        ["noTools.action", health.noTools.action],
      ] as const) {
        expect(value.trim(), `${locale}.home.health.${key}`).not.toBe("");
      }
      expect(health.issues_other).toContain("{{count}}");
      expect(health.lastChecked).toContain("{{time}}");
      // The card must never collide with Home's stale-refresh retry label.
      expect(health.recheck).not.toBe(messages.home.refreshError.action);
    });
  }
});
