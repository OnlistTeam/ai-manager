import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import { privacyProtectionKeys } from "@/entities/privacy-protection";
import { PrivacyProtectionSwitch } from "@/features/routing-privacy";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function mount(client = createTestQueryClient()) {
  render(<PrivacyProtectionSwitch />, { wrapper: withQueryClient(client) });
  return client;
}

describe("PrivacyProtectionSwitch", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { routing: en.routing, preferences: en.preferences },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("shows one labelled switch with its description, on by default", async () => {
    mount();

    const toggle = screen.getByRole("switch", {
      name: en.routing.privacy.label,
    });
    expect(toggle).toBeDisabled();
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).toHaveAttribute("aria-checked", "true");
    expect(toggle).toHaveAccessibleDescription(en.routing.privacy.description);
    expect(screen.getAllByRole("switch")).toHaveLength(1);
  });

  it("saves the choice and shows the value the backend read back", async () => {
    const user = userEvent.setup();
    let sent: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_privacy_protection_set`,
        async ({ request }) => {
          sent = await request.json();
          return HttpResponse.json({ enabled: false });
        },
      ),
    );
    const client = mount();
    const toggle = screen.getByRole("switch", {
      name: en.routing.privacy.label,
    });
    await waitFor(() => expect(toggle).toBeEnabled());

    await user.click(toggle);

    await waitFor(() =>
      expect(toggle).toHaveAttribute("aria-checked", "false"),
    );
    expect(sent).toEqual({ enabled: false });
    expect(
      await screen.findByRole("status", {
        name: en.preferences.advanced.saved,
      }),
    ).toBeInTheDocument();
    expect(client.getQueryData(privacyProtectionKeys.current())).toEqual({
      enabled: false,
    });
  });

  it("keeps the stored value and says so when saving fails", async () => {
    const user = userEvent.setup();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_privacy_protection_set`, () =>
        HttpResponse.json(
          {
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.settings.saveFailed",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    mount();
    const toggle = screen.getByRole("switch", {
      name: en.routing.privacy.label,
    });
    await waitFor(() => expect(toggle).toBeEnabled());

    await user.click(toggle);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.preferences.advanced.error,
    );
    expect(toggle).toHaveAttribute("aria-checked", "true");
  });

  it("offers a retry when the setting cannot be read", async () => {
    const user = userEvent.setup();
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_privacy_protection_get`, () => {
        calls += 1;
        return calls === 1
          ? HttpResponse.json({ enabled: "unknown" })
          : HttpResponse.json({ enabled: false });
      }),
    );
    mount();

    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.routing.privacy.unavailable,
    );
    const toggle = screen.getByRole("switch", {
      name: en.routing.privacy.label,
    });
    expect(toggle).toBeDisabled();

    await user.click(
      screen.getByRole("button", { name: en.preferences.retry }),
    );

    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
