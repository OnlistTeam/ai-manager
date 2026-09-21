import { useState } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { AppShell } from "@/app/AppShell";
import { isAppRoute } from "@/app/routes";
import { seedAppShellGallery } from "./AppShellGallery.fixtures";

const galleryClient = new QueryClient({
  defaultOptions: {
    queries: {
      enabled: false,
      retry: false,
      staleTime: Number.POSITIVE_INFINITY,
    },
  },
});

seedAppShellGallery(galleryClient);

/**
 * Exact production shell with deterministic local-only Query snapshots.
 * It never calls native commands and remains outside production bundles.
 */
export function AppShellGallery() {
  useState(() => {
    const search = new URLSearchParams(window.location.search);
    const requestedRoute = search.get("route");
    window.localStorage.setItem(
      "aimanager.route",
      requestedRoute && isAppRoute(requestedRoute) ? requestedRoute : "home",
    );
    // The browser viewport is square, while on a real device macOS clips the
    // WebView to the window's rounded corners. Design comparisons usually pit
    // a screenshot from here against a real-device screenshot, so without
    // faking the shape, "the browser is square" gets misread as
    // "this app has no rounded corners."
    document.documentElement.dataset.windowPreview = "browser";
    return null;
  });

  return (
    <QueryClientProvider client={galleryClient}>
      <AppShell />
    </QueryClientProvider>
  );
}
