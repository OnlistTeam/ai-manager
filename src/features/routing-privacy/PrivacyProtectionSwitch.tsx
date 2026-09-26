import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { PreferenceSaveStatus } from "@/shared/ui/PreferenceSaveStatus";
import { Switch } from "@/shared/ui/Switch";
import { usePrivacyProtectionSwitch } from "./usePrivacyProtectionSwitch";

export interface PrivacyProtectionSwitchProps {
  className?: string;
}

/**
 * The single privacy protection switch for traffic that goes through the
 * local routing proxy (ADR-0049). Self-contained: it loads and saves the
 * setting itself, so a page only has to place it.
 */
export function PrivacyProtectionSwitch({
  className,
}: PrivacyProtectionSwitchProps) {
  const { t } = useTranslation();
  const state = usePrivacyProtectionSwitch();
  const controlId = useId();
  const descriptionId = useId();
  const statusId = useId();
  const label = t("routing.privacy.label");
  const showsStatus = !state.loadFailed && state.saveState !== "idle";

  return (
    <div
      className={cn("flex items-start justify-between gap-4 py-3", className)}
    >
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
          {t("routing.privacy.description")}
        </p>
        {state.loadFailed ? (
          <div
            role="alert"
            className="mt-1.5 flex flex-wrap items-center gap-2 text-caption text-danger"
          >
            <span>{t("routing.privacy.unavailable")}</span>
            <Button
              variant="secondary"
              size="xs"
              loading={state.retrying}
              onClick={state.retry}
            >
              {t("preferences.retry")}
            </Button>
          </div>
        ) : (
          <PreferenceSaveStatus id={statusId} state={state.saveState} />
        )}
      </div>
      <Switch
        id={controlId}
        checked={state.enabled}
        disabled={state.disabled}
        aria-label={label}
        aria-describedby={
          showsStatus ? `${descriptionId} ${statusId}` : descriptionId
        }
        onCheckedChange={state.toggle}
        className="mt-0.5"
      />
    </div>
  );
}
