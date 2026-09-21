import { act, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RouteBackdrop } from "@/app/RouteBackdrop";
import { ROUTE_TRANSITION_MS } from "@/app/useRouteMotion";

describe("RouteBackdrop", () => {
  afterEach(() => vi.useRealTimers());

  it("cross-fades the previous complete palette before removing it", () => {
    vi.useFakeTimers();
    const { container, rerender } = render(<RouteBackdrop route="home" />);

    expect(
      container.querySelector('[data-route-backdrop="incoming"]'),
    ).toHaveAttribute("data-route", "home");
    expect(
      container.querySelector('[data-route-backdrop="outgoing"]'),
    ).toBeNull();

    rerender(<RouteBackdrop route="services" />);
    expect(
      container.querySelector('[data-route-backdrop="incoming"]'),
    ).toHaveAttribute("data-route", "services");
    expect(
      container.querySelector('[data-route-backdrop="outgoing"]'),
    ).toHaveAttribute("data-route", "home");

    act(() => vi.advanceTimersByTime(ROUTE_TRANSITION_MS));
    expect(
      container.querySelector('[data-route-backdrop="outgoing"]'),
    ).toBeNull();
  });

  it("carries no direction of travel: a colour change is not a slide", () => {
    const { container, rerender } = render(<RouteBackdrop route="services" />);

    rerender(<RouteBackdrop route="home" />);

    for (const layer of container.querySelectorAll("[data-route-backdrop]")) {
      expect(layer).not.toHaveAttribute("data-transition-direction");
    }
  });
});
