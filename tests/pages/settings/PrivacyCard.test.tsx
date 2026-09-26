import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import { PrivacyCard } from "@/pages/settings/PrivacyCard";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

interface Stored {
  maskSecrets: boolean;
  maskPersonal: boolean;
  words: string[];
}

function nativeError(messageKey: string) {
  return HttpResponse.json(
    {
      code: "CONFIG_WRITE_FAILED",
      messageKey,
      technicalMessage: null,
      remediation: null,
      contextId: null,
    },
    { status: 500 },
  );
}

/** A backend that stores patches like the real one: split, trim, keep. */
function backend(initial: Stored) {
  let stored = { ...initial };
  const patches: unknown[] = [];
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_privacy_protection_get`, () =>
      HttpResponse.json(stored),
    ),
    http.post(
      `${TAURI_ENDPOINT}/app_privacy_protection_set`,
      async ({ request }) => {
        const { patch } = (await request.json()) as {
          patch: Partial<Stored>;
        };
        patches.push(patch);
        if (patch.words?.some((word) => word.length > 128)) {
          return nativeError("error.privacy.wordTooLong");
        }
        stored = { ...stored, ...patch };
        return HttpResponse.json(stored);
      },
    ),
  );
  return patches;
}

function mount() {
  return render(<PrivacyCard />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

function wordsField() {
  return screen.getByRole("textbox", {
    name: en.preferences.privacy.words.label,
  });
}

describe("PrivacyCard", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, preferences: en.preferences },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("shows the three choices with their stored values", async () => {
    backend({
      maskSecrets: true,
      maskPersonal: false,
      words: ["acme", "张三"],
    });
    mount();

    const secrets = await screen.findByRole("switch", {
      name: en.preferences.privacy.secrets.label,
    });
    expect(secrets).toBeChecked();
    expect(
      screen.getByRole("switch", {
        name: en.preferences.privacy.personal.label,
      }),
    ).not.toBeChecked();
    expect(wordsField()).toHaveValue("acme, 张三");
    expect(wordsField()).toHaveAttribute(
      "placeholder",
      en.preferences.privacy.words.placeholder,
    );
    expect(
      screen.getByText(en.preferences.privacy.description),
    ).toBeInTheDocument();
  });

  it("saves a toggled switch as a one-field patch and says so", async () => {
    const patches = backend({
      maskSecrets: true,
      maskPersonal: false,
      words: [],
    });
    mount();

    const personal = await screen.findByRole("switch", {
      name: en.preferences.privacy.personal.label,
    });
    await userEvent.click(personal);

    await waitFor(() => expect(personal).toBeChecked());
    expect(patches).toEqual([{ maskPersonal: true }]);
    expect(
      await screen.findByRole("status", {
        name: en.preferences.advanced.saved,
      }),
    ).toBeInTheDocument();
  });

  it("saves the words when the field loses focus and shows them as stored", async () => {
    const patches = backend({
      maskSecrets: true,
      maskPersonal: false,
      words: [],
    });
    mount();

    const field = await screen.findByRole("textbox", {
      name: en.preferences.privacy.words.label,
    });
    await userEvent.type(field, "acme，Project Kite,, x");
    await userEvent.tab();

    await waitFor(() =>
      expect(patches).toEqual([{ words: ["acme", "Project Kite"] }]),
    );
    await waitFor(() => expect(field).toHaveValue("acme, Project Kite"));
  });

  it("saves the words on Enter and does not save them again on blur", async () => {
    const patches = backend({
      maskSecrets: true,
      maskPersonal: false,
      words: ["acme"],
    });
    mount();

    const field = await screen.findByRole("textbox", {
      name: en.preferences.privacy.words.label,
    });
    await userEvent.type(field, ", kite{Enter}");
    await waitFor(() => expect(field).toHaveValue("acme, kite"));
    await userEvent.tab();

    expect(patches).toEqual([{ words: ["acme", "kite"] }]);
  });

  it("does not save words that did not change", async () => {
    const patches = backend({
      maskSecrets: true,
      maskPersonal: false,
      words: ["acme", "kite"],
    });
    mount();

    const field = await screen.findByRole("textbox", {
      name: en.preferences.privacy.words.label,
    });
    await userEvent.clear(field);
    await userEvent.type(field, " acme ,kite, a{Enter}");

    expect(patches).toEqual([]);
    expect(field).toHaveValue("acme, kite");
  });

  it("shows why the words could not be saved and keeps the draft", async () => {
    backend({ maskSecrets: true, maskPersonal: false, words: [] });
    mount();

    const field = await screen.findByRole("textbox", {
      name: en.preferences.privacy.words.label,
    });
    const long = "x".repeat(129);
    await userEvent.click(field);
    await userEvent.paste(long);
    await userEvent.keyboard("{Enter}");

    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.error.privacy.wordTooLong,
    );
    expect(field).toHaveValue(long);
    expect(field).toHaveAttribute("aria-invalid", "true");
  });

  it("offers a retry when the settings cannot be read", async () => {
    let fail = true;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_privacy_protection_get`, () =>
        fail
          ? nativeError("error.settings.loadFailed")
          : HttpResponse.json({
              maskSecrets: true,
              maskPersonal: false,
              words: [],
            }),
      ),
    );
    mount();

    const title = await screen.findByText(en.preferences.privacy.error.title);
    fail = false;
    const card = title.closest("section");
    expect(card).not.toBeNull();
    await userEvent.click(
      within(card as HTMLElement).getByRole("button", {
        name: en.preferences.retry,
      }),
    );

    expect(
      await screen.findByRole("switch", {
        name: en.preferences.privacy.secrets.label,
      }),
    ).toBeChecked();
  });
});
