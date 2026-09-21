import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Progress } from "@/shared/ui/Progress";

describe("Progress", () => {
  it("exposes an accessible progressbar", () => {
    render(<Progress value={42} label="Installing Claude Code" />);
    const bar = screen.getByRole("progressbar", {
      name: "Installing Claude Code",
    });
    expect(bar).toHaveAttribute("aria-valuenow", "42");
    expect(bar).toHaveAttribute("aria-valuemin", "0");
    expect(bar).toHaveAttribute("aria-valuemax", "100");
  });

  it("clamps out-of-range and non-finite values", () => {
    const { rerender } = render(<Progress value={140} label="x" />);
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "100",
    );
    rerender(<Progress value={-8} label="x" />);
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "0",
    );
    rerender(<Progress value={Number.NaN} label="x" />);
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "0",
    );
  });

  it("drops aria-valuenow when indeterminate", () => {
    const { container } = render(
      <Progress value={0} label="x" indeterminate />,
    );
    expect(screen.getByRole("progressbar")).not.toHaveAttribute(
      "aria-valuenow",
    );
    const fill = container.querySelector("[data-slot='progress-fill']");
    expect(fill).toHaveClass("motion-safe:animate-pulse");
    expect(fill?.classList.contains("animate-pulse")).toBe(false);
  });

  it("animates the fill with the motion tokens", () => {
    const { container } = render(<Progress value={50} label="x" />);
    const fill = container.querySelector("[data-slot='progress-fill']");
    expect(fill?.getAttribute("class")).toContain("duration-base");
    expect(fill?.getAttribute("class")).toContain("ease-standard");
    expect(fill).toHaveStyle({ width: "50%" });
  });
});
