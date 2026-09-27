import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { delay, http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import { NetworkProxyCard } from "@/pages/settings/NetworkProxyCard";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const copy = en.preferences.network.proxy;

const SYSTEM = {
  mode: "auto",
  url: null,
  protected: false,
  inUse: "http://127.0.0.1:7890",
  source: "system",
};

function serve(
  current: Record<string, unknown>,
  saved?: (body: unknown) => Record<string, unknown>,
) {
  const bodies: unknown[] = [];
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, () =>
      HttpResponse.json(current),
    ),
    http.post(
      `${TAURI_ENDPOINT}/app_network_proxy_save`,
      async ({ request }) => {
        const body = await request.json();
        bodies.push(body);
        return HttpResponse.json(saved?.(body) ?? current);
      },
    ),
  );
  return bodies;
}

function mount() {
  render(<NetworkProxyCard />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("NetworkProxyCard", () => {
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
        return HttpResponse.json(SYSTEM);
      }),
    );
    mount();

    expect(
      await screen.findByRole("status", { name: copy.loading }),
    ).toHaveTextContent(en.common.detecting);
  });

  it("says which proxy the system gives while following it", async () => {
    serve(SYSTEM);
    mount();

    expect(
      await screen.findByText("Using the system proxy, http://127.0.0.1:7890"),
    ).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: copy.auto })).toBeChecked();
    expect(screen.queryByRole("textbox", { name: copy.label })).toBeNull();
  });

  it("goes direct at once when turned off", async () => {
    const bodies = serve(SYSTEM, () => ({
      mode: "off",
      url: null,
      protected: false,
      inUse: null,
      source: "off",
    }));
    mount();

    await userEvent.click(await screen.findByRole("radio", { name: copy.off }));

    await waitFor(() => expect(bodies).toEqual([{ mode: "off", url: null }]));
    expect(await screen.findByText(copy.status.off)).toBeInTheDocument();
  });

  it("asks for an address before a custom proxy is saved", async () => {
    const bodies = serve(SYSTEM, (body) => ({
      mode: "custom",
      url: (body as { url: string }).url,
      protected: false,
      inUse: (body as { url: string }).url,
      source: "custom",
    }));
    mount();

    await userEvent.click(
      await screen.findByRole("radio", { name: copy.custom }),
    );
    expect(bodies).toEqual([]);
    const field = screen.getByRole("textbox", { name: copy.label });
    await userEvent.type(field, "socks5://127.0.0.1:1080{Enter}");

    await waitFor(() =>
      expect(bodies).toEqual([
        { mode: "custom", url: "socks5://127.0.0.1:1080" },
      ]),
    );
    expect(
      await screen.findByText("Using socks5://127.0.0.1:1080"),
    ).toBeInTheDocument();
  });
});
