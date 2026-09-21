import "@testing-library/jest-dom";
import { afterAll, afterEach, beforeAll, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import i18n, { i18nReady } from "@/i18n";
import { server } from "./msw/server";
import "./msw/tauriMocks";

beforeAll(async () => {
  server.listen({ onUnhandledRequest: "warn" });
  await i18nReady;
  // Most focused component tests intentionally assert stable translation keys
  // unless they install the locale slice they exercise. Preserve that fixture
  // contract even though production now loads complete locales on demand.
  for (const language of ["zh", "zh-TW", "en", "ja"]) {
    i18n.removeResourceBundle(language, "translation");
  }
  i18n.addResourceBundle("zh", "translation", {}, true, true);
  i18n.addResourceBundle("en", "translation", {}, true, true);
  await i18n.changeLanguage("zh");
});

afterEach(() => {
  cleanup();
  server.resetHandlers();
  vi.clearAllMocks();
});

afterAll(() => {
  server.close();
});
