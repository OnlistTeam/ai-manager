import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

// The product type scale (tailwind.config.cjs fontSize) collides with
// tailwind-merge's default text-color group; register it as font-size so
// `text-body` and `text-content` can coexist after merging. The motion scale
// (transitionDuration/transitionTimingFunction, tailwind-merge v3 group ids
// `duration`/`ease`) is registered the same way so a caller's
// `duration-*`/`ease-*` override actually wins the merge instead of both
// classes surviving side by side.
const twMergeDs = extendTailwindMerge({
  extend: {
    classGroups: {
      "font-size": [
        { text: ["display", "title", "heading", "body", "caption", "mono-sm"] },
      ],
      duration: [{ duration: ["fast", "base", "modal"] }],
      ease: [{ ease: ["standard"] }],
    },
  },
});

export function cn(...inputs: ClassValue[]) {
  return twMergeDs(clsx(inputs));
}
