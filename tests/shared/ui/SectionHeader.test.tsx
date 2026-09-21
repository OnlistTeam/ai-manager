import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SectionHeader } from "@/shared/ui/SectionHeader";

describe("SectionHeader", () => {
  it("renders a level 2 heading by default", () => {
    render(<SectionHeader title="AI Tools" />);
    const heading = screen.getByRole("heading", { level: 2, name: "AI Tools" });
    expect(heading.className).toContain("text-title");
  });

  it("accepts another heading level so pages keep a valid outline", () => {
    render(<SectionHeader as="h1" title="Home" />);
    expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument();
  });

  it("renders an optional description and action slot", () => {
    render(
      <SectionHeader
        title="AI Tools"
        description="Install and update your coding tools"
        action={<button type="button">Check</button>}
      />,
    );
    expect(
      screen.getByText("Install and update your coding tools").className,
    ).toContain("text-content-muted");
    expect(screen.getByRole("button", { name: "Check" })).toBeInTheDocument();
  });
});
