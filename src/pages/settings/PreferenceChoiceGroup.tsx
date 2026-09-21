import { useId, type ComponentType } from "react";
import { cn } from "@/shared/ui/cn";

export interface PreferenceChoice<T extends string> {
  value: T;
  label: string;
  icon?: ComponentType<{ className?: string }>;
}

export interface PreferenceChoiceGroupProps<T extends string> {
  label: string;
  descriptionId?: string;
  value: T;
  options: readonly PreferenceChoice<T>[];
  onChange: (value: T) => void;
  disabled?: boolean;
  className?: string;
}

/**
 * Compact native-radio group used by Settings. Native radios keep arrow-key
 * behavior while their labels provide the product's segmented-control look.
 */
export function PreferenceChoiceGroup<T extends string>({
  label,
  descriptionId,
  value,
  options,
  onChange,
  disabled = false,
  className,
}: PreferenceChoiceGroupProps<T>) {
  const groupId = useId();

  return (
    <fieldset aria-describedby={descriptionId} className={className}>
      <legend className="sr-only">{label}</legend>
      <div className="grid auto-cols-fr grid-flow-col gap-1 rounded-lg border border-hairline bg-layer-1 p-1">
        {options.map((option) => {
          const Icon = option.icon;
          const id = `${groupId}-${option.value}`;
          return (
            <label key={option.value} htmlFor={id} className="min-w-0">
              <input
                id={id}
                type="radio"
                name={groupId}
                value={option.value}
                checked={value === option.value}
                disabled={disabled}
                onChange={() => onChange(option.value)}
                className="peer sr-only"
              />
              <span
                className={cn(
                  "flex h-8 cursor-pointer items-center justify-center gap-1.5 rounded-md px-2 text-caption font-medium text-content-muted",
                  "transition-colors duration-fast ease-standard hover:bg-layer-2 hover:text-content",
                  "peer-checked:bg-layer-1 peer-checked:text-content peer-checked:shadow-sm",
                  "peer-focus-visible:outline-none peer-focus-visible:ring-2 peer-focus-visible:ring-brand peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-canvas-subtle",
                  "peer-disabled:cursor-not-allowed peer-disabled:opacity-50",
                )}
              >
                {Icon ? (
                  <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                ) : null}
                <span className="truncate">{option.label}</span>
              </span>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
