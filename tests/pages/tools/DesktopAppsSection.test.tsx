import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { delay, http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { DesktopAppsSection } from "@/pages/tools/DesktopAppsSection";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const toastMocks = vi.hoisted(() => ({ success: vi.fn() }));
vi.mock("sonner", () => ({
  toast: { success: toastMocks.success },
}));

const inventory = [
  {
    id: "codex-app",
    name: "ChatGPT / Codex",
    status: "installed",
    version: "26.527.60818",
    latestVersion: null,
    relatedTool: "codex",
    configurationRelationship: "sharedConfiguration",
    canLaunch: true,
    environment: "macos",
    installerHandoff: "directOfficialPackage",
    uninstallHandoff: "revealApplication",
    updatesManagedByVendor: true,
    canRollback: false,
    canManageMcp: false,
  },
  {
    id: "claude-desktop",
    name: "Claude Desktop",
    status: "notInstalled",
    version: null,
    latestVersion: null,
    relatedTool: "claude-code",
    configurationRelationship: "separateConfiguration",
    canLaunch: false,
    environment: "macos",
    installerHandoff: "directOfficialPackage",
    uninstallHandoff: "revealApplication",
    updatesManagedByVendor: true,
    canRollback: false,
    canManageMcp: false,
  },
];

function mount() {
  return render(<DesktopAppsSection />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("DesktopAppsSection", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { error: en.error, tools: en.tools },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.success.mockClear();
  });

  it("shows compact app status, version and CLI relationships", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json(inventory),
      ),
    );
    mount();

    const codex = await screen.findByRole("article", {
      name: "ChatGPT / Codex",
    });
    const claude = screen.getByRole("article", { name: "Claude Desktop" });
    expect(codex).toHaveTextContent("26.527.60818");
    expect(codex).toHaveTextContent("Shares settings with Codex CLI");
    expect(codex).toHaveTextContent("Vendor-managed after installation");
    expect(codex).not.toHaveTextContent("No public rollback versions");
    expect(claude).toHaveTextContent("Separate from Claude Code");
    expect(
      within(codex).getByRole("button", { name: "Open ChatGPT / Codex" }),
    ).toBeEnabled();
    expect(
      within(codex).getByRole("button", {
        name: "More actions for ChatGPT / Codex",
      }),
    ).toBeEnabled();
    await userEvent.click(
      within(codex).getByRole("button", {
        name: "More actions for ChatGPT / Codex",
      }),
    );
    expect(
      screen.getByRole("menuitem", { name: "Download latest installer" }),
    ).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(
      within(claude).getByRole("button", {
        name: "Open the official download for Claude Desktop",
      }),
    ).toBeEnabled();
    expect(
      within(claude).getByRole("button", {
        name: "Open the official download for Claude Desktop",
      }),
    ).toHaveTextContent("Download official installer");
  });

  it("opens with only the app id and reports the handoff", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json(inventory),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_app_launch`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json("launched");
        },
      ),
    );
    mount();
    const codex = await screen.findByRole("article", {
      name: "ChatGPT / Codex",
    });
    await userEvent.click(
      within(codex).getByRole("button", { name: "Open ChatGPT / Codex" }),
    );
    await waitFor(() => expect(body).toEqual({ app: "codex-app" }));
    expect(toastMocks.success).toHaveBeenCalledWith(
      "ChatGPT / Codex is opening.",
    );
  });

  it("shows a proven Codex update and keeps launch as a secondary action", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json([
          {
            ...inventory[0],
            status: "updateAvailable",
            latestVersion: "26.825.51511",
          },
        ]),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_app_open_official_download`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({ handoff: "directOfficialPackage" });
        },
      ),
    );
    mount();

    const codex = await screen.findByRole("article", {
      name: "ChatGPT / Codex",
    });
    expect(codex).toHaveTextContent("Update available");
    expect(codex).toHaveTextContent(
      "Official version 26.825.51511 is available",
    );

    await userEvent.click(
      within(codex).getByRole("button", {
        name: "More actions for ChatGPT / Codex",
      }),
    );
    expect(
      screen.getByRole("menuitem", { name: "Open app" }),
    ).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");

    await userEvent.click(
      within(codex).getByRole("button", {
        name: "Download the official update for ChatGPT / Codex",
      }),
    );
    await waitFor(() => expect(body).toEqual({ app: "codex-app" }));
  });

  it("opens a native-owned official installer using only the app id", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json(inventory),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_app_open_official_download`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({ handoff: "directOfficialPackage" });
        },
      ),
    );
    mount();
    const claude = await screen.findByRole("article", {
      name: "Claude Desktop",
    });
    await userEvent.click(
      within(claude).getByRole("button", {
        name: "Open the official download for Claude Desktop",
      }),
    );
    await waitFor(() => expect(body).toEqual({ app: "claude-desktop" }));
    expect(toastMocks.success).toHaveBeenCalledWith(
      "The official Claude Desktop installer download opened in your browser. When it finishes, open the downloaded file and follow the system prompts.",
      {
        description:
          "If your browser cannot reach the address, set a proxy for your browser or system first.",
      },
    );
  });

  it("keeps a standalone app downloadable when native management is unavailable", async () => {
    let body: unknown;
    const zcode = {
      id: "zcode",
      name: "ZCode",
      status: "unsupported",
      version: null,
      latestVersion: null,
      relatedTool: null,
      configurationRelationship: "standaloneApplication",
      canLaunch: false,
      environment: "windows",
      installerHandoff: "officialDownloadPage",
      uninstallHandoff: "unsupported",
      updatesManagedByVendor: true,
      canRollback: false,
      canManageMcp: false,
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json([zcode]),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_app_open_official_download`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({ handoff: "officialDownloadPage" });
        },
      ),
    );
    mount();

    const card = await screen.findByRole("article", { name: "ZCode" });
    expect(card).toHaveTextContent("Standalone desktop application");
    expect(card).not.toHaveTextContent("Related CLI");
    const download = within(card).getByRole("button", {
      name: "Open the official download for ZCode",
    });
    expect(download).toHaveTextContent("Open official download");
    await userEvent.click(download);
    await waitFor(() => expect(body).toEqual({ app: "zcode" }));
    expect(toastMocks.success).toHaveBeenCalledWith(
      "The official ZCode download page opened in your browser.",
      {
        description:
          "If your browser cannot reach the address, set a proxy for your browser or system first.",
      },
    );
  });

  it("heads the section like every other section, with one heading and no label above it", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json(inventory),
      ),
    );
    mount();
    await screen.findByRole("article", { name: "ChatGPT / Codex" });

    // A section name, matching how the page above names itself — not a
    // sentence arguing for the design, and not a second label stacked on top.
    expect(
      screen.getByRole("heading", { level: 2, name: "Desktop applications" }),
    ).toBeVisible();
    expect(screen.getAllByRole("heading", { level: 2 })).toHaveLength(1);
    expect(screen.queryByText("Apps and CLIs, clearly separated")).toBeNull();
  });

  it("uses dedicated artwork for Cursor, ZCode and Cherry Studio", async () => {
    const standalone = (
      id: "cursor" | "zcode" | "cherry-studio",
      name: string,
    ) => ({
      id,
      name,
      status: "notInstalled",
      version: null,
      latestVersion: null,
      relatedTool: null,
      configurationRelationship: "standaloneApplication",
      canLaunch: false,
      environment: "macos",
      installerHandoff: "officialDownloadPage",
      uninstallHandoff: "revealApplication",
      updatesManagedByVendor: true,
      canRollback: false,
      canManageMcp: false,
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json([
          standalone("cursor", "Cursor"),
          standalone("zcode", "ZCode"),
          standalone("cherry-studio", "Cherry Studio"),
        ]),
      ),
    );
    const { container } = mount();

    await screen.findByRole("article", { name: "Cursor" });
    for (const id of ["cursor", "zcode", "cherry-studio"]) {
      expect(
        container.querySelector(`[data-desktop-app-artwork="${id}"] img`),
      ).toBeInTheDocument();
    }
  });

  it("keeps a vendor download-page fallback distinct from a direct installer", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json([
          { ...inventory[0], installerHandoff: "officialDownloadPage" },
          inventory[1],
        ]),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_app_open_official_download`,
        () => HttpResponse.json({ handoff: "officialDownloadPage" }),
      ),
    );
    mount();
    const codex = await screen.findByRole("article", {
      name: "ChatGPT / Codex",
    });
    await userEvent.click(
      within(codex).getByRole("button", {
        name: "More actions for ChatGPT / Codex",
      }),
    );
    expect(
      screen.getByRole("menuitem", { name: "Open official download" }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Open official download" }),
    );
    await waitFor(() =>
      expect(toastMocks.success).toHaveBeenCalledWith(
        "The official ChatGPT / Codex download page opened in your browser.",
        {
          description:
            "If your browser cannot reach the address, set a proxy for your browser or system first.",
        },
      ),
    );
  });

  it("shows a download icon only when a fixed official package is what opens", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json([
          // Same status, different handoff: only the icon may differ.
          { ...inventory[1], installerHandoff: "directOfficialPackage" },
          {
            ...inventory[1],
            id: "cursor",
            name: "Cursor",
            relatedTool: null,
            configurationRelationship: "standaloneApplication",
            installerHandoff: "officialDownloadPage",
          },
        ]),
      ),
    );
    mount();

    const direct = within(
      await screen.findByRole("article", { name: "Claude Desktop" }),
    ).getByRole("button", {
      name: "Open the official download for Claude Desktop",
    });
    const page = within(
      screen.getByRole("article", { name: "Cursor" }),
    ).getByRole("button", {
      name: "Open the official download for Cursor",
    });

    expect(direct.querySelector("svg")).toHaveClass("lucide-download");
    expect(page.querySelector("svg")).toHaveClass("lucide-external-link");
  });

  it("confirms a system-owned uninstall handoff without automatically rescanning", async () => {
    let body: unknown;
    let inventoryReads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () => {
        inventoryReads += 1;
        return HttpResponse.json(inventory);
      }),
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_app_open_uninstall`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json("opened");
        },
      ),
    );
    mount();
    const codex = await screen.findByRole("article", {
      name: "ChatGPT / Codex",
    });
    await userEvent.click(
      within(codex).getByRole("button", {
        name: "More actions for ChatGPT / Codex",
      }),
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Uninstall" }));
    const dialog = screen.getByRole("dialog", {
      name: "Uninstall ChatGPT / Codex through the system?",
    });
    expect(dialog).toHaveTextContent(
      "AI Manager does not remove the desktop app itself",
    );
    expect(dialog).toHaveTextContent(
      "After finishing in the system, return here and select Refresh apps.",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Open system uninstall" }),
    );
    await waitFor(() => expect(body).toEqual({ app: "codex-app" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(inventoryReads).toBe(1);
    expect(toastMocks.success).toHaveBeenCalledWith(
      "The system location for uninstalling ChatGPT / Codex is open.",
    );
  });

  it("explains the direct official download and restricted-network fallback in-app", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json(inventory),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_desktop_app_open_official_download`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({ handoff: "directOfficialPackage" });
        },
      ),
    );
    mount();
    const claude = await screen.findByRole("article", {
      name: "Claude Desktop",
    });
    await userEvent.click(
      within(claude).getByRole("button", {
        name: "More actions for Claude Desktop",
      }),
    );
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Download help" }),
    );

    const dialog = screen.getByRole("dialog", {
      name: "Download Claude Desktop",
    });
    expect(dialog).toHaveTextContent(
      "without sending you through a foreign product page first",
    );
    expect(dialog).toHaveTextContent("browser or system proxy");
    expect(dialog).toHaveTextContent(
      "CLI download proxy does not control the browser",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Retry official download" }),
    );
    await waitFor(() => expect(body).toEqual({ app: "claude-desktop" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("keeps launch failures inline without exposing technical details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json(inventory),
      ),
      http.post(`${TAURI_ENDPOINT}/app_desktop_app_launch`, () =>
        HttpResponse.json(
          {
            code: "LAUNCH_FAILED",
            messageKey: "error.desktopApp.launchFailed",
            technicalMessage: "/Applications/private/Codex.app",
            remediation: "error.remediation.openDesktopAppManually",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    mount();
    const codex = await screen.findByRole("article", {
      name: "ChatGPT / Codex",
    });
    await userEvent.click(
      within(codex).getByRole("button", { name: "Open ChatGPT / Codex" }),
    );
    const alert = await within(codex).findByRole("alert");
    expect(alert).toHaveTextContent(en.error.desktopApp.launchFailed);
    expect(alert).toHaveTextContent(
      en.error.remediation.openDesktopAppManually,
    );
    expect(alert).not.toHaveTextContent("/Applications/private");
  });

  it("shows a stable loading state and a retryable inventory error", async () => {
    let reads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, async () => {
        reads += 1;
        if (reads === 1) {
          await delay(20);
          return HttpResponse.text("private package inventory", {
            status: 500,
          });
        }
        return HttpResponse.json(inventory);
      }),
    );
    mount();
    expect(
      screen.getByRole("status", { name: en.tools.desktopApps.loading }),
    ).toBeInTheDocument();
    const alert = await screen.findByRole("alert", {
      name: en.tools.desktopApps.error.title,
    });
    expect(alert).not.toHaveTextContent("private package inventory");
    await userEvent.click(
      within(alert).getByRole("button", { name: en.tools.desktopApps.refresh }),
    );
    expect(await screen.findByText("ChatGPT / Codex")).toBeInTheDocument();
    expect(reads).toBe(2);
  });
});
