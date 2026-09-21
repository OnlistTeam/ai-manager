import { render, screen } from "@testing-library/react";
import { Inbox } from "lucide-react";
import { describe, expect, it } from "vitest";
import { EmptyState } from "@/shared/ui/EmptyState";

describe("EmptyState", () => {
  it("renders a title as a heading", () => {
    render(<EmptyState title="No tools yet" />);
    expect(
      screen.getByRole("heading", { name: "No tools yet" }),
    ).toBeInTheDocument();
  });

  it("hides the decorative icon from assistive tech", () => {
    const { container } = render(<EmptyState icon={Inbox} title="No tools" />);
    expect(container.querySelector("svg")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
  });

  it("renders the description and the action", () => {
    render(
      <EmptyState
        title="No tools"
        description="Install a tool to get started"
        action={<button type="button">Install</button>}
      />,
    );
    expect(
      screen.getByText("Install a tool to get started"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Install" })).toBeInTheDocument();
  });
});
