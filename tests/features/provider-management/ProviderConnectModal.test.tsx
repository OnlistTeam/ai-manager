import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { ProviderConnectModal } from "@/features/provider-management";
import type { ProviderConnectionProfile } from "@/entities/provider";
import { NativeError } from "@/native";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const profile: ProviderConnectionProfile = {
  defaultPresetId: "official",
  modelRequired: true,
  baseUrlTakesNoVersion: false,
  presets: [
    {
      id: "official",
      serviceName: "Anthropic API",
      defaultName: "Anthropic",
      defaultModel: "claude-sonnet-5",
      baseUrl: "https://api.anthropic.com",
      websiteUrl: "https://www.anthropic.com",
      apiKeyUrl: "https://console.anthropic.com",
      official: true,
    },
    {
      id: "deepseek-safe",
      serviceName: "DeepSeek",
      defaultName: "DeepSeek",
      defaultModel: "deepseek-v4-pro",
      baseUrl: "https://api.example.test",
      websiteUrl: "https://platform.deepseek.com",
      apiKeyUrl: "https://platform.deepseek.com",
      official: false,
    },
  ],
};

const optionalModelProfile: ProviderConnectionProfile = {
  ...profile,
  modelRequired: false,
  baseUrlTakesNoVersion: false,
};

