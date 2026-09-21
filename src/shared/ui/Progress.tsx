import { cn } from "./cn";

const TONE: Record<NonNullable<ProgressProps["tone"]>, string> = {
  brand: "bg-brand",
  success: "bg-success",
  warning: "bg-warning",
  danger: "bg-danger",
};

export interface ProgressProps {
  value: number;
  /** Required: a progressbar without a name is unusable with a screen reader. */
  label: string;
  indeterminate?: boolean;
  tone?: "brand" | "success" | "warning" | "danger";
  className?: string;
}

export function Progress({
  value,
  label,
  indeterminate = false,
  tone = "brand",
  className,
}: ProgressProps) {
  const clamped = Number.isFinite(value)
    ? Math.min(100, Math.max(0, Math.round(value)))
    : 0;

  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={indeterminate ? undefined : clamped}
      className={cn(
        "h-1 w-full overflow-hidden rounded-full bg-layer-1",
        className,
      )}
    >
      <div
        data-slot="progress-fill"
        className={cn(
          "h-full rounded-full transition-[width] duration-base ease-standard",
          TONE[tone],
          indeterminate && "motion-safe:animate-pulse",
        )}
        style={{ width: indeterminate ? "40%" : `${clamped}%` }}
      />
    </div>
  );
}
