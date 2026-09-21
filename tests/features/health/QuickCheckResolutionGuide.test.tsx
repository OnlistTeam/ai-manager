import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Tool } from "@/entities/tool";
import type { HealthSnapshot } from "@/native";
import en from "@/i18n/locales/en.json";
import {
  aggregateQuickCheck,
  QuickCheckResolutionGuide,
} from "@/features/health";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const CAPABILITIES = {
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

function tool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "tool.claude-code.description",
    status: "installed",
    version: "1.0.0",
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: "macos",
    ...overrides,
  };
}

function snapshot(overrides: Partial<HealthSnapshot> = {}): HealthSnapshot {
  return {
    providers: [
      {
        tool: "claude-code",
        configured: true,
        configuredCount: 1,
        checkTargets: [],
      },
    ],
    configs: [{ tool: "claude-code", status: "readable" }],
    mcp: { total: 3, enabled: 2 },
    ...overrides,
  };
}

function mount(tools: readonly Tool[], health: HealthSnapshot) {
  const props = {
    summary: aggregateQuickCheck(tools, health, {}),
    tools,
    onOpenTools: vi.fn(),
    onOpenServices: vi.fn(),
    onOpenExtensions: vi.fn(),
  };
  render(<QuickCheckResolutionGuide {...props} />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
  return props;
}

describe("QuickCheckResolutionGuide", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        home: en.home,
        preferences: en.preferences,
        tools: en.tools,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("leads with the most urgent step and folds the rest", async () => {
    const props = mount(
      [tool({ status: "updateAvailable", latestVersion: "2.0.0" })],
      snapshot({
        providers: [
          {
            tool: "claude-code",
            configured: false,
            configuredCount: 0,
            checkTargets: [],
          },
        ],
        configs: [{ tool: "claude-code", status: "unreadable" }],
      }),
    );
    expect(
      screen.getByText("Claude Code configuration could not be read"),
    ).toBeVisible();
    const more = screen.getByText("2 more");
    expect(more.closest("details")).not.toHaveAttribute("open");
    for (const item of screen.getAllByRole("listitem")) {
      expect(item).not.toBeVisible();
    }

    await userEvent.click(more);
    expect(more.closest("details")).toHaveAttribute("open");
    expect(screen.getByText("Claude Code has an update")).toBeVisible();
    expect(
      screen.getByText("Claude Code has no AI service configured"),
    ).toBeVisible();

    await userEvent.click(
      screen.getByRole("button", { name: "Review API Endpoints" }),
    );
    expect(props.onOpenServices).toHaveBeenCalledWith("claude-code");
    expect(props.onOpenTools).not.toHaveBeenCalled();
  });

  it("previews the selected tool before starting an update", async () => {
    let previewRequest: unknown;
    let updateRequest: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_tools_update_preview`,
        async ({ request }) => {
          previewRequest = await request.json();
          return HttpResponse.json([
            {
              state: "ready",
              preview: {
                tool: "claude-code",
                previewFingerprint: "a".repeat(64),
                targetVersion: "2.0.0",
                source: "nativeInstaller",
                installations: [
                  {
                    source: "nativeInstaller",
                    version: "1.0.0",
                    runnable: true,
                    isDefault: true,
                    location: "/Users/test/.local/bin/claude",
                  },
                ],
                attempts: [
                  {
                    method: "nativeSelfUpdate",
                    commands: ["/Users/test/.local/bin/claude update"],
                  },
                ],
                multipleInstallations: false,
              },
            },
          ]);
        },
      ),
      http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
        updateRequest = await request.json();
        return HttpResponse.json("00000000-0000-4000-8000-000000000001");
      }),
    );
    mount(
      [tool({ status: "updateAvailable", latestVersion: "2.0.0" })],
      snapshot(),
    );
    const update = await screen.findByRole("button", { name: "Update now" });
    await waitFor(() => expect(update).toBeEnabled());

    await userEvent.click(update);
    await waitFor(() =>
      expect(previewRequest).toEqual({ tools: ["claude-code"] }),
    );
    expect(updateRequest).toBeUndefined();
    const dialog = await screen.findByRole("dialog", {
      name: "Update Claude Code?",
    });
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.tools.confirm.update.confirm,
      }),
    );
    await waitFor(() =>
      expect(updateRequest).toEqual({
        tool: "claude-code",
        previewFingerprint: "a".repeat(64),
      }),
    );
  });
});
