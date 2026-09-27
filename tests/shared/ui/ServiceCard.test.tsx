import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { ServiceCard } from "@/shared/ui/ServiceCard";

describe("ServiceCard", () => {
  it("lists who currently uses the service", () => {
    render(<ServiceCard name="Claude" usedBy={["Claude Code", "OpenCode"]} />);
    expect(screen.getByText("Claude")).toBeInTheDocument();
    // The in-use badge and accent bar carry it on screen; the tool names are
    // for screen readers only, so a list row stays one line tall.
    expect(screen.getByText("Claude Code, OpenCode")).toHaveClass("sr-only");
    expect(screen.getByRole("article", { name: "Claude" })).toBeInTheDocument();
  });

  it("is a list row that marks keyboard focus without looking clickable", () => {
    render(<ServiceCard name="Claude" usedBy={[]} />);
    const card = screen.getByRole("article", { name: "Claude" });
    expect(card).toHaveClass(
      "ds-list-row",
      "min-w-0",
      "focus-within:bg-layer-1",
    );
    expect(card).not.toHaveClass("ds-card");
    expect(card).not.toHaveClass("hover:-translate-y-0.5");
    expect(card).not.toHaveClass("hover:border-brand/25");
    expect(card).not.toHaveClass("hover:shadow-md");
  });

  it("adds no relationship copy when nothing uses it", () => {
    render(<ServiceCard name="OpenRouter" usedBy={[]} />);
    const row = screen.getByRole("article", { name: "OpenRouter" });
    expect(row.querySelector(".sr-only")).toBeNull();
    expect(screen.queryByText("ds.service.notUsed")).toBeNull();
  });

  it("offers no use or switch action of its own", () => {
    render(<ServiceCard name="Claude" usedBy={[]} />);
    // Which service a tool uses is chosen on Home; the row only manages it.
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByRole("article", { name: "Claude" })).not.toHaveAttribute(
      "data-state",
    );
  });

  it("marks the service in use with a badge and an accent bar", () => {
    render(<ServiceCard name="Claude" active usedBy={["Claude Code"]} />);
    expect(screen.getByText("ds.service.nowActive")).toBeInTheDocument();
    const row = screen.getByRole("article", { name: "Claude" });
    expect(row).toHaveAttribute("data-state", "in-use");
    // A decorative accent bar lets the eye find the row in effect first.
    expect(row.querySelector('[aria-hidden="true"].bg-success')).not.toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("keeps a user-supplied long name readable beside its identity badges", async () => {
    const name =
      "Shared Team Production Relay for Claude Code and Every Workstation Anywhere";
    render(
      <ServiceCard
        name={name}
        active
        usedBy={["Claude Code"]}
        meta={<span>Custom service</span>}
        actions={<button type="button">Check address</button>}
      />,
    );

    const heading = screen.getByRole("heading", { name });
    expect(heading).toHaveClass("break-words");
    expect(heading).not.toHaveClass("truncate");
    expect(
      within(heading.parentElement!).getByText("ds.service.nowActive"),
    ).toBeInTheDocument();

    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Check address" })).toHaveFocus();
  });

  it("announces a busy row", () => {
    render(<ServiceCard name="Claude" usedBy={[]} busy />);
    expect(screen.getByRole("article", { name: "Claude" })).toHaveAttribute(
      "aria-busy",
      "true",
    );
  });

  it("renders the extra detail and actions when they are given", () => {
    render(
      <ServiceCard
        name="My Relay"
        usedBy={["Claude Code"]}
        active
        detail={<span>sk-ant-••••••••A12F</span>}
        actions={<button type="button">Check</button>}
      />,
    );
    expect(screen.getByText("sk-ant-••••••••A12F")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check" })).toBeInTheDocument();
  });
});
