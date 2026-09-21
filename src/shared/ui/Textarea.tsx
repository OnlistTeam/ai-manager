import * as React from "react";
import { cn } from "./cn";
import { FOCUS_RING } from "./focusRing";

export interface TextareaProps
  extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  invalid?: boolean;
}

export const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
  function Textarea({ className, invalid = false, ...props }, ref) {
    return (
      <textarea
        ref={ref}
        aria-invalid={invalid || undefined}
        className={cn(
          "w-full resize-y rounded-sm border bg-layer-1 px-3 py-2 text-body text-content",
          "placeholder:text-content-muted transition-colors duration-fast ease-standard",
          "disabled:cursor-not-allowed disabled:opacity-60",
          invalid
            ? "border-danger"
            : "border-hairline hover:border-hairline-strong",
          FOCUS_RING,
          className,
        )}
        {...props}
      />
    );
  },
);