describe("ProviderConnectModal", () => {
  beforeEach(async () => {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: vi.fn(),
    });
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, services: en.services },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("opens with safe backend defaults and starts on the only missing field", () => {
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByLabelText(en.services.connect.name)).toHaveValue(
      "Anthropic",
    );
    expect(
      within(dialog).getByLabelText(en.services.connect.model),
    ).toHaveValue("claude-sonnet-5");
    expect(
      within(dialog).getByLabelText(en.services.connect.key),
    ).toHaveFocus();
  });

  it("shows the selected preset's address, prefilled and editable", () => {
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
        onCustomSubmit={vi.fn()}
      />,
    );
    // One dialog, one form. An address that is only visible after the user
    // finds a second screen is how a service gets saved against an endpoint
    // nobody looked at.
    const address = screen.getByLabelText(en.services.connect.baseUrl);
    expect(address).toHaveValue("https://api.anthropic.com");
    expect(address).toBeEnabled();
    // Same form as the key, not a second screen reached from a banner.
    expect(address.closest("form")).toBe(
      screen.getByLabelText(en.services.connect.key).closest("form"),
    );
  });

  it("submits through the preset path while the address is untouched", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    const onCustomSubmit = vi.fn();
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
        onCustomSubmit={onCustomSubmit}
      />,
    );

    await user.type(screen.getByLabelText(en.services.connect.key), "sk-live");
    await user.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );

    // Only the preset id crosses: the backend still owns the address, which is
    // the rule that made hiding it look necessary in the first place.
    expect(onSubmit).toHaveBeenCalledWith({
      presetId: "official",
      name: "Anthropic",
      apiKey: "sk-live",
      model: "claude-sonnet-5",
    });
    expect(onCustomSubmit).not.toHaveBeenCalled();
  });

  it("switches to the custom path once the address is edited", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    const onCustomSubmit = vi.fn();
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
        onCustomSubmit={onCustomSubmit}
      />,
    );

    const address = screen.getByLabelText(en.services.connect.baseUrl);
    await user.clear(address);
    await user.type(address, "http://relay.example.test/v1");
    await user.type(
      screen.getByLabelText(en.services.connect.key),
      "sk-private",
    );
    await user.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );

    // Plain HTTP is refused before anything is sent.
    expect(screen.getByText(en.services.connect.baseUrlInvalid)).toBeVisible();
    expect(onCustomSubmit).not.toHaveBeenCalled();
    expect(onSubmit).not.toHaveBeenCalled();

    await user.clear(address);
    await user.type(address, "https://relay.example.test/v1");
    await user.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );

    expect(onCustomSubmit).toHaveBeenCalledWith({
      name: "Anthropic",
      apiKey: "sk-private",
      model: "claude-sonnet-5",
      baseUrl: "https://relay.example.test/v1",
    });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("starts on a compatible preset when opened from failed reachability guidance", () => {
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        preferCompatible
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByLabelText(en.services.connect.name)).toHaveValue(
      "DeepSeek",
    );
    expect(
      within(dialog).getByRole("combobox", {
        name: en.services.connect.preset,
      }),
    ).toHaveTextContent("DeepSeek");
  });

  it("opens the key page by preset id, never by handing the renderer a URL", async () => {
    // tauri-plugin-opener turns every `<a target="_blank">` into an IPC call,
    // so an anchor here would mean granting the whole window permission to open
    // any address. The renderer names a preset; the native side owns the URL.
    const opened: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_preset_key_page_open`,
        async ({ request }) => {
          opened.push(await request.json());
          return HttpResponse.json(null);
        },
      ),
    );

    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    expect(
      screen.queryByRole("link", { name: en.services.connect.getKey }),
    ).toBeNull();
    await userEvent.click(
      screen.getByRole("button", { name: en.services.connect.getKey }),
    );

    await waitFor(() =>
      expect(opened).toEqual([{ tool: "claude-code", preset: "official" }]),
    );
  });

  it("states the honest scope of the follow-up check", () => {
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    expect(
      screen.getByText(
        en.services.connect.afterSave.replace("{{tool}}", "Claude Code"),
      ),
    ).toBeVisible();
  });

  it("submits only name, key and model with surrounding whitespace removed", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const dialog = screen.getByRole("dialog");
    await userEvent.type(
      within(dialog).getByLabelText(en.services.connect.key),
      "  sk-secret  ",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.ds.action.connect }),
    );
    expect(onSubmit).toHaveBeenCalledWith({
      presetId: "official",
      name: "Anthropic",
      apiKey: "sk-secret",
      model: "claude-sonnet-5",
    });
  });

  it("submits a blank model when the tool supplies its own default", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderConnectModal
        profile={optionalModelProfile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const dialog = screen.getByRole("dialog");
    await userEvent.clear(
      within(dialog).getByLabelText(en.services.connect.model),
    );
    await userEvent.type(
      within(dialog).getByLabelText(en.services.connect.key),
      "sk-secret",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.ds.action.connect }),
    );
    expect(onSubmit).toHaveBeenCalledWith({
      presetId: "official",
      name: "Anthropic",
      apiKey: "sk-secret",
      model: "",
    });
    expect(
      screen.queryByText(en.services.connect.modelRequired),
    ).not.toBeInTheDocument();
  });

  it("blocks a blank model when the tool requires one", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const dialog = screen.getByRole("dialog");
    await userEvent.clear(
      within(dialog).getByLabelText(en.services.connect.model),
    );
    await userEvent.type(
      within(dialog).getByLabelText(en.services.connect.key),
      "sk-secret",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.ds.action.connect }),
    );
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText(en.services.connect.modelRequired)).toBeVisible();
  });

  it("searches compatible presets, applies their defaults, and clears the previous key", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const key = screen.getByLabelText(en.services.connect.key);
    await userEvent.type(key, "key-for-the-old-service");
    await userEvent.click(
      screen.getByRole("combobox", { name: en.services.connect.preset }),
    );
    const search = screen.getByPlaceholderText(
      en.services.connect.searchPresets,
    );
    const popover = search.closest("[data-side]");
    expect(popover).toHaveClass("max-w-[calc(100vw-2rem)]");
    expect(popover).not.toHaveClass("min-w-[22rem]");
    await userEvent.type(search, "DeepSeek");
    await userEvent.click(screen.getByRole("option", { name: /DeepSeek/ }));

    expect(screen.getByLabelText(en.services.connect.name)).toHaveValue(
      "DeepSeek",
    );
    expect(screen.getByLabelText(en.services.connect.model)).toHaveValue(
      "deepseek-v4-pro",
    );
    expect(key).toHaveValue("");
    await userEvent.type(key, "deepseek-key");
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );
    expect(onSubmit).toHaveBeenCalledWith({
      presetId: "deepseek-safe",
      name: "DeepSeek",
      apiKey: "deepseek-key",
      model: "deepseek-v4-pro",
    });
  });

  it("does not use the network until asked, then labels and sorts by local measurements", async () => {
    let requests = 0;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_presets_test`,
        async ({ request }) => {
          requests += 1;
          expect(await request.json()).toEqual({ tool: "claude-code" });
          return HttpResponse.json([
            {
              candidateId: "official",
              latencyMs: 320,
              httpStatus: 200,
              failure: null,
            },
            {
              candidateId: "deepseek-safe",
              latencyMs: 85,
              httpStatus: 204,
              failure: null,
            },
          ]);
        },
      ),
    );
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );

    const picker = screen.getByRole("combobox", {
      name: en.services.connect.preset,
    });
    await userEvent.click(picker);
    expect(requests).toBe(0);
    expect(screen.getAllByRole("option")[0]).toHaveTextContent("Anthropic API");

    await userEvent.click(
      screen.getByRole("button", { name: en.services.connect.speedTest }),
    );

    await waitFor(() => expect(requests).toBe(1));
    await screen.findByText(en.services.connect.speedSorted);
    const measured = screen.getAllByRole("option");
    expect(measured[0]).toHaveTextContent("DeepSeek");
    expect(measured[0]).toHaveTextContent("85 ms");
    expect(measured[1]).toHaveTextContent("Anthropic API");
    expect(measured[1]).toHaveTextContent("320 ms");
    expect(picker).toHaveTextContent("Anthropic API");
    expect(
      screen.getByRole("button", { name: en.services.connect.speedTestAgain }),
    ).toBeEnabled();
  });

  it("keeps preset speed-test command failures inline without exposing details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_presets_test`, () =>
        HttpResponse.json(
          {
            code: "NETWORK_ERROR",
            messageKey: "error.provider.testFailed",
            technicalMessage:
              "https://private.example.test/path?token=never-render",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );

    await userEvent.click(
      screen.getByRole("combobox", { name: en.services.connect.preset }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.services.connect.speedTest }),
    );

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(en.services.connect.speedTestFailed);
    expect(alert).not.toHaveTextContent("private.example.test");
    expect(alert).not.toHaveTextContent("never-render");
    expect(screen.getAllByRole("option")[0]).toHaveTextContent("Anthropic API");
  });

  it("labels unreachable presets, sorts them last, and keeps the selected preset", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_presets_test`, () =>
        HttpResponse.json([
          {
            candidateId: "official",
            latencyMs: null,
            httpStatus: null,
            failure: "timeout",
          },
          {
            candidateId: "deepseek-safe",
            latencyMs: 91,
            httpStatus: 200,
            failure: null,
          },
        ]),
      ),
    );
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );

    const picker = screen.getByRole("combobox", {
      name: en.services.connect.preset,
    });
    await userEvent.click(picker);
    await userEvent.click(
      screen.getByRole("button", { name: en.services.connect.speedTest }),
    );

    await screen.findByText(en.services.connect.unreachable);
    const measured = screen.getAllByRole("option");
    expect(measured[0]).toHaveTextContent("DeepSeek");
    expect(measured[0]).toHaveTextContent("91 ms");
    expect(measured[1]).toHaveTextContent("Anthropic API");
    expect(measured[1]).toHaveTextContent(en.services.connect.unreachable);
    expect(picker).toHaveTextContent("Anthropic API");
  });

  it("preserves the last measurements when a manual retest fails", async () => {
    let attempt = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_presets_test`, () => {
        attempt += 1;
        if (attempt === 1) {
          return HttpResponse.json([
            {
              candidateId: "official",
              latencyMs: 280,
              httpStatus: 200,
              failure: null,
            },
            {
              candidateId: "deepseek-safe",
              latencyMs: 72,
              httpStatus: 200,
              failure: null,
            },
          ]);
        }
        return HttpResponse.json(
          {
            code: "NETWORK_ERROR",
            messageKey: "error.provider.testFailed",
            technicalMessage: "retest failed",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          },
          { status: 500 },
        );
      }),
    );
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );

    await userEvent.click(
      screen.getByRole("combobox", { name: en.services.connect.preset }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.services.connect.speedTest }),
    );
    await screen.findByText("72 ms");
    await userEvent.click(
      screen.getByRole("button", { name: en.services.connect.speedTestAgain }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.services.connect.speedTestFailed,
    );
    expect(screen.getAllByRole("option")[0]).toHaveTextContent("DeepSeek");
    expect(screen.getAllByRole("option")[0]).toHaveTextContent("72 ms");
  });

  it("shows all required errors without calling the backend", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    await userEvent.clear(screen.getByLabelText(en.services.connect.name));
    await userEvent.clear(screen.getByLabelText(en.services.connect.model));
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getAllByRole("alert")).toHaveLength(3);
    for (const field of [
      en.services.connect.name,
      en.services.connect.key,
      en.services.connect.model,
    ]) {
      expect(screen.getByLabelText(field)).toHaveAttribute(
        "aria-invalid",
        "true",
      );
    }
  });

  it("shows the API key in plain text with a privacy description", () => {
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    const key = screen.getByLabelText(en.services.connect.key);
    expect(key).toHaveAttribute("type", "text");
    expect(key).toHaveAccessibleDescription(en.services.connect.keyHint);
  });

  it("keeps a typed key editable but blocks every submit path while actions are paused", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        mutationsBlocked
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const dialog = screen.getByRole("dialog");
    const key = within(dialog).getByLabelText(en.services.connect.key);
    await userEvent.type(key, "sk-kept-in-dialog");

    expect(
      within(dialog).getByRole("alert", {
        name: "Service actions are paused",
      }),
    ).toBeInTheDocument();
    expect(key).toHaveValue("sk-kept-in-dialog");
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.connect }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeEnabled();
    key.focus();
    await userEvent.keyboard("{Enter}");
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("keeps a failed connection safe and retryable inside the same form", async () => {
    const onSubmit = vi.fn();
    const onErrorReset = vi.fn();
    render(
      <ProviderConnectModal
        profile={profile}
        tool="claude-code"
        toolName="Claude Code"
        error={
          new NativeError({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.provider.createFailed",
            technicalMessage:
              "/private/settings.json token=never-render-this-value",
            remediation: "error.remediation.checkConnectionSettings",
            contextId: null,
          })
        }
        onOpenChange={vi.fn()}
        onErrorReset={onErrorReset}
        onSubmit={onSubmit}
      />,
    );

    const alert = screen.getByRole("alert", {
      name: "Could not finish connecting this service",
    });
    expect(alert).toHaveTextContent(en.error.provider.createFailed);
    expect(alert).toHaveTextContent(
      en.error.remediation.checkConnectionSettings,
    );
    // Technical detail stays folded behind View details, never primary copy.
    expect(
      within(alert).getByText(/private\/settings\.json/),
    ).not.toBeVisible();

    await userEvent.type(
      screen.getByLabelText(en.services.connect.key),
      "sk-kept-for-retry",
    );
    expect(onErrorReset).toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", {
        name: "Try connecting this service again",
      }),
    );
    expect(onSubmit).toHaveBeenCalledWith({
      presetId: "official",
      name: "Anthropic",
      apiKey: "sk-kept-for-retry",
      model: "claude-sonnet-5",
    });
  });

  it("renders nothing when the connection flow is closed", () => {
    render(
      <ProviderConnectModal
        profile={null}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("warns when a tool that appends its own version gets an address ending in one", async () => {
    const user = userEvent.setup();
    render(
      <ProviderConnectModal
        profile={{ ...profile, baseUrlTakesNoVersion: true }}
        tool="claude-code"
        toolName="Claude Code"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
        onCustomSubmit={vi.fn()}
      />,
    );
    const address = screen.getByLabelText(en.services.connect.baseUrl);
    const warning = en.services.form.baseUrlVersionDoubled.replace(
      /\{\{segment\}\}/g,
      "v1",
    );
    expect(screen.queryByText(warning)).toBeNull();

    await user.clear(address);
    await user.type(address, "https://relay.example.test/v1");

    expect(screen.getByText(warning)).toBeInTheDocument();
    expect(address).toHaveAccessibleDescription(warning);
  });

  it("says nothing about a trailing version for a tool that does not append one", async () => {
    const user = userEvent.setup();
    render(
      <ProviderConnectModal
        profile={profile}
        tool="codex"
        toolName="Codex"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
        onCustomSubmit={vi.fn()}
      />,
    );
    const address = screen.getByLabelText(en.services.connect.baseUrl);

    await user.clear(address);
    await user.type(address, "https://relay.example.test/v1");

    expect(
      screen.queryByText(
        en.services.form.baseUrlVersionDoubled.replace(
          /\{\{segment\}\}/g,
          "v1",
        ),
      ),
    ).toBeNull();
  });
});
