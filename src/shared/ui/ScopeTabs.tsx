import { useRef, type KeyboardEvent } from "react";
import { cn } from "./cn";
import { FOCUS_RING } from "./focusRing";

export interface ScopeTabItem {
  id: string;
  label: string;
  /** A scope can be visible while the selected page does not support it. */
  statusLabel?: string;
}

export interface ScopeTabsProps {
  items: readonly ScopeTabItem[];
  active: string | null;
  disabled?: boolean;
  /** Accessible name for the tablist, e.g. "AI tool". */
  label: string;
  /** Stable ID prefix for tabs when the caller renders a tabpanel. */
  idPrefix?: string;
  /** ID of the panel controlled by every tab in this dynamic tablist. */
  panelId?: string;
  onSelect: (id: string) => void;
  className?: string;
}

export function scopeTabId(prefix: string, itemId: string): string {
  return `${prefix}-${itemId.replace(/[^a-zA-Z0-9_-]/g, "-")}-tab`;
}

/**
 * A row of scope pills. Purely presentational: it never decides which items
 * belong in the row — the caller does that from capabilities or from a
 * static table, so no product rule lives in the design system.
 */
export function ScopeTabs({
  items,
  active,
  disabled = false,
  label,
  idPrefix,
  panelId,
  onSelect,
  className,
}: ScopeTabsProps) {
  const tabs = useRef<Array<HTMLButtonElement | null>>([]);
  // An active id that matches no item must not leave the whole row without a
  // tab stop; treat it exactly like "nothing selected".
  const hasActive = items.some((item) => item.id === active);

  const moveFocus = (
    event: KeyboardEvent<HTMLButtonElement>,
    current: number,
  ) => {
    if (items.length === 0) return;
    let next: number | null = null;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      next = (current + 1) % items.length;
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      next = (current - 1 + items.length) % items.length;
    } else if (event.key === "Home") {
      next = 0;
    } else if (event.key === "End") {
      next = items.length - 1;
    }
    if (next === null) return;
    event.preventDefault();
    tabs.current[next]?.focus();
    onSelect(items[next].id);
  };

  return (
    <div
      role="tablist"
      aria-label={label}
      aria-orientation="horizontal"
      aria-busy={disabled || undefined}
      className={cn("flex flex-wrap gap-2", className)}
    >
      {items.map((item, index) => {
        const selected = item.id === active;
        const tabbable = selected || (!hasActive && index === 0);
        return (
          <button
            key={item.id}
            type="button"
            role="tab"
            id={idPrefix ? scopeTabId(idPrefix, item.id) : undefined}
            aria-controls={panelId}
            aria-selected={selected}
            aria-label={
              item.statusLabel
                ? `${item.label}, ${item.statusLabel}`
                : undefined
            }
            disabled={disabled}
            tabIndex={tabbable ? 0 : -1}
            ref={(node) => {
              tabs.current[index] = node;
            }}
            onClick={() => onSelect(item.id)}
            onKeyDown={(event) => moveFocus(event, index)}
            className={cn(
              "rounded-full border px-3 py-1.5 text-caption",
              "transition-colors duration-fast ease-standard",
              "disabled:cursor-wait disabled:opacity-60",
              selected
                ? "border-brand bg-brand text-brand-foreground"
                : "border-hairline bg-layer-1 text-content hover:bg-layer-2",
              FOCUS_RING,
            )}
          >
            <span>{item.label}</span>
            {item.statusLabel ? (
              <span className="ml-1 text-[0.7em] opacity-70">
                {item.statusLabel}
              </span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
