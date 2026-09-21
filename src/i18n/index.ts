import i18n, {
  type BackendModule,
  type ReadCallback,
  type ResourceKey,
} from "i18next";
import { initReactI18next } from "react-i18next";

/**
 * The languages the application ships. Adding one means a locale file, an entry
 * here, an entry in `localeLoaders`, and a row in the settings picker; the
 * four-locale key-set test in `tests/i18n` then covers it automatically.
 */
export const APP_LANGUAGES = [
  "zh",
  "zh-TW",
  "en",
  "ja",
  "ko",
  "de",
  "fr",
  "es",
  "pt-BR",
  "it",
  "ru",
  "vi",
  "id",
] as const;

export type AppLanguage = (typeof APP_LANGUAGES)[number];

/**
 * Each language named in itself, in the picker's order.
 *
 * These are endonyms, so they do not change with the interface language: a
 * Japanese reader looking for Japanese looks for 日本語, not for whatever their
 * current interface calls it. Keeping them out of the locale files avoids
 * thirteen identical copies of every entry.
 */
export const LANGUAGE_ENDONYMS: Readonly<Record<AppLanguage, string>> = {
  zh: "简体中文",
  "zh-TW": "繁體中文",
  en: "English",
  ja: "日本語",
  ko: "한국어",
  de: "Deutsch",
  fr: "Français",
  es: "Español",
  "pt-BR": "Português (Brasil)",
  it: "Italiano",
  ru: "Русский",
  vi: "Tiếng Việt",
  id: "Bahasa Indonesia",
};

const DEFAULT_LANGUAGE: AppLanguage = "zh";

function isAppLanguage(value: unknown): value is AppLanguage {
  return APP_LANGUAGES.includes(value as AppLanguage);
}

/**
 * Prefixes that map straight onto a locale file. Traditional Chinese is not in
 * here because it needs script and region tests that a prefix cannot express,
 * and Portuguese is deliberately absent: `pt-BR` is the only Portuguese we
 * ship, so European Portuguese would be served Brazilian copy silently.
 */
const LANGUAGE_PREFIXES: ReadonlyArray<readonly [string, AppLanguage]> = [
  ["zh", "zh"],
  ["ja", "ja"],
  ["en", "en"],
  ["ko", "ko"],
  ["de", "de"],
  ["fr", "fr"],
  ["es", "es"],
  ["pt", "pt-BR"],
  ["it", "it"],
  ["ru", "ru"],
  ["vi", "vi"],
  ["id", "id"],
];

export function resolveAppLanguage(language?: string | null): AppLanguage {
  const normalized = language?.toLowerCase();
  if (!normalized) return DEFAULT_LANGUAGE;

  // Traditional Chinese has to be tested before Chinese, or Taiwan, Hong Kong
  // and Macau are served Simplified.
  if (
    normalized === "zh-tw" ||
    normalized.startsWith("zh-hk") ||
    normalized.startsWith("zh-mo") ||
    normalized.startsWith("zh-hant")
  ) {
    return "zh-TW";
  }

  for (const [prefix, language] of LANGUAGE_PREFIXES) {
    if (normalized.startsWith(prefix)) return language;
  }

  return DEFAULT_LANGUAGE;
}

const getInitialLanguage = (): AppLanguage => {
  if (typeof window !== "undefined") {
    try {
      const stored = window.localStorage.getItem("language");
      if (isAppLanguage(stored)) return stored;
    } catch (error) {
      console.warn("[i18n] Failed to read stored language preference", error);
    }
  }

  const navigatorLang =
    typeof navigator !== "undefined"
      ? (navigator.language?.toLowerCase() ??
        navigator.languages?.[0]?.toLowerCase())
      : undefined;

  return resolveAppLanguage(navigatorLang);
};

const localeLoaders: Record<
  AppLanguage,
  () => Promise<{ default: ResourceKey }>
> = {
  en: () => import("./locales/en.json"),
  ja: () => import("./locales/ja.json"),
  zh: () => import("./locales/zh.json"),
  "zh-TW": () => import("./locales/zh-TW.json"),
  ko: () => import("./locales/ko.json"),
  de: () => import("./locales/de.json"),
  fr: () => import("./locales/fr.json"),
  es: () => import("./locales/es.json"),
  "pt-BR": () => import("./locales/pt-BR.json"),
  it: () => import("./locales/it.json"),
  ru: () => import("./locales/ru.json"),
  vi: () => import("./locales/vi.json"),
  id: () => import("./locales/id.json"),
};

const localeRequests = new Map<AppLanguage, Promise<ResourceKey>>();

function loadLocale(language: AppLanguage): Promise<ResourceKey> {
  const existing = localeRequests.get(language);
  if (existing) return existing;

  const request = localeLoaders[language]().then((module) => module.default);
  localeRequests.set(language, request);
  return request;
}

const localeBackend: BackendModule = {
  type: "backend",
  init() {},
  read(language: string, _namespace: string, callback: ReadCallback) {
    void loadLocale(resolveAppLanguage(language)).then(
      (messages) => callback(null, messages),
      (error: unknown) =>
        callback(
          error instanceof Error
            ? error
            : new Error("Failed to load application translations"),
          false,
        ),
    );
  },
};

let pluginsInstalled = false;
let initializationQueue: Promise<void> = Promise.resolve();

export function initializeI18n(
  language: AppLanguage = getInitialLanguage(),
): Promise<void> {
  initializationQueue = initializationQueue.then(async () => {
    if (!pluginsInstalled) {
      i18n.use(localeBackend).use(initReactI18next);
      pluginsInstalled = true;
    }

    if (!i18n.isInitialized) {
      await i18n.init({
        lng: language,
        fallbackLng: "en",
        supportedLngs: [...APP_LANGUAGES],
        load: "currentOnly",
        ns: ["translation"],
        defaultNS: "translation",
        interpolation: {
          escapeValue: false,
        },
        debug: false,
      });
      return;
    }

    if (
      resolveAppLanguage(i18n.resolvedLanguage ?? i18n.language) !== language
    ) {
      await i18n.changeLanguage(language);
    }
  });

  return initializationQueue;
}

function syncDocumentLanguage(language?: string): void {
  if (typeof document === "undefined") return;
  document.documentElement.lang = resolveAppLanguage(language);
}

i18n.on("languageChanged", syncDocumentLanguage);

/** Main and tests await this before rendering translated UI. */
export const i18nReady = initializeI18n();

/** Product-shell language changes are local preferences and take effect now. */
export async function setAppLanguage(language: AppLanguage): Promise<void> {
  if (typeof window !== "undefined") {
    try {
      window.localStorage.setItem("language", language);
    } catch (error) {
      console.warn("[i18n] Failed to persist language preference", error);
    }
  }

  await initializeI18n(language);
}

export default i18n;
