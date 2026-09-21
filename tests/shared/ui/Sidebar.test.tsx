import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { Home, Package, Puzzle, Settings, Wrench } from "lucide-react";
import { describe, expect, it, vi } from "vitest";
import ja from "@/i18n/locales/ja.json";
import { Sidebar, type SidebarItem } from "@/shared/ui/Sidebar";

const ITEMS: SidebarItem[] = [
  { id: "home", label: "Home", icon: Home },
  { id: "tools", label: "AI Tools", icon: Wrench },
  { id: "services", label: "AI Services", icon: Package },
  { id: "extensions", label: "Extensions", icon: Puzzle },
];

const FOOTER_ITEMS: SidebarItem[] = [
  { id: "settings", label: "Settings", icon: Settings },
];

const DESTINATION_COUNT = ITEMS.length + FOOTER_ITEMS.length;

describe("Sidebar", () => {
  it("renders a labelled navigation landmark with every destination", () => {
    render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={vi.fn()}
      />,
    );
    const nav = screen.getByRole("navigation", { name: "ds.sidebar.primary" });
    expect(nav).toBeInTheDocument();
    expect(within(nav).getAllByRole("button")).toHaveLength(DESTINATION_COUNT);
    expect(
      within(nav)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["Home", "AI Tools", "AI Services", "Extensions", "Settings"]);
  });

  it("has no folded groups or disclosure controls", () => {
    render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={vi.fn()}
      />,
    );
    for (const button of screen.getAllByRole("button")) {
      expect(button).not.toHaveAttribute("aria-expanded");
    }
    expect(screen.queryByRole("group")).toBeNull();
  });

  it("pins the footer destinations to the bottom of the rail", () => {
    render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={vi.fn()}
      />,
    );
    const settings = screen.getByRole("button", { name: "Settings" });
    const footer = settings.closest("[data-sidebar-footer]");
    expect(footer).not.toBeNull();
    expect(footer).toHaveClass("mt-auto");
    expect(
      screen
        .getByRole("button", { name: "Home" })
        .closest("[data-sidebar-footer]"),
    ).toBeNull();
  });

  it("marks the active item for assistive tech, not only with colour", () => {
    render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="tools"
        onSelect={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "AI Tools" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: "Home" })).not.toHaveAttribute(
      "aria-current",
    );
  });

  it("highlights an active footer destination the same way", () => {
    render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="settings"
        onSelect={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("reports the selected id", async () => {
    const onSelect = vi.fn();
    render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={onSelect}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(onSelect).toHaveBeenCalledWith("settings");
  });

  it("uses the spec widths and keeps names when collapsed", () => {
    const { container, rerender } = render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={vi.fn()}
      />,
    );
    expect(container.firstElementChild?.className).toContain("w-[220px]");
    rerender(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={vi.fn()}
        collapsed
      />,
    );
    expect(container.firstElementChild?.className).toContain("w-[72px]");
    expect(
      screen.getByRole("button", { name: "AI Tools" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Settings" }),
    ).toBeInTheDocument();
  });

  it("reveals an icon-only destination on hover without a native title", async () => {
    const user = userEvent.setup();
    render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={vi.fn()}
        collapsed
      />,
    );
    const tools = screen.getByRole("button", { name: "AI Tools" });

    expect(tools).not.toHaveAttribute("title");
    await user.hover(tools);
    expect(await screen.findByRole("tooltip")).toHaveTextContent("AI Tools");
    expect(tools).toHaveAttribute("data-state", "delayed-open");
  });

  it("does not duplicate labels with a popup in the expanded layout", async () => {
    const user = userEvent.setup();
    render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={vi.fn()}
      />,
    );
    const tools = screen.getByRole("button", { name: "AI Tools" });

    expect(tools).not.toHaveAttribute("title");
    await user.hover(tools);
    await new Promise((resolve) => setTimeout(resolve, 220));
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    expect(tools).toHaveAttribute("data-state", "closed");
  });

  it("reveals the compact layout control from keyboard focus", async () => {
    const user = userEvent.setup();
    render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={vi.fn()}
        collapsed
        onToggleCollapse={vi.fn()}
      />,
    );

    await user.tab();
    expect(screen.getByRole("button", { name: "Home" })).toHaveFocus();
    // Every destination, footer included, sits before the layout control.
    for (let index = 1; index < DESTINATION_COUNT; index += 1) {
      await user.tab();
    }
    expect(screen.getByRole("button", { name: "Settings" })).toHaveFocus();
    await user.tab();

    const expand = screen.getByRole("button", { name: "ds.sidebar.expand" });
    expect(expand).toHaveFocus();
    expect(await screen.findByRole("tooltip")).toHaveTextContent(
      "ds.sidebar.expand",
    );
    expect(expand).toHaveAttribute("data-state", "instant-open");
  });

  it("renders the collapse control and the logo slot only when asked", async () => {
    const onToggleCollapse = vi.fn();
    render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={vi.fn()}
        onToggleCollapse={onToggleCollapse}
        logo={<span>AI Manager</span>}
      />,
    );
    expect(screen.getByText("AI Manager")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "ds.sidebar.collapse" }),
    );
    expect(onToggleCollapse).toHaveBeenCalledTimes(1);
  });

  it("lets a long localized collapse action grow instead of clipping its visible name", async () => {
    i18n.addResourceBundle("ja", "translation", { ds: ja.ds }, true, true);
    await i18n.changeLanguage("ja");
    const view = render(
      <Sidebar
        items={ITEMS}
        footerItems={FOOTER_ITEMS}
        activeId="home"
        onSelect={vi.fn()}
        onToggleCollapse={vi.fn()}
      />,
    );

    try {
      const labelText = ja.ds.sidebar.collapse;
      const control = screen.getByRole("button", { name: labelText });
      const visibleLabel = within(control).getByText(labelText);

      expect(control).toHaveClass("min-h-10", "py-2");
      expect(control).not.toHaveClass("h-10");
      expect(visibleLabel).toHaveClass(
        "min-w-0",
        "whitespace-normal",
        "break-words",
      );
      expect(visibleLabel).not.toHaveClass("truncate");
    } finally {
      view.unmount();
      await i18n.changeLanguage("zh");
    }
  });
});
