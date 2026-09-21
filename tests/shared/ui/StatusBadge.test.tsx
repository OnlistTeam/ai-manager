import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StatusBadge } from "@/shared/ui/StatusBadge";

describe("StatusBadge", () => {
  it.each([
    ["ready", "ds.status.ready", "success"],
    ["attention", "ds.status.attention", "warning"],
    ["action", "ds.status.action", "danger"],
  ] as const)(
    "renders %s with an icon and a text label, never colour alone",
    (status, labelKey, tone) => {
      const { container } = render(<StatusBadge status={status} />);
      const badge = screen.getByText(labelKey);
      expect(badge).toBeInTheDocument();
      expect(badge).toHaveAttribute("data-status", status);
      expect(badge.className).toContain(`bg-${tone}/10`);
      expect(badge.className).toContain("text-content");
      const icon = container.querySelector("svg");
      expect(icon).not.toBeNull();
      expect(icon?.getAttribute("class")).toContain(`text-${tone}`);
    },
  );

  it("uses a distinct icon per status so colour-blind users can tell them apart", () => {
    const icons = (["ready", "attention", "action"] as const).map((status) => {
      const { container, unmount } = render(<StatusBadge status={status} />);
      const path = container.querySelector("svg")?.innerHTML ?? "";
      unmount();
      return path;
    });
    expect(new Set(icons).size).toBe(3);
  });
});
