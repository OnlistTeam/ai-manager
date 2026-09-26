import { useCallback, useState } from "react";

const STORAGE_KEY = "aimanager.liveRouting.hideEmails";

function readStored(): boolean {
  try {
    return window.localStorage.getItem(STORAGE_KEY) !== "false";
  } catch {
    // Storage can be unavailable (private mode, quota); masking is the safe default.
    return true;
  }
}

/**
 * Whether the live panel masks email addresses in service names. A pure
 * view preference: on by default, remembered on this device only.
 */
export function useHideEmails(): [boolean, (hide: boolean) => void] {
  const [hide, setHide] = useState(readStored);

  const update = useCallback((next: boolean) => {
    setHide(next);
    try {
      window.localStorage.setItem(STORAGE_KEY, String(next));
    } catch {
      // Not remembered this time; the toggle still applies for this view.
    }
  }, []);

  return [hide, update];
}
