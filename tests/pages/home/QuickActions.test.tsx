import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Download, RefreshCw } from "lucide-react";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { QuickActions, type QuickAction } from "@/pages/home/QuickActions";

const install = vi.fn();
const actions: readonly QuickAction[] = [
  {
    id: "install",
    labelKey: "home.quickActions.installTool",
    descriptionKey: "home.quickActions.installDescription",
    icon: Download,
    onSelect: install,
  },
  {
    id: "update",
    labelKey: "home.quickActions.updateAll",
    descriptionKey: "home.quickActions.updateDescription",
    icon: RefreshCw,
    disabled: true,
  },
];

describe("QuickActions", () => {
  beforeEach(async () => {
    install.mockClear();
    i18n.addResourceBundle("en", "translation", { home: en.home }, true, true);
    await i18n.changeLanguage("en");
  });

  it("makes the full action tile operable and explains it", async () => {
    const { container } = render(<QuickActions actions={actions} />);
    expect(container.firstChild).toHaveClass("min-w-0");
    const action = screen.getByRole("button", { name: "Install AI Tool" });
    expect(action).toHaveClass(
      "quick-action-card",
      "min-h-[144px]",
      "rounded-2xl",
    );
    expect(action).toHaveAccessibleDescription(
      "Add Claude Code, Codex or OpenCode",
    );
    await userEvent.click(action);
    expect(install).toHaveBeenCalledTimes(1);
  });

  it("keeps lift and arrow movement behind the motion preference", () => {
    render(<QuickActions actions={actions} />);
    const action = screen.getByRole("button", { name: "Install AI Tool" });
    expect(action).toHaveClass("motion-safe:hover:-translate-y-0.5");
    expect(action.classList.contains("hover:-translate-y-0.5")).toBe(false);
    const arrow = action.querySelector('[data-slot="quick-action-arrow"]');
    expect(arrow?.parentElement).toHaveClass("quick-action-card__arrow");
    expect(arrow).toHaveClass(
      "motion-safe:group-hover:translate-x-0.5",
      "motion-safe:group-hover:-translate-y-0.5",
    );
  });

  it("keeps unavailable actions disabled without hiding their purpose", () => {
    render(<QuickActions actions={actions} />);
    const action = screen.getByRole("button", { name: "Update All" });
    expect(action).toBeDisabled();
    expect(action).toHaveAccessibleDescription(
      "Keep installed tools on their latest version",
    );
  });
});
