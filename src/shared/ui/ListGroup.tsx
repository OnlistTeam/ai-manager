import * as React from "react";
import { cn } from "./cn";
import { Card } from "./Card";

export interface ListGroupProps extends React.HTMLAttributes<HTMLDivElement> {
  children?: React.ReactNode;
}

/**
 * One card holding many rows, separated by hairlines.
 *
 * A page listing six tools used to render six free-standing cards, which gave
 * the eye six borders and six shadows to parse. Collapsing them into a single
 * container leaves one outline for the whole list and lets the rows themselves
 * be nothing but content. Row separators come from `.ds-list-group` in
 * index.css so callers never have to know which row is last.
 */
export const ListGroup = React.forwardRef<HTMLDivElement, ListGroupProps>(
  ({ className, ...props }, ref) => (
    <Card
      ref={ref}
      padding="none"
      className={cn("ds-list-group overflow-hidden rounded-lg", className)}
      {...props}
    />
  ),
);
ListGroup.displayName = "ListGroup";

export interface ListGroupRowProps
  extends React.HTMLAttributes<HTMLDivElement> {
  /** Adds the hover treatment used by rows that behave like a target. */
  interactive?: boolean;
}

export const ListGroupRow = React.forwardRef<HTMLDivElement, ListGroupRowProps>(
  ({ className, interactive = false, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        "ds-list-row relative min-w-0 p-4",
        interactive &&
          "transition-colors duration-fast ease-standard hover:bg-layer-1",
        className,
      )}
      {...props}
    />
  ),
);
ListGroupRow.displayName = "ListGroupRow";
