import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { ProviderConnectModal } from "@/features/provider-management";
import type {
  ProviderConnectionPreset,
  ProviderConnectionProfile,
} from "@/entities/provider";
import { NativeError } from "@/native";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const anthropic: ProviderConnectionPreset = {
  id: "official",
  serviceName: "Anthropic API",
  defaultName: "Anthropic",
  defaultModel: "claude-sonnet-5",
  baseUrl: "https://api.anthropic.com",
  websiteUrl: "https://www.anthropic.com",
  apiKeyUrl: "https://console.anthropic.com",
  official: true,
  kind: "vendor",
};

const onList: ProviderConnectionPreset = {
  id: "onlist",
  serviceName: "onList",
  defaultName: "onList",
  defaultModel: null,
  baseUrl: "https://onlist.io",
  websiteUrl: "https://onlist.io",
  apiKeyUrl: "https://onlist.io",
  official: false,
  kind: "relay",
};

const ollama: ProviderConnectionPreset = {
  id: "ollama",
  serviceName: "Ollama",
  defaultName: "Ollama",
  defaultModel: "qwen3-coder",
  baseUrl: "http://localhost:11434",
  websiteUrl: "https://ollama.com",
  apiKeyUrl: "https://ollama.com",
  official: false,
  kind: "local",
};

const profile: ProviderConnectionProfile = {
  defaultPresetId: "official",
  modelRequired: true,
  baseUrlTakesNoVersion: false,
  toolLogin: null,
  presets: [anthropic, onList, ollama],
};

const optionalModelProfile: ProviderConnectionProfile = {
  ...profile,
  modelRequired: false,
};

type ModalProps = ComponentProps<typeof ProviderConnectModal>;

function renderModal(overrides: Partial<ModalProps> = {}) {
  const props: ModalProps = {
    target: { kind: "preset", preset: anthropic },
    profile,
    tool: "claude-code",
    toolName: "Claude Code",
    onOpenChange: vi.fn(),
    onSubmit: vi.fn(),
    onCustomSubmit: vi.fn(),
    ...overrides,
  };
  render(<ProviderConnectModal {...props} />);
  return props;
}

