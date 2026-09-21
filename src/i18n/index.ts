import i18n, {
  type BackendModule,
  type ReadCallback,
  type ResourceKey,
} from "i18next";
import { initReactI18next } from "react-i18next";

export type AppLanguage = "zh" | "zh-TW" | "en" | "ja";

const DEFAULT_LANGUAGE: AppLanguage = "zh";

export function resolveAppLanguage(language?: string | null): AppLanguage {
  const normalized = language?.toLowerCase();

  if (
    normalized === "zh-tw" ||
    normalized?.startsWith("zh-hk") ||
    normalized?.startsWith("zh-mo") ||
    normalized?.startsWith("zh-hant")
  ) {
    return "zh-TW";
  }
  if (normalized?.startsWith("zh")) return "zh";
  if (normalized?.startsWith("ja")) return "ja";
  if (normalized?.startsWith("en")) return "en";

  return DEFAULT_LANGUAGE;
}

const getInitialLanguage = (): AppLanguage => {
  if (typeof window !== "undefined") {
    try {
      const stored = window.localStorage.getItem("language");
      if (
        stored === "zh" ||
        stored === "zh-TW" ||
        stored === "en" ||
        stored === "ja"
      ) {
        return stored;
      }
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
        supportedLngs: ["zh", "zh-TW", "en", "ja"],
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
