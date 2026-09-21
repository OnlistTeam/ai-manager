import * as React from "react";
import { ArrowRight, Check, PlugZap, RefreshCw, Waypoints } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "./cn";
import { Badge } from "./Badge";
import { Button } from "./Button";
import { Card } from "./Card";

export type ServiceCardUseActionState = "idle" | "pending" | "retry";

export interface ServiceCardProps {
  name: string;
  /** Slot for the service logo; the card only reserves the space. */
  icon?: React.ReactNode;
  /** Display names of the tools currently pointed at this service (§35). */
  usedBy: string[];
  connected?: boolean;
  active?: boolean;
  /** Whether the Use action should show even though the card is not the active badge holder. Defaults to `!active`; a card can be in effect without holding the DB selection, so callers may need to offer Use regardless of `active`. */
  useAvailable?: boolean;
  activeLabelKey?: string;
  usedByLabelKey?: string;
  notUsedLabelKey?: string;
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
  /** Removes repeated relationship copy for dense endpoint inventories. */
  compact?: boolean;
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

export function ServiceCard({
  name,
  icon,
  usedBy,
  connected = false,
  active = false,
  useAvailable,
  activeLabelKey = "ds.service.nowActive",
  usedByLabelKey = "ds.service.currentlyUsedBy",
  notUsedLabelKey = "ds.service.notUsed",
  useLabelKey = "ds.action.use",
  unavailableLabelKey = "ds.action.inUse",
  busy = false,
  actionDisabled = false,
  useActionState = "idle",
  meta,
  detail,
  actions,
  compact = false,
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
  // The button's position never shifts with state: when unusable it grays out
  // and says "In Use" instead of disappearing entirely and dragging the row of
  // buttons after it to the left.
  const showUseAction = connected;
  const showFooter =
    !connected ||
    showUseAction ||
    Boolean(actions) ||
    useActionState !== "idle";

  return (
    <Card
      padding="none"
      role="article"
      aria-labelledby={headingId}
      aria-busy={busy || undefined}
      data-state={active ? "in-use" : undefined}
      className={cn(
        "group relative isolate flex min-w-0 flex-col overflow-hidden rounded-xl border-hairline",
        "transition-[border-color,box-shadow] duration-fast ease-standard focus-within:border-brand/25 focus-within:shadow-md",
        className,
      )}
    >
      <span
        aria-hidden="true"
        className="pointer-events-none absolute -right-10 -top-12 h-36 w-36 rounded-full bg-brand/5 blur-2xl"
      />

      {/* Which one is in use is already stated by the badge in the card header;
          this corner mark just makes it pop first when scanning down a whole
          column of cards, so it's decorative — excluded from the a11y tree
          and not rendered as a solid fill. */}
      {active ? (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute left-0 top-0 z-10 flex h-6 w-6 items-center justify-center rounded-br-xl rounded-tl-xl bg-success/25 text-success"
        >
          <Check className="h-3.5 w-3.5" strokeWidth={3} />
        </span>
      ) : null}

      <div
        className={cn(
          "relative flex items-start gap-4 p-5 pb-4",
          compact && "gap-3 p-4",
        )}
      >
        {icon ?? (
          <span className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl border border-hairline bg-layer-1 text-content-muted shadow-sm">
            <Waypoints className="h-5 w-5" aria-hidden="true" />
          </span>
        )}
        <div className="min-w-0 flex-1">
          {meta || active ? (
            <div className="flex flex-wrap items-center gap-2">
              {meta}
              {active ? (
                <Badge tone="success" className="shrink-0">
                  {t(activeLabelKey)}
                </Badge>
              ) : null}
            </div>
          ) : null}
          <h3
            id={headingId}
            className={cn(
              "break-words text-heading text-content",
              (meta || active) && (compact ? "mt-1.5" : "mt-2"),
            )}
          >
            {name}
          </h3>
          {compact && usedBy.length > 0 ? (
            <span className="sr-only">{usedBy.join(", ")}</span>
          ) : null}
          {!compact ? (
            usedBy.length > 0 ? (
              <div className="mt-3 rounded-lg border border-hairline bg-layer-1 px-3 py-2">
                <p className="text-caption text-content-muted">
                  {t(usedByLabelKey)}
                </p>
                <p className="mt-0.5 text-body font-medium text-content">
                  {usedBy.join(", ")}
                </p>
              </div>
            ) : (
              <p className="mt-3 text-body text-content-muted">
                {t(notUsedLabelKey)}
              </p>
            )
          ) : null}
          {detail ? (
            <div className={compact ? "mt-3" : "mt-4"}>{detail}</div>
          ) : null}
        </div>
      </div>
      {showFooter ? (
        <div
          className={cn(
            "relative mt-auto flex flex-wrap items-center gap-2 border-t border-hairline bg-layer-1 px-5 py-3.5",
            compact && "px-4 py-2.5",
          )}
        >
          {!connected ? (
            <Button
              aria-label={connectAriaLabel}
              disabled={busy || actionDisabled}
              onClick={onConnect}
            >
              <PlugZap className="h-4 w-4" aria-hidden="true" />
              {t("ds.action.connect")}
            </Button>
          ) : showUseAction ? (
            <Button
              variant="secondary"
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
          ) : null}
          {actions}
        </div>
      ) : null}
    </Card>
  );
}
