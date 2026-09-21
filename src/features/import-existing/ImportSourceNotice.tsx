import { Database } from "lucide-react";
import { useTranslation } from "react-i18next";

/** Keeps the migration source transparent without turning upstream branding into the feature title. */
export function ImportSourceNotice() {
  const { t } = useTranslation();

  return (
    <p className="flex items-center gap-2 text-caption leading-5 text-content-muted">
      <Database className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      {t("preferences.import.sourceDetail")}
    </p>
  );
}
