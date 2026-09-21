import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ScopeTabs } from "@/shared/ui/ScopeTabs";

const ITEMS = [
  { id: "skill", label: "Skills" },
  { id: "mcp", label: "MCP" },
];

describe("ScopeTabs", () => {
  it("renders one tab per item under a named tablist", () => {
    render(
      <ScopeTabs
        items={ITEMS}
        active="skill"
        label="Extension type"
        onSelect={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("tablist", { name: "Extension type" }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("tab")).toHaveLength(2);
  });

  it("can explicitly connect every tab to its controlled panel", () => {
    render(
      <ScopeTabs
        items={ITEMS}
        active="skill"
        label="Extension type"
        idPrefix="extension-kind"
        panelId="extension-kind-panel"
        onSelect={vi.fn()}
      />,
    );
    const tablist = screen.getByRole("tablist", { name: "Extension type" });
    expect(tablist).toHaveAttribute("aria-orientation", "horizontal");
    expect(screen.getByRole("tab", { name: "Skills" })).toHaveAttribute(
      "id",
      "extension-kind-skill-tab",
    );
    for (const tab of screen.getAllByRole("tab")) {
      expect(tab).toHaveAttribute("aria-controls", "extension-kind-panel");
    }
  });

  it("marks the active tab and reports a change", async () => {
    const onSelect = vi.fn();
    render(
      <ScopeTabs
        items={ITEMS}
        active="skill"
        label="Extension type"
        onSelect={onSelect}
      />,
    );
    expect(screen.getByRole("tab", { name: "Skills" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: "MCP" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
    await userEvent.click(screen.getByRole("tab", { name: "MCP" }));
    expect(onSelect).toHaveBeenCalledWith("mcp");
  });

  it("renders nothing but an empty tablist when there is nothing to pick", () => {
    render(
      <ScopeTabs items={[]} active={null} label="AI tool" onSelect={vi.fn()} />,
    );
    expect(
      screen.getByRole("tablist", { name: "AI tool" }),
    ).toBeEmptyDOMElement();
  });

  it("keeps the first tab reachable when the active id matches nothing", () => {
    render(
      <ScopeTabs
        items={ITEMS}
        active="prompt"
        label="Extension type"
        onSelect={vi.fn()}
      />,
    );
    const [first, second] = screen.getAllByRole("tab");
    expect(first).toHaveAttribute("tabindex", "0");
    expect(first).toHaveAttribute("aria-selected", "false");
    expect(second).toHaveAttribute("tabindex", "-1");
  });

  it("uses roving tab stops and arrow keys to move and select", async () => {
    const onSelect = vi.fn();
    render(
      <ScopeTabs
        items={ITEMS}
        active="skill"
        label="Extension type"
        onSelect={onSelect}
      />,
    );
    const skills = screen.getByRole("tab", { name: "Skills" });
    const mcp = screen.getByRole("tab", { name: "MCP" });
    expect(skills).toHaveAttribute("tabindex", "0");
    expect(mcp).toHaveAttribute("tabindex", "-1");

    skills.focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(mcp).toHaveFocus();
    expect(onSelect).toHaveBeenLastCalledWith("mcp");
  });

  it("wraps and supports Home and End", async () => {
    const onSelect = vi.fn();
    render(
      <ScopeTabs
        items={ITEMS}
        active="mcp"
        label="Extension type"
        onSelect={onSelect}
      />,
    );
    const skills = screen.getByRole("tab", { name: "Skills" });
    const mcp = screen.getByRole("tab", { name: "MCP" });
    mcp.focus();

    await userEvent.keyboard("{ArrowRight}");
    expect(skills).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(mcp).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(skills).toHaveFocus();
  });
});
