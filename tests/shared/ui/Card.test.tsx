import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Card } from "@/shared/ui/Card";

describe("Card", () => {
  it("renders a lifted surface with a soft border and radius", () => {
    render(<Card data-testid="card">Body</Card>);
    const card = screen.getByTestId("card");
    expect(card.className).toContain("ds-card");
    expect(card.className).toContain("rounded-lg");
    expect(card.className).toContain("p-4");
    // Depth is a lighter film, not a drop shadow.
    expect(card.className).not.toContain("shadow-");
  });

  it("supports the padding scale including none", () => {
    const { rerender } = render(
      <Card data-testid="card" padding="lg">
        Body
      </Card>,
    );
    expect(screen.getByTestId("card").className).toContain("p-6");
    rerender(
      <Card data-testid="card" padding="none">
        Body
      </Card>,
    );
    expect(screen.getByTestId("card").className).not.toContain("p-4");
  });

  it("adds a token-timed hover treatment when interactive", () => {
    render(
      <Card data-testid="card" interactive>
        Body
      </Card>,
    );
    const className = screen.getByTestId("card").className;
    expect(className).toContain("hover:bg-layer-2");
    expect(className).toContain("duration-fast");
  });
});
