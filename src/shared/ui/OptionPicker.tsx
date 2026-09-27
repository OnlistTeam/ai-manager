import * as Popover from "@radix-ui/react-popover";
import { Command } from "cmdk";
import { Check, Search, type LucideIcon } from "lucide-react";
import * as React from "react";
import { cn } from "./cn";

export interface OptionPickerOption {
  id: string;
  label: string;
  /** A muted second fact on the same line, e.g. the model an endpoint pins. */
  detail?: string | null;
  checked?: boolean;
  /**
   * Listed but not choosable, e.g. the connection in force when it was set
   * outside this app. A checked one keeps full strength: it is the current
   * state, not an unavailable choice.
   */
  disabled?: boolean;
}

export interface OptionPickerAction {
  id: string;
  label: string;
  icon?: LucideIcon;
  onSelect: () => void;
}

export interface OptionPickerProps {
  /** Accessible name of the list. */
  label: string;
  /** The control that opens the list; it receives the trigger props. */
  children: React.ReactElement;
  options: readonly OptionPickerOption[];
  /** Shown in place of the options when there are none. */
  empty?: React.ReactNode;
  /** One muted line above the options. */
  note?: React.ReactNode;
  /** Always listed after the options, under a separator. */
  actions?: readonly OptionPickerAction[];
  filterPlaceholder: string;
  noMatch: string;
  /** A filter field appears once the list is longer than this. */
  filterAbove?: number;
  align?: "start" | "center" | "end";
  onSelect: (id: string) => void;
}

const ITEM_CLASS = cn(
  "mx-1 flex min-w-0 cursor-default select-none items-center gap-2 rounded-md px-2 py-1.5 text-caption text-content outline-none",
  "data-[selected=true]:bg-layer-2",
);

/**
 * A short list that opens from a compact button: pick one option, or one of
 * the actions under it. The option in effect is checked and announced as
 * current. Long lists gain a filter field; short ones stay a plain list that
 * the arrow keys walk through.
 */
export function OptionPicker({
  label,
  children,
  options,
  empty,
  note,
  actions = [],
  filterPlaceholder,
  noMatch,
  filterAbove = 7,
  align = "end",
  onSelect,
}: OptionPickerProps) {
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const listRef = React.useRef<HTMLDivElement>(null);
  const filterable = options.length > filterAbove;
  const needle = query.trim().toLocaleLowerCase();
  // Filtered here rather than by the list itself, which would also reorder
  // the options by match score and move the actions above them.
  const shown =
    filterable && needle
      ? options.filter((option) =>
          `${option.label} ${option.detail ?? ""}`
            .toLocaleLowerCase()
            .includes(needle),
        )
      : options;
  // Kept outside the listbox, which may only hold options.
  const message =
    note ??
    (options.length === 0 ? empty : shown.length === 0 ? noMatch : undefined);
  const openChange = (next: boolean) => {
    setOpen(next);
    if (!next) setQuery("");
  };
  const choose = (select: () => void) => {
    openChange(false);
    select();
  };

  return (
    <Popover.Root open={open} onOpenChange={openChange}>
      <Popover.Trigger asChild>{children}</Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align={align}
          sideOffset={6}
          collisionPadding={16}
          // Without a filter field nothing inside is tabbable, so the list
          // itself takes focus and the arrow keys work straight away.
          onOpenAutoFocus={(event) => {
            if (filterable) return;
            event.preventDefault();
            listRef.current?.focus();
          }}
          className={cn(
            "app-floating-menu z-[70] w-72 min-w-[var(--radix-popover-trigger-width)] max-w-[calc(100vw-2rem)] overflow-hidden rounded-lg border outline-none animate-ds-overlay-in",
            "flex max-h-[min(24rem,var(--radix-popover-content-available-height))] flex-col",
          )}
        >
          <Command
            ref={listRef}
            label={label}
            loop
            tabIndex={-1}
            shouldFilter={false}
            className="flex min-h-0 flex-col outline-none"
          >
            {filterable ? (
              <div className="flex shrink-0 items-center gap-2 border-b border-hairline px-3">
                <Search
                  className="h-4 w-4 shrink-0 text-content-muted"
                  aria-hidden="true"
                />
                <Command.Input
                  value={query}
                  onValueChange={setQuery}
                  placeholder={filterPlaceholder}
                  className="h-9 min-w-0 flex-1 bg-transparent text-caption text-content outline-none placeholder:text-content-muted"
                />
              </div>
            ) : null}
            {message ? (
              <p className="px-4 pb-1 pt-2 text-caption text-content-muted">
                {message}
              </p>
            ) : null}
            <Command.List
              label={label}
              className="min-h-0 flex-1 overflow-y-auto overscroll-contain py-1"
            >
              {shown.map((option) => (
                <Command.Item
                  key={option.id}
                  value={option.id}
                  aria-current={option.checked ? "true" : undefined}
                  disabled={option.disabled}
                  onSelect={() => choose(() => onSelect(option.id))}
                  className={cn(
                    ITEM_CLASS,
                    option.disabled && !option.checked && "opacity-50",
                  )}
                >
                  <Check
                    className={cn(
                      "h-3.5 w-3.5 shrink-0 text-brand",
                      !option.checked && "invisible",
                    )}
                    aria-hidden="true"
                  />
                  <span className="min-w-0 flex-1 truncate">
                    {option.label}
                  </span>
                  {option.detail ? (
                    <span className="max-w-[45%] shrink-0 truncate text-content-muted">
                      {option.detail}
                    </span>
                  ) : null}
                </Command.Item>
              ))}
              {shown.length > 0 && actions.length > 0 ? (
                <Command.Separator className="my-1 h-px bg-hairline" />
              ) : null}
              {actions.map((action) => {
                const Icon = action.icon;
                return (
                  <Command.Item
                    key={action.id}
                    value={`action:${action.id}`}
                    onSelect={() => choose(action.onSelect)}
                    className={ITEM_CLASS}
                  >
                    {Icon ? (
                      <Icon
                        className="h-3.5 w-3.5 shrink-0 text-content-muted"
                        aria-hidden="true"
                      />
                    ) : (
                      <span className="w-3.5 shrink-0" aria-hidden="true" />
                    )}
                    <span className="min-w-0 flex-1 truncate">
                      {action.label}
                    </span>
                  </Command.Item>
                );
              })}
            </Command.List>
          </Command>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
