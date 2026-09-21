import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { Loader2 } from "lucide-react";
import { cn } from "./cn";
import { FOCUS_RING } from "./focusRing";

const buttonVariants = cva(
  cn(
    "ds-button inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-lg font-medium",
    "transition-[transform,box-shadow,border-color,background-color,color] duration-fast ease-standard",
    "disabled:pointer-events-none disabled:opacity-50",
    FOCUS_RING,
  ),
  {
    variants: {
      variant: {
        primary:
          "ds-button-primary bg-brand text-brand-foreground hover:bg-brand-hover",
        secondary:
          "ds-button-secondary border border-hairline bg-layer-1 text-content hover:bg-layer-2",
        ghost:
          "ds-button-ghost text-content-muted hover:bg-layer-2 hover:text-content",
        danger:
          "ds-button-danger bg-danger text-brand-foreground hover:bg-danger/90",
      },
      size: {
        xs: "h-7 px-2.5 text-caption gap-1.5",
        sm: "h-9 px-3.5 text-caption",
        md: "h-10 px-4 text-body",
        lg: "h-12 px-6 text-body",
      },
    },
    defaultVariants: {
      variant: "primary",
      size: "md",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  /** Shows a spinner, blocks clicks and announces `aria-busy`. */
  loading?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      className,
      variant,
      size,
      loading = false,
      disabled = false,
      type = "button",
      children,
      ...props
    },
    ref,
  ) => (
    <button
      ref={ref}
      type={type}
      className={cn(buttonVariants({ variant, size }), className)}
      data-variant={variant ?? "primary"}
      data-size={size ?? "md"}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      {...props}
    >
      {loading ? (
        <Loader2
          data-loading-indicator=""
          className="h-4 w-4 motion-safe:animate-spin"
          aria-hidden="true"
        />
      ) : null}
      <span
        data-slot="button-content"
        className={cn("contents", loading && "[&>svg]:hidden")}
      >
        {children}
      </span>
    </button>
  ),
);
Button.displayName = "Button";

export { buttonVariants };
