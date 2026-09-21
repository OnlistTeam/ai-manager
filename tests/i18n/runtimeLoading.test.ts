import { afterEach, describe, expect, it } from "vitest";
import i18n, {
  APP_LANGUAGES,
  i18nReady,
  initializeI18n,
  resolveAppLanguage,
  setAppLanguage,
} from "@/i18n";
import ja from "@/i18n/locales/ja.json";

describe("runtime locale loading", () => {
  afterEach(async () => {
    await initializeI18n("zh");
    window.localStorage.removeItem("language");
  });

  it("loads a packaged locale before switching the interface", async () => {
    await i18nReady;
    await setAppLanguage("ja");

    expect(i18n.t("nav.appName")).toBe(ja.nav.appName);
    expect(i18n.resolvedLanguage).toBe("ja");
    expect(document.documentElement.lang).toBe("ja");
    expect(window.localStorage.getItem("language")).toBe("ja");
  });

  it("normalizes supported system-language aliases", () => {
    expect(resolveAppLanguage("zh-Hant-HK")).toBe("zh-TW");
    expect(resolveAppLanguage("zh-CN")).toBe("zh");
    expect(resolveAppLanguage("ja-JP")).toBe("ja");
    expect(resolveAppLanguage("fr-FR")).toBe("fr");
    expect(resolveAppLanguage("pt-PT")).toBe("pt-BR");
    expect(resolveAppLanguage("id-ID")).toBe("id");
  });

  it("falls back rather than guessing at a language it does not ship", () => {
    expect(resolveAppLanguage("nl-NL")).toBe("zh");
    expect(resolveAppLanguage("ar")).toBe("zh");
    expect(resolveAppLanguage("")).toBe("zh");
    expect(resolveAppLanguage(null)).toBe("zh");
  });

  /**
   * The point of this one is the locale files, not i18next's timing: a language
   * in the picker whose file is missing, malformed, or wired to the wrong
   * import must fail here rather than render as raw keys on someone's screen.
   * `reloadResources` goes through the same backend the application uses and
   * ignores any bundle already in the store.
   */
  it("loads every language the picker offers", async () => {
    await i18nReady;
    for (const language of APP_LANGUAGES) {
      await setAppLanguage(language);
      await i18n.reloadResources([language], ["translation"]);

      expect(i18n.language, `${language} did not become current`).toBe(
        language,
      );
      const bundle = i18n.getResourceBundle(language, "translation") as
        | Record<string, unknown>
        | undefined;
      expect(
        Object.keys(bundle ?? {}).length,
        `${language} has no locale file`,
      ).toBeGreaterThan(20);
      expect(
        i18n.getFixedT(language)("nav.appName"),
        `${language} has no copy`,
      ).not.toBe("nav.appName");
    }
  });
});
