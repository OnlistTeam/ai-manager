import { useCallback, useState } from "react";
import { DEFAULT_ROUTE, isAppRoute, type AppRoute } from "./routes";

const STORAGE_KEY = "aimanager.route";

/**
 * Desktop apps have no address bar, so routing never touches the URL (history
 * routing under `vite base: "./"` would only bring a pile of edge cases).
 * This only remembers "which page was last open"; fall back to home if it
 * can't be read.
 */
export function readStoredRoute(storage: Pick<Storage, "getItem">): AppRoute {
  try {
    const stored = storage.getItem(STORAGE_KEY);
    return isAppRoute(stored) ? stored : DEFAULT_ROUTE;
  } catch {
    return DEFAULT_ROUTE;
  }
}

export interface AppRouter {
  route: AppRoute;
  navigate: (next: AppRoute) => void;
}

export function useAppRoute(): AppRouter {
  const [route, setRoute] = useState<AppRoute>(() =>
    typeof window === "undefined"
      ? DEFAULT_ROUTE
      : readStoredRoute(window.localStorage),
  );

  const navigate = useCallback((next: AppRoute) => {
    setRoute(next);
    try {
      window.localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Failing to remember the last page doesn't affect usability; degrade silently.
    }
  }, []);

  return { route, navigate };
}
