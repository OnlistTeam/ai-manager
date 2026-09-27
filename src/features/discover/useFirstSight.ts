import { useEffect, useState, type RefObject } from "react";

/**
 * True once the element has come near the viewport, and from then on. The
 * Discover section sits below the list, so its sources are not asked until
 * someone scrolls towards it. Without an IntersectionObserver the answer is
 * simply "now".
 */
export function useFirstSight(ref: RefObject<HTMLElement>): boolean {
  const [seen, setSeen] = useState(
    () => typeof IntersectionObserver === "undefined",
  );

  useEffect(() => {
    if (seen) return undefined;
    const element = ref.current;
    if (!element) return undefined;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setSeen(true);
          observer.disconnect();
        }
      },
      { rootMargin: "240px" },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref, seen]);

  return seen;
}
