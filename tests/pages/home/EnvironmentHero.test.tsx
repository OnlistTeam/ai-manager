import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { HealthSnapshot } from "@/native";
import type { Tool } from "@/entities/tool";
import en from "@/i18n/locales/en.json";
import { aggregateQuickCheck } from "@/features/health";
import { EnvironmentHero } from "@/pages/home/EnvironmentHero";
import type { EnvironmentHeroProps } from "@/pages/home/EnvironmentHero";

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

function mount(overrides: Partial<EnvironmentHeroProps> = {}) {
  const tools = [tool()];
  const props: EnvironmentHeroProps = {
    summary: aggregateQuickCheck(tools, HEALTHY, {}),
    recommendation: null,
    startTool: tools[0],
    checkedAt: Date.UTC(2026, 8, 5, 9, 30),
    rechecking: false,
    checkingConnections: false,
    connectionCheckError: false,
    onRecheck: vi.fn(),
    onReview: vi.fn(),
    onStart: vi.fn(),
    ...overrides,
  };
  render(<EnvironmentHero {...props} />);
  return props;
}

/**
 * The hero used to state the environment's status while a second card below it
 * repeated the same headline just to host the rerun button and the timestamp.
 * Those two now live here, next to the sentence they describe.
 */
describe("EnvironmentHero", () => {
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

    expect(screen.getByText(/Last checked/)).toBeVisible();
    await userEvent.click(
      screen.getByRole("button", { name: en.home.health.recheck }),
    );
    expect(props.onRecheck).toHaveBeenCalledTimes(1);
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
});
