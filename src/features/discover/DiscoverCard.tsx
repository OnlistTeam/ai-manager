import type { ReactNode } from "react";
import { Card } from "@/shared/ui/Card";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";

export interface DiscoverCardProps {
  title: string;
  /** Accessible name of the whole card, which opens its details. */
  detailsLabel: string;
  logo: ReactNode;
  /** Beside the title, such as the official mark. */
  titleExtra?: ReactNode;
  meta: ReactNode;
  description: string | null;
  /** The description is still being fetched. */
  describing?: boolean;
  footer: ReactNode;
  action: ReactNode;
  onOpen: () => void;
}

/**
 * A compact Discover card. The whole card is one button that opens the
 * details; the Add control in its corner sits above that button, so the two
 * never nest.
 */
export function DiscoverCard({
  title,
  detailsLabel,
  logo,
  titleExtra,
  meta,
  description,
  describing = false,
  footer,
  action,
  onOpen,
}: DiscoverCardProps) {
  return (
    <Card
      interactive
      padding="none"
      role="listitem"
      className="relative flex min-w-0 flex-col gap-2 p-3"
    >
      <button
        type="button"
        aria-label={detailsLabel}
        onClick={onOpen}
        className={cn("absolute inset-0 rounded-lg", FOCUS_RING)}
      />
      <div className="pointer-events-none flex min-w-0 items-center gap-2.5">
        {logo}
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <div className="flex min-w-0 items-center gap-1.5">
            <h3 className="truncate text-body font-medium text-content">
              {title}
            </h3>
            {titleExtra}
          </div>
          <div className="flex min-w-0 items-center gap-1.5 text-caption text-content-muted">
            {meta}
          </div>
        </div>
      </div>
      {describing && !description ? (
        <span
          aria-hidden="true"
          className="pointer-events-none mb-1 mt-1.5 h-2 w-4/5 rounded-full bg-layer-2 motion-safe:animate-pulse"
        />
      ) : (
        <p
          title={description ?? undefined}
          className="pointer-events-none line-clamp-2 min-h-8 text-caption text-content-muted"
        >
          {description}
        </p>
      )}
      <div className="mt-auto flex min-h-7 items-center gap-2">
        <span className="pointer-events-none flex min-w-0 items-center gap-1 truncate text-caption text-content-muted">
          {footer}
        </span>
        <span className="relative z-10 ml-auto shrink-0">{action}</span>
      </div>
    </Card>
  );
}

/** A placeholder card while the first answer loads. */
export function DiscoverCardSkeleton() {
  return (
    <Card
      padding="none"
      aria-hidden="true"
      className="flex min-h-[7.5rem] flex-col gap-3 p-3"
    >
      <div className="flex items-center gap-2.5">
        <span className="h-8 w-8 rounded-md bg-layer-2 motion-safe:animate-pulse" />
        <div className="flex flex-1 flex-col gap-1.5">
          <span className="h-2.5 w-1/2 rounded-full bg-layer-2 motion-safe:animate-pulse" />
          <span className="h-2 w-1/3 rounded-full bg-layer-2 motion-safe:animate-pulse" />
        </div>
      </div>
      <span className="h-2 w-5/6 rounded-full bg-layer-2 motion-safe:animate-pulse" />
    </Card>
  );
}
