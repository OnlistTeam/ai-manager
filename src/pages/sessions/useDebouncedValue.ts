import { useEffect, useState } from "react";

/**
 * With type-to-search, firing a native request on every keystroke is
 * wasteful and makes the list jitter. Only hand off the value once the user
 * has paused for `delayMs`.
 */
export function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [settled, setSettled] = useState(value);

  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);

  return settled;
}
