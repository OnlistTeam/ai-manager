import { useId, type ComponentType, type ReactNode } from "react";
import {
  PreferenceSaveStatus,
  type PreferenceSaveState,
} from "@/shared/ui/PreferenceSaveStatus";
import { Switch } from "@/shared/ui/Switch";

export interface PreferenceRowProps {
  icon: ComponentType<{ className?: string }>;
  label: string;
  description: string;
  /** The `id` of the control, so the label is a real `<label for>`. */
  controlId: string;
  descriptionId: string;
  /** Save status or error, shown under the description. */
  status?: ReactNode;
  control: ReactNode;
}

/** One settings row: icon, label and description on the left, the control on the right. */
export function PreferenceRow({
  icon: Icon,
  label,
  description,
  controlId,
  descriptionId,
  status,
  control,
}: PreferenceRowProps) {
  return (
    <div className="grid gap-3 border-b border-hairline py-3 last:border-b-0 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
      <div className="flex min-w-0 items-start gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-layer-1 text-brand">
          <Icon className="h-4 w-4" aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <label
            htmlFor={controlId}
            className="text-body font-medium text-content"
          >
            {label}
          </label>
          <p
            id={descriptionId}
            className="mt-0.5 text-caption text-content-muted"
          >
            {description}
          </p>
          {status}
        </div>
      </div>
      {control}
    </div>
  );
}

export interface PreferenceSwitchRowProps {
  icon: ComponentType<{ className?: string }>;
  label: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  saveState: PreferenceSaveState;
  onChange: (checked: boolean) => void;
}

/** A settings row whose control is a switch saved as soon as it changes. */
export function PreferenceSwitchRow({
  icon,
  label,
  description,
  checked,
  disabled,
  saveState,
  onChange,
}: PreferenceSwitchRowProps) {
  const controlId = useId();
  const descriptionId = useId();
  const statusId = useId();
  const describedBy =
    saveState === "idle" ? descriptionId : `${descriptionId} ${statusId}`;

  return (
    <PreferenceRow
      icon={icon}
      label={label}
      description={description}
      controlId={controlId}
      descriptionId={descriptionId}
      status={<PreferenceSaveStatus id={statusId} state={saveState} />}
      control={
        <Switch
          id={controlId}
          checked={checked}
          disabled={disabled}
          aria-label={label}
          aria-describedby={describedBy}
          onCheckedChange={onChange}
        />
      }
    />
  );
}
