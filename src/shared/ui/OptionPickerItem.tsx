import { Command } from "cmdk";
import { Check, type LucideIcon } from "lucide-react";
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

const ITEM_CLASS = cn(
  "mx-1 flex min-w-0 cursor-default select-none items-center gap-2 rounded-md px-2 py-1.5 text-caption text-content outline-none",
  "data-[selected=true]:bg-layer-2",
);

export interface OptionPickerItemProps {
  option: OptionPickerOption;
  onSelect: () => void;
}

/** One choosable row; the option in effect is checked and announced as current. */
export function OptionPickerItem({ option, onSelect }: OptionPickerItemProps) {
  return (
    <Command.Item
      value={option.id}
      aria-current={option.checked ? "true" : undefined}
      disabled={option.disabled}
      onSelect={onSelect}
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
      <span className="min-w-0 flex-1 truncate">{option.label}</span>
      {option.detail ? (
        <span className="max-w-[45%] shrink-0 truncate text-content-muted">
          {option.detail}
        </span>
      ) : null}
    </Command.Item>
  );
}

export function OptionPickerActionItem({
  action,
  onSelect,
}: {
  action: OptionPickerAction;
  onSelect: () => void;
}) {
  const Icon = action.icon;
  return (
    <Command.Item
      value={`action:${action.id}`}
      onSelect={onSelect}
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
      <span className="min-w-0 flex-1 truncate">{action.label}</span>
    </Command.Item>
  );
}
