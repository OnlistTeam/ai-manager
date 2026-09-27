import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { HealthSnapshot } from "@/native";
import type { Tool } from "@/entities/tool";
import en from "@/i18n/locales/en.json";
import { aggregateQuickCheck } from "@/features/health";
import {
  HomeStatusLine,
  type HomeStatusLineProps,
} from "@/pages/home/HomeStatusLine";
import { recommendHomeAction } from "@/pages/home/homeRecommendation";

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

const HEALTHY: HealthSnapshot = {
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
};

function mount(
  overrides: Partial<HomeStatusLineProps> = {},
  tools: Tool[] = [tool()],
) {
  const props: HomeStatusLineProps = {
    summary: aggregateQuickCheck(tools, HEALTHY, {}),
    recommendation: null,
    startTool: tools[0] ?? null,
    checkedAt: Date.UTC(2026, 8, 5, 9, 30),
    rechecking: false,
    checkingConnections: false,
    connectionCheckError: false,
    onRecheck: vi.fn(),
    onReview: vi.fn(),
    onStart: vi.fn(),
    onOpenUpdates: vi.fn(),
    ...overrides,
  };
  render(<HomeStatusLine {...props} />);
  return props;
}

/**
 * The line states the verdict once, next to the timestamp and the rerun
 * control that describe it, and offers exactly one next step.
 */
describe("HomeStatusLine", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, home: en.home, tools: en.tools },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("carries the check's own timestamp and rerun control", async () => {
    const props = mount();

    expect(
      screen.getByRole("heading", { level: 1, name: en.home.status.allGood }),
    ).toBeVisible();
    expect(screen.getByText(/Last checked/)).toBeVisible();
    await userEvent.click(
      screen.getByRole("button", { name: en.home.health.recheck }),
    );
    expect(props.onRecheck).toHaveBeenCalledTimes(1);
  });

  it("offers Start as the one next step when everything is ready", async () => {
    const props = mount();

    await userEvent.click(
      screen.getByRole("button", {
        name: en.home.status.startTool.replace("{{name}}", "Claude Code"),
      }),
    );
    expect(props.onStart).toHaveBeenCalledWith(props.startTool);
  });

  it("announces a running connection check instead of a stale timestamp", () => {
    mount({ rechecking: true, checkingConnections: true });

    expect(screen.getByText(en.home.health.checkingConnections)).toBeVisible();
    expect(screen.queryByText(/Last checked/)).toBeNull();
    expect(
      screen.getByRole("button", { name: en.home.health.recheck }),
    ).toBeDisabled();
  });

  it("keeps a failed connection start inline rather than replacing the status", () => {
    mount({ connectionCheckError: true });

    expect(
      screen.getByRole("alert", { name: en.home.health.connectionError }),
    ).toBeVisible();
    expect(screen.getByText(/Last checked/)).toBeVisible();
  });

  it("counts updates beside the attention count and opens the recommendation", async () => {
    const tools = [tool({ status: "updateAvailable", latestVersion: "1.1.0" })];
    const summary = aggregateQuickCheck(tools, HEALTHY, {});
    const props = mount(
      {
        summary,
        recommendation: recommendHomeAction(summary),
        startTool: null,
      },
      tools,
    );

    expect(
      screen.getByRole("heading", {
        level: 1,
        name: "1 item needs attention",
      }),
    ).toBeVisible();
    expect(screen.getByText("1 update available")).toBeVisible();
    await userEvent.click(
      screen.getByRole("button", { name: en.home.status.actions.tools }),
    );
    expect(props.onReview).toHaveBeenCalledWith("tools");
  });

  it("counts updates as a way to the Software page, not as an update action", async () => {
    const tools = [tool({ status: "updateAvailable", latestVersion: "1.1.0" })];
    const summary = aggregateQuickCheck(tools, HEALTHY, {});
    const props = mount({ summary, startTool: null }, tools);

    await userEvent.click(
      screen.getByRole("button", { name: "1 update available" }),
    );
    expect(props.onOpenUpdates).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: /^Update/ })).toBeNull();
  });

  it("stays a quiet line rather than a banner card", () => {
    mount();
    const heading = screen.getByRole("heading", { level: 1 });
    expect(heading.closest(".ds-card")).toBeNull();
    for (const button of screen.getAllByRole("button")) {
      expect(button).not.toHaveClass("ds-button-primary");
    }
  });
});
