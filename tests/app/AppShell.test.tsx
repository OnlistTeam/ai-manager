import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { AppShell, RouteLoadingFallback } from "@/app/AppShell";
import brandMark from "@/assets/spatial/models/v5/mark.png";
import { server } from "../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const { isMacMock } = vi.hoisted(() => ({ isMacMock: vi.fn(() => false) }));

vi.mock("@/lib/platform", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/platform")>();
  return {
    ...actual,
    isMac: isMacMock,
    // These three constants are frozen at module-import time based on the
    // host UA (on Linux CI, DRAG_REGION_ENABLED is false and ATTR is empty);
    // mocking isMac alone can't affect them — they must be pinned down
    // together, otherwise macOS test cases would spuriously fail on
    // ubuntu-latest.
    DRAG_REGION_ENABLED: true,
    DRAG_REGION_ATTR: { "data-tauri-drag-region": true },
    DRAG_REGION_STYLE: { WebkitAppRegion: "drag" },
  };
});

function renderShell() {
  return render(<AppShell />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("AppShell", () => {
  beforeEach(async () => {
    window.localStorage.clear();
    isMacMock.mockReturnValue(false);
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_desktop_apps_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        HttpResponse.json({
          providers: [],
          configs: [],
          mcp: { total: 0, enabled: 0 },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_connection_profile`, () =>
        HttpResponse.json({
          defaultPresetId: "official",
          modelRequired: false,
          presets: [
            {
              id: "official",
              serviceName: "Anthropic API",
              defaultName: "Anthropic",
              defaultModel: "claude-sonnet-5",
              websiteUrl: "https://www.anthropic.com",
              apiKeyUrl: "https://console.anthropic.com",
              official: true,
            },
          ],
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.json({ files: [] }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, () =>
        HttpResponse.json({ items: [], totalCount: 0, limited: false }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({
          available: false,
          summary: { services: 0, mcpServers: 0, skills: 0 },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({
          advancedMode: false,
          importPromptSeen: true,
          toolScope: null,
          extensionKind: null,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/set_window_theme`, () =>
        HttpResponse.json(null),
      ),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        data: en.data,
        extensions: en.extensions,
        home: en.home,
        nav: en.nav,
        openClawWorkspace: en.openClawWorkspace,
        preferences: en.preferences,
        routing: en.routing,
        sessions: en.sessions,
        services: en.services,
        tool: en.tool,
        tools: en.tools,
        usage: en.usage,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("holds the home hero while the home chunk loads", () => {
    render(<RouteLoadingFallback route="home" />);

    const status = screen.getByRole("status", {
      name: en.nav.loadingPage,
    });
    const stage = status.querySelector<HTMLElement>("[data-spatial-stage]");

    expect(status).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 1, name: en.nav.home }),
    ).toBeInTheDocument();
    expect(stage).toHaveAttribute("data-model", "environment");
    expect(stage).toHaveAttribute("aria-busy", "true");
    expect(stage?.querySelector('[data-model="environment"]')).toHaveAttribute(
      "data-loading",
      "true",
    );
  });

  it("loads management pages behind a heading, not a hero they do not have", () => {
    render(<RouteLoadingFallback route="extensions" />);

    const status = screen.getByRole("status", { name: en.nav.loadingPage });

    expect(
      screen.getByRole("heading", { level: 1, name: en.nav.extensions }),
    ).toBeInTheDocument();
    // These pages open with a SectionHeader; there's no 3D model in the
    // middle of the page. If the placeholder drew a big model, it would
    // vanish into thin air the instant the chunk finished decoding.
    expect(status.querySelector("[data-spatial-stage]")).toBeNull();
    expect(status.querySelector("[data-model]")).toBeNull();
  });

  it("renders eight flat destinations without folded groups", () => {
    renderShell();
    const nav = screen.getByRole("navigation", { name: "Main navigation" });
    expect(
      within(nav)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual([
      "Home",
      "Software",
      en.nav.services,
      en.nav.extensions,
      en.nav.mcp,
      en.nav.prompts,
      en.nav.data,
      "Settings",
    ]);
    expect(screen.queryByRole("button", { name: "More tools" })).toBeNull();
    for (const merged of ["Local Routing", "Usage", "OpenClaw"]) {
      expect(screen.queryByRole("button", { name: merged })).toBeNull();
    }
    expect(
      screen
        .getByRole("button", { name: "Settings" })
        .closest("[data-sidebar-footer]"),
    ).toHaveClass("mt-auto");
  });

  it("lands on Home when the saved route merged into a parent page", () => {
    window.localStorage.setItem("aimanager.route", "usage");
    const { container } = renderShell();

    expect(container.querySelector("[data-app-window]")).toHaveAttribute(
      "data-route",
      "home",
    );
    expect(screen.getByRole("button", { name: "Home" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      window.localStorage.getItem("aimanager.sidebar.sections"),
    ).toBeNull();
  });

  it("reaches Local Routing and Usage as tabs of the AI Services page", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
        HttpResponse.json({
          running: false,
          address: null,
          port: null,
          activeConnections: 0,
          totalRequests: 0,
          successRequests: 0,
          failedRequests: 0,
          failoverCount: 0,
          targets: [],
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
        HttpResponse.json({
          periodDays: 30,
          startDate: "2026-08-06",
          endDate: "2026-09-05",
          summary: {
            requests: 0,
            estimatedCostUsd: "0.000000",
            tokens: 0,
            successRatePercent: 0,
            cacheHitRatePercent: 0,
          },
          byTool: [],
          trend: [],
        }),
      ),
    );
    const { container } = renderShell();

    await userEvent.click(
      screen.getByRole("button", { name: en.nav.services }),
    );
    await userEvent.click(
      await screen.findByRole("tab", { name: "Local Routing" }),
    );
    expect(
      await screen.findByRole("region", { name: "Local Routing" }),
    ).toBeInTheDocument();
    expect(container.querySelector("[data-app-window]")).toHaveAttribute(
      "data-route",
      "services",
    );
    expect(
      screen.getByRole("button", { name: en.nav.services }),
    ).toHaveAttribute("aria-current", "page");

    await userEvent.click(screen.getByRole("tab", { name: "Usage" }));
    expect(
      await screen.findByRole("region", { name: "Usage & Cost" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 1, name: en.services.title }),
    ).toBeInTheDocument();
  });

  it("reaches local sessions directly on the Local Data page", async () => {
    renderShell();

    await userEvent.click(screen.getByRole("button", { name: en.nav.data }));
    expect(
      await screen.findByRole("region", { name: "Local Sessions" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: en.nav.data })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("offers the OpenClaw workspace in software details once installed", async () => {
    const capabilities = {
      canInstall: true,
      canUpdate: true,
      canUninstall: true,
      canRepair: false,
      canManageProvider: false,
      canManageMcp: true,
      canManageSkills: true,
      canManagePrompts: false,
      canManageVersion: false,
      canLaunch: false,
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([
          {
            id: "openclaw",
            name: "OpenClaw",
            descriptionKey: "tool.openclaw.description",
            status: "installed",
            version: "1.0.0",
            latestVersion: null,
            capabilities,
            sessionsInsideSettings: false,
            environment: "macos",
          },
        ]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_extensions_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_extensions_local_inventory`, () =>
        HttpResponse.json({ items: [], scopes: [], truncated: false }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_skill_updates_check`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_overview`, () =>
        HttpResponse.json({
          files: [],
          existingFiles: 0,
          dailyMemoryCount: 0,
          dailyMemoryBytes: 0,
          totalBytes: 0,
          limited: false,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_openclaw_daily_memories`, () =>
        HttpResponse.json({
          items: [],
          totalCount: 0,
          totalBytes: 0,
          limited: false,
        }),
      ),
    );
    renderShell();
    await userEvent.click(screen.getByRole("button", { name: en.nav.tools }));
    await userEvent.click(
      await screen.findByRole("button", { name: "OpenClaw details" }),
    );
    await userEvent.click(screen.getByRole("tab", { name: "OpenClaw" }));
    expect(
      await screen.findByRole("region", { name: "OpenClaw Workspace" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.nav.tools, hidden: true }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("uses one continuous window canvas instead of a separated status toolbar", () => {
    const { container } = renderShell();
    const shell = container.querySelector("[data-app-window]");
    const statusbar = container.querySelector("[data-app-statusbar]");

    expect(shell).toHaveClass("app-window-canvas", "flex-row");
    expect(statusbar).toHaveClass(
      "absolute",
      "inset-x-0",
      "top-0",
      "z-30",
      "bg-transparent",
    );
    expect(statusbar).not.toHaveClass("border-b");
    expect(statusbar).toHaveAttribute("data-integrated", "true");
    // The top material belongs to the scrolling content, not the whole
    // window. Mounted on the shell it would span the sidebar, and the dark
    // gradient would cover the brand mark in the top-left corner; the
    // sidebar neither scrolls nor hosts controls.
    expect(statusbar?.parentElement).not.toBe(shell);
    expect(statusbar?.parentElement?.parentElement).toBe(shell);
    expect(
      statusbar?.parentElement?.querySelector("[data-app-scroll-viewport]"),
    ).not.toBeNull();
    expect(statusbar?.closest(".app-sidebar")).toBeNull();
    expect(statusbar?.querySelector("[data-tauri-no-drag]")).toHaveClass(
      "pointer-events-auto",
    );
  });

  it("uses the packaged product mark in the persistent shell brand", () => {
    renderShell();
    const brand = screen.getByText("AI Manager").parentElement;
    const mark = brand?.querySelector('img[alt=""]');

    expect(mark).not.toBeNull();
    expect(mark).toHaveAttribute("src", brandMark);
    expect(mark).toHaveClass("app-logo-mark", "h-9", "w-9");
    // The 3D piece already has its own volume; wrapping it in another
    // outlined square would just add a second rectangle in the window's corner.
    expect(mark?.parentElement).not.toHaveClass("app-logo-tile");
    expect(brand?.querySelector("svg")).toBeNull();
  });

  it("marks the active page for assistive technology", async () => {
    const { container } = renderShell();
    const shell = container.querySelector("[data-app-window]");
    expect(shell).toHaveAttribute("data-route", "home");
    expect(screen.getByRole("button", { name: "Home" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.nav.extensions }),
    );
    expect(shell).toHaveAttribute("data-route", "extensions");
    expect(
      screen.getByRole("button", { name: en.nav.extensions }),
    ).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "Home" })).not.toHaveAttribute(
      "aria-current",
    );
  });

  it("swaps the page body when the user navigates", async () => {
    const { container } = renderShell();
    const homeContent = container.querySelector(
      '[data-route-content-layer="incoming"]',
    );
    expect(homeContent).toHaveAttribute("data-app-route-content", "home");
    expect(homeContent).toHaveAttribute("data-transition-direction", "forward");
    expect(homeContent).toHaveClass(
      "app-route-transition",
      "app-route-transition--incoming",
    );

    await userEvent.click(
      screen.getByRole("button", { name: en.nav.services }),
    );
    expect(
      await screen.findByRole("heading", { name: en.services.title }),
    ).toBeInTheDocument();
    const servicesContent = container.querySelector(
      '[data-route-content-layer="incoming"]',
    );
    expect(servicesContent).toHaveAttribute(
      "data-app-route-content",
      "services",
    );
    expect(servicesContent).toHaveAttribute(
      "data-transition-direction",
      "forward",
    );
    expect(servicesContent).not.toBe(homeContent);
    const departingHome = container.querySelector(
      '[data-route-content-layer="outgoing"]',
    );
    expect(departingHome).toHaveAttribute("data-app-route-content", "home");
    expect(departingHome).toHaveAttribute("aria-hidden", "true");

    await userEvent.click(screen.getByRole("button", { name: "Home" }));
    expect(
      container.querySelector('[data-route-content-layer="incoming"]'),
    ).toHaveAttribute("data-transition-direction", "backward");
  });

  it("starts ordinary page navigation at the top of the shared content viewport", async () => {
    const user = userEvent.setup();
    const { container } = renderShell();
    const viewport = container.querySelector("main");
    expect(viewport).not.toBeNull();
    expect(viewport).toHaveClass(
      "app-scroll-viewport",
      "scrollbar-hidden",
      "overflow-x-hidden",
      "overflow-y-auto",
    );
    expect(viewport).toHaveAttribute("data-scrollbar-mode", "hidden");
    expect(viewport).not.toHaveClass("scrollbar-subtle");

    viewport!.scrollTop = 640;
    screen.getByRole("button", { name: "Software" }).focus();
    await user.keyboard("{Enter}");
    expect(viewport).toHaveProperty("scrollTop", 0);

    await user.click(screen.getByRole("button", { name: "Home" }));
    viewport!.scrollTop = 480;
    await user.click(
      await screen.findByRole("button", {
        name: en.home.quickActions.connectService,
      }),
    );
    expect(viewport).toHaveProperty("scrollTop", 0);
  });

  it("routes Home's Connect action to AI Services", async () => {
    renderShell();
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.home.quickActions.connectService,
      }),
    );
    expect(
      await screen.findByRole(
        "heading",
        { name: en.services.title },
        { timeout: 3_000 },
      ),
    ).toBeInTheDocument();
  });

  it("opens the service scope named by Home instead of an unrelated remembered tool", async () => {
    let settingsWrites = 0;
    const capabilities = {
      canInstall: true,
      canUpdate: true,
      canUninstall: true,
      canRepair: false,
      canManageProvider: true,
      canManageMcp: true,
      canManageSkills: true,
      canManagePrompts: true,
      canManageVersion: true,
      canLaunch: true,
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([
          {
            id: "claude-code",
            name: "Claude Code",
            descriptionKey: "tool.claude-code.description",
            status: "installed",
            version: "1.0.0",
            latestVersion: null,
            capabilities,
            sessionsInsideSettings: false,
            environment: "macos",
          },
          {
            id: "codex",
            name: "Codex",
            descriptionKey: "tool.codex.description",
            status: "installed",
            version: "1.0.0",
            latestVersion: null,
            capabilities,
            sessionsInsideSettings: false,
            environment: "macos",
          },
        ]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        HttpResponse.json({
          providers: [
            {
              tool: "claude-code",
              configured: false,
              configuredCount: 0,
              checkTargets: [],
            },
            {
              tool: "codex",
              configured: true,
              configuredCount: 1,
              checkTargets: [],
            },
          ],
          configs: [
            { tool: "claude-code", status: "readable" },
            { tool: "codex", status: "readable" },
          ],
          mcp: { total: 0, enabled: 0 },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({
          advancedMode: false,
          importPromptSeen: true,
          toolScope: "codex",
          extensionKind: null,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, () => {
        settingsWrites += 1;
        return HttpResponse.json({});
      }),
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
        HttpResponse.json([]),
      ),
    );
    renderShell();

    // The hero itself only carries the count and the primary action; the
    // health-check list below names the specific problem, including when
    // there is exactly one of them.
    const services = await screen.findByRole("button", {
      name: en.home.card.actions.services,
    });
    const guide = screen.getByRole("region", { name: en.home.health.title });
    expect(
      within(guide).getByText("Claude Code has no AI service configured"),
    ).toBeInTheDocument();
    await userEvent.click(services);

    expect(
      await screen.findByRole("tab", { name: "Claude Code" }),
    ).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Codex" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
    expect(settingsWrites).toBe(0);
  });

  it("keeps the current page mounted when its own rail item is selected again", async () => {
    renderShell();

    await userEvent.click(screen.getByRole("button", { name: en.nav.data }));
    const region = await screen.findByRole("region", {
      name: "Local Sessions",
    });
    const search = within(region).getByRole("textbox", {
      name: en.sessions.search.label,
    });
    await userEvent.type(search, "alpha");
    expect(search).toHaveValue("alpha");

    await userEvent.click(screen.getByRole("button", { name: en.nav.data }));

    // Same DOM node, same typed text: the page was re-rendered at most, never
    // torn down and rebuilt.
    expect(screen.getByRole("region", { name: "Local Sessions" })).toBe(region);
    expect(
      within(region).getByRole("textbox", { name: en.sessions.search.label }),
    ).toHaveValue("alpha");
  });

  it("keeps the current page mounted when a same-route navigation clears a one-shot intent", async () => {
    const capabilities = {
      canInstall: true,
      canUpdate: true,
      canUninstall: true,
      canRepair: false,
      canManageProvider: true,
      canManageMcp: true,
      canManageSkills: true,
      canManagePrompts: true,
      canManageVersion: true,
      canLaunch: true,
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([
          {
            id: "claude-code",
            name: "Claude Code",
            descriptionKey: "tool.claude-code.description",
            status: "installed",
            version: "1.0.0",
            latestVersion: null,
            capabilities,
            sessionsInsideSettings: false,
            environment: "macos",
          },
          {
            id: "codex",
            name: "Codex",
            descriptionKey: "tool.codex.description",
            status: "installed",
            version: "1.0.0",
            latestVersion: null,
            capabilities,
            sessionsInsideSettings: false,
            environment: "macos",
          },
        ]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        HttpResponse.json({
          providers: [
            {
              tool: "claude-code",
              configured: false,
              configuredCount: 0,
              checkTargets: [],
            },
            {
              tool: "codex",
              configured: true,
              configuredCount: 1,
              checkTargets: [],
            },
          ],
          configs: [
            { tool: "claude-code", status: "readable" },
            { tool: "codex", status: "readable" },
          ],
          mcp: { total: 0, enabled: 0 },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({
          advancedMode: false,
          importPromptSeen: true,
          toolScope: "codex",
          extensionKind: null,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
        HttpResponse.json([]),
      ),
    );
    renderShell();

    // Home hands the page a one-shot tool intent; the page consumes it once.
    await userEvent.click(
      await screen.findByRole("button", {
        name: en.home.card.actions.services,
      }),
    );
    const claudeTab = await screen.findByRole("tab", { name: "Claude Code" });
    expect(claudeTab).toHaveAttribute("aria-selected", "true");

    // Selecting the rail item for the page already showing replaces the intent
    // with an empty one. That must not rebuild the page: a rebuilt page would
    // re-read the intents, find none, and fall back to the remembered Codex.
    await userEvent.click(
      screen.getByRole("button", { name: en.nav.services }),
    );

    expect(screen.getByRole("tab", { name: "Claude Code" })).toBe(claudeTab);
    expect(claudeTab).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Codex" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
  });

  it("renders the real Extensions page instead of a placeholder", async () => {
    renderShell();
    await userEvent.click(
      screen.getByRole("button", { name: en.nav.extensions }),
    );
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: en.extensions.skill.title,
      }),
    ).toBeInTheDocument();
  });

  it("renders the real Settings page, the last placeholder is gone", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({
          advancedMode: false,
          importPromptSeen: true,
          toolScope: null,
          extensionKind: null,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.json({ files: [] }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({
          available: false,
          summary: { services: 0, mcpServers: 0, skills: 0 },
        }),
      ),
    );
    renderShell();
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(
      await screen.findByRole("heading", { name: en.preferences.title }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: en.home.health.title }),
    ).toBeNull();
  });

  it("keeps the health check on Home and routes its next step to the area that can help", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([
          {
            id: "claude-code",
            name: "Claude Code",
            descriptionKey: "tool.claude-code.description",
            status: "updateAvailable",
            version: "1.0.0",
            latestVersion: "2.0.0",
            capabilities: {
              canInstall: true,
              canUpdate: true,
              canUninstall: true,
              canRepair: false,
              canManageProvider: true,
              canManageMcp: true,
              canManageSkills: true,
              canManagePrompts: true,
              canManageVersion: true,
              canLaunch: true,
            },
            sessionsInsideSettings: false,
            environment: "macos",
          },
        ]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        HttpResponse.json({
          providers: [
            {
              tool: "claude-code",
              configured: false,
              configuredCount: 0,
              checkTargets: [],
            },
          ],
          configs: [{ tool: "claude-code", status: "readable" }],
          mcp: { total: 0, enabled: 0 },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.json({ files: [] }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_import_preview`, () =>
        HttpResponse.json({
          available: false,
          summary: { services: 0, mcpServers: 0, skills: 0 },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_providers_list`, () =>
        HttpResponse.json([]),
      ),
    );
    renderShell();
    const health = await screen.findByRole("region", {
      name: en.home.health.title,
    });
    expect(screen.queryByRole("button", { name: "Check Setup" })).toBeNull();
    await userEvent.click(
      await within(health).findByRole("button", {
        name: "Connect a service",
      }),
    );

    expect(
      await screen.findByRole("heading", { name: en.services.title }),
    ).toBeInTheDocument();
    expect(
      await screen.findByRole("tab", { name: "Claude Code" }),
    ).toHaveAttribute("aria-selected", "true");
  });

  it("keeps one quiet, fixed navigation rail without a collapse control", () => {
    window.localStorage.setItem("aimanager.sidebar.collapsed", "true");
    renderShell();

    expect(
      screen.queryByRole("button", { name: "Collapse sidebar" }),
    ).toBeNull();
    expect(screen.queryByRole("button", { name: "Expand sidebar" })).toBeNull();
    const sidebar = screen.getByRole("navigation", {
      name: "Main navigation",
    }).parentElement;
    expect(sidebar).toHaveClass("app-sidebar", "w-[220px]", "select-none");
    expect(sidebar?.querySelector('img[alt=""]')).toHaveAttribute(
      "src",
      brandMark,
    );
    expect(screen.getByText("AI Manager")).toBeInTheDocument();
  });
});

describe("AppShell macOS integrated Overlay titlebar", () => {
  beforeEach(async () => {
    window.localStorage.clear();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        HttpResponse.json({
          providers: [],
          configs: [],
          mcp: { total: 0, enabled: 0 },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({
          advancedMode: false,
          importPromptSeen: true,
          toolScope: null,
          extensionKind: null,
          downloadStrategy: "automatic",
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/set_window_theme`, () =>
        HttpResponse.json(null),
      ),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, nav: en.nav, tool: en.tool, tools: en.tools },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("overlays the drag region inside the continuous canvas on macOS", () => {
    isMacMock.mockReturnValue(true);
    const { container } = renderShell();

    const dragBar = container.querySelector("[data-window-drag-region]");
    expect(dragBar).not.toBeNull();
    expect(dragBar).toHaveStyle({ height: "28px" });
    expect(dragBar).toHaveClass("absolute", "inset-x-0", "top-0");

    // The overlay no longer consumes a separate horizontal layout row. The
    // sidebar owns the traffic-light inset while its background stays visible
    // behind the native titlebar.
    const root = container.querySelector("[data-app-window]");
    expect(dragBar?.parentElement).toBe(root);
    const sidebar = screen.getByRole("navigation", {
      name: "Main navigation",
    }).parentElement;
    expect(sidebar).toHaveClass("pt-10");
    const viewport = container.querySelector("[data-app-scroll-viewport]");
    expect(viewport).toHaveAttribute("data-scrollbar-mode", "hidden");
    expect(viewport).toHaveClass(
      "app-scroll-viewport",
      "scrollbar-hidden",
      "overflow-x-hidden",
      "overflow-y-auto",
    );
    expect(viewport).not.toHaveClass("scrollbar-subtle");
    expect(container.querySelector(".app-scroll-content")).toHaveClass(
      "pt-[88px]",
      "lg:pt-[92px]",
    );
    expect(screen.getByRole("button", { name: "Home" })).toBeInTheDocument();
  });

  it("renders no HTML drag overlay on non-macOS platforms", () => {
    isMacMock.mockReturnValue(false);
    const { container } = renderShell();

    expect(container.querySelector("[data-window-drag-region]")).toBeNull();
    expect(container.querySelector("[data-app-window]")).toHaveClass(
      "app-window-canvas",
    );
    expect(screen.getByRole("button", { name: "Home" })).toBeInTheDocument();
  });
});
