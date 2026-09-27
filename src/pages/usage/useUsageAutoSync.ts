import { useEffect } from "react";

/**
 * Brings the local session files in when the page opens and again whenever
 * the window comes back while it is open — the moment someone returns from a
 * coding session to look. `syncIfDue` owns the no-overlap and minimum-gap
 * rules, so this only decides when to ask.
 */
export function useUsageAutoSync(syncIfDue: () => void): void {
  useEffect(() => {
    syncIfDue();
    const onVisible = () => {
      if (document.visibilityState === "visible") syncIfDue();
    };
    window.addEventListener("focus", syncIfDue);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.removeEventListener("focus", syncIfDue);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [syncIfDue]);
}
