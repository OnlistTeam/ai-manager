import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { RouteLoadingFallback } from "@/app/RouteLoadingFallback";
import { isAppRoute } from "@/app/routes";
import type { AppLanguage } from "@/i18n";
import { Button } from "../Button";
import { AppShellGallery } from "./AppShellGallery";
import { LayoutSection } from "./sections/LayoutSection";
import { PrimitivesSection } from "./sections/PrimitivesSection";
import { ProductSection } from "./sections/ProductSection";

const GALLERY_LANGUAGES = [
  "en",
  "zh",
  "zh-TW",
  "ja",
] as const satisfies readonly AppLanguage[];

function isGalleryLanguage(value: string | null): value is AppLanguage {
  return GALLERY_LANGUAGES.some((language) => language === value);
}

/**
 * Dev-only state matrix for the design system (spec §97). Mounted from
 * main.tsx behind `import.meta.env.DEV` and a `?gallery` query flag, through
 * a dynamic import so nothing here reaches a production bundle.
 *
 * The theme is toggled by flipping the `dark` class directly rather than by
 * using the app ThemeProvider, so the gallery keeps working without any Tauri
 * runtime behind it.
 */
export function DesignGallery() {
  const { i18n } = useTranslation();
  const search = new URLSearchParams(window.location.search);
  const gallery = search.get("gallery");
  const focusedDesktopApps = gallery === "desktop-apps";
  const focusedShell = gallery === "shell";
  const focusedRouteLoading = gallery === "route-loading";
  const requestedDark = search.get("theme") === "dark";
  const requestedLanguage = search.get("lang");
  const requestedRoute = search.get("route");
  const [dark, setDark] = useState(
    () =>
      typeof document !== "undefined" &&
      document.documentElement.classList.contains("dark"),
  );

  useEffect(() => {
    if (!requestedDark) return;
    document.documentElement.classList.add("dark");
    setDark(true);
  }, [requestedDark]);

  useEffect(() => {
    if (!isGalleryLanguage(requestedLanguage)) return;
    void i18n.changeLanguage(requestedLanguage);
  }, [i18n, requestedLanguage]);

  const toggleTheme = useCallback(() => {
    setDark((previous) => {
      const next = !previous;
      document.documentElement.classList.toggle("dark", next);
      return next;
    });
  }, []);

  const toggleLanguage = useCallback(() => {
    void i18n.changeLanguage(i18n.language === "en" ? "zh" : "en");
  }, [i18n]);

  if (focusedShell) return <AppShellGallery />;
  if (focusedRouteLoading) {
    return (
      <main className="min-h-screen bg-layer-1 p-6 text-content">
        <div className="mx-auto max-w-[1400px]">
          <RouteLoadingFallback
            route={isAppRoute(requestedRoute) ? requestedRoute : "home"}
          />
        </div>
      </main>
    );
  }

  return (
    <div className="min-h-screen bg-layer-1 font-sans text-content">
      <div className="mx-auto flex max-w-[1400px] flex-col gap-8 p-8">
        <header className="flex items-center justify-between gap-4">
          <div>
            <h1 className="text-display text-content">Design System</h1>
            <p className="mt-1 text-body text-content-muted">
              {dark ? "Dark" : "Light"} · {i18n.language}
            </p>
          </div>
          <div className="flex gap-2">
            <Button variant="secondary" onClick={toggleTheme}>
              Toggle theme
            </Button>
            <Button variant="secondary" onClick={toggleLanguage}>
              {i18n.language === "en" ? "Chinese" : "English"}
            </Button>
          </div>
        </header>

        {focusedDesktopApps ? (
          <ProductSection />
        ) : (
          <>
            <PrimitivesSection />
            <LayoutSection />
            <ProductSection />
          </>
        )}
      </div>
    </div>
  );
}
