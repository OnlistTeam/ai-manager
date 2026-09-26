import { useSyncExternalStore } from "react";

const QUERY = "(prefers-reduced-motion: reduce)";

function subscribe(onChange: () => void): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => {};
  const preference = window.matchMedia(QUERY);
  preference.addEventListener?.("change", onChange);
  return () => preference.removeEventListener?.("change", onChange);
}

function snapshot(): boolean {
  return typeof window !== "undefined" && window.matchMedia
    ? window.matchMedia(QUERY).matches
    : false;
}

/** The system asks for less motion: the stage changes state without travel. */
export function usePrefersReducedMotion(): boolean {
  return useSyncExternalStore(subscribe, snapshot, () => false);
}
