import * as React from "react";
import { cn } from "./cn";
import { FOCUS_RING } from "./focusRing";

export interface InputProps
  extends React.InputHTMLAttributes<HTMLInputElement> {
  invalid?: boolean;
}

/**
 * The product text box. Nothing here knows about forms or validation — the
 * caller owns the value and decides what `invalid` means.
 */
export const Input = React.forwardRef<HTMLInputElement, InputProps>(
  function Input({ className, invalid = false, type = "text", ...props }, ref) {
    return (
      <input
        ref={ref}
        type={type}
        aria-invalid={invalid || undefined}
        className={cn(
          "h-10 w-full rounded-sm border bg-layer-1 px-3 text-body text-content",
          "placeholder:text-content-muted",
          "transition-colors duration-fast ease-standard",
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
