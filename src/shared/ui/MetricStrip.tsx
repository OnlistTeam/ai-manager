import { useId, type ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { Card } from "./Card";
import { cn } from "./cn";

export interface MetricStripItem {
  id: string;
  label: ReactNode;
  value: ReactNode;
  icon: LucideIcon;
  iconClassName?: string;
}

export interface MetricStripProps {
  title: ReactNode;
  description?: ReactNode;
  eyebrow?: ReactNode;
  eyebrowIcon?: LucideIcon;
  leading?: ReactNode;
  highlight?: ReactNode;
  detail?: ReactNode;
  footer?: ReactNode;
  metrics: readonly MetricStripItem[];
  className?: string;
}

/**
 * A compact factual follow-up to the page's spatial stage. Metrics share one
 * surface and use dividers instead of nested cards, keeping the page hierarchy
 * calm while preserving a labelled region and native definition-list markup.
 */
export function MetricStrip({
  title,
  description,
  eyebrow,
  eyebrowIcon: EyebrowIcon,
  leading,
  highlight,
  detail,
  footer,
  metrics,
  className,
}: MetricStripProps) {
  const headingId = useId();
  const metricColumns =
    metrics.length === 4
      ? "grid-cols-2"
      : metrics.length === 3
        ? "grid-cols-3"
        : "grid-cols-2";

  return (
    <section
      aria-labelledby={headingId}
      data-metric-count={metrics.length}
      className={cn("metric-strip min-w-0", className)}
    >
      <Card padding="none" className="metric-strip__surface">
        <div className="metric-strip__row">
          <div className="metric-strip__intro">
            {leading ? (
              <div className="metric-strip__leading">{leading}</div>
            ) : null}
            <div className="metric-strip__copy">
              {eyebrow ? (
                <p className="metric-strip__eyebrow">
                  {EyebrowIcon ? (
                    <EyebrowIcon className="h-4 w-4" aria-hidden="true" />
                  ) : null}
                  {eyebrow}
                </p>
              ) : null}
              <h2 id={headingId} className="metric-strip__title">
                {title}
              </h2>
              {highlight ? (
                <p className="metric-strip__highlight">{highlight}</p>
              ) : null}
              {description ? (
                <p className="metric-strip__description">{description}</p>
              ) : null}
              {detail ? (
                <div className="metric-strip__detail">{detail}</div>
              ) : null}
            </div>
          </div>

          <dl
            className={cn(
              "metric-strip__metrics grid w-full min-w-0",
              metricColumns,
            )}
          >
            {metrics.map(({ id, label, value, icon: Icon, iconClassName }) => (
              <div key={id} className="metric-strip__metric min-w-0">
                <dt className="metric-strip__metric-label flex min-w-0 items-start gap-2">
                  <Icon
                    className={cn(
                      "h-4 w-4 shrink-0",
                      iconClassName ?? "text-brand",
                    )}
                    aria-hidden="true"
                  />
                  <span className="min-w-0 break-words">{label}</span>
                </dt>
                <dd className="metric-strip__metric-value">{value}</dd>
              </div>
            ))}
          </dl>
        </div>
        {footer ? <div className="metric-strip__footer">{footer}</div> : null}
      </Card>
    </section>
  );
}
