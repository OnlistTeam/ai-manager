import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { ProviderAddPage } from "@/features/provider-management";
import type {
  ProviderConnectionPreset,
  ProviderConnectionProfile,
} from "@/entities/provider";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function preset(
  id: string,
  serviceName: string,
  baseUrl: string,
  kind: ProviderConnectionPreset["kind"],
): ProviderConnectionPreset {
  return {
    id,
    serviceName,
    defaultName: serviceName,
    defaultModel: null,
    baseUrl,
    websiteUrl: "https://example.test",
    apiKeyUrl: "https://example.test",
    official: false,
    kind,
  };
}

const profile: ProviderConnectionProfile = {
  defaultPresetId: "official",
  modelRequired: false,
  baseUrlTakesNoVersion: true,
  toolLogin: "claude",
  // Catalogue order, which is not group order: onList leads the catalogue.
  presets: [
    preset("onlist", "onList", "https://onlist.io", "relay"),
    {
      ...preset(
        "official",
        "Anthropic API",
        "https://api.anthropic.com",
        "vendor",
      ),
      official: true,
    },
    preset(
      "deepseek",
      "DeepSeek",
      "https://api.deepseek.com/anthropic",
      "vendor",
    ),
    preset("openrouter", "OpenRouter", "https://openrouter.ai/api", "relay"),
    preset("ollama", "Ollama", "http://localhost:11434", "local"),
  ],
};

type PageProps = ComponentProps<typeof ProviderAddPage>;

function renderPage(overrides: Partial<PageProps> = {}) {
  const props: PageProps = {
    tool: "claude-code",
    toolName: "Claude Code",
    profile,
    onPickPreset: vi.fn(),
    onPickCustom: vi.fn(),
    onPickToolLogin: vi.fn(),
    ...overrides,
  };
  render(<ProviderAddPage {...props} />);
  return props;
}

function headings() {
  return screen
    .getAllByRole("heading", { level: 3 })
    .map((heading) => heading.firstChild?.textContent);
}

