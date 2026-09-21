import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import { AboutSection } from "@/features/about/AboutSection";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function mount() {
  return render(<AboutSection />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("AboutSection", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { about: en.about, error: en.error },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  /** AGPL-3.0 §5: copyright, no-warranty, licence and the source offer. */
  it("shows every Appropriate Legal Notice next to the installed version", async () => {
    mount();

    expect(await screen.findByText(en.about.copyright)).toBeInTheDocument();
    expect(screen.getByText(en.about.license)).toBeInTheDocument();
    expect(en.about.license).toMatch(/GNU Affero General Public License/);
    expect(en.about.license).toMatch(/no warranty/i);
    expect(screen.getByText(en.about.upstream)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.about.sourceCode }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.about.licenseText }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(en.about.version.replace("{{version}}", "0.1.0")),
    ).toBeInTheDocument();
  });

  it("hands both links to the native opener instead of navigating the window", async () => {
    let sourceRequests = 0;
    let licenseRequests = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_about_open_source_code`, () => {
        sourceRequests += 1;
        return HttpResponse.json(true);
      }),
      http.post(`${TAURI_ENDPOINT}/app_about_open_license`, () => {
        licenseRequests += 1;
        return HttpResponse.json(true);
      }),
    );
    mount();

    await userEvent.click(
      await screen.findByRole("button", { name: en.about.sourceCode }),
    );
    await waitFor(() => expect(sourceRequests).toBe(1));

    await userEvent.click(
      screen.getByRole("button", { name: en.about.licenseText }),
    );
    await waitFor(() => expect(licenseRequests).toBe(1));

    expect(document.querySelector("a[href^='http']")).toBeNull();
  });
});
