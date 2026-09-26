import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ListGroup, ListGroupRow } from "@/shared/ui/ListGroup";

describe("ListGroup", () => {
  it("draws one outline for the whole collection", () => {
    render(
      <ListGroup data-testid="group">
        <ListGroupRow>One</ListGroupRow>
        <ListGroupRow>Two</ListGroupRow>
      </ListGroup>,
    );
    const group = screen.getByTestId("group");
    // Row dividers come from `.ds-list-group > * + *` in index.css.
    expect(group).toHaveClass("ds-card", "ds-list-group", "overflow-hidden");
    expect(group.className).not.toMatch(/\bp-\d/);
  });

  it("gives every row the compact row padding (ADR-0052)", () => {
    render(<ListGroupRow data-testid="row">Row</ListGroupRow>);
    const row = screen.getByTestId("row");
    expect(row).toHaveClass("ds-list-row", "px-4", "py-3", "min-w-0");
    expect(row).not.toHaveClass("p-4");
    expect(row).not.toHaveClass("hover:bg-layer-1");
  });

  it("adds a hover treatment only to rows that act as a target", () => {
    render(
      <ListGroupRow data-testid="row" interactive>
        Row
      </ListGroupRow>,
    );
    expect(screen.getByTestId("row")).toHaveClass(
      "hover:bg-layer-1",
      "duration-fast",
    );
  });
});
