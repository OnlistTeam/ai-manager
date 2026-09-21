import { useCallback, useState } from "react";
import { APP_ROUTES, type AppRoute } from "./routes";

export type RouteMotionDirection = "forward" | "backward";

/**
 * How long a destination change takes, in milliseconds. Must stay in step with
 * `--motion-page` in tokens.css: the CSS drives the animation, this constant
 * tells the two transition components when to drop the departing layer.
 */
export const ROUTE_TRANSITION_MS = 1000;

/** Keeps the page slide aligned with the visual order of the navigation rail. */
export function useRouteMotion(
  route: AppRoute,
  navigate: (next: AppRoute) => void,
) {
  const [direction, setDirection] = useState<RouteMotionDirection>("forward");

  const navigateWithMotion = useCallback(
    (next: AppRoute) => {
      if (next !== route) {
        setDirection(
          APP_ROUTES.indexOf(next) < APP_ROUTES.indexOf(route)
            ? "backward"
            : "forward",
        );
      }
      navigate(next);
    },
    [navigate, route],
  );

  return { direction, navigate: navigateWithMotion } as const;
}
