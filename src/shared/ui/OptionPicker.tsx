import * as Popover from "@radix-ui/react-popover";
import { Command } from "cmdk";
import { Search } from "lucide-react";
import * as React from "react";
import { cn } from "./cn";
import { matches } from "./optionPickerFilter";
import {
  OptionPickerActionItem,
  OptionPickerItem,
  type OptionPickerAction,
  type OptionPickerOption,
} from "./OptionPickerItem";

export type {
  OptionPickerAction,
  OptionPickerOption,
} from "./OptionPickerItem";

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
  /** Width of the open list; it is never narrower than the trigger. */
  contentClassName?: string;
  onSelect: (id: string) => void;
}

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
  contentClassName = "w-72",
  onSelect,
}: OptionPickerProps) {
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const listRef = React.useRef<HTMLDivElement>(null);
  const total = options.length;
  const filterable = total > filterAbove;
  const needle = query.trim().toLocaleLowerCase();
  // Filtered here rather than by the list itself, which would also reorder
  // the options by match score and move the actions above them.
  const shown =
    filterable && needle
      ? options.filter((option) => matches(option, needle))
      : options;
  // Kept outside the listbox, which may only hold options.
  const message =
    note ?? (total === 0 ? empty : shown.length === 0 ? noMatch : null);
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
            "app-floating-menu z-[70] min-w-[var(--radix-popover-trigger-width)] max-w-[calc(100vw-2rem)] overflow-hidden rounded-lg border outline-none animate-ds-overlay-in",
            "flex max-h-[min(24rem,var(--radix-popover-content-available-height))] flex-col",
            contentClassName,
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
                <OptionPickerItem
                  key={option.id}
                  option={option}
                  onSelect={() => choose(() => onSelect(option.id))}
                />
              ))}
              {shown.length > 0 && actions.length > 0 ? (
                <Command.Separator className="my-1 h-px bg-hairline" />
              ) : null}
              {actions.map((action) => (
                <OptionPickerActionItem
                  key={action.id}
                  action={action}
                  onSelect={() => choose(action.onSelect)}
                />
              ))}
            </Command.List>
          </Command>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
