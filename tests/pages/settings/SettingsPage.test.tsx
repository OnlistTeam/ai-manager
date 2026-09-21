import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { APP_LANGUAGES, LANGUAGE_ENDONYMS } from "@/i18n";
import en from "@/i18n/locales/en.json";
import zh from "@/i18n/locales/zh.json";
import { SettingsPage } from "@/pages/settings/SettingsPage";
import { isMac } from "@/lib/platform";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const { checkMock, getVersionMock, toastErrorMock } = vi.hoisted(() => ({
  checkMock: vi.fn(),
  getVersionMock: vi.fn(),
  toastErrorMock: vi.fn(),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: getVersionMock }));
vi.mock("@/lib/platform", () => ({ isMac: vi.fn(() => false) }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: checkMock }));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: toastErrorMock },
}));

const SETTINGS = {
  advancedMode: false,
  importPromptSeen: true,
  toolScope: null,
  extensionScope: null,
  extensionKind: null,
  downloadStrategy: "automatic",
  automaticProviderFailover: false,
  terminalApp: null,
};
const LIST = { files: [] };

function mount() {
  return render(<SettingsPage />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("SettingsPage", () => {
  beforeEach(async () => {
    window.localStorage.clear();
    document.documentElement.classList.remove("light", "dark");
    getVersionMock.mockReset().mockResolvedValue("1.0.0");
    checkMock.mockReset().mockResolvedValue(null);
    toastErrorMock.mockReset();
    vi.mocked(isMac).mockReturnValue(false);
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, preferences: en.preferences },
      true,
      true,
    );
    i18n.addResourceBundle(
      "zh",
      "translation",
      { ds: zh.ds, error: zh.error, preferences: zh.preferences },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(SETTINGS),
      ),
      http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, () =>
        HttpResponse.json({ configured: false, url: null, protected: false }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_desktop_preferences_get`, () =>
        HttpResponse.json({
          launchOnStartup: false,
          silentStartup: false,
          showInTray: true,
          minimizeToTrayOnClose: true,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/set_window_theme`, () =>
        HttpResponse.json(null),
      ),
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.json(LIST),
      ),
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({
          available: false,
          summary: { services: 0, mcpServers: 0, skills: 0 },
        }),
      ),
    );
  });

  it("announces that it is loading before anything is shown", () => {
    mount();
    expect(
      screen.getByRole("status", { name: en.preferences.loading }),
    ).toBeInTheDocument();
  });

  it("offers a retry when the settings cannot be read", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "UPSTREAM_ERROR",
            messageKey: "error.settings.loadFailed",
            technicalMessage: "sqlite says no",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    mount();
    expect(
      await screen.findByText(en.preferences.error.title),
    ).toBeInTheDocument();
    // Spec §42: no technical detail is ever shown on screen.
    expect(screen.queryByText(/sqlite/i)).toBeNull();
  });

  it("keeps the settings retry busy and returns focus after recovery", async () => {
    let reads = 0;
    let releaseRetry: (() => void) | undefined;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, async () => {
        reads += 1;
        if (reads === 1) {
          return HttpResponse.text(
            JSON.stringify({
              code: "UPSTREAM_ERROR",
              messageKey: "error.settings.loadFailed",
              technicalMessage: "private settings database",
              remediation: "error.remediation.retryOrViewDetails",
              contextId: null,
            }),
            { status: 500 },
          );
        }
        await new Promise<void>((resolve) => {
          releaseRetry = resolve;
        });
        return HttpResponse.json(SETTINGS);
      }),
    );
    mount();

    const alert = await screen.findByRole("alert", {
      name: en.preferences.error.title,
    });
    const retry = screen.getByRole("button", {
      name: en.preferences.retry,
    });

    await userEvent.click(retry);
    await waitFor(() => expect(reads).toBe(2));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toHaveAttribute("aria-busy", "true");
    expect(
      screen.queryByRole("status", { name: en.preferences.loading }),
    ).toBeNull();
    expect(document.body).not.toHaveTextContent("private settings database");

    act(() => releaseRetry?.());
    const settingsRegion = await screen.findByRole("region", {
      name: en.preferences.title,
    });
    await waitFor(() => expect(settingsRegion).toHaveFocus());
    expect(alert).not.toBeInTheDocument();
    expect(
      screen.getByRole("combobox", {
        name: en.preferences.experience.language.label,
      }),
    ).toBeEnabled();
  });

  it("returns focus to the same settings retry after another read failure", async () => {
    let reads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () => {
        reads += 1;
        return HttpResponse.text(
          JSON.stringify({
            code: "UPSTREAM_ERROR",
            messageKey: "error.settings.loadFailed",
            technicalMessage: `private settings attempt ${reads}`,
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        );
      }),
    );
    mount();

    const retry = await screen.findByRole("button", {
      name: en.preferences.retry,
    });
    await userEvent.click(retry);

    await waitFor(() => expect(reads).toBe(2));
    await waitFor(() => expect(retry).toBeEnabled());
    expect(screen.getByRole("button", { name: en.preferences.retry })).toBe(
      retry,
    );
    expect(retry).toHaveFocus();
    expect(
      screen.getByRole("alert", { name: en.preferences.error.title }),
    ).not.toHaveAttribute("aria-busy");
    expect(document.body).not.toHaveTextContent("private settings attempt");
  });

  it("lays out one column of four groups in a fixed order", async () => {
    mount();
    expect(
      await screen.findByRole("heading", { name: en.preferences.title }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: en.home.health.title }),
    ).toBeNull();
    await screen.findByRole("heading", {
      level: 2,
      name: en.preferences.groups.general,
    });
    expect(
      screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent),
    ).toEqual([
      en.preferences.groups.general,
      en.preferences.groups.network,
      en.preferences.groups.backup,
      en.preferences.groups.about,
    ]);
    // The backup section's data is a separate round trip, so it arrives after the page title.
    expect(
      await screen.findByRole("heading", {
        level: 3,
        name: en.preferences.backup.title,
      }),
    ).toBeInTheDocument();
    const backupGroup = screen
      .getByRole("heading", { name: en.preferences.groups.backup })
      .closest("section");
    expect(backupGroup).toContainElement(
      await screen.findByRole("heading", {
        level: 3,
        name: en.preferences.import.reimport,
      }),
    );
    expect(
      screen.getByRole("heading", {
        level: 3,
        name: en.preferences.updates.title,
      }),
    ).toBeInTheDocument();
    expect(document.querySelector(".lg\\:grid-cols-2")).toBeNull();
  });

  it("says backups run daily and saves switching them off", async () => {
    let received: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_backup_schedule_save`,
        async ({ request }) => {
          received = await request.json();
          return HttpResponse.json({ automatic: false, retainCount: 10 });
        },
      ),
    );
    mount();
    const toggle = await screen.findByRole("switch", {
      name: en.preferences.backup.automatic,
    });
    expect(toggle).toBeChecked();
    expect(
      screen.getByText(
        en.preferences.backup.summaryLine_other.replace("{{count}}", "10"),
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.preferences.backup.exportToOther }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: en.preferences.backup.importFromOther,
      }),
    ).toBeInTheDocument();

    await userEvent.click(toggle);

    await waitFor(() =>
      expect(received).toEqual({
        schedule: { automatic: false, retainCount: 10 },
      }),
    );
    expect(
      await screen.findByText(en.preferences.backup.automaticOff),
    ).toBeInTheDocument();
  });

  it("keeps language preferences but does not expose a theme switch", async () => {
    mount();

    const language = await screen.findByRole("combobox", {
      name: en.preferences.experience.language.label,
    });
    expect(language).toHaveValue("en");
    // Endonyms, so the list reads the same whatever the interface language is:
    // a Korean reader looks for 한국어, not for what English calls Korean.
    expect(
      within(language)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(APP_LANGUAGES.map((code) => LANGUAGE_ENDONYMS[code]));
    expect(
      within(language)
        .getAllByRole<HTMLOptionElement>("option")
        .map((option) => option.value),
    ).toEqual([...APP_LANGUAGES]);
    expect(language).toHaveAccessibleDescription(
      en.preferences.experience.language.description,
    );
    expect(document.querySelector(".lucide-globe")).toBeInTheDocument();
    expect(document.querySelector(".lucide-languages")).not.toBeInTheDocument();
  });

  it("applies and persists language changes immediately", async () => {
    mount();
    const language = await screen.findByRole("combobox", {
      name: en.preferences.experience.language.label,
    });

    await userEvent.selectOptions(language, "zh");

    await waitFor(() => {
      expect(window.localStorage.getItem("language")).toBe("zh");
      expect(document.documentElement.lang).toBe("zh");
    });
    expect(
      screen.getByRole("heading", { name: zh.preferences.title, level: 1 }),
    ).toBeInTheDocument();
  });

  it("removes the mode switch and keeps storage details collapsed", async () => {
    mount();
    expect(
      screen.queryByRole("switch", { name: en.preferences.advanced.label }),
    ).toBeNull();
    const disclosure = await screen.findByText(en.preferences.backup.details);
    expect(disclosure.parentElement).not.toHaveAttribute("open");
    await userEvent.click(disclosure);
    expect(disclosure.parentElement).toHaveAttribute("open");
    expect(screen.getByText(en.preferences.backup.folder)).toBeInTheDocument();
  });

  it("keeps automatic fallback region-neutral while exposing recovery to every user", async () => {
    mount();

    expect(screen.queryByText(en.preferences.advanced.label)).toBeNull();
    expect(
      screen.queryByRole("radio", { name: "Official sources" }),
    ).toBeNull();
    expect(
      screen.queryByRole("radio", { name: "China resilience" }),
    ).toBeNull();
    // The recovery entry point is there for everyone; it's just tucked behind a toggle instead of always showing an address bar.
    expect(
      await screen.findByRole("switch", {
        name: en.preferences.network.proxy.title,
      }),
    ).toBeVisible();
  });

  it("keeps automatic provider failover off by default and saves an explicit opt-in", async () => {
    let received: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        received = await request.json();
        return HttpResponse.json({
          ...SETTINGS,
          automaticProviderFailover: true,
        });
      }),
    );
    mount();

    const toggle = await screen.findByRole("switch", {
      name: en.preferences.providerFailover.label,
    });
    expect(toggle).not.toBeChecked();

    await userEvent.click(toggle);

    await waitFor(() =>
      expect(received).toEqual({
        settings: { ...SETTINGS, automaticProviderFailover: true },
      }),
    );
    expect(toggle).toBeChecked();
    expect(toggle).toHaveAccessibleDescription(
      `${en.preferences.providerFailover.description} ${en.preferences.advanced.saved}`,
    );
  });

  it("exposes safe startup and tray preferences", async () => {
    mount();

    expect(
      await screen.findByRole("switch", {
        name: en.preferences.desktop.launch.label,
      }),
    ).not.toBeChecked();
    expect(
      screen.getByRole("switch", {
        name: en.preferences.desktop.silent.label,
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("switch", { name: en.preferences.desktop.silent.label }),
    ).toHaveAccessibleDescription(en.preferences.desktop.silent.requiresLaunch);
    expect(
      screen.getByRole("switch", {
        name: "Show AI Manager in the system tray",
      }),
    ).toBeChecked();
    expect(
      screen.getByRole("switch", {
        name: en.preferences.desktop.close.label,
      }),
    ).toBeChecked();
  });

  it.each([
    {
      mac: true,
      location: "menu bar",
      launchDescription: en.preferences.desktop.launch.descriptionMac,
    },
    {
      mac: false,
      location: "system tray",
      launchDescription: en.preferences.desktop.launch.description,
    },
  ])(
    "uses $location wording for the desktop controls",
    async ({ mac, location, launchDescription }) => {
      vi.mocked(isMac).mockReturnValue(mac);
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_desktop_preferences_get`, () =>
          HttpResponse.json({
            launchOnStartup: true,
            silentStartup: false,
            showInTray: true,
            minimizeToTrayOnClose: true,
          }),
        ),
      );
      mount();
      expect(
        await screen.findByRole("switch", {
          name: `Show AI Manager in the ${location}`,
        }),
      ).toBeChecked();
      expect(
        screen.getByRole("switch", {
          name: en.preferences.desktop.launch.label,
        }),
      ).toHaveAccessibleDescription(launchDescription);
      expect(
        screen.getByRole("switch", {
          name: en.preferences.desktop.silent.label,
        }),
      ).toBeEnabled();
      expect(
        screen.getByRole("switch", {
          name: en.preferences.desktop.silent.label,
        }),
      ).toHaveAccessibleDescription(
        en.preferences.desktop.silent.description.replace(
          "{{location}}",
          location,
        ),
      );
      expect(
        screen.getByRole("switch", {
          name: en.preferences.desktop.close.label,
        }),
      ).toHaveAccessibleDescription(
        en.preferences.desktop.close.description.replace(
          "{{location}}",
          location,
        ),
      );
    },
  );

  it("explains why background startup and keep-running require a tray entry", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_preferences_get`, () =>
        HttpResponse.json({
          launchOnStartup: true,
          silentStartup: false,
          showInTray: false,
          minimizeToTrayOnClose: false,
        }),
      ),
    );
    mount();
    const silent = await screen.findByRole("switch", {
      name: en.preferences.desktop.silent.label,
    });
    const close = screen.getByRole("switch", {
      name: en.preferences.desktop.close.label,
    });
    for (const control of [silent, close]) {
      expect(control).toBeDisabled();
      expect(control).toHaveAccessibleDescription(
        en.preferences.desktop.requiresTray.replace(
          "{{location}}",
          "system tray",
        ),
      );
    }
  });

  it("turning off the tray also removes hidden-window behaviours", async () => {
    let received: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_preferences_save`,
        async ({ request }) => {
          received = await request.json();
          return HttpResponse.json({
            launchOnStartup: false,
            silentStartup: false,
            showInTray: false,
            minimizeToTrayOnClose: false,
          });
        },
      ),
    );
    mount();
    const tray = await screen.findByRole("switch", {
      name: "Show AI Manager in the system tray",
    });

    await userEvent.click(tray);

    await waitFor(() =>
      expect(received).toEqual({
        settings: {
          launchOnStartup: false,
          silentStartup: false,
          showInTray: false,
          minimizeToTrayOnClose: false,
        },
      }),
    );
    expect(tray).not.toBeChecked();
  });

  it("lets users route signed updates and native installers through a local proxy", async () => {
    let received: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_network_proxy_save`,
        async ({ request }) => {
          received = await request.json();
          return HttpResponse.json({
            configured: true,
            url: "http://127.0.0.1:7890",
            protected: false,
          });
        },
      ),
    );
    mount();

    // The address bar is collapsed by default: the toggle has to be turned on before the address field appears.
    await userEvent.click(
      await screen.findByRole("switch", {
        name: en.preferences.network.proxy.title,
      }),
    );

    const input = await screen.findByRole("textbox", {
      name: en.preferences.network.proxy.label,
    });
    await userEvent.type(input, "http://127.0.0.1:7890");
    await userEvent.click(
      screen.getByRole("button", {
        name: en.preferences.network.proxy.save,
      }),
    );

    await waitFor(() =>
      expect(received).toEqual({ url: "http://127.0.0.1:7890" }),
    );
    expect(
      await screen.findByText(en.preferences.network.proxy.active),
    ).toBeInTheDocument();
  });

  it("does not reveal a protected legacy proxy", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_network_proxy_get`, () =>
        HttpResponse.json({ configured: true, url: null, protected: true }),
      ),
    );
    mount();

    expect(
      await screen.findByText(en.preferences.network.proxy.protectedHint),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("textbox", { name: en.preferences.network.proxy.label }),
    ).toHaveValue("");
    expect(screen.queryByText(/user:secret|proxy\.example/i)).toBeNull();
  });

  it("shows a development build neutrally without contacting upstream", async () => {
    mount();
    expect(
      await screen.findByText(en.preferences.updates.devBuild),
    ).toBeInTheDocument();
    expect(
      screen.getByText(en.preferences.updates.devBuildNote),
    ).toBeInTheDocument();
    expect(screen.queryByText(en.preferences.updates.upToDate)).toBeNull();
    expect(checkMock).not.toHaveBeenCalled();
  });
});
