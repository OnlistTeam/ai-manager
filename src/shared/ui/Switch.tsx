import * as SwitchPrimitive from "@radix-ui/react-switch";
import { cn } from "./cn";
import { FOCUS_RING } from "./focusRing";

export interface SwitchProps {
  id?: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
  /** Required: spec §98 wants every control to carry its own name. */
  "aria-label": string;
  "aria-describedby"?: string;
}

/**
 * Radix owns role="switch", the aria state and the keyboard (Space / Enter);
 * this wrapper only supplies the product look. Controlled on purpose — the
 * caller holds the value so a failed write can hand back the old one.
 */
export function Switch({
  id,
  checked,
  onCheckedChange,
  disabled = false,
  className,
  "aria-label": ariaLabel,
  "aria-describedby": ariaDescribedBy,
}: SwitchProps) {
  return (
    <SwitchPrimitive.Root
      id={id}
      checked={checked}
      disabled={disabled}
      aria-label={ariaLabel}
      aria-describedby={ariaDescribedBy}
      onCheckedChange={onCheckedChange}
      className={cn(
        "inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-hairline-strong bg-layer-2 p-0.5",
        "transition-colors duration-fast ease-standard",
        "data-[state=checked]:border-brand data-[state=checked]:bg-brand",
        "disabled:cursor-not-allowed disabled:opacity-50",
        FOCUS_RING,
        className,
      )}
    >
      <SwitchPrimitive.Thumb
        className={cn(
          "block h-4 w-4 rounded-full bg-content shadow-sm",
          "transition-transform duration-fast ease-standard",
          "data-[state=checked]:translate-x-4",
        )}
      />
    </SwitchPrimitive.Root>
  );
}
