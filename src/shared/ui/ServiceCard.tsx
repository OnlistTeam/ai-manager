import * as React from "react";
import { ArrowRight, Check, PlugZap, RefreshCw, Waypoints } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "./cn";
import { Badge } from "./Badge";
import { Button } from "./Button";
import { ListGroupRow } from "./ListGroup";

export type ServiceCardUseActionState = "idle" | "pending" | "retry";

export interface ServiceCardProps {
  name: string;
  /** Slot for the service logo; the card only reserves the space. */
  icon?: React.ReactNode;
  /** Display names of the tools currently pointed at this service (§35). */
  usedBy: string[];
  connected?: boolean;
  active?: boolean;
  /** Whether the Use action should show even though the card is the active one. Defaults to `!active`; a card can be in effect without holding the DB selection, so callers may need to offer Use regardless of `active`. */
  useAvailable?: boolean;
  activeLabelKey?: string;
  useLabelKey?: "ds.action.use" | "ds.action.configure";
  unavailableLabelKey?: "ds.action.inUse" | "ds.action.readOnly";
  busy?: boolean;
  /** Blocks the primary action without marking this exact card as busy. */
  actionDisabled?: boolean;
  /** Keeps the same primary control mounted through a recoverable switch. */
  useActionState?: ServiceCardUseActionState;
  /** Small identity or category badge above the service name. */
  meta?: React.ReactNode;
  /** Extra line under the name, e.g. the masked key. */
  detail?: React.ReactNode;
  /** Extra controls after the primary button, e.g. Check and Edit. */
  actions?: React.ReactNode;
  /** Optional control at the row's trailing edge, e.g. a reorder handle. */
  dragHandle?: React.ReactNode;
  onUse?: () => void;
  onConnect?: () => void;
  useAriaLabel?: string;
  connectAriaLabel?: string;
  className?: string;
}

/** All possible copy for the primary button. Width is reserved for the widest one, see `UseActionLabel`. */
const USE_ACTION_LABEL_KEYS = [
  "ds.action.use",
  "ds.action.inUse",
  "ds.action.retry",
  "ds.action.configure",
  "ds.action.readOnly",
] as const;

type UseActionLabelKey = (typeof USE_ACTION_LABEL_KEYS)[number];

/**
 * "Use / In Use / Retry" have different widths; if the button resized to match,
 * it would drag the row of buttons after it left and right, and two cards in
 * different states would never line up. Stacking all three labels in the same
 * grid cell and showing only the active one pins the button width to the
 * widest label — no fixed width that breaks the moment the language changes.
 */
function UseActionLabel({
  activeKey,
  useKey,
  unavailableKey,
}: {
  activeKey: UseActionLabelKey;
  useKey: UseActionLabelKey;
  unavailableKey: UseActionLabelKey;
}) {
  const { t } = useTranslation();
  return (
    <span className="grid">
      {USE_ACTION_LABEL_KEYS.filter(
        (key) =>
          key === "ds.action.retry" || key === useKey || key === unavailableKey,
      ).map((key) => (
        <span
          key={key}
          // aria-hidden cannot rely on `invisible` alone: accessible-name
          // computation reads it, and the test environment has no Tailwind stylesheet.
          aria-hidden={key === activeKey ? undefined : "true"}
          className={cn(
            "col-start-1 row-start-1 whitespace-nowrap",
            key === activeKey ? undefined : "invisible",
          )}
        >
          {t(key)}
        </span>
      ))}
    </span>
  );
}

/**
 * One saved service as a row of a `ListGroup` (ADR-0052): identity, badges and
 * actions on the first line, the endpoint detail underneath. The tools that
 * point at it are announced to screen readers only; the greyed "In use" button
 * and the accent bar already say it on screen.
 */
export function ServiceCard({
  name,
  icon,
  usedBy,
  connected = false,
  active = false,
  useAvailable,
  activeLabelKey = "ds.service.nowActive",
  useLabelKey = "ds.action.use",
  unavailableLabelKey = "ds.action.inUse",
  busy = false,
  actionDisabled = false,
  useActionState = "idle",
  meta,
  detail,
  actions,
  dragHandle,
  onUse,
  onConnect,
  useAriaLabel,
  connectAriaLabel,
  className,
}: ServiceCardProps) {
  const { t } = useTranslation();
  const headingId = React.useId();
  const useIsAvailable = useAvailable ?? !active;
  const useActionLabelKey =
    useActionState === "retry"
      ? "ds.action.retry"
      : useIsAvailable
        ? useLabelKey
        : unavailableLabelKey;
  const buttonSaysInUse = connected && useActionLabelKey === "ds.action.inUse";

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
      {/* Which one is in use is already stated by the button; the bar only
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
        {/* Said once: while the greyed button reads "In use" the badge would
            repeat it, so it only shows when the button says something else. */}
        {active && !buttonSaysInUse ? (
          <Badge tone="success" className="shrink-0">
            {t(activeLabelKey)}
          </Badge>
        ) : null}
        {usedBy.length > 0 ? (
          <span className="sr-only">{usedBy.join(", ")}</span>
        ) : null}
      </div>
      <div className="flex shrink-0 flex-wrap items-center justify-end gap-1">
        {/* The primary button never disappears: when unusable it greys out
            and says "In use", so the actions after it keep their place and
            every row in the list lines up. */}
        {!connected ? (
          <Button
            size="sm"
            aria-label={connectAriaLabel}
            disabled={busy || actionDisabled}
            onClick={onConnect}
          >
            <PlugZap className="h-4 w-4" aria-hidden="true" />
            {t("ds.action.connect")}
          </Button>
        ) : (
          <Button
            variant="secondary"
            size="sm"
            aria-label={
              useIsAvailable || useActionState !== "idle"
                ? useAriaLabel
                : undefined
            }
            disabled={
              busy ||
              actionDisabled ||
              (!useIsAvailable && useActionState === "idle")
            }
            loading={useActionState === "pending"}
            onClick={onUse}
          >
            {useActionState === "retry" ? (
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
            ) : useActionState === "pending" ? null : useIsAvailable ? (
              <ArrowRight className="h-4 w-4" aria-hidden="true" />
            ) : (
              <Check className="h-4 w-4" aria-hidden="true" />
            )}
            <UseActionLabel
              activeKey={useActionLabelKey}
              useKey={useLabelKey}
              unavailableKey={unavailableLabelKey}
            />
          </Button>
        )}
        {actions}
        {dragHandle}
      </div>
      {detail ? <div className="col-span-2 min-w-0">{detail}</div> : null}
    </ListGroupRow>
  );
}