describe("ProviderAddPage", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, services: en.services },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("puts Custom first, then the tool's own sign-in, then each kind of service", () => {
    renderPage();
    const cards = screen.getAllByRole("button", {
      name: /./,
    });
    // The search field's neighbour is the speed test; the first card is Custom.
    expect(cards[1]).toHaveTextContent(en.services.add.custom);
    expect(headings()).toEqual([
      en.services.add.group.login,
      en.services.add.group.vendor,
      en.services.add.group.relay,
      en.services.add.group.local,
    ]);
    const relays = screen
      .getByRole("heading", { name: new RegExp(en.services.add.group.relay) })
      .closest("section")!;
    const relayCards = within(relays).getAllByRole("button");
    expect(relayCards[0]).toHaveTextContent("onList");
    expect(relayCards[1]).toHaveTextContent("OpenRouter");
  });

  it("names a service by its address, never by a model", () => {
    renderPage();
    const card = screen.getByRole("button", { name: /DeepSeek/ });
    expect(card).toHaveTextContent("api.deepseek.com");
  });

  it("leaves out the subscription group for a tool without its own sign-in", () => {
    renderPage({ profile: { ...profile, toolLogin: null } });
    expect(headings()).not.toContain(en.services.add.group.login);
  });

  it("says whether the tool is signed in on the sign-in card, and still opens it", async () => {
    const props = renderPage({
      loginStatus: {
        state: "signedIn",
        account: "someone@example.com",
        plan: "max",
      },
    });
    const card = screen.getByRole("button", {
      name: new RegExp(en.services.add.login.claude),
    });
    expect(card).toHaveTextContent(
      `${en.services.login.state.signedIn} · someone@example.com · Max`,
    );
    await userEvent.click(card);
    expect(props.onPickToolLogin).toHaveBeenCalled();
  });

  it("offers a signed-out user to sign in here, never says they are", () => {
    renderPage({
      loginStatus: { state: "signedOut", account: null, plan: null },
    });
    const card = screen.getByRole("button", {
      name: new RegExp(en.services.add.login.claude),
    });
    expect(card).toHaveTextContent(
      en.services.add.loginDetail.replace("{{tool}}", "Claude Code"),
    );
    expect(card).not.toHaveTextContent(en.services.login.state.signedIn);
  });

  it("keeps the plain sign-in hint while the tool could not say", () => {
    renderPage({
      loginStatus: { state: "unknown", account: null, plan: null },
    });
    const card = screen.getByRole("button", {
      name: new RegExp(en.services.add.login.claude),
    });
    expect(card).toHaveTextContent(
      en.services.add.loginDetail.replace("{{tool}}", "Claude Code"),
    );
  });

  it("hands the picked preset and the Custom card to their own dialogs", async () => {
    const props = renderPage();
    await userEvent.click(screen.getByRole("button", { name: /OpenRouter/ }));
    expect(props.onPickPreset).toHaveBeenCalledWith(profile.presets[3]);
    await userEvent.click(
      screen.getByRole("button", {
        name: new RegExp(en.services.add.custom),
      }),
    );
    expect(props.onPickCustom).toHaveBeenCalled();
  });

  it("filters by name or address, and offers Custom when nothing matches", async () => {
    const props = renderPage();
    const search = screen.getByRole("searchbox", {
      name: en.services.add.search,
    });
    await userEvent.type(search, "localhost");
    expect(screen.getByRole("button", { name: /Ollama/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: /DeepSeek/ })).toBeNull();
    expect(headings()).toEqual([en.services.add.group.local]);

    await userEvent.clear(search);
    await userEvent.type(search, "mistral");
    expect(screen.queryAllByRole("heading", { level: 3 })).toHaveLength(0);
    expect(
      screen.getByText(
        en.services.add.noMatch.replace("{{query}}", "mistral"),
        { exact: false },
      ),
    ).toBeVisible();
    await userEvent.click(
      screen.getByRole("button", { name: en.services.add.addAsCustom }),
    );
    expect(props.onPickCustom).toHaveBeenCalled();
  });

  it("does not use the network until asked, then tags each card with its latency", async () => {
    let requests = 0;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_presets_test`,
        async ({ request }) => {
          requests += 1;
          expect(await request.json()).toEqual({ tool: "claude-code" });
          return HttpResponse.json([
            {
              candidateId: "deepseek",
              latencyMs: 85,
              httpStatus: 204,
              failure: null,
            },
            {
              candidateId: "ollama",
              latencyMs: null,
              httpStatus: null,
              failure: "connection",
            },
          ]);
        },
      ),
    );
    renderPage();
    expect(requests).toBe(0);
    await userEvent.click(
      screen.getByRole("button", { name: en.services.connect.speedTest }),
    );
    await waitFor(() => expect(requests).toBe(1));
    expect(
      await screen.findByRole("button", { name: /DeepSeek/ }),
    ).toHaveTextContent("85 ms");
    expect(screen.getByRole("button", { name: /Ollama/ })).toHaveTextContent(
      en.services.connect.unreachable,
    );
    expect(
      screen.getByRole("button", { name: en.services.connect.speedTestAgain }),
    ).toBeEnabled();
  });

  it("keeps a failed speed test inline without exposing details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_presets_test`, () =>
        HttpResponse.json(
          {
            code: "NETWORK_ERROR",
            messageKey: "error.provider.testFailed",
            technicalMessage: "https://secret.example.test failed",
            remediation: null,
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    renderPage();
    await userEvent.click(
      screen.getByRole("button", { name: en.services.connect.speedTest }),
    );
    expect(
      await screen.findByText(en.services.connect.speedTestFailed),
    ).toBeVisible();
    expect(screen.queryByText(/secret\.example\.test/)).toBeNull();
  });
});
