import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTw from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

const locales = { en, ja, "zh-TW": zhTw, zh } as const;

describe("Skill catalog recovery locale contract", () => {
  for (const [locale, messages] of Object.entries(locales)) {
    it(`${locale} keeps retained-catalog recovery copy`, () => {
      const catalog = messages.extensions.skill
        .catalog as typeof messages.extensions.skill.catalog &
        Record<string, string | undefined>;

      for (const key of [
        "refreshingTitle",
        "refreshingDescription",
        "refreshErrorTitle",
        "refreshErrorDescription",
        "mirrorTitle",
        "mirrorDescription",
      ]) {
        expect(catalog[key], `${locale}.${key}`).toEqual(expect.any(String));
        expect(catalog[key]?.trim(), `${locale}.${key}`).not.toBe("");
      }
      expect(catalog.errorDescription, `${locale}.errorDescription`).toContain(
        messages.nav.settings,
      );
    });
  }
});
