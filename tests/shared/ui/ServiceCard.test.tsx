import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ServiceCard } from "@/shared/ui/ServiceCard";

describe("ServiceCard", () => {
  it("lists who currently uses the service", () => {
    render(
      <ServiceCard
        name="Claude"
        connected
        usedBy={["Claude Code", "OpenCode"]}
        onUse={vi.fn()}
      />,
    );
    expect(screen.getByText("Claude")).toBeInTheDocument();
    // The in-use badge and accent bar carry it on screen; the tool names are
    // for screen readers only, so a list row stays one line tall.
    expect(screen.getByText("Claude Code, OpenCode")).toHaveClass("sr-only");
    expect(screen.getByRole("article", { name: "Claude" })).toBeInTheDocument();
  });

  it("is a list row that marks keyboard focus without looking clickable", () => {
    render(<ServiceCard name="Claude" connected usedBy={[]} onUse={vi.fn()} />);
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
    render(
      <ServiceCard name="OpenRouter" connected usedBy={[]} onUse={vi.fn()} />,
    );
    const row = screen.getByRole("article", { name: "OpenRouter" });
    expect(row.querySelector(".sr-only")).toBeNull();
    expect(screen.queryByText("ds.service.notUsed")).toBeNull();
  });

  it("offers Use when connected but not active", async () => {
    const onUse = vi.fn();
    render(<ServiceCard name="Claude" connected usedBy={[]} onUse={onUse} />);
    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.use" }),
    );
    expect(onUse).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("article", { name: "Claude" })).not.toHaveAttribute(
      "data-state",
    );
  });

  it("replaces the Use button with a Now active badge once active", () => {
    render(
      <ServiceCard
        name="Claude"
        connected
        active
        usedBy={["Claude Code"]}
        onUse={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: "ds.action.use" })).toBeNull();
    expect(screen.getByText("ds.service.nowActive")).toBeInTheDocument();
    // `.ds-card` lives outside any `@layer`, so the green fill for the
    // in-use state has to key off this attribute, not a `bg-*` class the
    // unlayered rule would silently beat (see src/index.css `.ds-card`).
    expect(screen.getByRole("article", { name: "Claude" })).toHaveAttribute(
      "data-state",
      "in-use",
    );
  });

  it("keeps a user-supplied long name readable beside its identity badges", async () => {
    const name =
      "Shared Team Production Relay for Claude Code and Every Workstation Anywhere";
    render(
      <ServiceCard
        name={name}
        connected
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

  it("keeps the primary action in place when in use, disabled rather than gone", () => {
    render(
      <ServiceCard
        name="Relay"
        usedBy={["Claude Code"]}
        connected
        active
        useAvailable={false}
        actions={<button type="button">Edit</button>}
        onUse={vi.fn()}
      />,
    );

    // The button must not disappear: if it did, the row of buttons after it
    // would shift left as a block, and each card in the same list would end
    // up with its actions in a different position.
    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(2);
    expect(buttons[0]).toBeDisabled();
    expect(buttons[0]).toHaveTextContent("ds.action.inUse");
    expect(buttons[1]).toHaveTextContent("Edit");
  });

  it("reserves the widest label so the button does not resize between states", () => {
    const { rerender } = render(
      <ServiceCard name="Relay" usedBy={[]} connected onUse={vi.fn()} />,
    );
    const measure = () => {
      const button = screen.getAllByRole("button")[0];
      // All three labels are always present; only the active one is exposed
      // to the accessible name. The button width is therefore pinned to the
      // widest label, so switching states never drags the row of buttons
      // after it left or right.
      return {
        rendered: (button.textContent ?? "").trim(),
        exposed: button.getAttribute("aria-label") ?? button.textContent,
      };
    };

    const idle = measure();
    rerender(
      <ServiceCard
        name="Relay"
        usedBy={["Claude Code"]}
        connected
        active
        useAvailable={false}
        onUse={vi.fn()}
      />,
    );
    const inUse = measure();

    expect(idle.rendered).toBe(inUse.rendered);
    for (const key of ["ds.action.use", "ds.action.inUse", "ds.action.retry"]) {
      expect(idle.rendered).toContain(key);
    }
    expect(
      screen.getByRole("button", { name: "ds.action.inUse" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "ds.action.use" })).toBeNull();
  });

  it("keeps an active card's primary action available while a switch is retryable", async () => {
    const onUse = vi.fn();
    render(
      <ServiceCard
        name="Claude"
        connected
        active
        usedBy={["Claude Code"]}
        useActionState="retry"
        useAriaLabel="Try switching to Claude again"
        onUse={onUse}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: "Try switching to Claude again",
      }),
    );
    expect(onUse).toHaveBeenCalledTimes(1);
  });

  it("offers Connect when the service is not connected yet", async () => {
    const onConnect = vi.fn();
    render(<ServiceCard name="Custom" usedBy={[]} onConnect={onConnect} />);
    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.connect" }),
    );
    expect(onConnect).toHaveBeenCalledTimes(1);
  });

  it("disables its action while busy", () => {
    render(
      <ServiceCard name="Claude" connected usedBy={[]} busy onUse={vi.fn()} />,
    );
    expect(
      screen.getByRole("button", { name: "ds.action.use" }),
    ).toBeDisabled();
  });

  it("renders the extra detail and actions when they are given", () => {
    render(
      <ServiceCard
        name="My Relay"
        usedBy={["Claude Code"]}
        connected
        active
        detail={<span>sk-ant-••••••••A12F</span>}
        actions={<button type="button">Check</button>}
      />,
    );
    expect(screen.getByText("sk-ant-••••••••A12F")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check" })).toBeInTheDocument();
  });

  it("can give a repeated action a service-specific accessible name", () => {
    render(
      <ServiceCard
        name="OpenRouter"
        connected
        usedBy={[]}
        useAriaLabel="Use OpenRouter"
        onUse={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("button", { name: "Use OpenRouter" }),
    ).toHaveTextContent("ds.action.use");
  });
});
