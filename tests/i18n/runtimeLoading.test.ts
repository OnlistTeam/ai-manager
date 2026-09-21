import { afterEach, describe, expect, it } from "vitest";
import i18n, {
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
    expect(resolveAppLanguage("ja-JP")).toBe("ja");
    expect(resolveAppLanguage("fr-FR")).toBe("zh");
  });
});
