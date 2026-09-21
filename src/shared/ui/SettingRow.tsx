import * as React from "react";
import { cn } from "./cn";

export interface SettingRowProps {
  label: string;
  description?: string;
  /** When set, the label becomes a real `<label for>` for the control. */
  controlId?: string;
  children: React.ReactNode;
  className?: string;
}

export function SettingRow({
  label,
  description,
  controlId,
  children,
  className,
}: SettingRowProps) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-4 border-b border-hairline py-3 last:border-b-0",
        className,
      )}
    >
      <div className="min-w-0">
        {controlId ? (
          <label htmlFor={controlId} className="text-body text-content">
            {label}
          </label>
        ) : (
          <span className="text-body text-content">{label}</span>
        )}
        {description ? (
          <p className="mt-1 text-caption text-content-muted">{description}</p>
        ) : null}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
