import { forwardRef } from "react";
import { CopyButton } from "./CopyButton";
import { Input, type InputProps } from "./Input";
import { cn } from "./cn";

interface CopyableInputProps extends Omit<InputProps, "value"> {
  value: string;
  copyLabel: string;
}

/** Copy the current visible value without submitting the surrounding form. */
export const CopyableInput = forwardRef<HTMLInputElement, CopyableInputProps>(
  function CopyableInput({ value, copyLabel, className, ...props }, ref) {
    return (
      <div className="flex min-w-0 items-center gap-2">
        <Input
          ref={ref}
          {...props}
          value={value}
          className={cn("min-w-0 flex-1", className)}
        />
        {value ? <CopyButton value={value} label={copyLabel} /> : null}
      </div>
    );
  },
);
