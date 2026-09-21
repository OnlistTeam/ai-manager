import * as CheckboxPrimitive from "@radix-ui/react-checkbox";
import { Check } from "lucide-react";
import { cn } from "./cn";
import { FOCUS_RING } from "./focusRing";

export interface CheckboxProps {
  id: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
  /** Associates a side note (e.g. "Delete this tool's config directory") without joining the accessible name. */
  "aria-describedby"?: string;
}

/**
 * Radix owns role / aria / keyboard (Space toggles); this only supplies product appearance.
 * Controlled component: the checked state for destructive actions must be held by the caller
 * so it can be reset in one shot when the dialog closes.
 */
export function Checkbox({
  id,
  checked,
  onCheckedChange,
  disabled = false,
  className,
  "aria-describedby": ariaDescribedBy,
}: CheckboxProps) {
  return (
    <CheckboxPrimitive.Root
      id={id}
      checked={checked}
      disabled={disabled}
      aria-describedby={ariaDescribedBy}
      onCheckedChange={(value) => onCheckedChange(value === true)}
      className={cn(
        // The radius scale is tuned for panels; a 16px control needs its own.
        "flex h-4 w-4 shrink-0 items-center justify-center rounded-[5px] border border-hairline-strong bg-layer-2",
        "transition-colors duration-fast ease-standard",
        "data-[state=checked]:border-brand data-[state=checked]:bg-brand data-[state=checked]:text-brand-foreground",
        "disabled:cursor-not-allowed disabled:opacity-50",
        FOCUS_RING,
        className,
      )}
    >
      <CheckboxPrimitive.Indicator>
        <Check className="h-3 w-3" aria-hidden="true" />
      </CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
  );
}
