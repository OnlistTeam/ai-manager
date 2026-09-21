import { useEffect, useLayoutEffect, useState, type ReactNode } from "react";
import type { AppRoute } from "./routes";
import {
  ROUTE_TRANSITION_MS,
  type RouteMotionDirection,
} from "./useRouteMotion";

interface RouteContentTransitionProps {
  route: AppRoute;
  direction: RouteMotionDirection;
  renderRoute: (route: AppRoute) => ReactNode;
}

interface RouteContentState {
  current: AppRoute;
  previous: AppRoute | null;
  direction: RouteMotionDirection;
  /** Whether a real switch has happened yet. The first screen has no previous page to replace. */
  navigated: boolean;
}

/** Keeps the departing page alive while both content layers glide vertically. */
export function RouteContentTransition({
  route,
  direction,
  renderRoute,
}: RouteContentTransitionProps) {
  const [state, setState] = useState<RouteContentState>({
    current: route,
    previous: null,
    direction,
    navigated: false,
  });

  useLayoutEffect(() => {
    setState((current) =>
      current.current === route
        ? current
        : {
            current: route,
            previous: current.current,
            direction,
            navigated: true,
          },
    );
  }, [direction, route]);

  // Destructured because `state.current` reads like a ref to static analysis.
  const {
    current: currentRoute,
    previous: previousRoute,
    direction: activeDirection,
    navigated,
  } = state;

  useEffect(() => {
    if (previousRoute === null) return undefined;
    const timer = window.setTimeout(() => {
      setState((current) =>
        current.current === currentRoute
          ? { ...current, previous: null }
          : current,
      );
    }, ROUTE_TRANSITION_MS);
    return () => window.clearTimeout(timer);
  }, [currentRoute, previousRoute]);

  return (
    <div className="app-route-stage">
      {/* One key per route on both layers: the page that is leaving keeps
       * its DOM node (and its state) while it slides out, instead of being
       * rebuilt from scratch in its initial state for the transition. */}
      {previousRoute ? (
        <div
          key={previousRoute}
          aria-hidden="true"
          data-app-route-content={previousRoute}
          data-route-content-layer="outgoing"
          data-transition-direction={activeDirection}
          className="app-route-transition app-route-transition--outgoing"
        >
          {renderRoute(previousRoute)}
        </div>
      ) : null}
      <div
        key={currentRoute}
        data-app-route-content={currentRoute}
        data-route-content-layer="incoming"
        data-transition-direction={activeDirection}
        // Only play the enter animation when there's actually a page being
        // replaced. On the first screen it's a 1000ms elastic scale
        // (0.996 → 1, with the easing overshooting too), and there's nothing
        // underneath to swap out: its only effect is to continuously scale
        // the entire screen's text for a second, re-rasterizing the glyphs
        // every frame with a four-thousandths difference, which just looks
        // like the icons and text jittering.
        className={
          navigated
            ? "app-route-transition app-route-transition--incoming app-route-transition--entering"
            : "app-route-transition app-route-transition--incoming"
        }
      >
        {renderRoute(currentRoute)}
      </div>
    </div>
  );
}
