import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "./cn";

export type BadgeTone = "neutral" | "brand" | "success" | "warning" | "danger";

/**
 * The label always sits on `text-content`. Measured contrast for the status
 * hues on the light canvas is 3.29 / 2.81 / 4.23, which fails AA for body
 * text, so the hue is carried by the tint and the icon only (spec §98).
 */
const badgeVariants = cva(
  "inline-flex h-6 shrink-0 items-center gap-1 whitespace-nowrap rounded-full px-2 text-caption font-medium text-content",
  {
    variants: {
      tone: {
        neutral: "bg-layer-2",
        brand: "bg-brand/10",
        success: "bg-success/10",
        warning: "bg-warning/10",
        danger: "bg-danger/10",
      },
    },
    defaultVariants: {
      tone: "neutral",
    },
  },
);

const ICON_TONE: Record<BadgeTone, string> = {
  neutral: "text-content-muted",
  brand: "text-brand",
  success: "text-success",
  warning: "text-warning",
  danger: "text-danger",
};

export interface BadgeProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {
  icon?: React.ComponentType<{ className?: string }>;
}

export function Badge({
  className,
  tone,
  icon: Icon,
  children,
  ...props
}: BadgeProps) {
  return (
    <span className={cn(badgeVariants({ tone }), className)} {...props}>
      {Icon ? (
        <Icon
          className={cn("h-3.5 w-3.5 shrink-0", ICON_TONE[tone ?? "neutral"])}
          aria-hidden="true"
        />
      ) : null}
      {children}
    </span>
  );
}
