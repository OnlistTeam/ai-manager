import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import type { ToolId } from "@/entities/tool";
import { SessionsPage } from "@/pages/sessions/SessionsPage";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const reference = "c".repeat(64);
const session = {
  reference,
  tool: "codex",
  title: "Release the desktop app",
  preview: "Verify the package",
  projectName: "ai-manager",
  createdAt: 1_777_000_000,
  lastActiveAt: 1_777_000_100,
  resumable: true,
};

const SETTINGS = {
  advancedMode: true,
  importPromptSeen: true,
  toolScope: null,
  extensionScope: null,
  extensionKind: null,
  downloadStrategy: "automatic",
  automaticProviderFailover: false,
  terminalApp: null,
};

const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  success: vi.fn(),
  info: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

function mount(tool: ToolId | null = null) {
  return render(<SessionsPage tool={tool} />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("SessionsPage", () => {
  beforeEach(async () => {
    toastMocks.error.mockClear();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, () =>
        HttpResponse.json({ items: [session], totalCount: 1, limited: false }),
      ),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      { common: en.common, error: en.error, sessions: en.sessions },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("reads message content only after an explicit session selection", async () => {
    let threadCalls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_thread`, () => {
        threadCalls += 1;
        return HttpResponse.json({
          reference,
          messages: [
            {
              role: "user",
              content: "private local prompt",
              timestamp: 1_777_000_200,
              truncated: false,
            },
          ],
          totalCount: 1,
          limited: false,
          workingDirectory: "/work/project",
          resumeCommand: "cd /work/project && codex resume session-42",
        });
      }),
    );
    mount();

    expect(
      await screen.findByText("Release the desktop app"),
    ).toBeInTheDocument();
    expect(screen.getByRole("search")).toHaveClass("sm:grid-cols-[1fr_auto]");
    expect(screen.getByText(en.sessions.hero.local)).toBeInTheDocument();
    expect(
      screen.getByText(`${en.sessions.hero.metric.visible} 1`),
    ).toBeInTheDocument();
    expect(threadCalls).toBe(0);
    expect(screen.queryByText("private local prompt")).toBeNull();

    await userEvent.click(
      screen.getByRole("button", { name: /Release the desktop app/ }),
    );
    expect(await screen.findByText("private local prompt")).toBeInTheDocument();
    expect(threadCalls).toBe(1);
    expect(screen.queryByText(reference)).toBeNull();
    expect(screen.queryByText(/\/Users\/|sourcePath|resumeCommand/)).toBeNull();
    // Without clicking resume there's no need to troubleshoot, so the manual command shouldn't take up space.
    expect(screen.queryByText(/codex resume/)).toBeNull();
  });

  it("shows the session list full width before any selection", async () => {
    mount();

    expect(
      await screen.findByText("Release the desktop app"),
    ).toBeInTheDocument();
    // With no session selected, the detail panel doesn't render at all — it's not rendering an empty placeholder card.
    expect(screen.queryByText("Choose a conversation")).toBeNull();
    expect(
      screen.queryByRole("heading", { level: 2, name: "Conversation preview" }),
    ).toBeNull();
  });

  it("switches to a two-column layout once a session is selected", async () => {
    mount();

    await userEvent.click(
      await screen.findByRole("button", { name: /Release the desktop app/ }),
    );

    expect(
      await screen.findByRole("heading", {
        level: 2,
        name: "Release the desktop app",
      }),
    ).toBeInTheDocument();
  });

  it("shows an honest conversation read state instead of pulsing message blocks", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_session_thread`,
        () => new Promise<Response>(() => undefined),
      ),
    );
    mount();

    await userEvent.click(
      await screen.findByRole("button", { name: /Release the desktop app/ }),
    );
    const status = screen.getByRole("status", {
      name: en.sessions.thread.loading,
    });
    expect(status).toHaveTextContent(en.common.detecting);
    expect(status.querySelector(".animate-pulse")).toBeNull();
  });

  it("searches while typing and sends the selected product tool id", async () => {
    const payloads: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, async ({ request }) => {
        payloads.push(await request.json());
        return HttpResponse.json({
          items: [session],
          totalCount: 1,
          limited: false,
        });
      }),
    );
    mount("codex");
    await screen.findByText("Release the desktop app");
    expect(screen.queryByRole("button", { name: "Search" })).toBeNull();

    await userEvent.type(
      screen.getByRole("textbox", { name: "Search local sessions" }),
      "release",
    );
    await waitFor(() =>
      expect(payloads.at(-1)).toEqual({ query: "release", tool: "codex" }),
    );
    // Debounced input doesn't fire one request per character: just the initial list plus the one after typing stops.
    expect(payloads.length).toBeLessThan("release".length);
  });

  it("keeps the previous results on screen while a new search runs", async () => {
    let listCalls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, async () => {
        listCalls += 1;
        if (listCalls > 1) {
          await new Promise((resolve) => setTimeout(resolve, 40));
        }
        return HttpResponse.json({
          items: [session],
          totalCount: 1,
          limited: false,
        });
      }),
    );
    mount();
    await screen.findByText("Release the desktop app");

    await userEvent.type(
      screen.getByRole("textbox", { name: "Search local sessions" }),
      "rel",
    );
    await waitFor(() => expect(listCalls).toBe(2));
    expect(screen.getByText("Release the desktop app")).toBeInTheDocument();
    expect(screen.queryByText(en.sessions.loading)).toBeNull();
  });

  it("resumes by opaque reference and reports the terminal handoff", async () => {
    let resumePayload: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_thread`, () =>
        HttpResponse.json({
          reference,
          messages: [],
          totalCount: 0,
          limited: false,
          workingDirectory: "/work/project",
          resumeCommand: "cd /work/project && codex resume session-42",
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_session_resume`, async ({ request }) => {
        resumePayload = await request.json();
        return HttpResponse.json(null);
      }),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: /Release the desktop app/ }),
    );
    const resumeButton = await screen.findByRole("button", {
      name: "Resume in Terminal",
    });
    expect(resumeButton).toHaveAttribute("data-size", "xs");
    // The button only opens the dialog; the handoff needs a second confirmation inside it.
    await userEvent.click(resumeButton);
    expect(resumePayload).toBeUndefined();

    await userEvent.click(
      await screen.findByRole("button", { name: "Open in terminal now" }),
    );
    expect(
      await screen.findByText("The session was handed off to Terminal."),
    ).toBeInTheDocument();
    expect(resumePayload).toEqual({ reference });
    // A successful handoff doesn't guarantee it's actually running in the terminal, so the manual command stays in the dialog as a fallback.
    expect(
      screen.getByText("cd /work/project && codex resume session-42"),
    ).toBeInTheDocument();
  });

  it("remembers which terminal should take the handoff", async () => {
    let saved: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_thread`, () =>
        HttpResponse.json({
          reference,
          messages: [],
          totalCount: 0,
          limited: false,
          workingDirectory: "/work/project",
          resumeCommand: "cd /work/project && codex resume session-42",
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_terminals_list`, () =>
        HttpResponse.json(["system", "ghostty"]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(SETTINGS),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, async ({ request }) => {
        saved = await request.json();
        return HttpResponse.json({ ...SETTINGS, terminalApp: "ghostty" });
      }),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: /Release the desktop app/ }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Resume in Terminal" }),
    );

    const picker = await screen.findByRole("combobox", { name: "Open in" });
    await userEvent.selectOptions(picker, "ghostty");
    await waitFor(() =>
      expect(saved).toEqual({
        settings: { ...SETTINGS, terminalApp: "ghostty" },
      }),
    );
  });

  it("falls back to the system terminal when the remembered one is gone, and reports a failed save", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_thread`, () =>
        HttpResponse.json({
          reference,
          messages: [],
          totalCount: 0,
          limited: false,
          workingDirectory: "/work/project",
          resumeCommand: "cd /work/project && codex resume session-42",
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_terminals_list`, () =>
        HttpResponse.json(["system", "ghostty"]),
      ),
      // iTerm2 was chosen once and has since been uninstalled.
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json({ ...SETTINGS, terminalApp: "iterm2" }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_save`, () =>
        HttpResponse.json(
          {
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.settings.saveFailed",
            technicalMessage: null,
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: /Release the desktop app/ }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Resume in Terminal" }),
    );

    const picker = await screen.findByRole("combobox", { name: "Open in" });
    expect(picker).toHaveValue("system");

    await userEvent.selectOptions(picker, "ghostty");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.error.settings.saveFailed,
    );
    await waitFor(() => expect(picker).toHaveValue("system"));
    expect(picker).toBeEnabled();
  });

  it("does not ask which terminal when only the system one is installed", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_thread`, () =>
        HttpResponse.json({
          reference,
          messages: [],
          totalCount: 0,
          limited: false,
          workingDirectory: "/work/project",
          resumeCommand: "cd /work/project && codex resume session-42",
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_terminals_list`, () =>
        HttpResponse.json(["system"]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_settings_get`, () =>
        HttpResponse.json(SETTINGS),
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: /Release the desktop app/ }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Resume in Terminal" }),
    );

    expect(
      await screen.findByRole("button", { name: "Open in terminal now" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("combobox")).toBeNull();
  });

  it("opens the folder containing the selected session", async () => {
    let revealPayload: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_thread`, () =>
        HttpResponse.json({
          reference,
          messages: [],
          totalCount: 0,
          limited: false,
          workingDirectory: "/work/project",
          resumeCommand: "cd /work/project && codex resume session-42",
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_session_reveal`, async ({ request }) => {
        revealPayload = await request.json();
        return HttpResponse.json(null);
      }),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: /Release the desktop app/ }),
    );
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Show Release the desktop app in the file manager",
      }),
    );

    await waitFor(() => expect(revealPayload).toEqual({ reference }));
  });

  it("reports a toast when opening the session folder fails", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_thread`, () =>
        HttpResponse.json({
          reference,
          messages: [],
          totalCount: 0,
          limited: false,
          workingDirectory: "/work/project",
          resumeCommand: "cd /work/project && codex resume session-42",
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_session_reveal`, () =>
        HttpResponse.json(
          {
            code: "LAUNCH_FAILED",
            messageKey: "error.system.revealFailed",
            technicalMessage: null,
            remediation: "error.remediation.checkPermissions",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: /Release the desktop app/ }),
    );
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Show Release the desktop app in the file manager",
      }),
    );

    await waitFor(() =>
      expect(toastMocks.error).toHaveBeenCalledWith(
        en.error.system.revealFailed,
        { description: en.error.remediation.checkPermissions },
      ),
    );
  });

  it("shows an honest empty state without starting a broad filesystem action", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, () =>
        HttpResponse.json({ items: [], totalCount: 0, limited: false }),
      ),
    );
    mount();
    expect(
      await screen.findByText("No local sessions found"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Start a supported AI coding tool once/),
    ).toBeInTheDocument();
  });

  it("keeps the initial accessibility tree bounded and reveals more sessions on demand", async () => {
    const items = Array.from({ length: 65 }, (_, index) => ({
      ...session,
      reference: (index + 1).toString(16).padStart(64, "0"),
      title: `Session ${index + 1}`,
      preview: `Preview ${index + 1}`,
    }));
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, () =>
        HttpResponse.json({ items, totalCount: items.length, limited: false }),
      ),
    );

    mount();
    expect(await screen.findByText("Session 60")).toBeInTheDocument();
    expect(screen.queryByText("Session 61")).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Show 5 more" }));
    expect(await screen.findByText("Session 65")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Show .* more/ })).toBeNull();
  });
});
