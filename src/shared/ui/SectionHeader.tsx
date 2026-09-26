import * as React from "react";
import { cn } from "./cn";

export interface SectionHeaderProps {
  title: string;
  /** Lets a wrapping landmark point `aria-labelledby` at the visible title. */
  headingId?: string;
  description?: string;
  action?: React.ReactNode;
  /**
   * Lets the page keep a valid heading outline. Only the page title (`h1`)
   * uses the title size; section headings step down to the heading size so
   * a page reads as one title over its sections.
   */
  as?: "h1" | "h2" | "h3";
  className?: string;
}

export function SectionHeader({
  title,
  headingId,
  description,
  action,
  as: Heading = "h2",
  className,
}: SectionHeaderProps) {
  return (
    <div className={cn("flex items-start justify-between gap-3", className)}>
      <div className="min-w-0">
        <Heading
          id={headingId}
          className={cn(
            Heading === "h1" ? "text-title" : "text-heading",
            "text-content",
          )}
        >
          {title}
        </Heading>
        {description ? (
          <p className="mt-0.5 text-caption text-content-muted">
            {description}
          </p>
        ) : null}
      </div>
      {action ? <div className="shrink-0">{action}</div> : null}
    </div>
  );
}
