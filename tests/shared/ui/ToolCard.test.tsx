import fs from "node:fs";
import path from "node:path";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Tool, ToolCapabilities } from "@/native/schemas/tool";
import { ToolCard } from "@/shared/ui/ToolCard";

const TOOL_CARD_TSX = path.resolve(
  __dirname,
  "..",
  "..",
  "..",
  "src",
  "shared",
  "ui",
  "ToolCard.tsx",
);

const ALL_CAPABILITIES: ToolCapabilities = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: true,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: false,
  canLaunch: true,
};

function makeTool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: "claude-code",
    name: "Claude Code",
    // Not a real key on purpose: i18next echoes it back, so the description
    // can never collide with a status label in a query.
    descriptionKey: "toolDescriptionFixture",
    status: "installed",
    version: "2.3.1",
    latestVersion: null,
    capabilities: ALL_CAPABILITIES,
    sessionsInsideSettings: false,
    environment: null,
    ...overrides,
  };
}

describe("ToolCard", () => {
  async function openMore(name = "More actions for Claude Code") {
    await userEvent.click(screen.getByRole("button", { name }));
  }

  it("shows the name, translated description and version", () => {
    render(<ToolCard tool={makeTool()} onAction={vi.fn()} />);
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    expect(screen.getByText("toolDescriptionFixture")).toBeInTheDocument();
    expect(screen.getByText("2.3.1")).toBeInTheDocument();
    expect(screen.getByText("ds.tool.installed")).toBeInTheDocument();
  });

  it("shows up-to-date only when native versions match exactly", () => {
    const { rerender } = render(
      <ToolCard
        tool={makeTool({ latestVersion: "2.3.1" })}
        onAction={vi.fn()}
      />,
    );
    expect(screen.getByText("tools.upToDate")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "ds.action.named.open" }),
    ).toHaveAttribute("data-variant", "secondary");

    rerender(
      <ToolCard tool={makeTool({ latestVersion: null })} onAction={vi.fn()} />,
    );
    expect(screen.queryByText("tools.upToDate")).toBeNull();
  });

  it("emphasizes nested keyboard actions without making the whole row look clickable", () => {
    render(<ToolCard tool={makeTool()} onAction={vi.fn()} />);
    const row = screen.getByText("Claude Code").closest(".group");
    // The row lifts on focus; the card, its border and its rounding belong to
    // the surrounding ListGroup, so the row itself has none of them.
    expect(row).toHaveClass("ds-list-row", "focus-within:bg-layer-1");
    expect(row).not.toHaveClass("hover:-translate-y-0.5");
    expect(row).not.toHaveClass("hover:bg-layer-1");
    expect(row).not.toHaveClass("shadow-md");
  });

  it("offers Install when the tool is missing", async () => {
    const onAction = vi.fn();
    render(
      <ToolCard
        tool={makeTool({ status: "notInstalled", version: null })}
        onAction={onAction}
      />,
    );
    expect(screen.getByText("ds.tool.notInstalled")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.named.install" }),
    );
    expect(onAction).toHaveBeenCalledWith("install");
    expect(
      screen.queryByRole("button", { name: "ds.action.named.remove" }),
    ).toBeNull();
  });

  it("offers Update and shows both versions when an update is available", async () => {
    const onAction = vi.fn();
    render(
      <ToolCard
        tool={makeTool({
          status: "updateAvailable",
          version: "2.3.1",
          latestVersion: "2.4.0",
        })}
        onAction={onAction}
      />,
    );
    expect(screen.getByText("2.3.1 → 2.4.0")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.named.update" }),
    );
    expect(
      screen.getByRole("button", { name: "ds.action.named.update" }),
    ).toHaveAttribute("data-variant", "primary");
    expect(onAction).toHaveBeenCalledWith("update");
    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.named.open" }),
    );
    expect(onAction).toHaveBeenCalledWith("open");
  });

  it("offers Fix when broken and Check when unknown", () => {
    const { rerender } = render(
      <ToolCard tool={makeTool({ status: "broken" })} onAction={vi.fn()} />,
    );
    expect(
      screen.getByRole("button", { name: "ds.action.named.fix" }),
    ).toBeInTheDocument();
    rerender(
      <ToolCard tool={makeTool({ status: "unknown" })} onAction={vi.fn()} />,
    );
    expect(
      screen.getByRole("button", { name: "ds.action.named.check" }),
    ).toBeInTheDocument();
  });

  it("drives the buttons from capabilities, never from the tool id", () => {
    render(
      <ToolCard
        tool={makeTool({
          status: "updateAvailable",
          capabilities: {
            ...ALL_CAPABILITIES,
            canUpdate: false,
            canUninstall: false,
            canLaunch: false,
          },
        })}
        onAction={vi.fn()}
      />,
    );
    expect(
      screen.queryByRole("button", { name: "ds.action.named.update" }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: "ds.action.named.remove" }),
    ).toBeNull();
    expect(screen.queryAllByRole("button")).toHaveLength(0);
  });

  it("explains when the status has no safe automatic action", async () => {
    render(
      <ToolCard
        tool={makeTool({
          status: "broken",
          capabilities: { ...ALL_CAPABILITIES, canRepair: false },
        })}
        onAction={vi.fn()}
      />,
    );

    const notice = screen.getByText("ds.tool.repairUnavailable");
    expect(notice).toBeVisible();
    expect(notice.parentElement?.querySelector("svg")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
    expect(
      screen.queryByRole("button", { name: "ds.action.named.fix" }),
    ).toBeNull();

    const more = screen.getByRole("button", {
      name: "tools.actions.moreNamed",
    });
    await userEvent.tab();
    expect(more).toHaveFocus();
  });

  it.each([
    ["notInstalled", "canInstall", "install", "ds.tool.installUnavailable"],
    ["installed", "canLaunch", "open", "ds.tool.openUnavailable"],
    ["updateAvailable", "canUpdate", "update", "ds.tool.updateUnavailable"],
  ] as const)(
    "uses action-specific guidance for %s",
    (status, capability, action, messageKey) => {
      render(
        <ToolCard
          tool={makeTool({
            status,
            capabilities: { ...ALL_CAPABILITIES, [capability]: false },
          })}
          onAction={vi.fn()}
        />,
      );

      expect(screen.getByText(messageKey)).toBeVisible();
      expect(
        screen.queryByRole("button", { name: `ds.action.named.${action}` }),
      ).toBeNull();
    },
  );

  it("hides Open when launch is not a capability", async () => {
    render(
      <ToolCard
        tool={makeTool({
          capabilities: { ...ALL_CAPABILITIES, canLaunch: false },
        })}
        onAction={vi.fn()}
      />,
    );
    expect(
      screen.queryByRole("button", { name: "ds.action.named.open" }),
    ).toBeNull();
    await openMore("tools.actions.moreNamed");
    expect(screen.getByText("ds.action.remove")).toBeInTheDocument();
  });

  it("shows progress only on the pending launch action", () => {
    render(
      <ToolCard
        tool={makeTool()}
        busy
        pendingAction="open"
        onAction={vi.fn()}
      />,
    );
    const open = screen.getByRole("button", {
      name: "ds.action.named.open",
    });
    expect(open).toHaveAttribute("aria-busy", "true");
    expect(open.querySelector("[data-loading-indicator]")).toHaveClass(
      "motion-safe:animate-spin",
    );
    expect(
      screen.getByRole("button", { name: "tools.actions.moreNamed" }),
    ).not.toHaveAttribute("aria-busy");
  });

  it("does not offer version management before a tool is installed", () => {
    render(
      <ToolCard
        tool={makeTool({
          status: "notInstalled",
          version: null,
          capabilities: { ...ALL_CAPABILITIES, canManageVersion: true },
        })}
        onAction={vi.fn()}
      />,
    );
    expect(
      screen.queryByRole("button", { name: "tools.actions.moreNamed" }),
    ).toBeNull();
  });

  it("disables every action while busy", () => {
    render(<ToolCard tool={makeTool()} busy onAction={vi.fn()} />);
    const buttons = screen.getAllByRole("button");
    expect(buttons.length).toBeGreaterThan(0);
    for (const button of buttons) {
      expect(button).toBeDisabled();
    }
  });

  it("replaces actions with active task context", () => {
    render(
      <ToolCard
        tool={makeTool()}
        activeTask={<div>Installing safely</div>}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByText("Installing safely")).toBeInTheDocument();
    expect(screen.queryAllByRole("button")).toHaveLength(0);
    expect(
      screen.getByText("Claude Code").closest("[aria-busy]"),
    ).toHaveAttribute("aria-busy", "true");
  });

  it("groups software details with the right-hand actions without launching the tool", async () => {
    const onDetails = vi.fn();
    const onAction = vi.fn();
    render(
      <ToolCard
        tool={makeTool({ status: "updateAvailable" })}
        onAction={onAction}
        onDetails={onDetails}
      />,
    );
    const actions = screen.getByRole("group", { name: "Claude Code" });
    const details = within(actions).getByRole("button", {
      name: "tools.details.title",
    });
    expect(details).toHaveAttribute("data-variant", "secondary");
    expect(
      within(actions).getByRole("button", { name: "ds.action.named.update" }),
    ).toBeInTheDocument();
    expect(
      within(actions).getByRole("button", { name: "ds.action.named.open" }),
    ).toBeInTheDocument();
    await userEvent.click(details);
    expect(onDetails).toHaveBeenCalledOnce();
    expect(onAction).not.toHaveBeenCalled();
  });

  it("keeps details available alongside task feedback", async () => {
    const onDetails = vi.fn();
    render(
      <ToolCard
        tool={makeTool()}
        activeTask={<div>Installing safely</div>}
        onAction={vi.fn()}
        onDetails={onDetails}
      />,
    );
    const actions = screen.getByRole("group", { name: "Claude Code" });
    expect(within(actions).getByText("Installing safely")).toBeInTheDocument();
    await userEvent.click(
      within(actions).getByRole("button", { name: "tools.details.title" }),
    );
    expect(onDetails).toHaveBeenCalledOnce();
  });

  it("can present terminal task feedback without claiming work is still running", () => {
    render(
      <ToolCard
        tool={makeTool()}
        activeTask={<button type="button">Close failure</button>}
        activeTaskBusy={false}
        onAction={vi.fn()}
      />,
    );

    const card = screen.getByText("Claude Code").closest("[data-tool-card]");
    expect(card).not.toHaveAttribute("aria-busy");
    expect(screen.getByRole("button", { name: "Close failure" })).toBeEnabled();
  });

  it("never branches on a tool id", () => {
    const source = fs.readFileSync(TOOL_CARD_TSX, "utf8");
    expect(source).not.toContain("claude-code");
    expect(source).not.toContain("gemini-cli");
    expect(source).not.toContain("opencode");
  });
});
