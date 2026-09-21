import { AlertCircle, CheckCircle2, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

export type PreferenceSaveState = "idle" | "saving" | "saved" | "error";

export interface PreferenceSaveStatusProps {
  id: string;
  state: PreferenceSaveState;
}

export function PreferenceSaveStatus({ id, state }: PreferenceSaveStatusProps) {
  const { t } = useTranslation();

  if (state === "idle") return null;

  const label = t(`preferences.advanced.${state}`);
  if (state === "error") {
    return (
      <div
        id={id}
        role="alert"
        aria-label={label}
        aria-atomic="true"
        className="mt-1.5 flex items-start gap-1.5 text-caption leading-5 text-danger"
      >
        <AlertCircle
          className="mt-0.5 h-3.5 w-3.5 shrink-0"
          aria-hidden="true"
        />
        <p>
          <span className="font-medium">{label}</span>
          {": "}
          {t("preferences.advanced.errorHint")}
        </p>
      </div>
    );
  }

  const Icon = state === "saving" ? Loader2 : CheckCircle2;
  return (
    <span
      id={id}
      role="status"
      aria-label={label}
      aria-atomic="true"
      className={
        state === "saved"
          ? "mt-1.5 flex items-center gap-1.5 text-caption text-success"
          : "mt-1.5 flex items-center gap-1.5 text-caption text-content-muted"
      }
    >
      <Icon
        className={
          state === "saving"
            ? "h-3.5 w-3.5 motion-safe:animate-spin"
            : "h-3.5 w-3.5"
        }
        aria-hidden="true"
      />
      {label}
    </span>
  );
}
