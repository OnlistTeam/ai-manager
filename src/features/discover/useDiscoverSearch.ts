import { useCallback, useEffect, useState } from "react";

const DEBOUNCE_MS = 300;

export interface DiscoverSearch {
  /** What is in the box. */
  input: string;
  /** What was last searched for. */
  query: string;
  setInput: (value: string) => void;
  /** Enter: search now instead of waiting for the pause. */
  searchNow: () => void;
  /** Escape: back to the first screen. */
  clear: () => void;
}

/**
 * Typing searches after a short pause. A query shorter than `minLength`
 * (one letter, for Skills) keeps the previous results rather than asking a
 * source for everything that contains that letter.
 */
export function useDiscoverSearch(minLength = 1): DiscoverSearch {
  const [input, setInput] = useState("");
  const [query, setQuery] = useState("");
  const accepts = useCallback(
    (value: string) => value.length === 0 || value.length >= minLength,
    [minLength],
  );

  useEffect(() => {
    const next = input.trim();
    if (!accepts(next)) return undefined;
    const timer = window.setTimeout(() => setQuery(next), DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [input, accepts]);

  const searchNow = useCallback(() => {
    const next = input.trim();
    if (accepts(next)) setQuery(next);
  }, [input, accepts]);

  const clear = useCallback(() => {
    setInput("");
    setQuery("");
  }, []);

  return { input, query, setInput, searchNow, clear };
}
