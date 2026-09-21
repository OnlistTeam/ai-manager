import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { delay, http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import { DownloadProxyCard } from "@/pages/settings/DownloadProxyCard";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

describe("DownloadProxyCard", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        common: en.common,
        error: en.error,
        preferences: en.preferences,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("uses a truthful desktop detection status instead of a web skeleton", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, async () => {
        await delay("infinite");
        return HttpResponse.json({
          configured: false,
          url: null,
          protected: false,
        });
      }),
    );

    const { container } = render(<DownloadProxyCard />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    expect(
      await screen.findByRole("status", {
        name: en.preferences.network.proxy.loading,
      }),
    ).toHaveTextContent(en.common.detecting);
    expect(container.querySelector(".animate-pulse")).toBeNull();
  });

  it("keeps the address field out of the way until someone asks for a proxy", async () => {
    render(<DownloadProxyCard />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    const toggle = await screen.findByRole("switch", {
      name: en.preferences.network.proxy.title,
    });
    expect(toggle).not.toBeChecked();
    expect(
      screen.queryByRole("textbox", {
        name: en.preferences.network.proxy.label,
      }),
    ).toBeNull();

    await userEvent.click(toggle);

    expect(
      await screen.findByRole("textbox", {
        name: en.preferences.network.proxy.label,
      }),
    ).toBeInTheDocument();
  });

  it("turning the switch off is what goes back to a direct connection", async () => {
    let received: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, () =>
        HttpResponse.json({
          configured: true,
          url: "http://127.0.0.1:7890",
          protected: false,
        }),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_network_proxy_save`,
        async ({ request }) => {
          received = await request.json();
          return HttpResponse.json({
            configured: false,
            url: null,
            protected: false,
          });
        },
      ),
    );

    render(<DownloadProxyCard />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    const toggle = await screen.findByRole("switch", {
      name: en.preferences.network.proxy.title,
    });
    expect(toggle).toBeChecked();

    await userEvent.click(toggle);

    await waitFor(() => expect(received).toEqual({ url: null }));
    await waitFor(() =>
      expect(
        screen.queryByRole("textbox", {
          name: en.preferences.network.proxy.label,
        }),
      ).toBeNull(),
    );
  });
});
