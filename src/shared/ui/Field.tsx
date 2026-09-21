import * as React from "react";
import { AlertCircle } from "lucide-react";
import { cn } from "./cn";

export interface FieldProps {
  /** Must match the `id` of the control passed as `children`. */
  id: string;
  label: string;
  hint?: string;
  error?: string;
  children: React.ReactNode;
  className?: string;
}

/**
 * Label above, control, hint below. The hint is wired with
 * `aria-describedby` rather than folded into the label, so a screen reader
 * reads the field name first and the explanation second.
 */
export function Field({
  id,
  label,
  hint,
  error,
  children,
  className,
}: FieldProps) {
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const describedBy = [hint ? hintId : null, error ? errorId : null]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label htmlFor={id} className="text-caption text-content">
        {label}
      </label>
      {hint ? (
        <span id={hintId} className="sr-only">
          {hint}
        </span>
      ) : null}
      {React.isValidElement(children)
        ? React.cloneElement(children as React.ReactElement, {
            "aria-describedby": describedBy || undefined,
          })
        : children}
      {hint ? (
        <p aria-hidden="true" className="text-caption text-content-muted">
          {hint}
        </p>
      ) : null}
      {error ? (
        <p
          id={errorId}
          role="alert"
          className="flex items-center gap-1.5 text-caption text-danger"
        >
          <AlertCircle className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          {error}
        </p>
      ) : null}
    </div>
  );
}
