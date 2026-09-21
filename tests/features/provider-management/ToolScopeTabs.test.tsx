import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ToolScopeTabs } from "@/features/provider-management";
import type { Tool } from "@/entities/tool";

function tool(id: Tool["id"], name: string, canManage = true): Tool {
  return {
    id,
    name,
    descriptionKey: `tool.${id}.description`,
    status: "installed",
    version: "1.0.0",
    latestVersion: null,
    sessionsInsideSettings: false,
    environment: "macos",
    capabilities: {
      canInstall: true,
      canUpdate: true,
      canUninstall: true,
      canRepair: false,
      canManageProvider: canManage,
      canManageMcp: true,
      canManageSkills: false,
      canManagePrompts: false,
      canManageVersion: false,
      canLaunch: true,
    },
  };
}

describe("ToolScopeTabs", () => {
  it("keeps every installed tool visible and marks unsupported service scopes", () => {
    render(
      <ToolScopeTabs
        tools={[
          tool("claude-code", "Claude Code"),
          tool("codex", "Codex", false),
        ]}
        active="claude-code"
        label="Tool"
        unsupportedLabel="Not supported"
        onSelect={vi.fn()}
      />,
    );
    expect(screen.getAllByRole("tab")).toHaveLength(2);
    expect(
      screen.getByRole("tab", { name: "Claude Code" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: "Codex, Not supported" }),
    ).toBeInTheDocument();
  });

  it("marks the current tool and reports a change", async () => {
    const onSelect = vi.fn();
    render(
      <ToolScopeTabs
        tools={[tool("claude-code", "Claude Code"), tool("codex", "Codex")]}
        active="claude-code"
        label="Tool"
        onSelect={onSelect}
      />,
    );
    expect(screen.getByRole("tab", { name: "Claude Code" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await userEvent.click(screen.getByRole("tab", { name: "Codex" }));
    expect(onSelect).toHaveBeenCalledWith("codex");
  });

  it("renders all eight provider-capable tools from the backend capability table", () => {
    const tools = [
      tool("claude-code", "Claude Code"),
      tool("codex", "Codex CLI"),
      tool("gemini-cli", "Gemini CLI"),
      tool("opencode", "OpenCode"),
      tool("grok-build", "Grok Build"),
      tool("openclaw", "OpenClaw"),
      tool("hermes", "Hermes"),
      tool("pi", "Pi"),
    ];
    render(
      <ToolScopeTabs
        tools={tools}
        active="grok-build"
        label="Tool"
        onSelect={vi.fn()}
      />,
    );

    expect(screen.getAllByRole("tab")).toHaveLength(8);
    expect(screen.getByRole("tab", { name: "Grok Build" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: "OpenClaw" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Hermes" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Pi" })).toBeInTheDocument();
  });
});