describe("ProviderConnectModal", () => {
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

  it("opens on the picked preset's defaults and starts on the key", () => {
    renderModal();
    const dialog = screen.getByRole("dialog", {
      name: en.services.connect.title.replace("{{service}}", "Anthropic API"),
    });
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

  it("shows a preset's real address but never lets it be edited", async () => {
    const props = renderModal();
    // The address is always in view, so a key is never saved against an
    // endpoint nobody looked at; a different one is the Custom card's job.
    const address = screen.getByLabelText(en.services.connect.baseUrl);
    expect(address).toHaveValue("https://api.anthropic.com");
    expect(address).toHaveAttribute("readonly");
    await userEvent.type(address, "/evil");
    expect(address).toHaveValue("https://api.anthropic.com");

    await userEvent.type(
      screen.getByLabelText(en.services.connect.key),
      "sk-secret",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );
    expect(props.onCustomSubmit).not.toHaveBeenCalled();
    expect(props.onSubmit).toHaveBeenCalledWith({
      presetId: "official",
      name: "Anthropic",
      apiKey: "sk-secret",
      model: "claude-sonnet-5",
    });
  });

  it("submits only name, key and model with surrounding whitespace removed", async () => {
    const props = renderModal();
    await userEvent.type(
      screen.getByLabelText(en.services.connect.key),
      "  sk-secret  ",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );
    expect(props.onSubmit).toHaveBeenCalledWith({
      presetId: "official",
      name: "Anthropic",
      apiKey: "sk-secret",
      model: "claude-sonnet-5",
    });
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
    renderModal();
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
    renderModal();
    expect(
      screen.getByText(
        en.services.connect.afterSave.replace("{{tool}}", "Claude Code"),
      ),
    ).toBeVisible();
  });

  it("leaves the model to the tool for a preset that names none", async () => {
    const props = renderModal({
      target: { kind: "preset", preset: onList },
      profile: optionalModelProfile,
    });
    expect(screen.getByLabelText(en.services.connect.model)).toHaveValue("");
    await userEvent.type(
      screen.getByLabelText(en.services.connect.key),
      "sk-secret",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );
    expect(props.onSubmit).toHaveBeenCalledWith({
      presetId: "onlist",
      name: "onList",
      apiKey: "sk-secret",
      model: "",
    });
  });

  it("blocks a blank model when the tool requires one", async () => {
    const props = renderModal();
    await userEvent.clear(screen.getByLabelText(en.services.connect.model));
    await userEvent.type(
      screen.getByLabelText(en.services.connect.key),
      "sk-secret",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );
    expect(props.onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText(en.services.connect.modelRequired)).toBeVisible();
  });

  it("lets a server on this machine go without a key", async () => {
    const props = renderModal({ target: { kind: "preset", preset: ollama } });
    const key = screen.getByLabelText(en.services.connect.key);
    expect(key).toHaveAccessibleDescription(
      en.services.connect.keyOptionalHint,
    );
    // Nothing to fetch a key from on this machine.
    expect(
      screen.queryByRole("button", { name: en.services.connect.getKey }),
    ).toBeNull();
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );
    expect(props.onSubmit).toHaveBeenCalledWith({
      presetId: "ollama",
      name: "Ollama",
      apiKey: "",
      model: "qwen3-coder",
    });
  });

  it("shows all required errors without calling the backend", async () => {
    const props = renderModal();
    await userEvent.clear(screen.getByLabelText(en.services.connect.name));
    await userEvent.clear(screen.getByLabelText(en.services.connect.model));
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.connect }),
    );
    expect(props.onSubmit).not.toHaveBeenCalled();
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
    renderModal();
    const key = screen.getByLabelText(en.services.connect.key);
    expect(key).toHaveAttribute("type", "text");
    expect(key).toHaveAccessibleDescription(en.services.connect.keyHint);
  });

  it("keeps a typed key editable but blocks every submit path while actions are paused", async () => {
    const props = renderModal({ mutationsBlocked: true });
    const dialog = screen.getByRole("dialog");
    const key = within(dialog).getByLabelText(en.services.connect.key);
    await userEvent.type(key, "sk-kept-in-dialog");
    expect(
      within(dialog).getByRole("alert", {
        name: "Endpoint actions are paused",
      }),
    ).toBeInTheDocument();
    expect(key).toHaveValue("sk-kept-in-dialog");
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.connect }),
    ).toBeDisabled();
    key.focus();
    await userEvent.keyboard("{Enter}");
    expect(props.onSubmit).not.toHaveBeenCalled();
  });

  it("keeps a failed connection safe and retryable inside the same form", async () => {
    const onErrorReset = vi.fn();
    const props = renderModal({
      error: new NativeError({
        code: "CONFIG_WRITE_FAILED",
        messageKey: "error.provider.createFailed",
        technicalMessage:
          "/private/settings.json token=never-render-this-value",
        remediation: "error.remediation.checkConnectionSettings",
        contextId: null,
      }),
      onErrorReset,
    });
    const alert = screen.getByRole("alert", {
      name: "Could not add this endpoint",
    });
    expect(alert).toHaveTextContent(en.error.provider.createFailed);
    expect(
      within(alert).getByText(/private\/settings\.json/),
    ).not.toBeVisible();

    await userEvent.type(
      screen.getByLabelText(en.services.connect.key),
      "sk-kept-for-retry",
    );
    expect(onErrorReset).toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", { name: "Try adding this endpoint again" }),
    );
    expect(props.onSubmit).toHaveBeenCalledWith({
      presetId: "official",
      name: "Anthropic",
      apiKey: "sk-kept-for-retry",
      model: "claude-sonnet-5",
    });
  });

  it("renders nothing without a picked card", () => {
    renderModal({ target: null });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  describe("custom address", () => {
    it("starts on an empty, editable address and submits through the custom path", async () => {
      const props = renderModal({ target: { kind: "custom" } });
      const dialog = screen.getByRole("dialog", {
        name: en.services.connect.customTitle,
      });
      const address = within(dialog).getByLabelText(
        en.services.connect.baseUrl,
      );
      expect(address).toHaveValue("");
      expect(address).not.toHaveAttribute("readonly");
      expect(address).toHaveFocus();

      await userEvent.type(address, "https://relay.example.test/v1/");
      await userEvent.type(
        within(dialog).getByLabelText(en.services.connect.name),
        "Team relay",
      );
      await userEvent.type(
        within(dialog).getByLabelText(en.services.connect.key),
        "sk-secret",
      );
      await userEvent.type(
        within(dialog).getByLabelText(en.services.connect.model),
        "model-a",
      );
      await userEvent.click(
        within(dialog).getByRole("button", { name: en.ds.action.connect }),
      );
      expect(props.onSubmit).not.toHaveBeenCalled();
      expect(props.onCustomSubmit).toHaveBeenCalledWith({
        name: "Team relay",
        baseUrl: "https://relay.example.test/v1",
        apiKey: "sk-secret",
        model: "model-a",
      });
    });

    it("takes a keyless server on this machine over plain HTTP", async () => {
      const props = renderModal({
        target: { kind: "custom" },
        profile: optionalModelProfile,
      });
      await userEvent.type(
        screen.getByLabelText(en.services.connect.baseUrl),
        "http://localhost:1234/v1",
      );
      await userEvent.type(
        screen.getByLabelText(en.services.connect.name),
        "LM Studio",
      );
      await userEvent.click(
        screen.getByRole("button", { name: en.ds.action.connect }),
      );
      expect(props.onCustomSubmit).toHaveBeenCalledWith({
        name: "LM Studio",
        baseUrl: "http://localhost:1234/v1",
        apiKey: "",
        model: "",
      });
    });

    it("refuses plain HTTP off this machine and a missing key there", async () => {
      const props = renderModal({ target: { kind: "custom" } });
      await userEvent.type(
        screen.getByLabelText(en.services.connect.baseUrl),
        "http://relay.example.test/v1",
      );
      await userEvent.type(
        screen.getByLabelText(en.services.connect.name),
        "Relay",
      );
      await userEvent.click(
        screen.getByRole("button", { name: en.ds.action.connect }),
      );
      expect(props.onCustomSubmit).not.toHaveBeenCalled();
      expect(
        screen.getByText(en.services.connect.baseUrlInvalid),
      ).toBeVisible();
      expect(screen.getByText(en.services.connect.keyRequired)).toBeVisible();
    });

    it("warns when a tool that appends its own version gets an address ending in one", async () => {
      renderModal({
        target: { kind: "custom" },
        profile: { ...profile, baseUrlTakesNoVersion: true },
      });
      const address = screen.getByLabelText(en.services.connect.baseUrl);
      const warning = en.services.form.baseUrlVersionDoubled.replace(
        /\{\{segment\}\}/g,
        "v1",
      );
      expect(screen.queryByText(warning)).toBeNull();
      await userEvent.type(address, "https://relay.example.test/v1");
      expect(address).toHaveAccessibleDescription(warning);
    });

    it("says nothing about a trailing version for a tool that does not append one", async () => {
      renderModal({ target: { kind: "custom" }, tool: "codex" });
      await userEvent.type(
        screen.getByLabelText(en.services.connect.baseUrl),
        "https://relay.example.test/v1",
      );
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
});
