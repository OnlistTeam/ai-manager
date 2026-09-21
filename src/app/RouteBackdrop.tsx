import { useEffect, useLayoutEffect, useState } from "react";
import type { AppRoute } from "./routes";
import { ROUTE_TRANSITION_MS } from "./useRouteMotion";

interface RouteBackdropProps {
  route: AppRoute;
}

interface BackdropState {
  current: AppRoute;
  previous: AppRoute | null;
}

/**
 * Cross-fades complete route palettes; gradients cannot interpolate.
 *
 * The backdrop has no direction of travel — a colour does not arrive from above
 * or below. Only the content layer slides; here the outgoing palette simply
 * dissolves into the incoming one at a constant rate.
 */
export function RouteBackdrop({ route }: RouteBackdropProps) {
  const [state, setState] = useState<BackdropState>({
    current: route,
    previous: null,
  });

  useLayoutEffect(() => {
    setState((current) =>
      current.current === route
        ? current
        : { current: route, previous: current.current },
    );
  }, [route]);

  // Destructured because `state.current` reads like a ref to static analysis.
  const { current: currentRoute, previous: previousRoute } = state;

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
    <div className="app-route-backdrop" aria-hidden="true">
      {previousRoute ? (
        <div
          data-route={previousRoute}
          data-route-backdrop="outgoing"
          className="app-window-canvas app-route-backdrop__layer app-route-backdrop__layer--outgoing"
        />
      ) : null}
      <div
        key={currentRoute}
        data-route={currentRoute}
        data-route-backdrop="incoming"
        className="app-window-canvas app-route-backdrop__layer app-route-backdrop__layer--incoming"
      />
    </div>
  );
}
