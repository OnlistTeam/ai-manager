import * as React from "react";
import { Waypoints } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "./cn";
import { Badge } from "./Badge";
import { ListGroupRow } from "./ListGroup";

export interface ServiceCardProps {
  name: string;
  /** Slot for the service logo; the card only reserves the space. */
  icon?: React.ReactNode;
  /** Display names of the tools currently pointed at this service. */
  usedBy: string[];
  /** The service a tool uses right now; marked, never offered as an action. */
  active?: boolean;
  activeLabelKey?: string;
  busy?: boolean;
  /** Small identity or category badge above the service name. */
  meta?: React.ReactNode;
  /** Extra line under the name, e.g. the masked key. */
  detail?: React.ReactNode;
  /** Controls at the row's trailing edge, e.g. Check and Edit. */
  actions?: React.ReactNode;
  /** Optional control after the actions, e.g. a reorder handle. */
  dragHandle?: React.ReactNode;
  className?: string;
}

/**
 * One saved service as a row of a `ListGroup` (ADR-0052): identity, badges and
 * actions on the first line, the endpoint detail underneath. The row only
 * says which service is in use; choosing one happens on Home (ADR-0053). The
 * tools that point at it are announced to screen readers only; the "in use"
 * badge and the accent bar already say it on screen.
 */
export function ServiceCard({
  name,
  icon,
  usedBy,
  active = false,
  activeLabelKey = "ds.service.nowActive",
  busy = false,
  meta,
  detail,
  actions,
  dragHandle,
  className,
}: ServiceCardProps) {
  const { t } = useTranslation();
  const headingId = React.useId();

  return (
    <ListGroupRow
      role="article"
      aria-labelledby={headingId}
      aria-busy={busy || undefined}
      data-state={active ? "in-use" : undefined}
      className={cn(
        "group isolate grid grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-x-3 gap-y-1.5",
        "transition-colors duration-fast ease-standard focus-within:bg-layer-1",
        className,
      )}
    >
      {/* Which one is in use is already stated by the badge; the bar only
          makes it the first thing the eye finds scanning down the list, so it
          is decorative and stays out of the a11y tree. */}
      {active ? (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute inset-y-2 left-0 w-0.5 rounded-full bg-success"
        />
      ) : null}

      <div className="row-span-2">
        {icon ?? (
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-hairline bg-layer-1 text-content-muted">
            <Waypoints className="h-4 w-4" aria-hidden="true" />
          </span>
        )}
      </div>
      <div className="flex min-h-9 min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
        <h3
          id={headingId}
          className="break-words text-body font-medium text-content"
        >
          {name}
        </h3>
        {meta}
        {active ? (
          <Badge tone="success" className="shrink-0">
            {t(activeLabelKey)}
          </Badge>
        ) : null}
        {usedBy.length > 0 ? (
          <span className="sr-only">{usedBy.join(", ")}</span>
        ) : null}
      </div>
      <div className="flex shrink-0 flex-wrap items-center justify-end gap-1">
        {actions}
        {dragHandle}
      </div>
      {detail ? <div className="col-span-2 min-w-0">{detail}</div> : null}
    </ListGroupRow>
  );
}
