import * as React from "react";
import { cn } from "./cn";

const PADDING: Record<NonNullable<CardProps["padding"]>, string> = {
  none: "",
  sm: "p-3",
  md: "p-4",
  lg: "p-6",
};

export interface CardProps extends React.HTMLAttributes<HTMLDivElement> {
  padding?: "none" | "sm" | "md" | "lg";
  /** Adds the hover treatment used by cards that behave like a target. */
  interactive?: boolean;
}

export const Card = React.forwardRef<HTMLDivElement, CardProps>(
  ({ className, padding = "md", interactive = false, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        // `.ds-card` owns the colours; a card separates itself by being lighter
        // than the canvas, so it carries no shadow.
        "ds-card rounded-lg border text-content",
        PADDING[padding],
        interactive &&
          "transition-colors duration-fast ease-standard hover:bg-layer-2",
        className,
      )}
      {...props}
    />
  ),
);
Card.displayName = "Card";
