import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";

/** Local fail-closed explanation for a mutation dialog that was already open. */
export function ExtensionMutationPausedNotice() {
  const { t } = useTranslation();

  return (
    <aside
      role="alert"
      aria-label={t("extensions.actionsPaused.title")}
      className="mb-4 flex min-w-0 items-start gap-2 rounded-lg border border-warning/30 bg-warning/10 p-3"
    >
      <AlertTriangle
        className="mt-0.5 h-4 w-4 shrink-0 text-warning"
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-caption font-medium text-content">
          {t("extensions.actionsPaused.title")}
        </p>
        <p className="mt-0.5 text-caption leading-5 text-content-muted">
          {t("extensions.actionsPaused.description")}
        </p>
      </div>
    </aside>
  );
}
