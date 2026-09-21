import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RefreshCw } from "lucide-react";
import { describe, expect, it, vi } from "vitest";
import { Button } from "@/shared/ui/Button";

describe("Button", () => {
  it("defaults to a non-submitting brand button", () => {
    render(<Button>Install</Button>);
    const button = screen.getByRole("button", { name: "Install" });
    expect(button).toHaveAttribute("type", "button");
    expect(button).toHaveAttribute("data-variant", "primary");
    expect(button).toHaveAttribute("data-size", "md");
    expect(button).toHaveClass("ds-button", "h-10", "rounded-lg");
    expect(button.className).toContain("bg-brand");
    expect(button.className).toContain("text-brand-foreground");
  });

  it("exposes a visible keyboard focus ring", () => {
    render(<Button>Install</Button>);
    // The button itself is filled bg-brand, so its own inner ring has to be
    // the dark brand-foreground token — a brand-coloured ring would fuse
    // with the fill. The global outline (index.css) supplies the second,
    // accent-coloured ring outside it.
    expect(screen.getByRole("button").className).toContain(
      "focus-visible:ring-brand-foreground",
    );
  });

  it("is keyboard operable", async () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Install</Button>);
    await userEvent.tab();
    expect(screen.getByRole("button")).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("blocks interaction and announces busy while loading", async () => {
    const onClick = vi.fn();
    render(
      <Button loading onClick={onClick}>
        Install
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Install" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("aria-busy", "true");
    await userEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("keeps its accessible name while loading", () => {
    render(
      <Button loading>
        <RefreshCw data-testid="action-icon" aria-hidden="true" />
        Refresh
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Refresh" });
    const indicator = button.querySelector("[data-loading-indicator]");
    expect(indicator).toHaveClass("motion-safe:animate-spin");
    expect(indicator?.classList.contains("animate-spin")).toBe(false);

    const content = screen
      .getByTestId("action-icon")
      .closest('[data-slot="button-content"]');
    expect(content).toHaveClass("contents", "[&>svg]:hidden");
  });

  it("stays disabled without the loading spinner when disabled", () => {
    const { container } = render(<Button disabled>Install</Button>);
    expect(screen.getByRole("button")).toBeDisabled();
    expect(screen.getByRole("button")).not.toHaveAttribute("aria-busy");
    expect(container.querySelector("svg")).toBeNull();
  });

  it("renders the secondary, ghost and danger variants", () => {
    const { rerender } = render(<Button variant="secondary">A</Button>);
    expect(screen.getByRole("button").className).toContain("bg-layer-1");
    expect(screen.getByRole("button")).toHaveAttribute(
      "data-variant",
      "secondary",
    );
    rerender(<Button variant="ghost">A</Button>);
    expect(screen.getByRole("button").className).toContain(
      "text-content-muted",
    );
    rerender(<Button variant="danger">A</Button>);
    expect(screen.getByRole("button").className).toContain("bg-danger");
  });

  it("animates with the motion tokens only", () => {
    render(<Button>Install</Button>);
    const className = screen.getByRole("button").className;
    expect(className).toContain("duration-fast");
    expect(className).toContain("ease-standard");
  });

  it("keeps the variant text color when a caller passes a className", () => {
    render(<Button className="w-full">Install</Button>);
    const className = screen.getByRole("button").className;
    expect(className).toContain("w-full");
    expect(className).toContain("text-brand-foreground");
  });

  it("keeps the ghost variant text color when a caller passes a className", () => {
    render(
      <Button variant="ghost" className="w-full">
        Install
      </Button>,
    );
    const className = screen.getByRole("button").className;
    expect(className).toContain("w-full");
    expect(className).toContain("text-content-muted");
  });
});
