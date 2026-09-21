import * as React from "react";
import { cn } from "./cn";

export interface EmptyStateProps {
  icon?: React.ComponentType<{ className?: string }>;
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}

export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  className,
}: EmptyStateProps) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-3 px-6 py-12 text-center",
        className,
      )}
    >
      {Icon ? (
        <span className="flex h-12 w-12 items-center justify-center rounded-full bg-layer-1">
          <Icon className="h-6 w-6 text-content-muted" aria-hidden="true" />
        </span>
      ) : null}
      <h3 className="text-heading text-content">{title}</h3>
      {description ? (
        <p className="max-w-sm text-body text-content-muted">{description}</p>
      ) : null}
      {action ? <div className="mt-1">{action}</div> : null}
    </div>
  );
}
