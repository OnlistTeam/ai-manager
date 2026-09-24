import * as React from "react";
import { AlertCircle, AlertTriangle } from "lucide-react";
import { cn } from "./cn";

export interface FieldProps {
  /** Must match the `id` of the control passed as `children`. */
  id: string;
  label: string;
  hint?: string;
  /** Non-blocking caution about the current value; `error` still wins. */
  warning?: string;
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
  warning,
  error,
  children,
  className,
}: FieldProps) {
  const hintId = `${id}-hint`;
  const warningId = `${id}-warning`;
  const errorId = `${id}-error`;
  const shownWarning = error ? undefined : warning;
  const describedBy = [
    hint ? hintId : null,
    shownWarning ? warningId : null,
    error ? errorId : null,
  ]
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
      {shownWarning ? (
        <p
          id={warningId}
          role="status"
          className="flex items-start gap-1.5 text-caption text-warning"
        >
          <AlertTriangle
            className="mt-0.5 h-3.5 w-3.5 shrink-0"
            aria-hidden="true"
          />
          {shownWarning}
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
