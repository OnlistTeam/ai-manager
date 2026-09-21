import { render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTW from "@/i18n/locales/zh-TW.json";
import { DesignGallery } from "@/shared/ui/__gallery__/DesignGallery";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

describe("DesignGallery", () => {
  afterEach(() => {
    window.history.replaceState({}, "", "/");
    window.localStorage.clear();
    document.documentElement.classList.remove("light", "dark");
  });

  it("renders every section heading", () => {
    render(<DesignGallery />);
    expect(
      screen.getByRole("heading", { level: 1, name: /design system/i }),
    ).toBeInTheDocument();
    for (const section of ["Primitives", "Layout", "Product"]) {
      expect(
        screen.getByRole("heading", { level: 2, name: section }),
      ).toBeInTheDocument();
    }
  });

  it("shows the state matrix required by spec section 97", () => {
    render(<DesignGallery />);
    for (const state of ["Loading", "Empty", "Disabled", "Hover", "Focus"]) {
      expect(screen.getAllByText(state).length).toBeGreaterThan(0);
    }
  });

  it("offers a theme and a language switch", () => {
    render(<DesignGallery />);
    expect(
      screen.getByRole("button", { name: /toggle theme/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /english|chinese/i }),
    ).toBeInTheDocument();
  });

  it("renders the exact settings shell with production providers and seeded success data", async () => {
    i18n.addResourceBundle("en", "translation", en, true, true);
    await i18n.changeLanguage("en");
    server.use(
      http.post(`${TAURI_ENDPOINT}/set_window_theme`, () =>
        HttpResponse.json(null),
      ),
    );
    window.history.replaceState(
      {},
      "",
      "/?gallery=shell&route=settings&lang=en&theme=dark",
    );

    render(<DesignGallery />);

    expect(
      await screen.findByRole("heading", { level: 1, name: "Settings" }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText("Make AI Manager yours"),
    ).toBeInTheDocument();
    expect(
      await screen.findByDisplayValue("http://127.0.0.1:7890"),
    ).toBeInTheDocument();
    await waitFor(() => expect(document.documentElement).toHaveClass("dark"));
  });

  it("renders a route-specific loading stage for visual QA", async () => {
    i18n.addResourceBundle("en", "translation", en, true, true);
    await i18n.changeLanguage("en");
    window.history.replaceState(
      {},
      "",
      "/?gallery=route-loading&route=extensions&lang=en&theme=dark",
    );

    render(<DesignGallery />);

    const status = screen.getByRole("status", { name: en.nav.loadingPage });
    expect(
      screen.getByRole("heading", { level: 1, name: en.nav.extensions }),
    ).toBeInTheDocument();
    // Management pages open with a SectionHeader and have no 3D model in the middle, so the loading placeholder follows suit and uses the title too.
    expect(status.querySelector("[data-spatial-stage]")).toBeNull();
    await waitFor(() => expect(document.documentElement).toHaveClass("dark"));
  });

  it.each([
    ["ja", ja.tools.title],
    ["zh-TW", zhTW.tools.title],
  ] as const)(
    "honors the %s query language in the production shell gallery",
    async (language, title) => {
      const resources = language === "ja" ? ja : zhTW;
      i18n.addResourceBundle(language, "translation", resources, true, true);
      await i18n.changeLanguage("en");
      server.use(
        http.post(`${TAURI_ENDPOINT}/set_window_theme`, () =>
          HttpResponse.json(null),
        ),
      );
      window.history.replaceState(
        {},
        "",
        `/?gallery=shell&route=tools&lang=${language}`,
      );

      render(<DesignGallery />);

      expect(
        await screen.findByRole("heading", { level: 1, name: title }),
      ).toBeInTheDocument();
      await waitFor(() =>
        expect(document.documentElement).toHaveAttribute("lang", language),
      );
    },
  );
});
