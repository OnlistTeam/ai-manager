import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RouteContentTransition } from "@/app/RouteContentTransition";
import { ROUTE_TRANSITION_MS } from "@/app/useRouteMotion";

describe("RouteContentTransition", () => {
  afterEach(() => vi.useRealTimers());

  it("keeps the departing page for the whole vertical transition", () => {
    vi.useFakeTimers();
    const renderRoute = (route: string) => <p>{route}</p>;
    const { container, rerender } = render(
      <RouteContentTransition
        route="home"
        direction="forward"
        renderRoute={renderRoute}
      />,
    );

    rerender(
      <RouteContentTransition
        route="tools"
        direction="forward"
        renderRoute={renderRoute}
      />,
    );

    const outgoing = container.querySelector(
      '[data-route-content-layer="outgoing"]',
    );
    const incoming = container.querySelector(
      '[data-route-content-layer="incoming"]',
    );
    expect(outgoing).toHaveAttribute("data-app-route-content", "home");
    expect(outgoing).toHaveAttribute("aria-hidden", "true");
    expect(incoming).toHaveAttribute("data-app-route-content", "tools");
    expect(incoming).toHaveAttribute("data-transition-direction", "forward");

    act(() => vi.advanceTimersByTime(ROUTE_TRANSITION_MS));
    expect(
      container.querySelector('[data-route-content-layer="outgoing"]'),
    ).toBeNull();
  });

  it("lets the departing page slide out with its own state instead of a fresh copy", () => {
    vi.useFakeTimers();
    const renderRoute = (route: string) => (
      <input aria-label={route} defaultValue="" />
    );
    const { container, rerender } = render(
      <RouteContentTransition
        route="home"
        direction="forward"
        renderRoute={renderRoute}
      />,
    );
    const homeInput = screen.getByRole("textbox", { name: "home" });
    fireEvent.change(homeInput, { target: { value: "typed on home" } });

    rerender(
      <RouteContentTransition
        route="tools"
        direction="forward"
        renderRoute={renderRoute}
      />,
    );

    const outgoing = container.querySelector(
      '[data-route-content-layer="outgoing"]',
    );
    expect(outgoing).toContainElement(homeInput);
    expect(homeInput).toHaveValue("typed on home");
  });

  it("reverses both content layers for an earlier navigation item", () => {
    vi.useFakeTimers();
    const renderRoute = (route: string) => <p>{route}</p>;
    const { container, rerender } = render(
      <RouteContentTransition
        route="services"
        direction="forward"
        renderRoute={renderRoute}
      />,
    );

    rerender(
      <RouteContentTransition
        route="home"
        direction="backward"
        renderRoute={renderRoute}
      />,
    );

    expect(
      container.querySelector('[data-route-content-layer="outgoing"]'),
    ).toHaveAttribute("data-transition-direction", "backward");
    expect(
      container.querySelector('[data-route-content-layer="incoming"]'),
    ).toHaveAttribute("data-transition-direction", "backward");
  });
});
