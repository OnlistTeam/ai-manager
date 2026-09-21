import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";

/** A persistence failure must stay in the startup choice, without leaking settings detail. */
export function ImportDecisionNotice() {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      aria-label={t("preferences.import.decisionError.title")}
      className="flex items-start gap-3 rounded-lg border border-danger/25 bg-danger/10 px-4 py-3"
    >
      <AlertTriangle
        className="mt-0.5 h-4 w-4 shrink-0 text-danger"
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-body font-medium text-content">
          {t("preferences.import.decisionError.title")}
        </p>
        <p className="mt-0.5 text-caption leading-5 text-content-muted">
          {t("preferences.import.decisionError.description")}
        </p>
      </div>
    </div>
  );
}
