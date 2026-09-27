import * as React from "react";
import { cn } from "./cn";
import { FIELD_FOCUS } from "./focusRing";

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
          "h-9 w-full rounded-sm border bg-layer-1 px-3 text-body text-content",
          "placeholder:text-content-muted",
          "transition-colors duration-fast ease-standard",
          "disabled:cursor-not-allowed disabled:opacity-60",
          invalid
            ? "border-danger"
            : cn("border-hairline hover:border-hairline-strong", FIELD_FOCUS),
          className,
        )}
        {...props}
      />
    );
  },
);
