import { render, screen } from "@testing-library/react";
import { CheckCircle2 } from "lucide-react";
import { describe, expect, it } from "vitest";
import { Badge } from "@/shared/ui/Badge";

describe("Badge", () => {
  it("renders neutral by default", () => {
    render(<Badge>Installed</Badge>);
    const badge = screen.getByText("Installed");
    expect(badge.className).toContain("bg-layer-2");
    expect(badge.className).toContain("text-content");
  });

  it("tints the background but never the label", () => {
    render(<Badge tone="warning">Update available</Badge>);
    const badge = screen.getByText("Update available");
    expect(badge.className).toContain("bg-warning/10");
    expect(badge.className).toContain("text-content");
    expect(badge.className).not.toContain("text-warning");
  });

  it("colours only the icon with the status hue", () => {
    const { container } = render(
      <Badge tone="success" icon={CheckCircle2}>
        Ready
      </Badge>,
    );
    const icon = container.querySelector("svg");
    expect(icon).not.toBeNull();
    expect(icon?.getAttribute("class")).toContain("text-success");
    expect(icon).toHaveAttribute("aria-hidden", "true");
  });

  it("supports every tone", () => {
    const { rerender } = render(<Badge tone="brand">A</Badge>);
    expect(screen.getByText("A").className).toContain("bg-brand/10");
    rerender(<Badge tone="danger">A</Badge>);
    expect(screen.getByText("A").className).toContain("bg-danger/10");
  });
});
