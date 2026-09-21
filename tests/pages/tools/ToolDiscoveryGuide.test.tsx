import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Tool } from "@/entities/tool";
import en from "@/i18n/locales/en.json";
import { ToolDiscoveryGuide } from "@/pages/tools/ToolDiscoveryGuide";

const CAPABILITIES: Tool["capabilities"] = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: false,
  canLaunch: true,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: true,
};

const TOOLS: Tool[] = [
  {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "tool.claude-code.description",
    discovery: {
      publisher: "Anthropic",
      access: "vendorOrProvider",
      useCases: ["officialCoding"],
    },
    status: "notInstalled",
    version: null,
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: "macos",
  },
  {
    id: "opencode",
    name: "OpenCode",
    descriptionKey: "tool.opencode.description",
    discovery: {
      publisher: "OpenCode",
      access: "provider",
      useCases: ["modelChoice"],
    },
    status: "notInstalled",
    version: null,
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: "macos",
  },
];

describe("ToolDiscoveryGuide", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { tool: en.tool, tools: en.tools },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("compares not-installed tools by native discovery metadata", async () => {
    const onChoose = vi.fn();
    render(<ToolDiscoveryGuide tools={TOOLS} onChoose={onChoose} />);

    const guide = screen.getByRole("region", {
      name: en.tools.discovery.title,
    });
    expect(within(guide).getByText("Claude Code")).toBeInTheDocument();
    expect(
      within(guide).getByText("Published by Anthropic"),
    ).toBeInTheDocument();
    expect(within(guide).queryByText("OpenCode")).toBeNull();

    await userEvent.click(
      within(guide).getByRole("button", {
        name: en.tools.discovery.scenario.modelChoice,
      }),
    );
    expect(within(guide).getByText("OpenCode")).toBeInTheDocument();
    expect(within(guide).queryByText("Claude Code")).toBeNull();

    await userEvent.click(
      within(guide).getByRole("button", { name: "Show OpenCode" }),
    );
    expect(onChoose).toHaveBeenCalledWith(TOOLS[1]);
  });

  it("stays hidden when no uninstalled row has discovery metadata", () => {
    render(
      <ToolDiscoveryGuide
        tools={TOOLS.map((tool) => ({ ...tool, discovery: undefined }))}
      />,
    );
    expect(
      screen.queryByRole("region", { name: en.tools.discovery.title }),
    ).toBeNull();
  });
});
